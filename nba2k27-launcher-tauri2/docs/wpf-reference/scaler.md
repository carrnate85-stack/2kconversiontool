# Player scaler and character groups â€” WPF reference
Source build: WPF quality-pass-qa-20261010 (frozen). Last researched: 2026-10-17.

Path abbreviations used in citations (all relative to `C:\Users\carrn\CodexProjects\2kconversiontool`):
- `WR` = `nba2k27-launcher-wpf/release/quality-pass-qa-20261010/App/Reference`
- `SH` = `nba2k27-mod-launcher/scaler-helper`

The shipped helper `WR/runtime-resources/scaler/NBA2K27PlayerScaler.exe` has the same SHA-256 (`e8c42490â€¦c1205`) as `SH/NBA2K27PlayerScaler.exe`. That exe was built 6 s after `SH/PlayerScaler.cs` was last changed (2026-09-18 14:11:15 â†’ 14:11:21), so `SH/*.cs` is the source of the shipped helper.

## Summary
- **What it does:** one Scaler page with a 95â€“100% slider and four switches: players & referees (the master switch), coaches, cheerleaders, and arena personnel & crowd. The launcher saves the choice. When you press Play, it runs a short-lived helper (`NBA2K27PlayerScaler.exe`) on a timer. The helper writes game memory from outside the game process (ReadProcessMemory/WriteProcessMemory). There is no injection, no code patch and no per-frame write (`SH/PlayerScaler.cs:13-19`).
- **Players and referees** share **one global float**, the "shared scale input" at `base+InputRva`. Its stock value is `0x3F7AE148` (0.98f). The target is `0.98 Ã— percent/100` (`SH/ScalerPolicy.cs:5-11`). Players already loaded only change after you reload the mode.
- **Coaches** opt into that same float through bit `0x02` of `description+0x7D`. **Arena generic/cheerleader actors** set that bit too, and the helper writes their `actor+0xBCC` scale/inverse pair once as a starting value.
- **Where the logic lives:** `WR/src/core/scaler-manager.js` decides when the helper runs (500 ms while applying, 3000 ms while observing, 120 s apply deadline). `SH/PlayerScaler.cs` holds all memory logic and keeps a crash-recovery journal (`scaler/session.json`).
- **Most likely to break in a port:**
  1. **Two different gates.** Writes are refused unless the game mode `[settings]+0xA04` is 0, 4 or 6 ("main menu, Freestyle or Blacktop") **and** the 30-slot player table is stable. Observe runs and group sync do not need that gate. Applying in the middle of a match must wait.
  2. **Ownership and conflict rules.** The first write only happens if the float equals stock `0x3F7AE148`. Any foreign value means `conflict`, not an overwrite. Every new target is computed from the original value, never from the value already scaled.
  3. **Re-apply model.** The player float is written **once per game process**. Only coaches and arena groups are re-synced, on every 3 s observe tick. A game-side reset of the float becomes phase `reset` and is **not** re-written automatically.

## Key source map
| Concern | file:line |
|---|---|
| Page markup, labels, help text | `WR/src/renderer/index.html:432-453` |
| Nav button "Scaler" | `WR/src/renderer/index.html:51`, title `WR/src/renderer/app.js:1960` |
| Renderer behavior (draft slider, auto-save switches, retry visibility) | `WR/src/renderer/scaler.js:1-18` |
| Styling and status colors | `WR/src/renderer/scaler.css:1-28` |
| Preload API | `WR/src/preload.js:37-40` |
| IPC handlers (main window only) | `WR/src/main.js:129-132` |
| Manager constructed, prefs loaded, recover on startup | `WR/src/main.js:2019-2023` |
| Armed inside `launchGame` (before Steam starts) | `WR/src/main.js:1797-1799` |
| Stopped if the launch fails | `WR/src/main.js:1815-1817` |
| Stopped on quit / host loss | `WR/src/main.js:2331`, `WR/src/main.js:2287` |
| Preference validation, category mask | `WR/src/core/scaler-manager.js:5-10` |
| Poll loop, deadline, state machine | `WR/src/core/scaler-manager.js:42-60` |
| save / restore / retry | `WR/src/core/scaler-manager.js:61-81` |
| Helper exec (8 s timeout, args) | `WR/src/core/scaler-manager.js:16-20` |
| Build identification, RVAs, code AOB | `SH/PlayerScaler.cs:57-93` |
| Readiness and mode gate (`Loaded`) | `SH/PlayerScaler.cs:95-113` |
| Coach/arena list walk | `SH/PlayerScaler.cs:144-161` |
| Coach flag sync | `SH/PlayerScaler.cs:162-189` |
| Arena flag and cache-seed sync | `SH/PlayerScaler.cs:190-245` |
| Command dispatch apply/observe/restore/check | `SH/PlayerScaler.cs:247-304` |
| Target math, ownership, journal schema | `SH/ScalerPolicy.cs:1-40` |
| Offline policy tests | `SH/ScalerPolicyTests.cs:1-25` |

