# Codex handoff: 2K Launcher (Rust/Tauri port of the WPF launcher)

Date: 2026-10-18. Written by Claude at the end of a long porting session (builds 0.44 to 0.53.1).
This supersedes `CLAUDE-HANDOFF.txt` (keep it; it holds per-build history and the original paths).

---

## 1. What this project is

- **App:** `C:\Users\carrn\CodexProjects\2kconversiontool\nba2k27-launcher-tauri2`, built with Rust + Tauri 2 + React/TypeScript on the installed WebView2.
- **Goal:** port every feature of the retained WPF launcher (0.1.0-wpf.24) into this app. It should have the same features, behavior and look. Code-level parity isn't required.
- **WPF is the spec for every feature.** Before you brief or code anything, read how WPF did it (UI, timing, helpers, version.dll). Features drifted away from WPF several times, and the user had to point it out.
  - WPF behavior notes: `docs\wpf-reference\` (`index.md`, `HOW-TO-USE.md`, plus one file per area).
  - WPF frozen source: `nba2k27-launcher-wpf\release\quality-pass-qa-20261010\App\Reference\src` (`main.js`, `renderer\`) plus `App\Backend\host.cjs`.
  - WPF version.dll source: `nba2k27-version-proxy\src`. It has no V2 profile, so use it as a logic template.
- **Game:** NBA 2K27 Version 2. SHA256 `29A96471…E576A1`, PE timestamp `0x6AC53765`, SizeOfImage `0x35BDE000`, path `C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27`.

## 2. Hard rules (user and safety)

- **Live game access is read-only** (`PROCESS_QUERY_INFORMATION | PROCESS_VM_READ`). The only live writes are the launcher's guarded native transactions (exact identity, journal intent, before/after compare, readback, restore).
- **Ask before** installing over Main or adding any new kind of live-game write.
- **Never delete old builds or backups.** Preserve `Data` and `Library`, and keep settings and diagnostics in the portable folder.
- **The project folder is untracked in git.** Back up every file you edit to `Backups\qaNNN-<topic>-<date>\` first.
- **Never print or ship the captured roster encryption key.** It stays local in `artifacts\roster-key-offline\`. Note that `Builds\0.52.0\Portable\Data\wnba-roster-v2\roster-key.bin` exists at runtime; make sure it never goes into a shared build.
- **Run only one launcher at a time.** The outfit worker DLL refuses to arm while another launcher-named process exists. A stray dev launcher once silently broke outfit carry.
- **Don't run a launcher EXE while the user is live-testing.** Use Vite and static checks instead.
- **Remote Control stays off** for all sessions; the user believes it crashes the app.
- **WNBA:** see §8. WPF's Freestyle unlock is a data route (category tables), but on V2 it also needs a roster with WNBA rows, which is still unsolved. The Claude sessions' executable-stub loader route was stopped by a safety classifier.

## 3. User preferences

- **Priorities, highest first:** Blacktop, overlay, outfits, cameras, scaler, Jukebox, WNBA. The roster/team editor is the LOWEST.
- **UI:** reuse WPF's HTML/CSS. The user approved it as "close enough to build off of", and every page is now within 0.1–5.9% pixel mismatch of WPF.
- **Automatic random outfits** are on by default, as in WPF, and players dressed by hand are skipped. **Random all** randomizes every player at once. **Random player** opens a small menu asking which player.
- **Overlay behavior:**
  - It hides at match start and when leaving Blacktop, and reappears after Alt-Tab.
  - Hide stays in effect until F8 or the next selection.
  - There are no per-item Enable checkboxes; WPF had none.
  - The F8 arena picker uses the ball/scoreboard panel style. This is an intentional divergence from WPF that the user asked for.
  - The PLAYER dropdown is a small two-per-row menu, like WPF's `blacktop-player-menu.js`.
- **After each test build:** open the build's `App` folder in Explorer, give the full test list (in game and out of game), and quote the weighted parity %.
- **Manage side sessions automatically:** start the ones you need and archive finished ones.

## 4. Dev workflow (new as of 0.53.1; use it)

| You change | How it rebuilds |
|---|---|
| CSS, layout, TSX components, UI interactions | Vite hot reload inside the running dev app, in seconds |
| Rust commands/backend | Incremental debug build: rerun `Dev.ps1` |
| `Cargo.toml` dependencies | Bigger recompile |
| `tauri.conf.json` | Restart the dev app |
| **FINAL build for sharing** | `Build.ps1 -Verify`, a separate standalone EXE in `Builds\<version>\Portable\App\` |

- **`Dev.ps1`** (project root) does four things:
  1. Starts Vite on `http://localhost:5173` once, then reuses it.
  2. Runs `cargo build --no-default-features`, a debug build without `custom-protocol`, so the app loads its UI from `devUrl`.
  3. Copies the EXE to `Dev\Portable\App\2k Launcher Rust Prototype.exe`.
  4. Launches it. Use `-NoRun` to compile only.
