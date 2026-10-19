# Play button and launch flow — WPF reference
Source build: WPF quality-pass-qa-20261010 (frozen). Last researched: 2026-10-17.

Path abbreviations (relative to `C:\Users\carrn\CodexProjects\2kconversiontool`):
- **WA** = `nba2k27-launcher-wpf\release\quality-pass-qa-20261010\App\`
- **WS** = `WA\Reference\src\`
- **VP** = `nba2k27-version-proxy\src\`
- **R** = `nba2k27-launcher-tauri2\`
- **SN** = `C:\Users\carrn\AppData\Roaming\Claude\side-session-notes\local_98bf0092-fe39-4ae3-abce-e88edad0a633\`

See also `architecture.md` §3 (version.dll handshake) and §6 (restore on exit).

## Summary
- Play is **one click**: there is no confirmation dialog. The click calls the single backend command `game:launch-without-eac` (`WS\main.js:1688-1853`). That command does every preparation step in a fixed order, then starts the provider and installs `version.dll` (`VersionProxySession.start`). Inside a `launchGame` callback it arms the in-game helpers, starts the game monitor and only then runs `steam.exe steam://launch/<appId>/option1`.
- The command **returns as soon as Steam has spawned**. Attach, the 20 s settle, the trigger and `loader-active` all happen in the background and reach the UI only through `loader:status` pushes.
- Everything that changes files (playlist audio, camera names, scoreboard layouts, ball, mod-profile junction, player-folder links) is prepared **before** the provider indexes the mods folder. Everything that touches memory (scaler, unlocks, debug, presentation, crowd, ball, cameras, outfit carry) is **armed before Steam starts but polls for the game itself**. None of it blocks launch.
- **Top 3 port breakers:**
  1. Ordering. File preparation must finish before the provider starts. Memory helpers must be armed (polling) before Steam starts, and each uses its own deadline (scaler/unlocks: 120 s to find the game). They must not wait for `loader-active`.
  2. Rollback. Any failure before Steam returns must undo **every** preparation step (the list in §3) and push `loader-failed`. A failure after Steam starts must **not** touch files until verified game exit.
  3. Play returns at Steam spawn, not at game ready. The progress overlay is driven entirely by `loader:status` phases and clears itself on `loader-active` (100%) or 2.6 s after `loader-failed`.

## Key source map
| Concern | file:line |
|---|---|
| Play button markup ("Play session", `#startSession`) | `WS\renderer\index.html:92` |
| Play click handler and toasts | `WS\renderer\app.js:2957-2968` |
| Play enable/disable rules | `WS\renderer\app.js:203-223, 265-300` |
| Launch overlay and phase→progress table | `WS\renderer\app.js:39-102` |
| Backend Play command | `WS\main.js:1688-1853` |
| `launchGame` callback (arm order, Steam, F12, video) | `WS\main.js:1796-1824` |
| Steam launcher (manifest lookup, option1 URL) | `WS\core\steam-launcher.js:7-74` |
| Provider/DLL session start and background activation | `WS\core\version-proxy-session.js:160-402` |
| Provider-layout resolver gate | `WS\main.js:764-808, 1740-1742` |
| Playlist preparation | `WS\core\archive-jukebox.js:409-438`, `WS\core\jukebox-session.js:106-170` |
| Prepared playlist cache | `WS\core\playlist-launch-cache.js:6-48` |
| AudioReadiness gate | `WS\main.js:1753-1756`, `WS\core\audio-readiness.js:5-14`, `VP\audio_readiness.cpp:14-77` |
| Mod-profile junction | `WS\core\mod-profile-manager.js:73-120` |
| Player-folder hard links | `WS\core\player-folder-session.js:36-70` |
| Session monitor (camera start after attach) | `WS\core\session.js:65-137`, `WS\main.js:504-510` |
| Outfit carry starts on `loader-active` | `WS\main.js:2067-2071` |
| Startup video (game window wait, mute, watchdog) | `WS\core\video-overlay.js:13-50`, `WS\core\game-window.js:3-17`, `WS\core\game-volume.js:8-52` |
| Game-owner pipe (one launcher per machine) | `WA\Backend\game-owner.cjs:5-16`, `WA\Backend\host.cjs:124-134` |
| Closing dialog | `WS\renderer\launcher-closing.js:1-9`, `WS\main.js:444-473` |
| Timings file | `WS\main.js:1826-1827` |