---

### Scaler page UI
#### 1. User-visible behavior
- Nav item **Scaler** (`index.html:51`). The page uses `.scaler-layout`, a two-column grid that collapses to one column under 850 px (`scaler.css:1,27`).
- **Card "Character groups"**, subtitle "Choose who uses the saved scale", badge "Session setting" (`index.html:435`). Four checkbox rows:
  - `#scalerEnabled`: **"Players & referees"** / small "Main scale switch" (`:437`)
  - `#scalerCoaches`: "Coaches" (`:438`)
  - `#scalerCheerleaders`: "Cheerleaders" (`:439`)
  - `#scalerArenaPersonnel`: **"Arena personnel & crowd"** / small "Announcers, security and individually loaded crowd actors" (`:440`)
- **Card "Character size"**, subtitle "Relative to the original model", with a large `#scalerValue` readout that starts at "100%" (`:444`). Range `#scalerPercent`: min 95, max 100, step 1, default 100 (`:445`). Labels "95% Â· Smaller" and "100% Â· Original", with tick marks 95â€¦100 (`:445-446`).
- **Command bar:** status dot plus `#scalerStatus`, which starts as "Loading saved sizeâ€¦". Buttons: **"Save size"** (primary), **"Retry apply"** (secondary, `hidden` by default), **"Reset to 100%"** (`:449-450`).
- **Help text** (`:452`): "Players and referees remain paired because they share one native scale input. Roster height, wingspan and weight stay unchanged. Individually loaded crowd actors are included with arena personnel; props and batched background crowd geometry are excluded. Loaded arena groups update automatically; players and referees already loaded require reloading the mode."
- **Interaction rules** (`scaler.js`):
  - Moving the slider only updates the readout and sets `dirty`. **Nothing is saved until you press "Save size"** (`:12,14`).
  - Toggling **"Players & referees"** saves immediately (`:12`). Toggling any group switch also saves immediately (`:13`).
  - "Reset to 100%" saves `percent:100` and keeps the current enabled and group state (`:15`).
  - "Retry apply" calls `applyScaler` (`:16`). It is visible **only** when `phase === 'timeout'` (`:10`).
  - The slider and the three group switches are disabled while the master switch is off or a call is in flight. All buttons are disabled while busy (`:9`).
  - Status updates from the main process repaint the controls only when there is no unsaved draft (`!dirty && !busy`), so they never overwrite a draft (`:10`).
  - If a save throws, the status reads "Could not save character size. Please try again." (`:11`). If the first status read fails: "Scaler is not ready yet." (`:17`).
- **Status color** (`scaler.css:20-23`): `applied`/`saved` use the court color. `conflict`/`error`/`write-failed` are red (`#ff9eaa` text, `#ff6370` dot). Everything else is muted.

#### 2. Timing and gating
- `scaler:status`, `scaler:save` and `scaler:apply` only answer the main launcher window. Any other sender gets "Scaler is available in the launcher." (`main.js:129-132`).
- Saving is serialized behind any helper call already running (`scaler-manager.js:24`).

#### 3. Mechanism
- Preferences live in `<userData>/scaler/preferences.json`. They are written to `preferences.json.tmp` and then renamed (`scaler-manager.js:61-63`).
- Schema (`:6-8`): `{enabled:boolean, percent:int 95..100, coaches?:bool, cheerleaders?:bool, arenaPersonnel?:bool}`.
  - Missing group keys default to **true**.
  - **Unknown keys are rejected** with "Choose a whole percentage from 95 to 100 and valid character groups."
  - First-run default: `{enabled:false, percent:100}` (`:13`).
- Category mask passed to the helper: coaches = 1, cheerleaders = 2, arenaPersonnel = 4 (`:10`).
- If `preferences.json` cannot be read, scaling is forced off with phase `error`: "Scaler preferences could not be read. Automatic scaling is off; save your preference to continue." (`:27`).

#### 4. Build profiles
- UI is the same for every build.