- **The dev app has its own data.** It uses `Dev\Portable` for `Data` and `Logs`, so it never mixes with a release build.
- **Dev config:** `tauri.conf.json` has `build.devUrl` and `app.security.devCsp`, which allows `localhost:5173` and `ws:` for HMR. Release builds still embed `dist` and use the strict CSP.
- **Dev build status (2026-10-18):**
  - `Dev.ps1 -NoRun` compiles; the first debug build was slow, and incremental rebuilds now take about 1.5 s.
  - `Dev\Portable\App\2k Launcher Rust Prototype.exe` exists, and Vite answered on port 5173.
  - **Not yet verified:** the dev app was never launched (the user was live in game with 0.53.1). So hot reload inside the app and the `devCsp` are unproven. First thing to do: launch it with no other launcher open, edit a CSS value, and confirm it updates live.
  - `Dev.ps1` caches the process handle before `WaitForExit`, because without it PowerShell's `ExitCode` reads back empty and the script reported a false "Debug build failed".
  - Vite was stopped afterwards; `Dev.ps1` restarts it.
- **Day-to-day work and live testing happen on the dev build.** Make a release build only when the user says "this is the FINAL build" (for sharing) or asks for a test drop.

## 5. Build and test facts (this machine)

- **Toolchain:** the workspace-local one at `..\.toolchains\rust-exporter`. Set `CARGO_HOME` and `RUSTUP_HOME`, and always pass `--offline --locked`.
- **8-thread affinity is mandatory.** Run rustc with `ProcessorAffinity = 0xFF000000` (4278190080). Widening it mid-build made rustc crash with `0xC0000005 ACCESS_VIOLATION`. `Dev.ps1` and the build wrapper both set it.
- **pnpm is not installed.** `Build.ps1` calls `pnpm install --offline --frozen-lockfile`; put a no-op `pnpm.cmd` shim on PATH, since `node_modules` already matches the lockfile.
- **Node isn't on PATH for `.cmd` shims.** Run the tools directly:
  - `& "C:\Program Files\nodejs\node.exe" node_modules\typescript\bin\tsc --noEmit`
  - `node node_modules\vitest\vitest.mjs run`