---

### 1. Play button and launch overlay

#### 1. User-visible behavior
- **Home card.** Title "NBA 2K27 " plus status pill `#launchStatus`, a **Play session** button (`aria-label="Play NBA 2K27"`), and a hidden **Stop waiting** button (always kept hidden, `WS\renderer\app.js:283`) (`WS\renderer\index.html:87-94`).
- **Status pill text** (`WS\renderer\app.js:220, 276`):
  - Before any session: "Ready" / "Checking" / "Check scan" / "Check setup".
  - During a session: "Needs attention" (phase matches /error|failed/), "Running" (session `game-running`), "Session active" (loader busy), otherwise "Ready" or "Check setup".
- **Play is disabled** when any of these holds (`:217, 318`):
  - setup problem is `blocked` or `checking`
  - `homeSetupBusy`, `homeBusy` (saving a profile or playlist pick) or `homeLoading` (reading the jukebox)
  - a session is active (`profilesLocked()`)
  - an overlay is already running (`launchStartedAt`)

  Play is **hidden** while the loader is busy or the game is running (`:282`).
- **Click** (`WS\renderer\app.js:2957-2968`):
  1. Return if busy or `validation.ready` is false.
  2. Disable the button.
  3. `showLaunchOverlay('Checking the current game build and selected profile…')`.
  4. `await launchGameWithoutEac()`.
  5. Success toast: `"NBA 2K27 launched without EAC (Steam App <appId>, option1)."`, or "NBA 2K27 is already running." when `alreadyRunning`.
  6. Error: hide the overlay and show an error toast with the message (5 s, `:93-100`).
- **Overlay** (`#launchOverlay`, `WS\renderer\index.html:543-553`):
  - Eyebrow "STARTING GAME", title "Starting NBA 2K27…", a stage line, a progress bar, the step label, and elapsed time "N seconds" (updated every **250 ms**).
  - Footer: "Keep this launcher open. This window clears automatically when the game is ready."
- **Phase → progress** (`WS\renderer\app.js:39-53`):

  | phase | % | step label |
  |---|---|---|
  | preparing-launch | 8 | Checking game build |
  | preparing-selectors | 22 | Scanning selected profile |
  | preparing-jukebox | 32 | Preparing playlist audio |
  | preparing-profile | 38 | Activating mod profile |
  | preparing-session | 50 | Starting session monitor |
  | preparing-names | 58 | Preparing camera names |
  | preparing-loader | 66 | Starting local asset loader |
  | launching-game | 78 | Opening NBA 2K27 |
  | waiting-for-game | 86 | Waiting for the game |
  | loader-armed | 91 | Game connected |
  | activating-loader | 96 | Finishing unique assets |
  | loader-active / game-running | 100 | Ready |

  Unmapped phases (`preparing-scoreboards`, `preparing-ball`) show **12%** with the phase name, dashes replaced by spaces, as the step (`:86`). This is a WPF quirk: the bar jumps back.
- At 100%: the title becomes "NBA 2K27 is ready" and the overlay hides after **900 ms** (`:89-92`).
- On `loader-failed`: title "Startup needs attention", stage = message (or "The game could not be started."), step "Stopped safely", bar 100%, hide after **2600 ms** (`:82-87`).
- **Home "session active" box.** "Preparing your session" / "Session in progress" + "Keep the launcher open while you play." It is shown while the loader is in `preparing-loader … stopping-loader` or the session is `game-running` (`:270, 278-280`, `WS\renderer\index.html:94`).
- While active, the profile radio buttons, the add/edit/remove profile buttons and the Home profile/playlist pickers are disabled. Text: "Profiles are locked while your game session is active." / "Selections are locked while the game session is active." (`:291-320`).
- **Developer page "Session progress"**: three steps (Local asset loader → NBA 2K27 → Camera unlock) with active/done states (`WS\renderer\app.js:284-290`, `WS\renderer\index.html:496-503`).