#### 5. Rust must match
- [ ] Slider moves are a local draft. Only "Save size" persists them.
- [ ] Toggling the master switch or any group switch persists immediately.
- [ ] "Reset to 100%" persists percent 100 without changing enabled or group switches.
- [ ] "Retry apply" is visible only in phase `timeout`.
- [ ] Group switches and the slider are disabled while "Players & referees" is off.
- [ ] Preferences with an unknown key, a non-integer percent, or a percent outside 95â€“100 are rejected. Missing group keys default to true.

---

### Session lifecycle (manager)
#### 1. User-visible behavior: status messages by phase
| Situation | phase | message | source |
|---|---|---|---|
| Startup, disabled | `disabled` | "Scaling is off. Arena characters use their original size." | `scaler-manager.js:14` |
| Startup, enabled | `saved` | "Saved â€” applies on your next launch." | `:26` |
| Play pressed, disabled | `disabled` | "Automatic character scaling is off." | `:43` |
| Play pressed, enabled | `waiting` | "Saved â€” waiting for game initialization." | `:45` |
| Game not started yet (helper) | `no-game` | "Saved â€” waiting for NBA 2K27." | `PlayerScaler.cs:50` |
| 120 s apply deadline passed | `timeout` | "Player data was not ready in time. Retry apply is available here." | `scaler-manager.js:50` |
| Observe errors 1â€“2 | `waiting` | "Game is closing; verifying scaler cleanup." | `:54` |
| Game ended after apply | `saved` | "Game session ended. Your size is saved for the next launch." | `:57` |
| Save while running | `waiting` | "Saved â€” waiting to apply player size." | `:70` |
| Retry while the helper is idle and no Play has armed it | `saved` | "Launch NBA 2K27 through Play to start a scaling session." | `:78` |
| Retry with no game root | `saved` | "Launch NBA 2K27 through Play first." | `:75` |
| Retry while disabled | `disabled` | "Enable automatic character scaling first." | `:74` |
| Applied, players were loaded | `reload` | "Reload the mode to update player and referee size. Loaded selected arena groups update automatically." | `PlayerScaler.cs:300` |
| Applied, nothing loaded | `applied` | "Shared size applied to the selected arena groups." | `PlayerScaler.cs:300` |
| Already applied (same session) | `applied` | "Shared size is already applied to the selected arena groups." | `PlayerScaler.cs:290` |
| Game reset the float | `reset` | "The game reset player size. Apply the saved size again before loading a mode." | `PlayerScaler.cs:285` |

#### 2. Timing and gating
- **Launcher startup** (`main.js:2022-2023`): `load()` and then `recover(gameRoot)`. Recover runs **one `observe` only** and makes no game writes ("No game writes on launcher startup", `scaler-manager.js:36`). If the journal shows an owned live session (`applied`/`reload`/`reset`), observe polling resumes. Otherwise nothing is emitted for `no-game`/`new-session`/`ended`.
- **Play** (`main.js:1797-1799`): `scalerManager.arm(settings.gameRoot)` is the **first** call inside the `launchGame` callback. It runs after profile activation, camera names, scoreboards, ball preparation and version-proxy arming, and **before** `launchWithoutEac` starts Steam. Arm order after it: freestyleUnlocks, blacktopUnlocks, debugMenus, blacktopPresentation, blacktopCrowd, balls, gameCapture, then launch (`:1799-1807`).
- **Arm** (`scaler-manager.js:42-46`):
  - If disabled, it emits `disabled` and stops.
  - Otherwise it sets `authorized=true`, `applying=true`, and `deadline = now + 120000 ms`, then ticks immediately (`schedule(0)`).
- **Poll interval** (`:47`): **500 ms while `applying`**, **3000 ms while observing**. A tick never overlaps another because of the promise queue (`:24`).
- **Helper process per tick** (`:16-19`): `execFile(helper, [action, gameRoot, <userData>/scaler/session.json, percent, categories], {windowsHide, timeout:8000, maxBuffer:16384})`. The helper prints one JSON line. The process exits non-zero only on an unexpected exception (`PlayerScaler.cs:305`). If stdout is missing: "Scaler helper could not complete. Check that the game and launcher use the same permission level."
- **Tick state machine** (`:48-60`):
  - While applying, the action is `apply`, or `restore` if a restore is in progress. Otherwise it is `observe`.
  - **Observe errors:** an observe that returns `error` counts toward `maxObservationErrors=3`. The first two show the "Game is closingâ€¦" waiting message and retry after **1000 ms**. The third is emitted as `error` and polling stops.
  - `applied`/`reload` ends applying, and polling switches to 3 s observe.
  - `no-game`/`new-session`/`ended` while **still applying (not restoring)** keeps polling at 500 ms until the 120 s deadline. This covers Steam's slow start. Otherwise it becomes `saved` "Game session endedâ€¦" and polling stops (`authorized=false`).
  - `waiting` and `reset` keep polling. Every other phase (`unsupported`, `conflict`, `error`, `write-failed`) **stops polling**.
