# Blacktop F8 overlay and Blacktop session — WPF reference
Source build: WPF quality-pass-qa-20261010 (frozen). Last researched: 2026-10-17.

Path abbreviations (all relative to `C:\Users\carrn\CodexProjects\2kconversiontool`):
- **WR** = `nba2k27-launcher-wpf\release\quality-pass-qa-20261010\App\Reference` (frozen Electron-style JS that the WPF worker runs unchanged)
- **WH** = `nba2k27-launcher-wpf` (C# WPF host: `NativeBridge.cs`, `NativeWindow.cs`, `worker\host.cjs`, `bridge.js`)
- **RS** = `nba2k27-roster-research\src` (source of `NBA2K27Blacktop.exe` and `NBA2K27F8Watcher.exe`. The `dist\` binaries are SHA-identical to the ones in `WR\runtime-resources\roster-research\`: Blacktop `3f7f34b7…`, F8Watcher `4066cf90…`.)
- **VP** = `nba2k27-version-proxy\src`
- **SN** = `C:\Users\carrn\AppData\Roaming\Claude\side-session-notes\local_98bf0092-fe39-4ae3-abce-e88edad0a633`
- **ML** = `nba2k27-mod-launcher`

## Summary
- The "F8 overlay" is one always-on-top WebView window titled **"Blacktop outfits"**. It renders `WR\src\renderer\blacktop-panel.html` with a narrow preload (`WR\src\blacktop-panel-preload.js`).
  - All window logic is in `WR\src\core\blacktop-panel.js` (class `BlacktopPanel`).
  - The WPF host emulates Electron's `BrowserWindow` in `WH\worker\host.cjs:38-60` and turns each call into a Win32 action in `WH\NativeBridge.cs:141-157` and `WH\NativeWindow.cs:333-335`.
- The window has two states:
  - **Compact bar** (default): non-focusable (`WS_EX_NOACTIVATE`), shown without activation, top of the monitor, 78 or 120 px tall. The game keeps focus.
  - **Expanded editor** (Edit outfit, Game rules, scoreboard catalog, ball catalog): focusable, 75% of the screen height, takes focus.
  - Leaving either state hands focus back to the game. A separate helper calls `SetForegroundWindow(game.MainWindowHandle)` (`RS\BlacktopTool.cs:421-429`).
- **F8 is not a hook and not RegisterHotKey.** It comes from a polling child process, `NBA2K27F8Watcher.exe`:
  - It reads `GetAsyncKeyState(VK_F8)` every 20 ms.
  - It reports an F8 press only when the foreground window belongs to the game exe or to the launcher's node worker (`RS\F8Watcher.cs:17-51`).
  - The WPF host explicitly ignores F8 registration (`WH\NativeBridge.cs:209-213`).
- One read-only `detect` poll drives three things: auto-open/auto-hide, hide-when-game-loses-focus, and auto-random outfits.
  - It runs `NBA2K27Blacktop.exe detect` every **2000 ms**, or every **100 ms** while the Blacktop selection screen is up (`WR\src\core\blacktop-auto-open.js:43-68`).
- Outfit writes go through `NBA2K27Blacktop.exe apply` at player selection. Each verified result is recorded to a session file, and a PowerShell watcher replays it into the loaded match through version.dll. That is the "match carry-over".

**Top 3 port risks**
1. **Focus model.** The compact bar must never take focus; it is shown inactive and non-focusable.
   - Only an expanded view takes focus, and collapsing must re-focus the game.
   - Collapsing then suppresses "game lost focus → hide" for **900 ms** (`blacktop-panel.js:146-153`).
   - The game-foreground test is **foreground PID == game PID** (`RS\BlacktopTool.cs:398,418`).
2. **Auto-open gating.** The panel opens only after **2 consecutive** detect reads with `selection && lineupReady` while the game is in front, once per visit.
   - It hides after **2 consecutive** reads outside selection, or when the lineup becomes incomplete.
   - A user dismissal stays respected while the lineup remains complete (`blacktop-auto-open.js:10-40`).
3. **Auto-random always runs** (WPF has no setting for it).
   - It runs for each newly loaded player at selection, once per `slot:description:roster` identity per game session.
   - It skips players that already have a manual journal entry, and never retries an uncertain write (`WR\src\core\blacktop-manager.js:211-253`).

## Key source map
| Concern | file:line |
|---|---|
| Panel window class (create/show/hide/expand/toggle/focus) | `WR\src\core\blacktop-panel.js:3-213` |
| Window geometry (compact/expanded/menu rows) | `WR\src\core\blacktop-panel.js:53-61,156-161` |
| F8 listener wrapper (spawns watcher, READY/F8 protocol) | `WR\src\core\f8-shortcut.js:4-57` |
| F8 watcher exe (polling, foreground filter) | `RS\F8Watcher.cs:9-57` |
| Auto-open / auto-hide / foreground / auto-random poll | `WR\src\core\blacktop-auto-open.js:3-71` |
| Wiring (helper paths, onHide=focusGame, onPresent) | `WR\src\main.js:874-900` |
| Panel IPC (open/hide/expanded/player-menu/status/rules) | `WR\src\main.js:269-293` |
| Outfit IPC (scan/apply/carry-status/thumbnail/outfits-list) | `WR\src\main.js:295-347` |
| Outfit manager (scan, apply, journal, auto-random) | `WR\src\core\blacktop-manager.js:150-330` |
| Native outfit helper (detect/focus/scan/apply/original) | `RS\BlacktopTool.cs:175-206,388-487` |
| Match-choices store | `WR\src\core\blacktop-match-choices.js:6-34` |
| Match carry watcher (PowerShell) | `WR\runtime-resources\session-loader\session_saved_outfits.ps1:1-160` |
| Plan writer (BOP3) | `WR\runtime-resources\session-loader\write_saved_outfit_plan.ps1:1-28` |
| In-game carry worker (version.dll) | `VP\character_description_clone_sep11.hpp:371-443`, `VP\character_saved_plan.hpp` |
| Session start/stop of carry watcher | `WR\src\core\blacktop-session.js:4-23`, `WR\src\main.js:2052-2057,2066-2070` |
| Overlay markup | `WR\src\renderer\blacktop-panel.html:6-42` |
| Overlay CSS (bar geometry) | `WR\src\renderer\blacktop-live-bar.css:1-69`, `blacktop-topbar.css:1-117`, `blacktop-panel.css:1-40` |
| Overlay logic (players, editor, random, presets) | `WR\src\renderer\blacktop.js:1-458` |
| Player dropdown | `WR\src\renderer\blacktop-player-menu.js:1-46` |
| Random all batch | `WR\src\renderer\blacktop-batch.js:1-18` |
| Status badge | `WR\src\renderer\blacktop-live-bar.js:1-13` |
| WPF window emulation | `WH\worker\host.cjs:21-60,83,105-108`; `WH\NativeBridge.cs:137-157,209-215`; `WH\NativeWindow.cs:39-59,74-87,333-336` |
| Escape forwarding | `WH\bridge.js:16-19` → `WH\worker\host.cjs:106` → `blacktop-panel.js:92-101` |
| Main-page Blacktop section | `WR\src\renderer\index.html:107-188`; `app.js:545,2751-2756` |
| Setting default `autoOpenBlacktop: true` | `WR\src\core\paths.js:23` |
| Launch-time arming (unlocks/presentation/crowd) | `WR\src\main.js:1799-1805` |

---

### 1. Main launcher Blacktop page

#### 1. User-visible behavior
- Nav item **"Blacktop"** (`index.html:49`). The page `#page-blacktop` (`index.html:107`) has two `<details open>` sections.
- **"Game setup"** (`index.html:108-149`) contains three cards.
  - **"Scoreboard, arena & ball"**: three rows (Scoreboard / Arena / Ball). Each row has a current label, a short status line and a **"Catalog"** button.
    - The button opens a modal `<dialog class="blacktop-catalog">` (`blacktop-setup-picker.js:92`).
    - Dialog title is `"<Kind> catalog"` with the eyebrow "BLACKTOP PRESENTATION" (`:67`).
    - The search placeholder is "Search <kind> catalog". The category chips start with "All". Arena adds "★ Favorites" plus the arena groups. Scoreboard uses Classic/Broadcast/Eras/Experimental (`:34-37`).
    - The grid shows 24 per page with Previous/Next and an "a–b of n" count (`:70,88`).
    - Choosing an item closes the dialog and saves it (`:54`).
    - Short status strings (`:12-25`):
      - "Waiting for Blacktop"
      - "Waiting for safe selection" (ball)
      - "Ball loaded"
      - "Active"
      - "Saved for next launch"
      - "Ready for Blacktop"
      - "Stock scoreboard"
      - "Choose for Blacktop"
      - "Not available for live swap"
      - "Stock ball changed; swap paused"
    - Arena, ball and scoreboard mechanics belong to other reference files. This file covers only the overlay copies.
  - **"Court presentation"**: switches "Remove drone", "Remove DJ", "Remove MC" and "Remove crowd", plus a hidden button "Restore original crowd setting" (`index.html:131-142`; logic `blacktop-presentation.js`, `blacktop-crowd.js`).
    - The crowd checkbox is **inverted**: checked means `enabled:false` (`renderer\blacktop-crowd.js:6,10`).
    - These options are saved for the **next Play launch** and armed at launch (`main.js:1803-1804`). They are not changed from the F8 overlay.
  - **"Blacktop overlay"** (`index.html:143-147`):
    - Switch **"Open overlay automatically"** (`#autoOpenBlacktop`). It saves `autoOpenBlacktop` immediately (`app.js:2751-2756`); on failure it reverts the checkbox and shows `alert(error.message)`. **Default true** (`paths.js:23`).
    - Button **"Preview F8 overlay"**. It opens `<dialog id="blacktopOverlayPreview">` titled "F8 overlay preview" with the subtitle "Interactive preview · dummy player · no game changes".
      - The dialog contains an iframe that reloads the real F8 markup on every open (`blacktop-overlay-preview.js:29-35`).
      - The dummy player is `'Preview player'` (`blacktop-overlay-preview-frame.js`).
- **"Outfit creator"** section (`index.html:150-178`): this is the saved-outfit library. Saved outfits appear in the overlay's "Saved outfit" select. See `outfits.md`.
- **Hidden legacy controls** (`index.html:180-188`, `class="hidden"`):
  - Button "Open game panel · F8" with the hint "F8 opens the outfit panel over borderless/windowed NBA 2K27. Keep the launcher open."
  - "Refresh players".
  - These are not visible in the WPF build. Their handler (`blacktop.js:430-457`) is the **only** path that re-calls `register()` after an F8 watcher failure (see §2.2).

#### 2. Timing and gating
- Crowd and presentation choices take effect at the next Play. They arm before Steam starts the game (`main.js:1799-1805`).
- `blacktopPresentation.arm` and `blacktopCrowd.arm` give up after **120 000 ms** without finding a new game session (`core\blacktop-presentation.js:35,41`; `core\blacktop-crowd.js:39,47`).
- Both tick every **500 ms**; crowd ticks every **250 ms** while applied (`core\blacktop-crowd.js:66`).
- Crowd restore at the main menu happens "Auto-restoration ends a Blacktop visit, not the Play-session monitor" (`core\blacktop-crowd.js:62-64`). A later Blacktop visit in the same game can apply again.

#### 3. Mechanism
- `autoOpenBlacktop` lives in the launcher settings JSON. Other keys: `blacktopPresentation:{drone,dj,mc}` and `blacktopCrowdEnabled` (`paths.js:23,26-27,35-37`).
- Presentation and crowd helpers: `freestyleUnlocks.helperPath` (the unlocks helper) with the actions `debug-bind` / `presentation-tick` / `crowd-bind|tick|restore`, 8000 ms timeout (`core\blacktop-presentation.js:15-16`, `core\blacktop-crowd.js:14`).
- Session files are under `<userData>\unlocks\` (`main.js:2035,2041`). userData is `<Portable>\Data` (`WH\worker\host.cjs:61`).

#### 4. Build profiles
- WPF helpers pin the original build (`6A974D13`) and the Sep-11 build (`6AA2013D`) only. V2 is unsupported in WPF (SN `blacktop-presentation-overlay.md`).

#### 5. Rust must match
- [ ] "Open overlay automatically" defaults to ON and saves immediately. A save failure reverts the switch.
- [ ] "Preview F8 overlay" renders the same markup as the live overlay with a dummy "Preview player" and no game access.
- [ ] Court presentation switches apply at the next Play, not live.

---

### 2. F8 overlay window: creation, show/hide, topmost, focus

#### 1. User-visible behavior
- **Creation.** The window is created hidden at launcher start (`prepare()`, `main.js:891`; `blacktop-panel.js:52`), so the first F8 is instant.
  - Options: title "Blacktop outfits", `frame:false`, `show:false`, `alwaysOnTop:true`, `skipTaskbar:true`, `focusable:false`, `resizable:false` (`blacktop-panel.js:66-73`).
  - The webview denies new windows and navigation (`:90-91`).
- **Compact bar** (default every time it is shown, see §4): one row, or two rows on narrow screens. Contents:
  - crown brand
  - Mode: "Blacktop"
  - Player
  - Actions: "Edit outfit" / "Random player" / "Random all players"
  - Arena, Scoreboard and Ball buttons, initially "Loading…"
  - Rules: "Game rules"
  - Status badge
  - Overlay: "Hide overlay" (title "Hide overlay (F8 or Escape)")
  - Source: `blacktop-panel.html:9-27`.
- **Expanded views** cover Edit outfit, Game rules, scoreboard catalog and ball catalog (`blacktop.js:24-51`).
  - A heading shows "BLACKTOP / Edit outfit" or "Game rules" with a "Back to game" button (`blacktop-panel.html:7`).
  - The footer reads "F8 to hide · Esc back to game" and "Edit on player selection · Session-only outfits" (`:38`).
  - Clicking the active button again, "Back to game", or Escape returns to the compact bar.
- **Escape**:
  - Expanded: collapse to the compact bar and send `blacktop:panel-collapsed`.
  - Compact: hide the panel.
  - Auto-repeat is ignored (`blacktop-panel.js:92-101`).
  - Page-level menus (random menu, arena menu) consume Escape first (`WH\bridge.js:16-19`).
  - In the compact bar the window cannot hold keyboard focus, so Escape only reaches it in practice when expanded or after a click. **UNVERIFIED**, inferred from `WS_EX_NOACTIVATE`.
- **Closing** the window (`close` event) is turned into a hide (`blacktop-panel.js:102`).
- **Renderer crash** (`render-process-gone`) destroys the window and resets the state. The next F8 creates a fresh UI (`:84-89`).

#### 2. Timing and gating
- **F8 toggle** (`blacktop-panel.js:196-201`):
  - It **hides** when `wanted && (!expanded || !ready || window.isFocused())`. Otherwise it **shows**.
  - So an expanded but unfocused panel is re-summoned, not hidden.
- **show()** sets `wanted=true` and `focusSuspended=false`, ensures the window exists, then `present()`.
  - If the page has not finished loading, `present()` is skipped and runs on `did-finish-load` (`:104,110,183`).
- **present()** runs synchronously, in this order (`:109-130`):
  1. Restore if minimized.
  2. `setBounds(panelBounds(displayArea(), expanded))`.
  3. `applyTopmost()`: `setAlwaysOnTop(true,'screen-saver')`, and throw if not retained.
  4. `setFocusable(expanded)`.
  5. Expanded: `show()` plus `onPresent()`. Compact: `showInactive()`.
  6. `moveTop()`.
  7. Expanded only: `focus()`.
  8. Send `blacktop:panel-shown`.
  - Trace observations follow at 100 ms and 750 ms (`:31-41`).
- **hide()** sets `wanted=false` and clears suspend/handoff. Then `await onHide()`, which focuses the game while the panel still has focus. Then `win.hide()`, or `present()` again if `wanted` became true meanwhile (`:184-195`).
- **Game focus loss.** Each detect tick calls `setGameForeground(value.foreground)` (`blacktop-auto-open.js:53`).
  - On `false`: if within the 900 ms focus hand-off, ignore. Otherwise, if `wanted` and **not** (expanded and focused), set `focusSuspended=true` and hide the window (`blacktop-panel.js:162-175`).
  - On `true`: clear the hand-off. If it was suspended, un-suspend and `present()` (`:176-181`).
  - This gives the **Alt-Tab** behavior: the panel disappears within one poll (≤2000 ms off selection, ≤100 ms on selection) and reappears when the game is in front again.
  - When the game exits, `detect` returns `foreground:false` (`RS\BlacktopTool.cs:394`), so the panel hides as suspended.
- **Expand** (`setExpanded(true)`, `:137-146`): `playerMenuRows=0`, `setFocusable(true)`, expanded bounds, `onPresent()`, `window.focus()`.
  - Switching between expanded views does **not** make another native call (`blacktop.js:45-47`).
- **Collapse** (`setExpanded(false)` from an expanded state, `:146-154`):
  - Set `focusHandoff=true`.
  - Run `onHide()` (focus the game).
  - Then `applyTopmost()`, `showInactive()` and `moveTop()`.
  - Clear `focusHandoff` after **900 ms**.
- **Display change** (`display-metrics-changed`) re-applies bounds (`blacktop-panel.js:9-10`).
  - The WPF host polls displays and the cursor every **500 ms** and pushes changes (`WH\NativeBridge.cs:46-57`).

#### 3. Mechanism (WPF host specifics)
How each Electron call maps to the WPF host (`WH\worker\host.cjs:45-53` → `WH\NativeBridge.cs:141-157` → `WH\NativeWindow.cs`):

| Electron call | WPF action |
|---|---|
| `setAlwaysOnTop(true,'screen-saver')` | `Window.Topmost = true`. The level is ignored, and `isAlwaysOnTop()` returns a cached flag, so the "not retained" throw never fires in WPF. |
| `setFocusable(v)` | Toggles `WS_EX_NOACTIVATE` (0x08000000) on GWL_EXSTYLE, and `ShowActivated = v` (`NativeWindow.cs:333`). |
| `show()` / `showInactive()` | `ShowActivated = activate; Show();`. If activating, also `Activate()` and `View.Focus()`. |
| `moveTop()` | `SetWindowPos(HWND_TOPMOST, 0,0,0,0, SWP_NOSIZE\|SWP_NOMOVE\|SWP_NOACTIVATE = 0x13)` (`NativeWindow.cs:334`). |
| `focus()` | `Activate(); View.Focus();` |
| `setBounds` | `SetWindowPos(..., 0x14 = SWP_NOZORDER\|SWP_NOACTIVATE)` in **physical pixels**, clamped to minWidth/minHeight (`NativeWindow.cs:74-87`). |
| `hide()` | `Window.Hide()` |

- `isFocused()` is tracked from WPF `Activated`/`Deactivated` events (`NativeWindow.cs:55-56`, `host.cjs:105`).
- `screen.coordinateSpace='physical'` and the display list come from `DisplayGeometry.Read()`, ordered with the primary first. The scale factor is per-monitor DPI/96 (`WH\DisplayGeometry.cs:14-21`, `host.cjs:21-24`).
- The display is picked as `getDisplayNearestPoint(getCursorScreenPoint())`. It uses `display.bounds` (the full monitor, **not** the work area) (`blacktop-panel.js:54`).
- **onHide** = `NBA2K27Blacktop.exe focus <gameRoot> <layoutPath>` (`main.js:888`, `blacktop-manager.js:182`). The helper (`RS\BlacktopTool.cs:421-429`):
  - requires exactly one NBA2K27 process whose path matches the game root
  - takes `MainWindowHandle`
  - calls `ShowWindowAsync(SW_RESTORE=9)` if iconic
  - calls `SetForegroundWindow` and prints `{focused}`
  - It uses no input injection and no AttachThreadInput (comment at `blacktop-panel.js:189-190`).
- **onPresent** sets `blacktopAutoOpen.entered = true` (`main.js:887`). Opening the editor manually counts as this visit's open.
- **Diagnostics**: every trace record goes to `<logs>\blacktop-panel-<version>-<pid>.jsonl`, capped at 1 MiB (`core\panel-diagnostics.js:6-7`).

#### 4. Build profiles
The window logic is build-independent. `focus` checks only the exe path, not the build (`RS\BlacktopTool.cs:421-429`).

#### 5. Rust must match
- [ ] The compact bar appears without stealing focus. The game keeps keyboard and controller input. The bar sits topmost above the game.
- [ ] Opening Edit outfit / Game rules / scoreboard / ball catalogs focuses the overlay. Closing it ("Back to game", Escape, or re-clicking) returns focus to the game window (SetForegroundWindow on the game main window, restored if minimized) and keeps the compact bar visible.
- [ ] For 900 ms after a collapse, a "game not foreground" reading does not hide the bar.
- [ ] Alt-Tab away from the game hides the bar, unless the expanded overlay itself is focused. Returning to the game shows it again without focus theft.
- [ ] "Hide overlay" and Escape (compact) hide the bar and focus the game.
- [ ] A crashed overlay renderer is recreated on the next F8. No helper restarts.
- [ ] Bounds follow the monitor under the cursor and refresh on display changes.

---

### 2.2 Hotkeys (F8 and others)

#### 1. User-visible behavior
- **F8** toggles the overlay (§2 toggle rule). There are no other overlay hotkeys besides **Escape** inside the overlay.
- **Ctrl+Shift+F12** captures a game snapshot (diagnostics). It is registered at launch (`main.js:1813`, unregistered `:2326`) as a WPF `RegisterHotKey(mainHwnd, 273, MOD_CONTROL|MOD_SHIFT|MOD_NOREPEAT = 0x4006, VK_F12 = 0x7B)` (`WH\NativeWindow.cs:335-336`, `WH\NativeBridge.cs:209-215`). It is unrelated to the overlay.

#### 2. Timing and gating
- F8 registration starts at launcher start: `prepareBlacktopPanel()` → `register()` (`main.js:874-891`).
- The watcher must print `READY` within **5000 ms**, or it fails with "F8 listener startup timed out." (`f8-shortcut.js:26`).
- Any unexpected stdout line, or a buffer over 4096 bytes, fails it (`:30,36`).
- If the child exits, it fails with the stderr text or "F8 listener stopped. Reopen the game panel to retry." (`:42`).
- After a failure, `register()` retries only when called again (`blacktop-panel.js:43-44`). That happens only from `blacktop:panel-open` (`main.js:272`), whose button is hidden in WPF. **So a dead watcher is not restarted automatically.** (Inferred from code.)
- Watcher loop (`RS\F8Watcher.cs:40-52`):
  - The initial "held" state is read before `READY`, so a key held at startup does not fire.
  - Edge detection: it fires on the up→down edge only.
  - Poll interval is **20 ms**. The parent liveness check runs every 25 ticks (~500 ms). Stdin EOF stops it.
  - An F8 is emitted only if the foreground window's PID is `parentPid`, or the process image path equals `<gameRoot>\NBA2K27.exe` (`:17-28`, OpenProcess `0x1000` QUERY_LIMITED only).
- There is **no gating on game state**. F8 works in menus, selection and matches whenever the game is in front.
  - The overlay shows. If not at selection, the scan fails with "Open the Blacktop player-selection screen first." (`RS\BlacktopTool.cs:207`), which shows as the status `"<msg> Reopen the overlay to reconnect; original outfits are saved."` (`blacktop.js:410`).

#### 3. Mechanism
- `F8Shortcut` is passed as the panel's `globalShortcut` (`main.js:877-883`). It spawns `<runtime>\roster-research\NBA2K27F8Watcher.exe <parentPid> <gameRoot>` with `windowsHide` (`f8-shortcut.js:17`).
- `parentPid` defaults to `process.pid` (`f8-shortcut.js:5`).
  - **In WPF this is the node worker's PID, not the WPF exe that owns the overlay window.** So F8 pressed while the expanded (focused) overlay is in front is **not** reported. Escape or "Back to game" is the way out.
  - This is inferred from `WH\worker\host.cjs` (node child) and `NativeBridge.cs` (window owner). **UNVERIFIED live.**
- On WPF, `globalShortcut.register` is a no-op for F8, by design (`WH\NativeBridge.cs:211-212`: "F8 is handled by the existing foreground-aware native watcher").
- Stop: `unregister('F8')` destroys stdin, kills the child and bumps the generation (`f8-shortcut.js:49-55`). This runs at dispose (`blacktop-panel.js:208`).
- Shutdown waits up to 2500 ms (`main.js:2348`).
- Status text:
  - `{mode:'F8-only polling', state, error}` (`f8-shortcut.js:9`).
  - Hidden hint strings: "F8 listener ready (no hook)…" / "F8 is unavailable (another app may be using it)…" (`blacktop.js:446-448`).

#### 4. Build profiles
Build-independent.

#### 5. Rust must match
- [ ] F8 does nothing when neither the game nor the overlay/launcher is foreground. No key is consumed from other apps; WPF never blocks F8 for other apps because it only polls.
- [ ] F8 fires once per press (edge), not on auto-repeat, and not for a key already held at startup.
- [ ] F8 with the game in front toggles the overlay in every game state (menu, selection, match).
- [ ] Ctrl+Shift+F12 snapshot is separate and unaffected.

---

### 3. Exact layout and sizes

#### 1. User-visible behavior / numbers
All sizes are in physical pixels. `scale` = monitor DPI/96 (`blacktop-panel.js:53-61`).

| Quantity | Formula |
|---|---|
| width | `area.width − round(24·scale)` |
| x | `area.x + round((area.width − width)/2)` (centered, 12·scale gutters) |
| y | `area.y + round(12·scale)` |
| compact height | `round((area.width/scale − 24 ≥ 1380 ? 78 : 120)·scale)` |
| compact + menu | `min(compactHeight + round((rows ? rows·42 + 16 : 0)·scale), area.height − round(24·scale))`. `rows` is 0..7, clamped (`:156-161`). |
| expanded height | `min(area.height − round(24·scale), max(round(360·scale), round(area.height·0.75)))` |

- At creation the window is vertically centered (`:67`). `present()` moves it to the top.
- `minWidth = min(420, width)`, `minHeight = compact height` (`:68`).
- Bar CSS, wide (≥1380 CSS px) (`blacktop-live-bar.css:3-11`):
  - Grid columns `36px 100px minmax(130px,.88fr) 344px repeat(3,minmax(130px,.9fr)) 120px 90px 104px`.
  - Height 70 px, margin 4 px, padding 5×7, radius 18 px.
  - Border `#6b8b75`, bottom border `#59da80`.
  - Background `linear-gradient(105deg,#101915,#18221d 55%,#101915)`.
  - 70 + 2×4 = **78** = compact height.
- Bar CSS, narrow (≤1379 px) (`:53-67`): two rows `48px 54px`, height 112 px (+8 = **120**).
  - Row 1: brand, mode, player (span 2), actions (span 2), hide.
  - Row 2: arena (span 3), scoreboard, ball, rules, status.
- Field cells: label 9 px uppercase `#a7b6aa`; controls 27 px tall, radius 7, 11 px font, background `#17251c`, border `#5c7762`. Cell height 50 px with a left divider `#74857865` (`:15-19`).
- Actions group: 344 px, three buttons in a `1fr 1.4fr 1.6fr` grid, 10 px font (`:34-43`). "Random player" has a ▾ that rotates when open (`:45-46`).
- Status badge states (`:30-33`): ready (green `#184b2c`), loading (amber `#443419`), error (red `#492622`).
- Body background `#0b1110` with no border radius (`:2`).
- Expanded workspace (`blacktop-topbar.css:54-110`):
  - Left "Players" column 168 px.
  - Piece tiles 158 px tall, in a 4-column grid (2 columns ≤1000 px).
  - Accessory tiles 94 px.
  - Close button 34×34 at top-right (8, 10).
  - Status toast at `bottom:36px; left:210px; right:24px`.

#### 5. Rust must match
- [ ] On a 1920-wide, 100% DPI monitor, the compact bar is 1896×78 at (12, 12). At 1280 width it is 1256×120.
- [ ] Expanded height is 75% of the monitor height (minimum 360, maximum height−24).
- [ ] An open player menu with N players adds `ceil(N/2)·42 + 16` px (max 7 rows). The arena menu adds 7 rows (310 px). The random-mode strip adds 1 row (2 if the viewport is under 700 px).

---

### 4. Player dropdown, Edit outfit, Random player, Random all, presets

#### 1. User-visible behavior
- **On every show** (`blacktop:panel-shown`), the renderer:
  - collapses to the compact bar (`expandEditor(false)`)
  - runs `reloadPlayers()` (`blacktop.js:428`): it lists saved outfit presets (`blacktop:outfits-list`), then `scanBlacktop` with a fresh apply token
  - shows the status "Checking Blacktop players…" during the scan (`:418-425`)
  - **There is no periodic refresh while open** (`:429`).
- **Player select** (`#blacktopPlayerSelect`, hidden). It is replaced by a button `"<name> ▾"` (`blacktop-player-menu.js:4-7,19`).
  - Initial option text: "Select players in game" (`blacktop-panel.html:12`). After an empty or failed scan the button reads "Select player ▾".
  - **Contents and order:** one entry per scanned player, in helper scan order. That is slot 0→9 (`RS\BlacktopTool.cs:349-350`).
  - Each label is `player.name || "Player <slot+1>"` (`blacktop.js:140`). The option value is `"<pid>:<slot>:<roster||name>"` (`:108`).
  - **No search and no thumbnails** in the player dropdown.
  - The menu is a 2-column grid of 36 px `menuitemradio` buttons, `max-height:226px`, placed directly under the bar (`blacktop-topbar.css:9-12`, `blacktop-player-menu.js:35`).
  - Clicking outside, a panel show or a collapse closes it (`:42-44`).
  - The first player is auto-selected when the previous selection is gone (`blacktop.js:111`).
  - Opening the player menu while the random strip is open closes the strip, and re-opens it for the newly picked player (`blacktop-player-menu.js:24,33`).
- **Button enablement** (`blacktop.js:73-79`):
  - Player and Edit outfit are disabled with no players.
  - Random player is disabled while busy or with no players.
  - Random all is disabled if busy, no players, any player has `error`, or any clothing pool is not `male`/`female`.
- **Edit outfit** toggles the expanded editor (`:53`):
  - left: player buttons ("Player N" / name / "Needs reload")
  - top row: Top / Bottom / Socks / Shoes tiles with thumbnails, then accessory cards
  - heading row: "Saved outfit" select + "Apply preset"
  - Any compatible change **auto-applies after 350 ms** (`:326-329`).
  - Thumbnails come from preview catalogs first, then the native `blacktop:thumbnail` (PNG data URL) (`:174-196`).
  - Details are in `outfits.md`.
- **Random player** (`:11-15,340-366`) opens a strip under the bar (compact only) with four buttons:
  - "Random streetwear", "Random team clothes", "Random all", "Random accessories"
  - Layout: a 4-column grid (2 columns under 700 px), left/right 12 px.
  - A click applies to the currently selected player with `applyBlacktop({token, slot, randomFit:true, randomMode})`. The modes are `streetwear` / `team` / `all` / `accessories`.
  - The strip stays open; its buttons are disabled during the apply.
  - Escape, an outside click, or re-clicking "Random player" closes it.
  - It does nothing if the player has no separable streetwear with exactly one shoe (`:338,341`).
- **Random all players** (`blacktop-batch.js:2-16`, `blacktop.js:413-417`) applies `randomMode:'all', rosterShoes:true` to **each** player in sequence.
  - Status: "Randomizing i/N: <name>…". Button text: "Applying i/N…" with a spinner.
  - It stops with "Lineup changed after K outfits; stopped." if any slot, name or cyberface changes.
  - It stops with "Stopped after K confirmed outfits; refresh was not confirmed for <name>." on an unverified result.
  - The precondition error is "Refresh players; every selected player must have a verified clothing fit."
- **Apply report messages** (`blacktop.js:64-72`):
  - unconfirmed: "Outfit request sent, but the update was not confirmed. Keep player selection open, then reopen the overlay."
  - `unsupported-piece-count`: "Outfit changed at player selection, but match carry-over needs 3–7 verified clothing pieces."
  - not queued: "Outfit changed at player selection, but match carry-over was not verified. Reopen the overlay before starting the match."
- **Status badge** (`blacktop-live-bar.js:4-10`) is derived from `#blacktopStatus` text:
  - `/error|failed|unavailable|could not|reopen|needs reload/i` → "Check status"
  - `/reading|loading|scanning|applying|randomizing|saving/i` → "Loading"
  - otherwise "Ready"
  - The tooltip is the full message ("Blacktop overlay ready" when empty).
- **Carry status line** (`blacktop.js:80-85`) shows `blacktop:carry-status` messages. Warnings and errors also replace the status. Messages (`blacktop-session.js:5,13`):
  - "Saved outfits apply once per Blacktop match."
  - "Waiting for your saved outfits and Blacktop match…"
  - "Loading your saved outfits…"
  - "Outfit data applied. Clothing may take a moment to appear."
  - "Outfit session ended."
- Arena (dropdown, compact, 7-row resize, search + "★ Favorites", `arena-picker.js:1-31`), Scoreboard and Ball (expanded catalogs) and Game rules (expanded; requires an active launcher session, `main.js:290-291`) live in the same bar. See their own reference files.

#### 2. Timing and gating
- `scan` and `apply` require the selection screen: `Q(root)==image+BlacktopVtable && I(root+0xEA8)==3` (`RS\BlacktopTool.cs:207`).
- Every apply needs the current token from the last scan. Otherwise: "Refresh Blacktop players before applying." (`blacktop-manager.js:255`).
- One operation at a time: "An outfit operation is already running." (`:165-169`).
- Helper calls time out after **15 000 ms** (`:174`).
- Native apply sets the refresh flag, then waits **30 × 100 ms** for the game to consume it. It aborts if the player or controller changes (`RS\BlacktopTool.cs:473-480`).

#### 3. Mechanism (write path, brief; details in outfits.md)
- The journal of original pointers lives at `<userData>\blacktop\original-outfits.json`. It is keyed `"<pid>:<started>:<description>:<roster>"` and fsynced **before** any memory write (`blacktop-manager.js:183,297-301`).
- The request JSON is written to `original-outfits.json.<uuid>.request.json` and handed to the helper (`:302-308`).
- Native write order (`RS\BlacktopTool.cs:459-473`):
  1. optional uniform context at `desc+0xF0`
  2. source pointer and count (12 bytes) at `desc+0xC8`
  3. clear uniform if needed
  4. refresh flag byte `1` at `controller+0x30+slot·0x7B0`
- Composed outfits allocate a private read-only 4 KiB list. A session has at most 256 of these (`blacktop-manager.js:288-295`).
- Each verified apply records the match choice (§6).
- `blacktop-preset-layouts.json` is **not** UI layout. It is the native outfit-record layout table:
  - 1055 entries `{entry, length, count, descriptor, hash, outfitCount, records[{record, marker, array, strings[], names[]}]}`, for example `clothing_defaultoutfits.CLOF`.
  - It is passed as the helper's third argument (`blacktop-manager.js:59-63,173`, `RS\BlacktopTool.cs:432`).
  - The packaged copy is `runtime-resources\roster-research\blacktop-preset-layouts.json`, byte-identical to `src\data\`. The streetwear catalog sits beside it (`streetwear-components.bin`, preferred when newer) (`RS\BlacktopTool.cs:433-435`).

#### 4. Build profiles
- Helper builds: original `6A974D13/368C3000` and updated (Sep-11) `6AA2013D/36558000`. Anything else is refused with "Unsupported game build; outfit controls disabled." (`RS\BlacktopTool.cs:203-205`).
- Per-build RVAs (`:175-189`):

| | original | updated |
|---|---|---|
| BlacktopRoot | `0xAEB29D0` | `0xAEB69D0` |
| BlacktopVtable | `0x78AABA8` | `0x78AEA48` |
| PreviewModelVtable | `0x6775FA8` | `0x6778FC8` |
| PreviewDescVtable | `0x682A858` | `0x682D858` |
| Lineup | `0x9BBB0A0` | `0x9BBF0A0` |
| Refresh | `0x3ACA20` | `0x3ACB20` (20-byte signature check `:206`) |

- V2 equivalents: see `outfits-v2-research-20261017.md` and SN `qa048-outfits-v2-random.md` (the V2 list has no team id, so Rust omits "Random team clothes").

#### 5. Rust must match
- [ ] With no players or an unreadable lineup, the bar shows "Select players in game" / "Select player", and Edit / Random / Random all are disabled.
- [ ] The player dropdown lists exactly the scanned players in slot order, labeled by name (fallback "Player N"), as a 2-column grid under the bar. Picking one closes it and binds Edit / Random to that player.
- [ ] Random player offers the four modes listed above and applies only to the selected player. The strip stays open for repeat rolls.
- [ ] Random all applies sequentially, shows "i/N" progress, and stops on a lineup change or the first unconfirmed result. Its precondition is that every player has a verified male/female clothing fit.
- [ ] Every overlay show rescans the players and starts collapsed. There is no background refresh while it is open.
- [ ] The status badge reflects the status text with the regexes above.

---

### 5. Auto-open, auto-hide, readiness gating (detect poll)

#### 1. User-visible behavior
- With "Open overlay automatically" ON: the compact bar pops up once the full lineup is chosen at Blacktop player selection and the game is in front.
- It disappears when you leave the selection screen (match loading, menus) or when the lineup becomes incomplete again.
- If you hide it while the lineup is still complete, it stays hidden for that visit.
- With the setting OFF, nothing auto-opens or auto-hides. F8 still works, and the hide-on-focus-loss still applies.

#### 2. Timing and gating (`blacktop-auto-open.js`)
- **Poll loop** (`:42-69`):
  - Started once at launcher start (`main.js:898`). Never overlapping; the next tick is scheduled in `finally`.
  - Delay is **2000 ms**, or **100 ms** when the last read had `selection===true`.
  - Each tick re-reads the settings. The condition `autoOpenBlacktop || panel.wanted || prepareLineup` is always true, because `prepareLineup` is always supplied (`main.js:897`). So it **always polls**.
- **Detect output** (`RS\BlacktopTool.cs:392-419`) is JSON:
  - `selection`, `lineupReady`, `requiredPlayers`, `selectedPlayers`, `playersReady`
  - `lineupPlayers[{slot, description, roster}]`
  - `session:"<pid>:<startTicks>"`, `foreground`
  - With no game: `{selection:false, session:"", foreground:false}`.
  - `selection` = `Q(root)==image+BlacktopVtable && I(root+0xEA8)==3`.
  - `required = I(lineup+4)`, `selected = I(lineup+0xB8)`.
  - Validity requires `required ∈ {2,4,..,10}`, `I(lineup+8)==required`, `0≤selected≤required`, `controller=Q(root+0x1080)≠0` and `byte(controller+0x18)==1`.
  - A loaded player = slot entry `controller+0x20+slot·0x7B0` with model/description vtables matching the preview vtables. `roster = Q(desc+0xB8)`.
  - `playersReady` = loaded==selected, rechecked after the reads. `lineupReady` = playersReady && selected==required.
  - Detect errors (unsupported build, two game processes, and so on) throw. The tick logs the error and does **nothing else**; it never re-arms a dismissed panel (`:65`).
- **observe()** (only when `autoOpenBlacktop===true`, `:60`):
  - A new `session` string resets `entered`, `inside` and `outside`.
  - `selection=false`: `inside=0`. If `++outside ≥ 2`, then `entered=false` and hide the panel if wanted.
  - `selection=true`: `outside=0`. If the panel is already wanted, `entered=true` (a manual open counts).
  - `lineupReady≠true`: `inside=0`. Re-arm only on an explicit incomplete lineup (valid even `required` 2..10 and `selected<required`): then `entered=false` and hide. Loading or unknown reads do not re-arm.
  - `lineupReady===true`: `inside++`. Show when `inside ≥ 2 && !entered && foreground===true`, then set `entered=true` and call `panel.show()`.
- Per-tick order (`:52-63`):
  1. Ball observation (fire-and-forget).
  2. `setGameForeground`.
  3. Auto-random (awaited).
  4. `observe`.

#### 3. Mechanism
- `detect` opens the process read-only (`new Reader(game.Id,false)`) and calls `CheckGame(..., requireSelection:false)` (`RS\BlacktopTool.cs:396-397`).
- `CheckGame` verifies the exe path matches the game root, refuses any process or module containing "easyanticheat", and checks the PE header and build (`:197-207`).

#### 4. Build profiles
Same two builds as §4. On V2 the WPF detect throws ("Unsupported game build…"), so WPF never auto-opens on V2.

#### 5. Rust must match
- [ ] Auto-open fires only after 2 consecutive lineup-complete reads (100 ms apart) at selection with the game foreground, and only once per visit.
- [ ] Hiding it during a complete lineup is respected until you leave selection (2 reads) or the lineup becomes incomplete.
- [ ] Leaving selection for 2 reads hides the overlay (setting ON only).
- [ ] Deselecting a player (selected<required) hides it and re-arms.
- [ ] Transient or unknown reads (loading models, read errors) neither open nor re-arm.
- [ ] A new game process (session string) resets the visit state.
- [ ] Polling is 100 ms at selection and 2000 ms elsewhere, never overlapping.

---

### 6. Auto-random and re-apply across matches (match carry-over)

#### 1. User-visible behavior
- At player selection, each player gets a random full outfit (streetwear+team pool, roster shoes) automatically as soon as they are loaded. This needs no overlay and has no setting. Players already edited manually keep their outfit.
- The outfits chosen at selection (manual or automatic) are rebuilt in the loaded match by the in-game worker. The status reads "Outfit data applied. Clothing may take a moment to appear."
- This repeats for every later match in the same game.

#### 2. Timing and gating
- **Auto-random trigger** (`blacktop-auto-open.js:54-57`):
  - `signature = JSON.stringify([session, lineupPlayers])` when `selection && playersReady && lineupPlayers[]`.
  - `prepareLineup` (= `autoRandomSelection`) runs only when the signature **equals the previous tick's**, that is 2 identical consecutive reads (~100 ms apart). It is awaited inside the tick.
- **autoRandomSelection** (`blacktop-manager.js:211-253`):
  - Validates `≤10` entries, slots 0..9, positive description and roster, and unique slots.
  - A new `session` clears the seen set, retry state and status throttles.
  - Identity is `"slot:description:roster"`. Identities no longer present are removed from the seen set.
  - Skips if busy, or if all identities are seen.
  - Throttles the same lineup signature to one attempt per **1000 ms**.
  - Exclusive: it scans without the catalog, and requires the scan to match the observation (same `pid:started`, same count, same identities). Otherwise it emits `scan-mismatch` and stops.
  - Per unseen entry:
    - Skip if the player has an error or no male/female pool.
    - **If a journal entry exists for this description/roster, mark it seen and keep it** (`manual-preserved`).
    - Otherwise mark it seen **before** applying, then `applyUnlocked({randomFit:true, randomMode:'all', omitCatalog:true, rosterShoes:true})`.
    - Break on an unverified report. A throw propagates; the slot stays seen, so there is no retry.
  - Diagnostics go to `selectionStatusLog` (`<logs>\blacktop-selections-<version>-<pid>.jsonl`), throttled to 1 per key per 10 s (`:159-164`).
  - Inferred: preview description objects are recreated on every return to selection (ML `BLACKTOP-V51.md` "Returning to the selection screen creates new objects"). So identities and journal keys change, and players are re-randomized on each new selection visit. **UNVERIFIED live.**
- **Match-choice record** (`blacktop-match-choices.js:8-33`):
  - Written after every exactly verified apply (`blacktop-manager.js:278,323-325`) to `<userData>\blacktop\original-outfits.json.match-choices.json`.
  - Schema `blacktop-match-choices-v1`, `{pid, started, players:[{choiceId:uuid, roster, slot, name, items[1..7], status:'preview-verified-not-applied-to-match'}]}`.
  - A choice displaces any prior entry with the same roster **or** the same slot. At most 10 entries. Written atomically (temp file + fsync + rename).
  - `matchCarryover` is `'queued'` only for 3–7 pieces.
- **Carry watcher lifecycle:**
  - Started when the loader reports `loader-active` (`main.js:2069`). Stopped on `complete`/`stopped` (`:2070`) and in cleanup.
  - Runs `powershell.exe -NoProfile -ExecutionPolicy Bypass -File session_saved_outfits.ps1 -GameRoot -ChoicesPath -DiagnosticsDir <logs>\game-session` (`blacktop-session.js:10`).
  - Protocol is stdout lines `OUTFITS:waiting|applying|applied|stopped|warning:…|error:…` (`:15-16`).
- **Watcher loop**, per match (`session_saved_outfits.ps1:85-158`):
  1. `OUTFITS:waiting`.
  2. Poll every **250 ms**. Read the choices; they must match the game pid and start ticks.
     - Keep fresh (unconsumed `choiceId`) players that are `InLineup` (exactly one match in the `Lineup+0xC0` array) with 3–7 items.
     - For each, `Ready()` must hold: game mode `I(Q(Settings)+0xA04)==6`; exactly one match-table row (30 × 0x50 at `MatchTable`, vtable `RecordClass`) whose `roster+0x500==origin`; manager idle (`m+0xA0==0`, `byte m+0xB8==0`); the player/description vtables match; the description has **exactly 2** names.
     - Need **3 identical snapshots** (≥0.5 s).
  3. Wait ≤**10 s** for `<prefix>.log` to contain `STATE=ARMED` and `PID=<pid>`.
  4. Write the BOP3 plan (`write_saved_outfit_plan.ps1`).
  5. Mark the choice IDs consumed and persist `…match-choices.json.consumed.json`.
  6. Create the `<prefix>.trigger` file containing `SAVED_OUTFITS_V1`.
  7. `OUTFITS:applying`. Wait ≤**650 s** (1 s polls) for `STATE=REBUILT` (`applied`). On `FAILED`/`INCONCLUSIVE`, emit an error.
  8. Re-arm only after **4 consecutive selection samples 500 ms apart** (`Selection()` = BtRoot vtable, stage 3, game mode 6).
  9. `matchNumber++`. The prefix becomes `nba2k27_saved_outfits_<pid>_match<N>`.
- **In-DLL worker** (`VP\character_description_clone_sep11.hpp:371-443`):
  - Logs `ARMED`. Waits for the trigger (every 500 ms; unbounded under `SESSION_OUTFITS`).
  - Deletes the trigger. Parses the plan (pid and process creation FILETIME must match).
  - Requires every roster to resolve uniquely in the match table before any rebuild.
  - Rebuilds players sequentially (`RUNNING` → `REBUILT`). The first failure is `INCONCLUSIVE` with no retry.
  - Then waits for 4 selection samples and moves to the next match.

#### 3. Mechanism
- Shipped proxy: `session-loader\version.dll`, SHA `ECEF8C6C…`, built with `-DNBA2K27_SESSION_OUTFITS -DNBA2K27_SCOREBOARD_TEXT_TEST` (`nba2k27-launcher-tauri2\artifacts\wpf-version-dll-audit-20261017.md` §0, §1a).
- `selector_override.hpp` is **not** Blacktop-outfit related. It redirects `char/signatureconfigs_all.iff`, `char/signatureconfigs_editplayer.iff` and `char/fpplayer_global.iff` to the prepared provider copies when the loader config has `SELECTOR_OVERRIDE=1` (`VP\selector_override.hpp:4-10`, `VP\version_proxy.cpp:419-422,738`).
  - That flag is set from `multiConfigManager.prepare` ("preparing player selectors", `main.js:1728-1739,1793`; `core\version-proxy-session.js:247-252,286`).

#### 4. Build profiles
- The PowerShell watcher binds original or updated build RVAs (`session_saved_outfits.ps1:24-39`).
- The in-DLL re-arm loop hard-codes **updated-build** addresses (`0xAEB69D0`, `0x78AEA48`, `0xAACB9A0`) (`VP\character_description_clone_sep11.hpp:433-436`).
- So multi-match carry was Sep-11 only. V2 is a Rust-only DLL (`version-outfits-v2.dll`; SN `qa048-outfits-v2-random.md`).

#### 5. Rust must match
- [ ] Auto-random runs with the overlay closed and with "Open overlay automatically" off. It applies to each newly loaded selected player after 2 identical lineup reads.
- [ ] A player with a manual outfit this session is never auto-randomized over it.
- [ ] An auto-random attempt for a slot identity is made at most once per game session, even if it failed or was unverified.
- [ ] A lineup change mid-batch stops the batch. The same lineup is retried at most once per second.
- [ ] Only verified 3–7-piece selection outfits are queued for the match. Each queued choice is consumed once and never replayed in a later match. The next match re-arms only after the selection screen is stable for 4×500 ms.
- [ ] The carry status line shows the WPF messages listed in §4.

---

## Known divergences in current Rust port (brief; checked `nba2k27-launcher-tauri2\src\native_overlay.rs` and SN notes)
| Area | WPF | Rust now |
|---|---|---|
| F8 source | Polling watcher, always active, any game state (`RS\F8Watcher.cs`) | `RegisterHotKey(MOD_NOREPEAT, VK_F8)`, only while the game is foreground at selection or match (`native_overlay.rs:196-205`; SN `f8-overlay-visual-parity-pass2.md` table) |
| Visibility policy | Shows on lineupReady×2 at selection; hides after 2 reads off selection; F8 works anywhere | `Policy::update` allows only boundaries `selection`/`match`. It auto-shows on entering selection (not lineup-complete) and auto-hides at match only with `auto_hide` (`native_overlay.rs:155-180`) |
| Foreground test | Game PID only, plus the 900 ms post-collapse hand-off | Game PID or the overlay/owned popups/WebView2 children, with 350 ms loss grace (`native_overlay.rs:879`) |
| Placement | Monitor under the cursor (`display.bounds`) | Game client rect, 12 px inset (`native_overlay.rs:36-49`) |
| Edit outfit | Auto-apply after 350 ms | Apply / Restore / Save buttons (SN pass2) |
| Random modes | 4 incl. "Random team clothes" | 3; team omitted on V2 (SN `qa048-outfits-v2-random.md`) |
| Auto-random | Always on | Setting `autoRandomOutfits` (default true) (SN `qa048-outfits-v2-random.md`) |
| UI watchdog | none | 15 s heartbeat disables F8 (SN pass2) |

## Open questions / UNVERIFIED
- In the WPF host, F8 pressed while the expanded overlay is focused is probably ignored (watcher `parentPid` = node worker PID). This is inferred from code and not tested live.
- In the WPF compact (non-activating) window, keyboard input to the arena search box and Escape probably does not work until the window is focused. Not verified.
- Whether WPF re-randomizes all players on every return to selection (new description pointers mean new identities and no journal hit). This is inferred from ML `BLACKTOP-V51.md` plus `blacktop-manager.js:183,216-241`, not observed.
- After an F8 watcher crash, nothing visible in WPF restarts it (the "Open game panel" button is hidden). Users would have to restart the launcher. Not verified live.
- Exact `logsPath()` folder for the diagnostics files (`blacktop-panel-*.jsonl`, `blacktop-selections-*.jsonl`, `game-session\`) under portable storage: not traced here.