- **`Build.ps1 -Verify`** runs these steps in order: the startup-audio guardian, the diagnostics race check, assets, the parity audit, tsc, the vite build, vitest, `cargo test -- --test-threads=1`, `cargo build --release -j 1`, packaging, and finally the 119-check packaged UI smoke (`frontend\parity-smoke.ts`).
- **Build times:** about 20 minutes cold, or about 10 minutes when the test steps are skipped. For a quick fix build after a full test pass, a temporary copy of `Build.ps1` without the vitest and cargo-test lines is acceptable; delete it afterwards.
- **The UI smoke breaks whenever markup changes.** Update `frontend\parity-smoke.ts` expectations before calling a build done.
- **Ignored disk-audit tests** need these env vars, each set to the installed `NBA2K27.exe`: `C27_CAMERA_V2_DISK` and `C27_PROVIDER_DISK_AUDIT`.
- **Shared `target\` contention:** an `LNK1104` lock error just needs a rerun, or set `CARGO_TARGET_DIR` per session.
- **Script gotchas:**
  - Bash heredocs mangle backslashes; write scripts with a file-writing tool.
  - Python here can't see `AppData\Roaming` and isn't long-path aware; use `\\?\` paths.
- **Snapshot workspace:** `CodexProjects\_snapws` was the old snapshot build mirror. It grew to 113 GB and has been deleted. Its sibling entries were junctions into the real project, so unlink junctions with `cmd /c rmdir <link>` (no `/s`) before deleting anything like it.
- **Parity scoring:** `python tools\parity_score.py --file artifacts\parity-status.json --build <ver> [--top N]`. Statuses are LIVE (1.0), UNV (0.7), PART (0.4), DIS (0). Update rows only from real live evidence.
- **Last full results (0.53.1):** 1160 Rust passed, 0 failed, 47 ignored; 204 vitest passed; tsc clean; 119/119 UI smoke.

## 6. Architecture and key technical knowledge

### Native write layer
- **`src\feature_writer.rs`**:
  - `begin_batch`/`end_batch` give one ownership/profile proof per transaction (`BATCH_PROOF` lasts 2 s).
  - `data_protection()` accepts `PAGE_WRITECOPY` for `MEM_IMAGE` `.data` pages; this was the 0.45 camera hold.
  - `write_object` is for heap writes (1 or 8 bytes, private RW only, verified by closure).
  - `CharacterMoved` is a typed retryable error (`NPC_MOVED_LIMIT` 3).
- **`src\feature_runtime.rs`**:
  - Camera tick/publish uses all-or-nothing publish with RestoreAll and retry (`CAMERA_RESTORED_LIMIT` 10).
  - Scaler groups tick/apply.
  - `ACCESS_OPEN_FAILURE_LIMIT` is 40.
- **`src\provider_launch.rs`**:
  - The launch state machine: verifying, then preparing (parallel since 0.53), waiting-for-game, loader-armed (20 s settle gate), activating, loader-active.
  - The features Runner is created at loader-armed and ticks every 500 ms.
  - **Lesson from 0.52:** any `?` in the active loop ends the whole launch and cancels the outfit lease. 0.53 stops a status-file write failure at match load from being fatal. Keep feature errors per-feature, the way presentation already does.

### Cameras
- No menu or game-mode gate (WPF parity).
- Starts at loader-armed.
- Game-owned settings/runtime bytes (registry block `0x20..0x30`) are tolerated.
- On FullyReset, republish from a fresh capture.

### Scaler and NPC groups (`src\scaler_v2.rs`, `src\scaler_npc_v2.rs`)
- Description `+0x7D` bit 2 is the opt-in.
- Character list sentinel: `0xA409E90`.
- `MANAGER_INSTANCE` node holds the actor at `+0x110`.
- Evaluator `0x6E4630` sits at interface slot `+0x190`.
- Courtside celebrity is a PLAYER class outside the roster table. The user wants a future feature to identify and change the celebrity, but not now.

### Outfits
- **Carry:** manager = interface − `0x78`, where the interface is the `MANAGER_INSTANCE +0x78` pointer (WPF version.dll parity).
- **Worker DLL:** `assets\native\version-outfits-v2.dll`, pinned SHA `645984B7…2CFF7C`. The pin lives in two places: `PROXY_SHA` in `outfit_worker_v2_loader.rs` and `DLL_SHA` in `provider_outfit_profile.rs`.
- **No game-mode gate at selection.** V2 reported game mode 0, 4 and 6.
- **Random all** uses one `apply-many` batch.
- **Frontend:**
  - `frontend\outfit-random.ts`: `AutoRandomPolicy`, `applyRandomBatch`.
  - `frontend\NativeOutfits.tsx`: lineup polling and the manual set.
  - `frontend\App.tsx`: the overlay.

### Overlay flicker fix
- WPF 0.37.73 stopped rebuilding views that hadn't changed, and its renderer never polled.
- 0.53 does the same:
  - Background lineup reads don't re-render or disable the PLAYER list.
  - No-op overlay publishes are skipped in Rust.

### Ball prepare
- V2 now matches WPF's flow: 4 samples, 100 ms apart. Estimated ~7–8 s down to ~0.4 s, not yet confirmed live.

### Selectors (`src\selector_runtime.rs`)
- Ported from WPF `multi-config-manager.js`.
- WPF's ownership state lives in **WPF's** portable folder, so a complete user bundle that isn't in that state is now served unchanged, with a note in the summary source (0.53.1).
- A partial conflicting bundle still stops Play with WPF's message.
- **Follow-up:** read WPF's real multi-config state through `wpf_migration`, and show a visible warning when generated selectors are skipped.

### Game-update resilience
- `feature_relocation.rs` holds a 199-anchor catalog.
- Settings → Advanced shows each feature as Mapped or Needs mapping.
- An unknown build loads mods only when every proof passes.

### WNBA roster
- Freestyle uses `TKU_DELEGATE` → `roster/learn2k_roster.iff` (334 teams, NBA only).
- The merged roster (403 rows, 30 NBA + 15 WNBA) is built locally by `tools\roster_merge_wnba.py`.
- The game loads the roster through the VCRESOURCE async path (`0x3C76D20` → `0x5168BE0`) from the archive, never through the hooked file device.

### Read-only probes
- `scripts\Read-CharacterScaleInventory.ps1`
- `tools\Read-EditorServicesProbe.ps1`
- `tools\Read-WnbaRouteState.ps1`
- `tools\Read-JukeboxFreestyleEvent.ps1`
- `tools\Read-ProxyCounters.ps1`

## 7. Feature status (parity 80.2% weighted, 76.0% plain, at 0.53.1)

The source of truth is `artifacts\parity-status.json`. LIVE means the user confirmed it in game; UNV means built but not live-confirmed.

**LIVE (26):**
- one-click Play, mod loading, mod-profile junction/restore
- mod library UI
- V2 roster read
- outfit live apply at selection
- arena, ball, scoreboard, Blacktop removals, F8 overlay auto-open
- cameras plus names plus status card, scaler players/referees/groups/retry/full-game status
- Jukebox library, playback and Freestyle playlist
- Freestyle gym
- folders/portable storage, hang capture
- UI shell and visual parity

**Needs a live check (highest value first):**
1. **Outfit carry into the match.** It failed in 0.52 because the session ended at match load. The fix is in 0.53.
2. **Automatic random**, plus the flicker check (sit at selection 30 s).
3. **Ball prepare speed.**
4. **Mod install** (ZIP/RAR/folder plus conflict review), profiles CRUD, tags.
5. **Saved arena/scoreboard** without opening F8, presentation re-apply on a second match, crowd restore, rules.
6. **Startup MP4**, Jukebox covers and repair.
7. **Launch progress overlay, cancel, wording, prepared-playlist cache timing**, secondary Steam library.
8. **Settings:** support bundle, health, developer tools, launcher update/rollback, WPF migration.

**PART (editors, lowest priority):**
- V2 player/team/uniform saves (appearance struct for height/wingspan unproven)
- staff/seasons/arena clone
- artwork (WIC decode, sound IDs)
- export/2KDB
- DNA copy from the V2 read
- BBRef season import
- ball thumbnail VCZ/DDS decode

**DIS:**
- WNBA Freestyle and WNBA Blacktop (§8)
- debug menus: WPF's route is Sept-build only (RVA `0xA167348`); needs a read-only probe of `[base+0xA1F1248]+0xF8/+0x110`

Top gains if made LIVE are listed by `parity_score.py --top 20`.

## 8. WNBA in Freestyle: how WPF did it, and where the port stands

**Full WPF write-up, with file:line citations:** `docs\wpf-reference\unlocks-wnba.md`. Read it first. Summary:

### How WPF implemented it
- **UI and settings:**
  - Unlocks page, toggle "Unlock WNBA Teams in Freestyle" (`unlockWnbaFreestyle`, default ON; WPF `renderer/index.html:404`).
  - Changing it only saves the setting; nothing is written.
  - The work is armed at Play, inside `launchGame`, just before `launchWithoutEac` (`W/Reference/src/main.js:1798-1806`).
- **Helper:** one C# helper, `NBA2K27FreestyleTeams.exe` (`W/Reference/runtime-resources/unlocks/`). Its source is `nba2k27-mod-launcher\unlocks-helper\*.cs`, and `Main` dispatches on `args[0]` (`FreestyleTeams.cs:180`).
  - Freestyle runs as one-shot polled helper calls: 6 positional args, one JSON line on stdout.
  - The JS state machine is `W/Reference/src/core/freestyle-unlocks.js`.
- **The live Freestyle unlock is a DATA route, not code injection.** It's the `category-*` route in `unlocks-helper\TeamCategories.cs:15-107`, with tables, allowlist and journal in `CategoryPolicy.cs:4-24`:
  - It writes four static DWORDs in two 6-entry team-category-order tables so WNBA appears as a Freestyle category.
  - It flips page protection to RW and back around the write.
  - WPF's version.dll is NOT involved; `version_proxy.cpp` has no WNBA logic.
- **Withdrawn route, don't use:** the older mask `0x68400033` / category-5 "combined list" route in `FreestyleTeams.cs:122-179`. It was withdrawn 2026-09-20 (`nba2k27-mod-launcher\ABANDONED_FEATURES.md:54-59`).
- **Session binding:** WPF strictly validates the "new session" binding (pid, created, basis versus the Play timestamp) before writing.
- **Blacktop is different (separate toggle, default off).** A long-lived `blacktop-watch` child:
  - swaps a vtable callback pointer (`BLACKTOP_DELEGATE` +0xD10 on the old build, +0xD30 on V2) to the stock Learn2K roster loader;
  - at selection, adds a remote RX bridge, a rel32 jump-table patch and a label-key swap (Euro→Women) so the Y category button reaches WNBA (`BlacktopTeams.cs`, `BlacktopCategory.cs`).
  - **Those parts are executable-code patches.**

### Why it doesn't just work on Version 2
- The category route only exposes WNBA teams that exist in the loaded Freestyle roster.
- On V2 the default Freestyle roster (`roster/learn2k_roster.iff`, via `TKU_DELEGATE`) has 334 teams and **0 WNBA rows**. Evidence: `artifacts\wnba-v2-research-20261017.md:16-18`, `artifacts\wnba-freestyle-default-roster-audit-20261017.md`. So flipping the category DWORDs alone shows no WNBA teams.
- **This port built:**
  - a merged roster (403 rows: 30 NBA + 15 WNBA) via `tools\roster_merge_wnba.py`, kept local (it uses the captured roster key; never ship the key);
  - a provider forced-override slot for `roster/learn2k_roster.iff` (case-insensitive, with a path trace).
- **Problem:** the 0.52 trace showed **0** `learn2k` requests reach the hooked file device. V2 streams that roster from the archive through VCRESOURCE (`0x3C76D20` → `0x5168BE0`), so the file override never applies.

### What Codex needs to do
1. Port WPF's **category route** for V2:
   - Find the two category-order tables in the V2 exe; the anchors are in `TeamCategories.cs`/`CategoryPolicy.cs`, with old-build offsets in `FreestyleTeams.cs:41-107`.
   - Apply the same guarded data write: journal, before/after compare, readback, restore.
   - Use the same bind validation as WPF.
   - Note: `src\freestyle_v2.rs` already has the V2 table code (`Tables`, `allowed_mixture`, `verify_teams`, currently unused).
2. **Get a roster with WNBA rows loaded.** This is the open problem on V2. Options:
   - (a) A custom roster that contains WNBA teams. WPF's own UI notes "Custom rosters must contain WNBA teams for them to appear".
   - (b) Make the merged roster reach the engine. The file override doesn't work because of the archive path; any data/file route needs research.
   - (c) WPF Blacktop-style loader redirection, which is an executable-code patch.
3. **Note from the Claude sessions:**
   - The WNBA work that built a launcher-owned executable stub (a loader redirect) was stopped by Claude's safety classifier. A later request for data-only research was also stopped, so this port never researched (b).
   - Decide the approach with the user.
4. **Keep the WNBA toggle OFF until it works.** In 0.52 it was on during the run where the session ended.

## 9. Open bugs from 0.53/0.53.1 live testing (`artifacts\qa053-live-results.md`)

The user stopped testing here. The source tree is exactly 0.53.1; nothing was changed after that build.

- **Automatic random at selection doesn't fire (outfits, high priority).**
  - **Live evidence (0.53.1):**
    - `autoRandomOutfits=true`.
    - The native side is healthy: List reads complete every 1–2 s (revision = completedRevision, update rising), 4/4 players with pool `male`, and bindings identical across reads.
    - The overlay never sends, or never lands, the `apply-many`.
  - **Suspects:**
    - `NativeOutfits.tsx` `send()` returns `null` while `pending.current` is true, for example during a background `refresh()`. `queueMany` then yields null, the policy treats the batch as refused, `unmark`s it and retries, so it may never land.
    - The App effect's early returns (`clothing.busy` / `randomRunning`) and its deps. The ready reply arrives only via the idle poll, every 4th 250 ms tick when no panel or menu is open.
    - Queued/ready reply interleaving against the `status.update` dedupe in `AutoRandomPolicy.observe`.
  - **Next:** write a test of the real sequence (idle poll, queued reply from refresh, ready reply via poll, refresh in flight when due fires), then fix. Manual Random all works and is fast.
- **Edit outfit is broken.**
  - **Apply outfit does nothing.** Same suspect: `send()` silently drops the request while a refresh is pending.
  - **Selecting a player doesn't load what they're wearing** into the editor dropdowns. The `outfit-for-pool` effect in `App.tsx` only reloads when the pool changes, so switching between two male players never loads the selected player's current items.
  - WPF read the player's current items on selection; see `docs\wpf-reference\outfits.md`.
- **Edit outfit player dropdown is jumpy and laggy.** It is likely the same background lineup refresh (every 1–2 s at selection) re-rendering or resetting the dropdown, plus each player switch firing native requests. Fix: keep the open dropdown's list stable while it's open, debounce player switches, and don't let background reads replace the selected player. Compare WPF's `blacktop-player-menu.js`, which never polled.
- **The overlay sometimes freezes for a few seconds.** Likely suspects:
  - IPC calls that wait on the native outfit worker or a slow backend command. `native-outfit-command` and `native-session-status` run every 250 ms–1 s at selection, and `wait()` polls every 50 ms during Random.
  - Synchronous Tauri commands blocking the main thread (check for non-async `#[tauri::command]` handlers that do file or process I/O).
  - React re-render storms from the lineup refresh.
  - How to diagnose: log command durations in the backend, check the WebView2 devtools Performance tab in the dev build, and make overlay commands async/off-thread. WPF's overlay never polled; its renderer only rebuilt changed views.