- **Helper readiness gates for `apply`** (`PlayerScaler.cs`):
  - Exactly one `NBA2K27` process. More than one gives an error (`:258`).
  - The image path must equal `<gameRoot>\NBA2K27.exe` (`:265`).
  - **No process whose name starts with `EasyAntiCheat`**: "Use the launcher's without-EAC session for Scaler." (`:267`).
  - Process creation time must be unchanged (`:269`).
  - PE timestamp and SizeOfImage must match a supported build, plus two 8-byte code signatures (`:71-90`).
  - The input page must be `MEM_COMMIT` + `MEM_IMAGE`, with allocation base equal to the module base and protection `PAGE_READWRITE (4)` or `PAGE_WRITECOPY (8)`. Otherwise: waiting "Waiting for the initialized player size input." (`:91-92`).
  - **Mode gate:** `[base+SettingsRva]+0xA04` âˆˆ {0, 4, 6}. Otherwise: waiting "Waiting for the main menu, Freestyle or Blacktop." (`:96-97`).
  - The player table must be stable. It is read twice and must be byte-identical, or the result is waiting "The game is changing modes; waiting." (`:98-111`).
  - On first apply in a new session: if the float is 0, waiting "Waiting for the original player size input to initialize." If it is not stock, **conflict** "Player size was changed outside Scaler. No memory changed." (`:277-278`).
  - Just before the write, all checks run again: `Build()`, `Loaded()`, game alive, creation time, and the float still equal to the before-value. Otherwise: conflict "The game or player size changed during preparation. No memory changed." (`:292-294`).
- **Concurrency:** a named mutex `Local\2kLauncherScaler-<SHA256(UPPER(journalPath))>` with `WaitOne(0)`. If it is busy: waiting "A player size operation is already running." (`:252-254`).
- **Re-apply across matches:**
  - The **player/referee float is not re-written** after it is applied. It stays in memory for the life of the process.
  - Each 3 s observe calls `SyncCharacters`, which opts in **newly loaded coaches** and **newly loaded arena actors**, and restores any that should no longer be scaled (`:286`).
  - If the float no longer equals `requested`, observe returns `reset` with **no write** (`:285`). The user must press "Save size" to apply again. The Retry button is hidden in `reset`.
  - Observe also clears `reloadRequired` once nothing is loaded (main menu), and the phase then becomes `applied` (`:283-284`).
- **Save while a session is running** (`scaler-manager.js:65-71`):
  - **enabled â†’ false:** stops polling and runs `restore` immediately. If `waiting`, it becomes a restore loop (500 ms, 120 s deadline). `no-game`/`ended` shows `disabled` "Automatic character scaling is off."
  - **enabled true and running:** `applying=true`, a new 120 s deadline, and a tick right away.
  - **Not running:** `saved` "Saved â€” applies on your next launch."
- **Retry apply** (`:73-81`) first runs `observe`. If there is no game or a new session and Play has not authorized one, it refuses. If observe returns `unsupported`/`conflict`/`error`/`write-failed`, that result is shown and nothing else happens. Otherwise it starts the 120 s apply loop.
- **Game exit:** no memory restore is attempted because the process is gone. The helper marks the journal `phase:"ended"` (`PlayerScaler.cs:50`), and the manager shows "Game session ended. Your size is saved for the next launch."
- **Launcher exit / host loss:** `scalerManager.stop()` only clears the timer (`main.js:2331`, `:2287`). Game memory is not restored. The quit guard already refuses to close while NBA 2K27 is running ("Close NBA 2K27 first.", `main.js:2311-2317`).
- **Failed launch:** `scalerManager.stop()` in the `launchGame` catch block (`main.js:1817`).

#### 3. Mechanism
- **Helper CLI** (`PlayerScaler.cs:247-251`): `NBA2K27PlayerScaler.exe <apply|observe|restore|check> <gameRoot> <journalPath> <percent> <categories 0..7>`. `restore` forces categories=0 and target=100%.
- **Process access** (`:268`): `OpenProcess(0x438)` (QUERY_INFORMATION | VM_OPERATION | VM_READ | VM_WRITE) for writes, `0x410` for `check`.
- **Supported builds** (`:72-89`). The PE timestamp is at `pe+8` and SizeOfImage at `pe+80`.