#### 2. Timing and gating
- The overlay is not polled. It reacts only to `loader:status` and `session:status` pushes (`WS\renderer\app.js:323-333, 2970-2971`).
- The renderer never re-sends Play. The backend rejects a concurrent Play with "Game launch is already in progress." (`WS\main.js:1691`).

#### 3. Mechanism
`launcher.launchGameWithoutEac()` → `ipcRenderer.invoke('game:launch-without-eac')` (`WS\preload.js:153`). The backend's IPC wrapper runs the build gate first (feature `session`), see architecture §4.

#### 4. Build profiles
The session feature is supported on OLD `6A974D13:368C3000` and Sep 11 `6AA2013D:36558000` only (`WS\core\game-compatibility.js:3,6`).

#### 5. Rust must match
- [ ] One click starts Play: there is no confirmation for a normal modded Play. (Rust 0.47 matches; see `SN\qa047-launch-speed-handover.md`.)
- [ ] The overlay shows the phase message, step and elapsed seconds. It auto-hides 900 ms after `loader-active` and 2.6 s after `loader-failed`.
- [ ] The success toast includes the Steam App id and `option1`.
- [ ] Profile and playlist pickers are locked from Play click until the session ends.

---

### 2. Backend Play command: ordered preparation (before game start)

#### 1. User-visible behavior
Status messages (`loader:status` phase → message), in order (`WS\main.js:1721-1780`):
1. `preparing-launch` "Checking the current NBA 2K27 build…"
2. `preparing-selectors` "Scanning the selected profile and preparing player selectors…"
3. `preparing-jukebox` "Preparing the selected playlist for in-game playback…", then per song "Preparing song i/n: <title>" (`WS\core\jukebox-session.js:127`)
4. `preparing-profile` "Activating the selected mod profile…"
5. `preparing-session` "Starting the local game and camera monitor…"
6. `preparing-names` "Checking the optional in-game camera names…"
7. `preparing-scoreboards` "Preparing scoreboard data bindings…"
8. `preparing-ball` "Preparing the selected Blacktop ball…"
9. Then the loader phases from architecture §3 (`preparing-loader` → `launching-game` → …).

Each `launchStage` push carries `{phase, message, startupStartedAt, timings}` (`:1699-1704`).

#### 2. Timing and gating
Exact sequence in `game:launch-without-eac` (`WS\main.js:1688-1828`):
1. Guards:
   - `requireNativeHost()`
   - "Wait for the startup video import to finish or cancel it before Play." if a video import is busy
   - "Game launch is already in progress."
   - Then set `launchInProgress=true`.
2. Cancel background work: animation warm-up, clothing cache job, browser roster server (`:1693-1695`).
3. "Close NBA 2K27 before starting a new session." if the game is running (`:1706`).
4. Read settings. If `startupVideoEnabled`, read the presentation library; failures only log "Startup video skipped: …" (`:1708-1712`).
5. **Restore leftovers from any previous session**, in order:
   1. `videoOverlay.finish('launch-failed')`
   2. `sharedPresentationSession.restore()`
   3. `playerFolders().prepare(modsRoot)`: restores old links, then creates new hard links
   4. `jukeboxSession.restore()`
   5. `cameraManager.namesSession.restore()`
   6. `scoreboardLayouts.restore()`
   7. `highschoolDigitProbe.restore()`
   8. `ballScaleRecovery.restore()`

   (`:1713-1720`)
6. **Run in parallel:** `refreshProviderLayout(settings)` (resolver exe, timeout 30 s, cached per exe path/size/mtime) and `multiConfigManager.prepare({gameRoot, modsRoot, enabled:true})` (player selectors). Both are awaited. If the layout status is not `ready`, throw "The current NBA 2K27 build is not safely resolved: <message>" (`:1722-1742`).
7. Playlist:
   - `jukeboxManager.launchSource(modsRoot, {cachePreparedPlaylists})` → `jukeboxSession.prepare(…)` writes music.iff and slot WAVs into the active mods folder.
   - `audio.freestylePlaylist = useFreestylePlaylist(settings, audio)`.
   - If `audio.audioPolicy` is set, the readiness gate becomes AudioReadiness (`:1744-1757`).