- **Outfit carry into the match: STILL FAILS in 0.53.1 (top priority).**
  - The 0.52 cause (the session ending at match load) is fixed: the session stayed `loader-active`, `error` None.
  - **New cause.** `provider-state.outfits` in the match says:
    > Blacktop match outfits not carried yet: native outfit worker 1 has not armed after 18 s; nothing was written … Other launcher processes stop the native worker: 2k Launcher Rust Prototype.exe (PID 31812); close them and restart the game
  - **Process check (read-only):**

    | Process | PID | Parent |
    |---|---|---|
    | Main launcher UI | 31812 | 8904 |
    | Provider worker (`workerPid`) | 68652 | 31812 |
    | Game | 42064 | — |
    | `MoviesMobileLauncher.exe` (unrelated; doesn't match the blocker names) | 17360 | — |

    No other launcher was running.
  - **So the worker refuses to arm because of the launcher itself.** `native\outfit_match_v2_proxy_bridge.hpp` `protected_processes(owner_pid, parent_pid_)` excludes the worker and the lease's `parent_pid_`. The Rust `worker_blocking_name` list includes `2k launcher rust prototype.exe`. Either the lease's parent field (u32 at offset 16, path at 1120) isn't the main launcher PID, or the exclusion/diagnostic compares the wrong PIDs.
  - **Next:** dump the lease written by the launcher (`Data\blacktop-outfit-v2-*.lease`) and compare owner/parent PIDs with the live process tree. Make the DLL and the Rust diagnostic both exempt the owning launcher's main process and its worker, as WPF's version.dll exempted its own launcher. The worker DLL hash is pinned in two places (§6, Outfits), so a DLL change needs both pins updated.
  - This matches the 0.51 symptom ("worker did not arm within 10 s"). Carry has probably never worked live in the Rust app.
- **Ball selection is still very slow (0.53.1), especially when switching to a different ball.**
  - The 0.53 change (WPF's 4 samples, 100 ms apart, in the `ball_gameplay_*` files) estimated ~0.4 s, but the user still sees long waits, worst on a change.
  - Suspects:
    - A change re-runs the full prepare instead of reusing the prepared state.
    - The F8 "Preparing" badge waits on arena and scoreboard too.
    - The request is queued behind the 250 ms arena/ball/scoreboard loop in `provider_launch.rs`.
    - The asset is re-read or re-decoded per change.
  - Measure with per-step timing in `provider-state.ball.details` and compare with WPF (`docs\wpf-reference\arena-ball-scoreboard.md`). WPF swapped balls quickly.
- **F8 doesn't toggle the overlay (0.53.1).** Automatic hide works (match start / leaving Blacktop), but pressing F8 neither hides nor shows the overlay. Check the F8 hotkey path: global hotkey registration (RegisterHotKey can fail while the game holds focus or has exclusive input) and whether it needs the keyboard-poll fallback used for the hang-capture hotkey (Ctrl+Shift+F12, which fixed the same symptom in 0.51). Also check that the F8 toggle respects the 'sticky Hide' state and isn't swallowed while the overlay is unresponsive.
- **Overlay F8 diagnosis notes (0.53.1).** In `Logs
ative-overlay-state.json` during the match, the boundary flaps between `match` and `transition` every 0.3–7 s (waiting-for-focus / waiting-for-blacktop / held). That sample was taken while the Claude app had focus, so the 'other app foreground' part is expected, but the boundary flapping isn't. Also, `native_overlay.rs:411`: if the UI heartbeat is late for 15 s (`ui_stalled`), the loop RELEASES the F8 hotkey and disables the overlay (phase `ui-unresponsive`). This may tie the 'unresponsive at times' report to the 'F8 does nothing' report. Check whether the hotkey is re-registered after the UI recovers, and why boundary detection flaps during a match.
- **Launch is still slow: about 30 s from Play to Steam dispatch (0.53.1).** Phase timings (ms):

  | Phase | 0.53.1 | 0.52 |
|---|---|---|
  | verifying-game | 1073 | 1088 |
| **preparing-players** | **24391** | 14927 |
| preparing-scoreboards | 48 | 1503 |
  | preparing-selectors | 4195 | 357 |
| preparing-loader | 177 | 210 |
| waiting-for-game | 2699 | 18233 |
  | loader-armed (fixed settle gate) | 20194 | 20170 |
| **Total to loader-active** | **54.8 s** | 58.4 s |

  - `preparing-players` dominates and is worse than 0.52. The parallel prepare didn't shorten it; the other steps now finish inside it.
  - `preparing-selectors` jumped from 0.36 s to 4.2 s, probably because 0.53 runs selector generation before serving the user bundle (§6, Selectors). If the user bundle wins anyway, skip generation.
  - Fixes:
    - Use the per-step player timing launch-speed-2 added to find the slow part of player-folder linking (361 files) and cache it by mods-folder fingerprint. launch-speed-2 skipped the index cache as '3%'; re-measure.
    - Skip selector generation when a complete user bundle is present.
    - Enable the readiness probe to cut the 20 s settle gate.
- **Flicker: fixed live in 0.53.1** (no flicker). The overlay still goes unresponsive at times (see the freeze item above).
- **View on mannequin doesn't load immediately** with the default clothing choices.
- **Mannequin shoes:** shoes with a preview render, shoes without one don't. Build the missing shoe assets on demand when the mannequin opens. Workaround: Prepare all previews first.
- **Selector launch stop:** fixed in 0.53.1 (§6, Selectors). Follow-ups are listed there.
- The user is still testing 0.53.1; append further results to `artifacts\qa053-live-results.md`.

Passed live in 0.53/0.53.1:
- 0.53.1 Play launched the game with the user's selector mod installed (selector fix confirmed).
- Prepare all previews ("works very well").
- Settings → Advanced shows every feature as Mapped.

## 10. Improvement backlog (summary of `artifacts\improvement-suggestions-20261017.md`)

1. **Done in 0.53:** run the independent prepare steps in parallel (saves 6–11 s).
2. **Readiness probe:** replace the fixed 20 s settle gate. It's present in 0.53 but off by default; validate it live, then turn it on, keeping the 20 s fallback.
3. **Jukebox verification:** collapse the redundant passes (about 3–5 s).
4. **Restore progress:** show "Restoring n/N" and batch the post-exit restore (5–13 s today).
5. **Skip unchanged prepare steps** (scoreboards, verify-game) by file identity.
6. **UI polling:** one shared status store instead of several pollers; back off when hidden.
7. **Build speed:** split the single 141k-line crate into a workspace (memory/game layer, outfits, presentation, UI backend) so changes rebuild incrementally.

## 11. Artifacts index (all in `artifacts\` unless noted)

- `parity-status.json`, `parity-score-20261017.md`: parity rows and scores.
- `qa053-live-results.md`: current live results.
- `safety-review-2026-10-18.md`: 10 fixes, 5 high (rules/launch deadlock, Freestyle half-publication, camera journal budget, launch-blocking Jukebox receipt, torn outfit report).
- `feature-gap-inventory-20261017.md`, `wpf-version-dll-audit-20261017.md`: what WPF has that we don't.
- `improvement-suggestions-20261017.md`, `game-update-resilience-20261018.md`.
- `outfits-v2-research-20261017.md`, `editor-services-v2-research-20261017.md`, `editor-fields-v2-research-20261017.md`.
- `jukebox-covers-v2-route-20261017.md`, `jukebox-v2-freestyle-cover-research-20261017.md`, `prepared-playlist-cache-gap-20261017.md`.
- `wnba-v2-research-20261017.md`, `wnba-merge-20261017.md`, `learn2k-general-mixed-roster-evidence-20261017.md`, `wnba-*-audit-20261017.md`.
- `camera-scaler-launch-audit-20261017.md`, `game-capture-port-audit-20261017.md`, `startup-intro-integration-20261016.md`.
- In the project root: `QA-REPORT.md` has a note per build, newest first; `CLAUDE-HANDOFF.txt` has per-build history and the original WPF paths.

## 12. Builds

| Build | Status |
|---|---|
| 0.46 | First camera LIVE pass |
| 0.51 | Outfits, Random, dropdown, ball, scoreboard and hang capture LIVE |
| 0.52 | Random all fast; carry, auto-random, flicker and WNBA failed |
| 0.53.0 | All 0.53 work; Play stopped by the selector rule. SHA `2436BF72…F63F37` |
| **0.53.1** | **Current test build.** Selector fix. SHA `8F746C64C73DD910576F0DC3E45C2C82FFF3BEA34C453837FCD31445EFF7F7CB` |

Builds are in `Builds\<version>\Portable\App\2k Launcher Rust Prototype.exe`.

## 13. Suggested next steps for Codex

1. Verify the dev app launches and hot-reloads (§4), and move all work onto it.
2. Fix automatic random and Edit outfit (§9). They probably share the `send()`/pending root cause. Then live-check carry, flicker and ball speed.
3. Fix the mannequin's immediate load and the no-preview shoes.
4. If carry still fails live, read `Logs\native-sessions\<id>\provider-state.json` (`timing.transitions`, `error`, `outfits`).
5. Validate the settle-gate readiness probe live, then enable it.
6. Work through the UNV rows by user priority (Blacktop, overlay, outfits first) and keep `parity-status.json` honest.
7. Leave editors and WNBA for last.