| Field | Original `6A974D13:368C3000` | Updated "Sep 11" `6AA2013D:36558000` |
|---|---|---|
| InputRva (shared float) | `0xA38F19C` | `0xA39319C` |
| SettingsRva (mode at `[ptr]+0xA04`) | `0xAAC79A0` | `0xAACB9A0` |
| PlayerTableRva (30 Ã— 0x50) | `0xAAC79B0` | `0xAACB9B0` |
| Wrapper vtable | `0x75BD1F8` | `0x75C0D58` |
| Manager vtable | `0x681D088` | `0x6820088` |
| Character (player) vtable | `0x6829528` | `0x682C528` |
| Staff/character list sentinel | `0xA37BFB0` | `0xA37FFB0` |
| Manager-node vtable | `0x681CCC8` | `0x681FCC8` |
| Coach vtable / coach description vtable | `0x6804E58` / `0x6804818` | `0x6807E58` / `0x6807818` |
| Arena population (generic/cheerleader) | **not supported** (`PopulationSupported=false`) | Generic `0x68180D8`/desc `0x6817E08`; Cheerleader `0x6801DE8`/desc `0x6855FD8`; scale method `0x6DE310` |
| Code AOB #1 | `+0x6DE34A` = `C5FA10054A0ECB09` | `+0x6DE44A` = `C5FA10054A4DCB09` |
| Code AOB #2 | `+0x6DE66E` = `C5FA1005260BCB09` | `+0x6DE76E` = `C5FA1005264ACB09` |

- If the build does not match: phase `unsupported` "This game update is not supported by Scaler. No memory changed." If an AOB does not match: "Player size code differs from the supported build. No memory changed." (`:72-90`).
- **Player table validation** (`Loaded`, `:95-113`). For each of 30 entries at `base+PlayerTableRva + i*0x50`:
  - Fields: `vt=+0x00`, `manager=+0x10`, `roster=+0x30`. An entry that is all zero is skipped.
  - `vt` must equal the wrapper vtable. `manager` must have the manager vtable. `character = [manager+0x98]` must have the character vtable, and `[roster+0x518] == manager`.
  - Scale pair at `character+0xBCC` (float scale, float inverse): 0.1 < s < 10, and |sÂ·inv âˆ’ 1| â‰¤ 0.001.
  - `loaded` is true if any player passes. The function returns `loaded || mode != 0`.
- **Player/referee write** (`:291-298`):
  - The journal is saved first with `applied=before, requested=target, phase="prepared", reloadRequired=loaded`.
  - Then a single **4-byte** `WriteProcessMemory` of the target bits to `basis+InputRva`, then a readback. A mismatch gives `write-failed` "Player size readback failed. Recovery data was retained."
  - The journal is then updated with `applied=target` and `phase` = `applied`, or `restored` at 100%.
  - Referees: the GENERIC referee characters already opt into the shared input through `description+0x7D` mask 2. **A second percentage must not be applied to them** (`:17-18`).
- **Target values** (`ScalerPolicy.cs:5-11`, computed in double, stored as float): 95 â†’ `0x3F6E5604` (0.931), 96 â†’ `0x3F70D845`, 97 â†’ `0x3F735A86`, 98 â†’ `0x3F75DCC7`, 99 â†’ `0x3F785F07`, 100 â†’ `0x3F7AE148` (0.98, stock). The 95 and 100 values are asserted in `ScalerPolicyTests.cs:6`. The others were computed with the same formula.
- **Ownership** (`ScalerPolicy.cs:12`): the current value counts as owned only if it equals Original, `applied`, or `requested`.
- **Coach/arena list walk** (`Characters`, `:144-161`):
  - A doubly linked list starts at `sentinel = base+StaffListRva`, with first node `[sentinel+0x10]`. It is capped at 256 nodes.
  - Each node: `[node] == ManagerNodeVtable`, `[node+8] == previous`, `manager = node+0x78`, `actor = [manager+0x98]`, next node `[node+0x10]`. At the end, `node == sentinel` and `[sentinel+8] == last`.
  - **Coach:** manager vtable and actor == CoachVtable. `description = actor+0xF20` must have CoachDescriptionVtable. `[manager+0xB8]` must be 0 (`:149-152`).
  - **Arena (updated build only):** manager vtable, `[manager+0xB8]==0`, and actor vtable Generic or Cheerleader.
    - The description is found from the interface getter: `iface = actor+0x78`, `getter = [[iface]+0x158]`. The getter code must be `48 8D 81 <disp32> C3`, and `description = iface + disp32` (`:125-129`).
    - The class is confirmed by the description vtable and by `[[iface]+0x188] == base+ScaleMethodRva` (`:130-134`).