8. `modProfileManager.activate({gameRoot, modsRoot})`: the junction swap (§4) (`:1759-1762`).
9. `session.start(settings)` if not already running: the 1 s game monitor (`:1764-1767`).
10. Camera names: `cameraManager.readLibrary()` → `namesSession.prepare({gameRoot, modsRoot, names})`. If `cameraNames.installed`, add the forced override `loc/english.iff` (`:1769-1774, 1783`).
11. `scoreboardLayouts.prepare(...)` (`:1777`).
12. `balls().prepare(...)` (`:1780`).
13. `retiredFrontMenuRecovery.restore()` (`:1782`).
14. `versionProxySession.start({…, selectorOverride: selectors.selectorOverride===true, scoreboardTextTest:true, ...audio, launchGame})`. This verifies, starts the provider, copies the DLL, writes the cfg and calls `launchGame` (§3) (`:1785-1825`).
15. Record `timings.untilSteamLaunchMs` and write `<root>\Data\startup-timings.json` `{recordedAt, timings}` (best effort) (`:1826-1827`).
16. Return `{...launchResult, loader, startupTimings}`.

Timings keys:
- `buildCheckMs`, `playerSelectorsMs`, `launchCache` (`{hits, misses, bytes, totalMs}`), `playlistExtractionMs`, `jukeboxPreparationMs`
- `playlistPreparation` (`{cacheHits, cacheMisses, artworkMs, audioPreparationMs, publishMs, totalMs}`)
- `profileActivationMs`, `sessionMonitorMs`, `cameraNamesMs`, `untilSteamLaunchMs`

**There are no explicit timeouts on the preparation steps themselves.** Only the sub-tools have them: resolver 30 s, provider start 30 s.

#### 3. Mechanism (rollback on failure)
Any throw inside the try block runs (`WS\main.js:1829-1849`):
1. `videoOverlay.finish('launch-failed')`
2. Stop the freestyle unlocks, blacktop unlocks, debug menus, presentation and crowd helpers
3. `multiConfigManager.emit({phase:'error'})`
4. push `loader:status {phase:'loader-failed', message, error, startupStartedAt, timings}`
5. Restore, each wrapped in `.catch(()=>{})`: jukebox, shared presentation, camera names, scoreboards, ball cleanup, front menu, high-school digits, ball scale, `jukeboxManager.releaseCache()`, `modProfileManager.restoreIfSafe()`, `playerFolders().restore()`
6. Rethrow, so the renderer toasts the message
7. `finally { launchInProgress=false }`

Inside `versionProxySession.start`, a preflight failure first runs `cleanup()`, which waits for game exit, stops the provider and removes the DLL and session files, then emits `loader-failed` (`WS\core\version-proxy-session.js:315-318`).

#### 4. Build profiles
Old and Sep11 only. The resolver itself is signature-based and has no profiles.

#### 5. Rust must match
- [ ] Pressing Play while NBA2K27 is running shows "Close NBA 2K27 before starting a new session." and changes nothing.
- [ ] Previous-session leftovers are restored before new preparation starts.
- [ ] An unresolved build fails with "The current NBA 2K27 build is not safely resolved: …" before any file is changed.
- [ ] Files are prepared in this order: playlist → mod-profile junction → session monitor → camera names → scoreboards → ball → provider/DLL.
- [ ] Any failure before Steam spawns rolls back every prepared item and pushes `loader-failed` with the message.
- [ ] `startup-timings.json` (or an equivalent) records per-phase durations.

---

### 3. `launchGame`: arming helpers, Steam, and what happens after attach

#### 1. User-visible behavior
- Phase `launching-game` "Local provider ready. Launching NBA 2K27 without EAC…" is emitted just before the callback (`WS\core\version-proxy-session.js:300`).
- Steam is asked for launch option 1, labelled `"NBA 2K27 without EAC (offline only)"` (`WS\core\steam-launcher.js:45`).
- Steam errors:
  - "steam.exe was not found above the configured NBA 2K27 folder."
  - "NBA 2K27 was not found in the configured Steam library manifest."
  - "The Steam app manifest is incomplete."
  - "The NBA 2K27 without-EAC executable was not found in the configured game folder."
  - (`WS\core\steam-launcher.js:11,21,32,39`)