- **Coach sync** (`:162-189`):
  - The original flag byte at `description+0x7D` is read. If bit 2 is already set, the coach is skipped (it is not ours).
  - The coach is journaled **before** writing. Then `flag | 2` is written and read back.
  - Disabling writes the original flag back only if the current value equals `original|2`. Any other value gives `conflict` "A tracked coach size flag changed outside Scaler. No memory changed."
- **Arena sync** (`:190-245`):
  - On first sight, the helper records `originalFlag`, `originalScale` and `originalInverse`. It computes `targetScale = originalScale Ã— F(sharedTarget)`, where the factor is **the shared float itself**, for example 0.931 at 95% (not 0.95). It also computes `targetInverse = 1/targetScale` and sets `cachePrepared=false`, then journals.
  - Next it sets the flag to `|2`. If the cache is not yet prepared, it writes the **8-byte** pair `(inverse<<32)|scale` to `actor+0xBCC` **once**, reads it back, and marks `cachePrepared=true`. The game maintains the value after that.
  - Restore: clears the flag. It writes the original pair back only if the exact seed is still present. If the game has already recalculated the value, clearing the flag is enough.
  - `OwnedArenaScale` accepts a ratio of 1, or a ratio equal to Target(95..99) (`:119-124`).
- **Journal** `<userData>/scaler/session.json` (`ScalerPolicy.cs:33-40`):
  - Fields: `schema=1`, `pid`, `created` (process start FILETIME UTC), `basis`, `address`, `image`, `build`, `phase`, `original`, `applied`, `requested`, `reloadRequired`.
  - `coaches[]` (max 16): `{manager, actor, description, originalFlag}`.
  - `arenaCharacters[]` (max 220): `{kind: generic|cheerleader, manager, actor, description, originalFlag, originalScale, originalInverse, targetScale, targetInverse, cachePrepared}`.
  - Validated on load (`:21-23`). The file may be at most 262144 bytes.
  - Saved atomically: `.tmp` with `FileMode.CreateNew` and `Flush(true)`, then `File.Replace` or `File.Move`. Any reparse point in the path is refused (`PlayerScaler.cs:41-48`).
  - "Same session" means the same pid, created time, basis, build, address and image, and a phase other than `ended` (`ScalerPolicy.cs:18-20`).
- **Version.dll proofs** (`nba2k27-version-proxy/src/coach_scale_flag_proof_sep11.cpp`, `coach_scale_cache_proof_sep11.cpp`, `arena_character_scale_flag_proof_sep11.cpp`, `arena_population_scale_cache_proof_sep11.cpp`, `character_scale_inventory_sep11.cpp`) are **standalone research tools**, not shipped code. They hardcode the Sep 11 build (`0x6AA2013D/0x36558000`) and use the same RVAs: list `0xA37FFB0`, manager `0x6820088`, input `0xA39319C`, generic `0x68180D8/0x6817E08`, cheerleader `0x6801DE8/0x6855FD8` (e.g. `arena_character_scale_flag_proof_sep11.cpp:37,88,96,140-143`). The production version.dll does **not** scale anything. The audit lists coach clone/rebuild/all-scale as "test DLLs only" (`nba2k27-launcher-tauri2/artifacts/wpf-version-dll-audit-20261017.md:84`).

#### 4. Build profiles
- WPF supports two builds: original `6A974D13:368C3000` (players, referees and coaches) and updated Sep 11 `6AA2013D:36558000` (adds the cheerleader and arena-personnel groups). Any other build gives `unsupported` with no writes.
- **The arena groups do nothing on the original build** (`SyncArena` returns 0 when `!PopulationSupported`, `PlayerScaler.cs:192`).
- **V2 equivalents** come from the Rust source (`nba2k27-launcher-tauri2/src/scaler_v2.rs:12-25`), **not from WPF**. They are individually proven on the V2 disk image, not uniform shifts:
  - INPUT `0x0A41D07C`, OWNER (settings) `0x0AB5CE20`, TABLE `0x0AB5CE30`
  - WRAPPER `0x0762D278`, MANAGER `0x068844D8`, PLAYER `0x06890D88`, DESCRIPTION `0x068920C8`
  - **MANAGER_ACTOR `0x110`** (WPF uses `0x98`), **ROSTER_OFFSET `0x28`**, SCALE_PAIR `0xBCC`
  - NPC list sentinel `0x0A409E90`, node actor `0x110`, OPT_IN `0x7D` (`src/scaler_npc_v2.rs:23-31`)
- Live status per the artifacts: players and referees LIVE. Groups LIVE (0.47 inventory). Group retry after a refused write is UNV (0.49). See `artifacts/feature-gap-inventory-20261017.md:81-85` and `artifacts/parity-score-20261017.md:77-81`.