#### 2. Timing and gating
Callback order (`WS\main.js:1796-1814`):
1. `requireNativeHost()`.
2. `scalerManager.arm(gameRoot)`: not awaited. It polls every 500 ms while applying and every 3000 ms while observing, with a **120 000 ms** deadline. On timeout it shows "Player data was not ready in time. Retry apply is available here." (`WS\core\scaler-manager.js:42-52`).
3. `freestyleUnlocks.arm(gameRoot, unlockWnbaFreestyle)`: not awaited. It polls every 500 ms until bound, then every 1000 ms, then every 3000 ms after it applies. The deadline is **120 000 ms**; on timeout: "The game did not start in time. Unlocks will retry on your next launch." It only accepts a game process created after arm time ("Unlocks require a new game session started with Play.") (`WS\core\freestyle-unlocks.js:21-37`).
4. `await blacktopUnlocks.arm(gameRoot, unlockWnbaBlacktop)`.
5. `await debugMenus.arm(gameRoot)` (polls every 500 ms, then 3000 ms once applied, `WS\core\debug-menus.js:30-36`).
6. `await blacktopPresentation.arm(gameRoot)` (500 ms).
7. `await blacktopCrowd.arm(gameRoot)` (500 ms).
8. `await balls().arm(gameRoot)`.
9. `await gameCapture.start(gameRoot).catch(warn)`: the exact-handle monitor starts **before** Steam.
10. `requireNativeHost()` → `launchWithoutEac(gameRoot)`.
11. If a startup MP4 is selected, call `videoOverlay.showWhenGameReady(...)`, fire-and-forget ("Do not block provider readiness while waiting for a visible game window", `:1811`).
12. Register `Ctrl+Shift+F12` → `gameCapture.capture()` if not registered.
13. Return the launch result.

If any of steps 1-12 throws, the callback stops game capture, scaler, freestyle unlocks, blacktop unlocks, debug, presentation and crowd, cleans up the ball, and rethrows (`:1815-1823`).

**After Steam spawns** (background, `WS\core\version-proxy-session.js:301-313, 374-402`):
1. Wait ≤ 90 s (250 ms poll) for the armed log.
2. Readiness: a 20 s settle (game checked every ≤ 1 s), or AudioReadiness when a playlist is active.
3. Write the trigger.
4. Wait ≤ 180 s for the report with `LIVE_SESSION_ACTIVE=YES`.
5. `loader-active`.
6. Monitor every 500 ms until verified exit, then cleanup and restore.

Features that start **after attach**, driven by their own observers rather than the trigger:
- **Cameras.** `SessionManager.tick` runs every **1000 ms**. When `NBA2K27.exe` appears, it calls `cameraManager.start(modsRoot)` and `playerResearchManager.startAutomaticArenas(...)` (`WS\core\session.js:85-137`, `WS\main.js:504-510`). Messages: "NBA 2K27 is running and the integrated camera setup is active." or "NBA 2K27 is running, but camera activation failed: <err>". On game exit: "NBA 2K27 closed. Local loader remains armed for the next normal launch."
- **Outfit carry (PowerShell `session_saved_outfits.ps1`).** Starts only when the loader emits `loader-active`, and stops on `complete` / `stopped` (`WS\main.js:2067-2071`). Its readiness protocol is in `R\artifacts\wpf-version-dll-audit-20261017.md` §1a.
- **Startup video.** `GameWindow.exe --wait` (≤ **121 s**) waits for a visible game window. Then the overlay opens fullscreen and always-on-top, mutes or ducks the game via `GameVolume.exe --duck <pid> <exe>` (detached, so it survives launcher teardown), and arms the controller skip. A **615 000 ms** watchdog closes it. If the audio helper fails, the video closes ("audio-helper-failed") (`WS\core\video-overlay.js:13-50`, `WS\core\game-volume.js:8-52`). The initial volume comes from the cache file `Data\startup-video-volume.json`; the fallback is 0.25 (`game-volume.js:12-13`), with a 750 ms wait for the first live sample (`:50`).
- **Scoreboard, arena, ball and Blacktop auto-open** run their own long-lived observers, started at launcher boot (`WS\main.js:883-898, 249-266`). They are covered in other reference files.