#### 5. Rust must match
- [ ] Pressing Play with scaling disabled shows "Automatic character scaling is off." and starts no helper or poll.
- [ ] Pressing Play with scaling enabled arms the scaler **before** the game process starts. It polls every 500 ms, and `no-game` keeps it waiting (not failed) until 120 s.
- [ ] After 120 s without `applied`/`reload`, the phase is `timeout` with message "Player data was not ready in time. Retry apply is available here." The Retry button appears.
- [ ] No player float write happens while the game mode at `[settings]+0xA04` âˆ‰ {0,4,6}, or while the player table changes between two reads.
- [ ] The first write in a session happens only if the float equals `0x3F7AE148`. A foreign value gives `conflict` with no write. A value of 0 means waiting.
- [ ] The target is always `0.98 Ã— p/100` computed from the original value. Moving 97 â†’ 95 never multiplies an already scaled value.
- [ ] The player float is written once, with readback. It is never rewritten on later observe ticks. A changed value is reported as `reset` with no write.
- [ ] After applying, polling switches to 3 s. Each tick opts in newly loaded coaches (and, on supported builds, arena actors) for the enabled categories only.
- [ ] If players were loaded at write time, the phase is `reload` with the exact "Reload the modeâ€¦" message. It switches to `applied` once nothing is loaded.
- [ ] Coach and arena changes are journaled before the write. Restore never overwrites a flag or pair that changed outside the scaler (`conflict`).
- [ ] An arena actor's cache seed is written once (`cachePrepared`) with factor = the shared float value.
- [ ] Unticking "Players & referees" during a session restores the original float and clears group flags immediately. A `waiting` result retries every 500 ms for up to 120 s.
- [ ] Refuse writes when an `EasyAntiCheat*` process exists, when more than one NBA2K27 process exists, or when the running image is not `<gameRoot>\NBA2K27.exe`.
- [ ] Launcher startup performs **observe only**, with no writes.
- [ ] Three consecutive observe errors are needed before showing `error`. The first two show "Game is closing; verifying scaler cleanup." and retry after 1 s.
- [ ] Game exit marks the journal ended and shows "Game session ended. Your size is saved for the next launch."

## Known divergences in current Rust port (brief)
- The Rust port targets a V2 build with different offsets: actor at `manager+0x110` and roster at `+0x28` (`src/scaler_v2.rs:12-25`).
- Per artifacts, a refused group write in 0.48 held the **whole** feature. 0.49 switched to per-character retry, which is not live-verified (`artifacts/parity-score-20261017.md:79`). WPF has no "hold". Each tick is a fresh helper run, and a `conflict` stops polling.
- The 0.48 audit made the slider a draft until "Save size", which matches WPF (`artifacts/camera-scaler-launch-audit-20261017.md:13-15`).
- An out-of-range saved preference is shown as repairable in Rust. WPF instead forces scaling off with phase `error` (`scaler-manager.js:27`).

## Open questions / UNVERIFIED
- **UNVERIFIED:** whether WPF's `reset` phase ever occurred live (the game re-initializing the float on mode change). The code path exists, but no live log was found.
- **UNVERIFIED:** the in-game meaning of mode values 0/4/6. The labels come only from the helper's message string "main menu, Freestyle or Blacktop".
- Courtside celebrity scaling: not found in WPF sources as a separate group. It presumably falls under GENERIC "arena personnel".
- The help text says "Individually loaded crowd actors" are included. WPF only distinguishes GENERIC from CHEERLEADER vtables. No separate crowd class was found.


## 2026-10-18 QA078 separate initial-timing fix

The user reports sizing now appears later, as players load. Source comparison found no first-apply loaded-player requirement: `src/scaler_v2.rs::snapshot` permits empty wrappers and existing tests prove a zero-loaded-player shared-input plan. WPF's scaler manager runs independently before camera helpers; the Rust feature worker had serialized camera publication ahead of the first scaler write. The saved QA077 journal confirms that ordering, but contains no timestamps and cannot prove the duration or the cause of the user's visible delay.

QA078 changes only initial scheduling: the unapplied, non-reset, non-held scaler receives the first lane before camera setup; pending camera cleanup keeps first priority. Each lane runs once. Later scaler observation/group/reset behavior and all scaler snapshot/profile/identity/durable-intent/protection/original-byte guards remain unchanged. The500ms provider cadence and ARMED admission are unchanged. See `src/feature_runtime.rs::camera_scaler_order` and `Runner::tick`.