#### 3. Mechanism (Steam)
- `resolveSteamGame` (`WS\core\steam-launcher.js:15-33`):
  - `steamRoot` = gameRoot/../../.. (so `steamapps\common\<game>` → `steamapps` → Steam root). It requires `steam.exe` there.
  - It scans `steamapps\appmanifest_*.acf` and picks the first whose `"name"` equals `nba 2k27` (case-insensitive) or whose `"installdir"` equals the game folder's name.
  - It parses `appid`, `name` and `installdir` with regexes.
- Launch: `spawn(steamExe, ['steam://launch/<appId>/option1'], {cwd: steamDir, detached:true, stdio:'ignore', windowsHide:false})`. It waits for the `spawn` event, then calls `unref()`. If the game is already running it returns `{alreadyRunning:true, launched:false}` without spawning (`:50-74`).
- Only the Steam library that contains the configured game folder is used. Secondary libraries are found only implicitly through that path.

#### 4. Build profiles
Steam launch is build-independent. The helpers have their own profiles (other files).

#### 5. Rust must match
- [ ] Memory helpers are armed **before** Steam starts and poll on their own, with 120 s game-find deadlines. Steam is never delayed by them.
- [ ] The game-exit monitor starts before Steam starts.
- [ ] Steam is launched with `steam://launch/<appid>/option1` via `steam.exe` from the library root above the game folder.
- [ ] Camera activation starts when the game process appears (1 s monitor), not when the loader becomes active.
- [ ] Outfit carry starts on `loader-active` and stops on loader `complete`/`stopped`.
- [ ] The startup video waits for a visible game window (≤ 121 s) and never blocks the loader.

---

### 4. Mod profile junction and player-folder links (pre-launch file changes)

#### 1. User-visible behavior
Errors:
- "Close NBA 2K27 before changing the active mod profile."
- "The selected mod-profile folder does not exist."
- "A mod profile cannot be stored inside the live mods folder. Choose a sibling such as modsfiba."
- "The game mods path is already a link or non-folder and was left untouched."
- "`.nba2k27-mod-launcher-default-mods` already exists. The launcher will not overwrite it."

(`WS\core\mod-profile-manager.js:79-93`)

Player-folder errors: "Player-folder conflict: <file> is in both "<a>" and "<b>". Keep one copy or put your preferred copy loose in char/sig." and "Player folders exceed the 10,000-file safety limit." (`WS\core\player-folder-session.js:48-49`).

#### 2. Timing and gating
- The profile is activated after playlist preparation and before the session monitor and provider (`WS\main.js:1761`).
- It is restored on game exit (step "Mod profile"), on Play failure, and at startup (`restoreIfSafe`, which refuses while the game runs).