The retained WPF `Reference/src/core/scaler-manager.js` exactly matches the cited legacy source hash73F7CD1702242343DC2F77AA951CC36C16E600195460496EDC650FDEC3FDBBCE. Detailed source citations and the qualification about populated-but-initializing wrappers are in `artifacts/qa078-scaler-first-apply-timing-source-review-20261018.md`; the exact scheduling source audit is `artifacts/qa078-scaler-initial-priority-source-audit-20261018.json`. Visual timing remains a live check; do not promise immediate model changes from this ordering fix alone.


## 2026-10-18 QA079 first-attempt timing and retained observations

The user confirmed QA078 WNBA categories in both modes, then clarified that basketball players and referees visibly change size after appearing. This is not a status-only report or a coach/arena-character complaint. The QA078 initial scaler priority alone has not resolved the visible delay.

QA079 removes the incidental 500 ms wait before the first existing feature tick for scaler-enabled Play. That opportunity is immediately after the owned loader reaches ARMED and the feature Runner is created, with fresh authorized/live identity checks. The first opportunity is consumed once; subsequent 500 ms cadence, ARMED admission, and all scaler input, profile, object, description, opt-in, table, ownership, journal and write checks are unchanged. The shared Runner means other enabled lanes can also run on that first opportunity; no feature gets a separate admission path. Scaler-disabled Play keeps its prior scheduling. The tick precedes the loader-ready early break.

The existing provider-state feature status now retains bounded informational `scalerTiming` facts through session end: first attempt, first static-allowlisted readiness refusal, plan ready, completed global apply, first observed loaded-player cache, completed existing NPC work and reset observation. Plan/apply facts include the already captured mode and loaded-player count. Unknown errors are reduced to an unclassified refusal; there are no addresses, IDs, keys, extra memory reads, UI popups or recurring elapsed-time churn. These facts never grant permission, retry a write or reopen a hold. NPC scheduling and all player/referee write behavior are otherwise unchanged.

This removes a known startup wait, but does not establish that visible resizing is fixed. The next normal dev Play should distinguish a late first attempt from an early attempt waiting for a specific readiness condition, and whether the shared value was written before any players were loaded. See `artifacts/qa079-scaler-first-due-diagnostics-source-audit-20261018.json`, `artifacts/qa079-scaler-independent-review.json`, and the QA079 handoff. No WNBA source, native DLL, outfit carry, Random All or UI change is included.


## 2026-10-18 QA080 PLAYER/NPC boundary correction

The user still sees basketball players/referees visibly resize on load in QA079. Its retained facts show a global write at13.630s with mode0 and zero verified wrapper players; the first fully verified26-player observation was46.510s. This does not establish actor birth or first rendered-frame timing. The completed session journal was removed by normal cleanup before capture, so there is no retained effect history for that run.

WPF's NPC path only considers coaches, GENERIC arena characters and cheerleaders. Rust additionally inferred a courtside celebrity from a PLAYER-class actor absent from the current30-wrapper table. That absence is insufficient to identify a celebrity: it can include a loading player. A player can also bind into the wrapper table between scan and the NPC writer, whose identity recheck did not repeat roster exclusion. The standard native V2 factory sets its shared-scale bit before actor construction, but this does not prove every alternate actor-loading path is protected.

QA080 therefore withholds PLAYER-class NPC adoption at identity admission and pure planning, including forged/retained celebrity receipts, until positive celebrity identity is proven. The four-class disk pins remain as evidence. The ordinary shared player/referee input and Coach/GENERIC/Cheerleader behavior remain unchanged. This temporarily withholds the additional courtside PLAYER feature, which was unverified, to match WPF's supported NPC boundary; it does not prove the visible player or referee issue fixed.

Existing provider status also retains fixed, informational NPC writer counters: total, four class groups and an unclassified bucket. Entry means a guarded writer was called, not that it wrote. Successful one-byte/eight-byte returns are reported separately from uncertain returns; none imply the later Applied transaction event committed. Counters and monotonic first/last entry times survive ended status, contain no IDs/addresses, and never grant permission or cause game reads/writes. Their purpose is to distinguish NPC writer involvement during the next normal Play, including GENERIC characters which cannot yet be positively separated as referees versus arena personnel. Only a fresh quiescent or normally closed snapshot with zero total entries excludes this NPC writer lane. Counters publish after the batch and are not durable pre-effect receipts; a stale or crashed-session zero cannot exclude effects. Nonzero entries alone do not prove an effect.

The next normal dev test remains necessary. WNBA, native DLL, player/referee shared-input guards and outfit carry are unchanged. See the QA080 evidence/review artifacts and final handoff.