#### 3. Mechanism
- **Mod profile** (`WS\core\mod-profile-manager.js:73-120`):
  - If the selected profile is `<game>\mods` itself, nothing happens.
  - Otherwise, in order:
    1. Write the marker `Data\active-mod-profile.json` (schema `nba2k27-session-mod-profile-v1`, stage `preparing`, including the original folder's dev/ino/birth identity).
    2. Rename `<game>\mods` → `<game>\.nba2k27-mod-launcher-default-mods`.
    3. Set stage `moved`.
    4. `symlink(selected, <game>\mods, 'junction')`.
    5. Set stage `active`.
  - Restore renames the backup back, only if the identity matches.
- **Player folders** (`WS\core\player-folder-session.js:36-70`):
  - Every `*.iff` in subfolders of `mods\char\sig\*` and `mods\players\*` is hard-linked into `mods\char\sig\`.
  - A loose file in `char\sig` wins. The journal `Data\player-folder-session.json` is written **before** the links are created.
  - Restore unlinks only files whose dev:ino still match the journal.

#### 4. Build profiles
n/a

#### 5. Rust must match
- [ ] The journal is written before any rename or link. Restore checks identity and never follows a replaced junction.
- [ ] Selecting `<game>\mods` as the active profile is a no-op.

---

### 5. Prepared playlist cache (launch speed)

#### 1. User-visible behavior
- Setting **Cache prepared playlists** (default on): "Keep the most recently played playlist ready for faster launches. Turning this off clears only the reusable cache after the game closes." (`WS\renderer\index.html:481`). Toggle toasts: "Faster-launch playlist cache enabled." / "Playlist cache disabled. Cleanup waits until the game is closed." (`WS\renderer\app.js:2767-2771`).
- Errors: "Prepared playlist cache changed. Relaunch to rebuild it.", "Playlist source changed while caching.", "An existing audio mod occupies soundtrack position N; it was left untouched.", "Not enough disk space for the temporary playlist files." (`WS\core\jukebox-session.js:130,135,149`, `WS\core\playlist-launch-cache.js:32`).

#### 2. Timing and gating
- Used only during Play, step 7. With caching off, `launchSource` calls `launchCache.retain([])` (deletes all verified entries) and materializes from the archives every launch (`WS\core\archive-jukebox.js:417-421`).
- Turning the setting off clears caches immediately only if the game is not running (`WS\main.js:961-964`). Otherwise the clearing happens at session end (restoration steps "Playlist cache" and "Audio conversion cache", `WS\main.js:2085-2091`) and at startup (`:1962, 1992`).
- At most 2 banks are processed in parallel (`boundedWork(banks,2,…)`, `archive-jukebox.js:424`).

#### 3. Mechanism
- Cache root: `Library\Music\archive-library\<sha256(lower(modsRoot))[0:16]>\launch-ready-v1\` (`archive-jukebox.js:40-51`).
- Entries: `<sourceHash>.wav` (native-wrapped PCM) and `<sourceHash>.json` = `{schema:1, sourceHash, hash, size}`.
- A read is valid only if all of these hold: meta ≤ 4096 B, `sourceHash` matches the key, size matches, 54 B ≤ size ≤ 256 MiB, and the full SHA256 matches. There is no partial trust. Linked directories are refused (`playlist-launch-cache.js:8-29`).
- `get(key, load)`: on a miss it re-hashes the decoded source; a change throws. `retain(keys)` deletes every verified entry not in `keys`, so only the most recent playlist is kept (`:30-47`).
- Only custom songs (bank > 71) use the cache. Stock banks only materialize artwork (`archive-jukebox.js:425-428`).
- A second cache, `Library\Music\game-session\native-pcm-v1\<hash>.wav`, keeps up to **256 MiB** (`RETAINED_AUDIO_BYTES`, `WS\core\jukebox-session.js:12, 86-101`), or 0 when caching is off.
- Session publication is always fresh. Cached audio is re-verified by SHA256 before each copy into the mods folder (`jukebox-session.js:128-136, 159-163`).
- Rust status: `SN\qa047-launch-speed-handover.md` and `R\artifacts\prepared-playlist-cache-gap-20261017.md` (`Cache\Jukebox\Prepared\prepared-v1.json`; measured warm 12.9-13.0 s vs ~16 s; there is no setting toggle or clear action yet).

#### 4. Build profiles
Bound to the jukebox build support (orig and Sep11 in WPF).

#### 5. Rust must match
- [ ] Reuse requires a full hash match, every time. Any mismatch rebuilds; nothing is partially trusted.
- [ ] Only the current playlist's entries are retained.
- [ ] Turning the setting off deletes only verified cache entries, and only when the game is not running (otherwise at session end).
- [ ] Saved library audio is never deleted as cache.

---

### 6. One game owner and launcher closing

#### 1. User-visible behavior
- See architecture §1 for the game-owner refusal message.
- Closing during a session: "Game session active" / "Close NBA 2K27 first." (`WS\main.js:461`).
- Closing modal (`#launcherClosing`, cannot be dismissed): its title comes from `launcher:closing {active, message}`: "Checking the game before closing…" and then "Closing launcher… Restoring session files and stopping helpers." It closes on `{active:false}` (`WS\renderer\launcher-closing.js:3-8`, `WS\main.js:444-447, 456, 2323`).

#### 2. Timing and gating
- Close is refused during `launchInProgress`, initialization, or while `versionProxySession.background` exists, which means the loader is still monitoring the game: "Wait for launcher initialization or game launch to finish before closing." (`WS\main.js:2309`).
- The game-owner pipe is held from backend start until headless cleanup completes or the process exits (`WA\Backend\host.cjs:69, 127-128`).

#### 3. Mechanism
- `net.createServer().listen('\\.\pipe\2kLauncher-Wpf-GameOwner-v1')`. EADDRINUSE → `GAME_OWNER_BUSY`. The updater runner also acquires it (`WA\Backend\app-update-runner.cjs:21`).

#### 4. Build profiles
n/a

#### 5. Rust must match
- [ ] Only one launcher (any copy, any folder) can own the game at a time.
- [ ] The closing modal shows during cleanup and cannot be dismissed.

---

### 7. Game volume helper

#### 1. User-visible behavior
None directly. It controls the startup-video volume and mutes the game while the intro plays.

#### 2. Timing and gating
- Waits ≤ **750 ms** for the first live volume sample (`WS\core\game-volume.js:50`).
- Stop: closes stdin, then waits **2000 ms** (mute/duck mode) or **800 ms** (watch). It never kills a ducking helper, because that helper owns mute restoration; instead it logs `game-audio-restoration-pending` (`:44-49`).
- A new `start()` is refused while a restoration is pending: "Previous intro audio restoration is still pending." (`:10`).

#### 3. Mechanism
- `GameVolume.exe --duck|--watch <launcherPid> <gamePath>` (`Runtime\<ver>\video`).
- stdout JSON: `{event:'volume', found, volume(0..1), muted}`, `game-audio-held|restored|restore-failed`.
- The last live volume is saved atomically to `Data\startup-video-volume.json` `{schema:1, gamePath, volume, muted}` (`:36-41`).

#### 4. Build profiles
Build-independent (it uses the Windows audio session).

#### 5. Rust must match
- [ ] A game muted for the intro is always restored, even if the launcher dies (a detached helper with a parent watchdog).

---

## Known divergences in current Rust port (from artifacts, not re-verified)
- Rust gates ARMED for ≤ 90 s, then creates the camera/scaler/Freestyle runners **after ARMED**, then a 20 s settle, then the trigger (`R\artifacts\wpf-version-dll-audit-20261017.md` Appendix A). WPF arms these helpers **before Steam starts** (§3). Check that this does not delay the first apply.
- Rust uses a fixed 20 s settle even with a playlist. WPF used AudioReadiness (D8).
- Play status polling in Rust is 5 s idle and 1 s while preparing or running (`R\artifacts\camera-scaler-launch-audit-20261017.md`). WPF is push-only for the overlay, with the 1 s game monitor.
- Rust's player-link preparation does 738 activation scans (5.4 s) on a 361-file fixture (`camera-scaler-launch-audit`). WPF does one hard link per file with no per-link process scan.
- The launch overlay in Rust shows phase and elapsed time only, with no stage list. "Stop waiting" is partial (`R\artifacts\feature-gap-inventory-20261017.md:24,26`).
- The prepared-playlist cache exists in Rust 0.47 (`SN\qa047-launch-speed-handover.md`), but there is no Settings toggle or clear action.

## Open questions / UNVERIFIED
- What `balls().prepare/arm`, `scoreboardLayouts.prepare` and `cameraManager.namesSession.prepare` write is not detailed here; see their feature files.
- `blacktopUnlocks.arm` poll intervals were not read (it spawns a helper, `WS\core\blacktop-unlocks.js:8`).
- WPF does not re-launch or retry Steam if the game never appears. Only the 90 s ARMED wait fails ("NBA 2K27 did not produce an armed report within 90 seconds."). The game-capture monitor separately reports "No matching game process appeared within 90 seconds."
