# Architecture, host, session loader and restore-on-exit — WPF reference
Source build: WPF quality-pass-qa-20261010 (frozen). Last researched: 2026-10-17.

Path abbreviations (all relative to `C:\Users\carrn\CodexProjects\2kconversiontool`):
- **WH** = `nba2k27-launcher-wpf\` (C# host source; `App.xaml.cs:119` writes version `0.1.0-wpf.24`, matching `WA\release.json`)
- **WA** = `nba2k27-launcher-wpf\release\quality-pass-qa-20261010\App\` (frozen build)
- **WS** = `WA\Reference\src\` (JS game/session backend that runs inside Node)
- **VP** = `nba2k27-version-proxy\src\` (version.dll source; hash-proven to be the shipped revision, see `R\artifacts\wpf-version-dll-audit-20261017.md` §0)
- **R** = `nba2k27-launcher-tauri2\`

`WA\Backend\host.cjs` and `WH\worker\host.cjs` are byte-identical (checked with `diff`, 2026-10-17).

## Summary
- WPF is **three processes**: a C# WPF/WebView2 window host (`2k Launcher WPF.exe`), one child `node.exe` running `Backend\host.cjs` that loads the old Electron `main.js` unchanged behind a fake `electron` module, and the WebView2 renderers. All game logic is in Node. C# only owns windows, dialogs, hotkeys, displays and process queries.
- Renderer → main goes `bridge.js` (`chrome.webview.postMessage`) → `NativeWindow.OnMessage` → stdin JSON line to Node → `ipcMain.handle` handler. The reply returns as a stdout JSON line → `NativeBridge.ReadOutput` → `PostWebMessageAsJson`.
- Mods reach the game through a **search-order `version.dll` proxy** that is copied into the game folder for each Play session, configured by `nba2k27_provider_audit.cfg` (game folder) and activated with a trigger file. The DLL serves missing assets from a named-pipe provider (`NBA2K27ProviderHost.exe`).
- Restore on exit uses one shared, retried, game-exit-gated restoration batch, run on game exit (`VersionProxySession.onCleanup`), on launcher close (`before-quit`) and on host loss (headless recovery).
- **Top 3 port breakers:**
  1. The trigger handshake. The DLL writes `..._armed.log` and then polls for `..._audit.trigger` once per second for 600 s. The launcher must write the trigger only after the readiness gate (20 s settle, or AudioReadiness when a playlist is active). It must never delete session files or the DLL while the game might still be alive.
  2. Game-exit proof. A provider or session file is removed only after a **verified** exit: the exact-handle monitor says exited **and** a fresh process list agrees, or there are 3 fresh absent checks spanning at least 1 s. Unknown counts as running.
  3. Close and host-loss semantics. While NBA2K27 runs, the launcher **refuses to close** ("Close NBA 2K27 first."). If the window host dies, the backend keeps the session and restores headlessly once the game exits.

## Key source map
| Concern | file:line |
|---|---|
| Single-instance mutex, portable folder creation, starting Node | `WH\App.xaml.cs:48-61` |
| Portable directory validation (no reparse points) | `WH\PortableStorage.cs:265-283` |
| Spawning `node.exe Backend\host.cjs` with env vars | `WH\NativeBridge.cs:26-48` |
| Node exits → WPF shuts down with the same code | `WH\NativeBridge.cs:43` |
| stdout message dispatch (`host`, `reply`, `push`, `quit`, `startup-refused`) | `WH\NativeBridge.cs:74-102` |
| Host services (windows, dialogs, open-external allowlist, hotkey, process query) | `WH\NativeBridge.cs:113-161` |
| Protocol (`preview-image://`) round trip, 20 s timeout | `WH\NativeBridge.cs:199-206` |
| Per-message fault isolation | `WH\BridgeMessagePump.cs:7-14` |
| WebView2 window: virtual host, security settings, resource interception | `WH\NativeWindow.cs:91-153, 247-292` |
| Renderer crash → "Reload interface" UI that keeps the session | `WH\NativeWindow.cs:299-323` |
| Renderer IPC shim (`ipcRenderer`, `contextBridge`) | `WA\bridge.js:1-21` (`WH\bridge.js`) |
| Fake Electron (`app`, `BrowserWindow`, `dialog`, `shell`, `screen`, `globalShortcut`) | `WA\Backend\host.cjs:16-84` |
| Host-loss detection (stdin close, stdout error, parent PID poll 1 s) | `WA\Backend\host.cjs:70-78, 119-122` |
| Exclusive game-owner pipe before `main.js` loads | `WA\Backend\game-owner.cjs:5-16`, `host.cjs:124-134` |
| IPC wrapper: trusted sender, `nativeClosing`, per-feature build gate | `WS\main.js:12-28` |
| Boot sequence (`app.whenReady`) | `WS\main.js:1863-2232` |
| Session loader (version.dll install, cfg, trigger, monitor, cleanup) | `WS\core\version-proxy-session.js:123-487` |
| DLL file names and the 600 s trigger poll | `VP\version_proxy.cpp:51-55, 2100-2147` |
| DLL cfg parser (≤ 2047 bytes, keys) | `VP\version_proxy.cpp:386-470` |
| DLL diagnostics-directory routing | `VP\session_diagnostics.hpp:30-131`, `VP\version_proxy.cpp:1366-1378` |
| Process check (native probe, or tasklist fallback, 500 ms cache) | `WS\core\session.js:8-44` |
| Session monitor (1 s tick, camera start) | `WS\core\session.js:46-147` |
| Build detection and per-feature support list | `WS\core\game-compatibility.js:3-35` |
| Game window wait helper | `WS\core\game-window.js:3-17` |
| Game capture / exact-handle presence monitor | `WS\core\game-capture.js:21-96` |
| Restoration batch runner | `WS\core\session-restoration.js:3-13` |
| Game-exit restoration list | `WS\main.js:2072-2100` |
| Close / quit cleanup list | `WS\main.js:2238-2271, 2294-2357` |
| Host-loss headless recovery | `WS\core\host-loss-cleanup.js:4-16`, `WS\main.js:2272-2289` |
| Exit gate (before-quit) | `WS\core\exit-handler.js:5-34` |
| Storage layout | `WS\core\launcher-storage.js:18-46` |
| Runtime resource extraction (hash-verified) | `WS\core\runtime-resources.js:28-48` |

---

### 1. Process model and the renderer ↔ main ↔ native bridge

#### 1. User-visible behavior
- Starting a second copy from the **same portable folder** does nothing: the mutex `Local\2kLauncherWpf-<first 20 hex of SHA256(upper(PortableFolder))>` is already held, so the second copy calls `Shutdown()` silently (`WH\App.xaml.cs:49-51`).
- Starting a copy from a **different** portable folder while another backend owns the game shows a message box titled **"2k Launcher — session already owned"** with this text: "Another launcher or recovery worker owns NBA 2K27. Close the other launcher and game, then allow session cleanup to finish before retrying." The copy then exits with code 2 (`WA\Backend\game-owner.cjs:9`, `WH\NativeBridge.cs:90-93`).
- If startup itself fails (missing .NET 8 Desktop or WebView2), a message box shows: "Could not start the WPF launcher.\n\n<error>\n\nThis build needs .NET 8 Desktop and Microsoft Edge WebView2." (`WH\App.xaml.cs:67`).
- If the main renderer process crashes, the window is replaced by the text "The launcher interface stopped. Your game session is still managed." and a **Reload interface** button. Reload recreates only the WebView. It never replays a request or launches the game (`WH\NativeWindow.cs:305-315`).
- Main window: title "2k Launcher", 1320×860, minimum 1060×680, background `#090b10`, hidden until `ready-to-show` (`WS\main.js:904-928`).

#### 2. Timing and gating
- C# polls displays and the cursor every **500 ms** and pushes `displays` / `cursor` messages only when they change (`WH\NativeBridge.cs:46-57`).
- Node polls the parent PID every **1000 ms**; ESRCH means host loss (`WA\Backend\host.cjs:122`).
- Every `ipcMain.handle` call, in order (`WS\main.js:18-27`):
  1. Rejects untrusted senders ("Unknown launcher window.").
  2. Rejects during close ("Launcher cleanup is in progress. Wait for it to finish.").
  3. Runs `assertGameFeature` (build compatibility, see §4).
  4. Rechecks the first two.
- Node also refuses `game:launch-without-eac` and `session:start` while a launcher update is busy or pending: "Finish or cancel the pending launcher update before starting the game." (`WA\Backend\host.cjs:104`).
- Timeouts: WebView navigation **25 s** (`WH\NativeWindow.cs:195`). `Evaluate` **190 s** (`:210`). Protocol request **20 s** (`WH\NativeBridge.cs:204`). C# queues at most **500** pushes per window while a page is loading (`WH\NativeWindow.cs:242`).

#### 3. Mechanism
- **Start-up order (C#)** (`WH\App.xaml.cs:48-61`):
  1. PortableFolder = the parent of the App folder.
  2. Acquire the mutex.
  3. `PortableStorage.Validate`: the root and its `Data`, `Library`, `Library/Music|Videos|Preview Packs|Outfits`, `Cache`, `Cache/Temp|Chromium|WebView2`, `Runtime`, `Recovery` and `Logs` must be regular directories (no reparse point), and `Data/settings.json` must be a regular file.
  4. Create `Data, Library, Cache, Logs, Recovery, Runtime, Cache/Temp`.
  5. Start `NativeBridge`.
- **Node child** (`WH\NativeBridge.cs:28-41`): `App\Backend\node.exe App\Backend\host.cjs` with UTF-8 pipes, cwd App, and these environment variables:
  - `WPF_ROOT=<portable>`, `WPF_APP=<App>`, `WPF_PARENT_PID`
  - `WPF_DISPLAYS=<json>`, and `WPF_SMOKE=1` in smoke runs
  - `TMP`/`TEMP=<portable>\Cache\Temp`
- **Wire protocol.** Newline-delimited JSON in both directions. Node → C# kinds:
  - `host` {id, method, args, window}: request a native service; the answer comes back as `host-reply` {id, value, error}.
  - `reply` {window, session, id, value|error}: answer to a renderer invoke.
  - `push` {window, channel, value}: `webContents.send`.
  - `quit` {code}, `fatal`, `startup-refused`, `protocol-reply`.
  - (`WA\Backend\host.cjs:13,17,30,104,117`; `WH\NativeBridge.cs:83-97`)
- **C# → Node kinds:** `invoke` {window, session, id, channel, args}, `event` {window, name: focus|blur|move|resize|close|render-process-gone}, `key` (Escape only), `hotkey`, `cursor`, `displays`, `performance` {name:'startup', ms}, `close-app`, `protocol`, `host-reply` (`WA\Backend\host.cjs:99-118`).
- **Native host methods:**
  - `process-running`, `create-window`, `load`, `load-url`, `execute`
  - `bounds`, `topmost`, `focusable`, `show`, `hide`, `focus`, `raise`, `restore`, `minimize`, `maximize`, `fullscreen`, `enabled`, `audio-muted`, `destroy`
  - `open-dialog`, `save-dialog`, `message-dialog`, `open-path`, `open-external` (only http/https/steam schemes), `reveal`, `clipboard`
  - `hotkey-register` (only `CommandOrControl+Shift+F12`; F8 is handled elsewhere)
  - (`WH\NativeBridge.cs:118-158, 209-215`)
- **Renderer side.**
  - Pages load from the virtual host `https://launcher.local/` mapped to the App folder (`WH\NativeWindow.cs:134`).
  - `bridge.js` plus the page's Electron preload (wrapped so that `require('electron')` returns `window.__wpfRenderer`) is injected with `AddScriptToExecuteOnDocumentCreated` (`WH\NativeWindow.cs:163-170`).
  - Each page load gets a random `session` UUID, so replies from a previous page are dropped (`WA\bridge.js:3,11`).
  - Pages can only load files inside App, and preloads only from `Reference\src` (`WH\NativeWindow.cs:158,167`).
  - Remote pages are only allowed for `2kdb.net` and `2kauction.com` over https (`:181`).
  - Dynamic files are served from `https://launcher-resources.invalid/__file/<token>` (tokens issued only for paths in trusted backend replies, `RendererMessageJson.Prepare`) and from `/__preview/<key>` → the Node `preview-image://` protocol (`WH\NativeWindow.cs:244, 260-288`).
- **Window close.** Clicking the X is cancelled in C#, which sends `event close` to Node. Node runs `BrowserWindow.close()`, which runs `protectWorkspaceClose` (§7). Only Node's `destroy` sets `ForceClose` (`WH\NativeWindow.cs:61-66`, `WH\NativeBridge.cs:157`).
- **Typed arrays** are converted to plain arrays and BigInt to string before JSON encoding (`WA\Backend\host.cjs:13`). This was needed because WebGL received empty geometry without it.
- **Node startup** (`WA\Backend\host.cjs:86-134`):
  1. Replace `resolvePortableStorage` with `resolveContainedStorage(root)`.
  2. Set `PORTABLE_EXECUTABLE_DIR` and `WPF_NATIVE_HOST=1`.
  3. Install the native process probe.
  4. Create `AppUpdates`.
  5. Acquire the exclusive pipe `\\.\pipe\2kLauncher-Wpf-GameOwner-v1` (smoke runs use `-qa-<hash>`).
  6. Only then `require(main.js)`.

#### 4. Build profiles
Not build-specific.

#### 5. Rust must match
- [ ] A second launcher from the same folder exits silently. A launcher from another folder, while one owns the game, shows "Another launcher or recovery worker owns NBA 2K27…" and exits without touching game files.
- [ ] A crashed UI never stops the session or provider. A reload never re-sends Play.
- [ ] Every command checks sender, closing state and build compatibility before it runs.
- [ ] Play is refused while a launcher update is pending.
- [ ] `open-external` only accepts http, https and steam URLs.

---

### 2. main.js boot order

#### 1. User-visible behavior
- On an ordinary start the main window is created **before** recovery runs (`createWindow({preparePanel:false})`, `WS\main.js:1941`). It shows its own loading overlay (`#appStartup`, cleared in `WS\renderer\app.js:3008`).
- The renderer's first `app:bootstrap` call waits on `initializationBarrier`, so the UI can never see a half-recovered session (`WS\main.js:376-383, 936-937`).
- On a startup failure, the window is still created (one bounded attempt) and bootstrap returns the actionable error (`WS\main.js:2223-2231`). The renderer shows the toast "Startup needs attention: <msg>" (`WS\renderer\app.js:3007`).

#### 2. Timing and gating
The exact order of `app.whenReady` (`WS\main.js:1863-2222`). Each recovery step is `.catch(console.warn)` unless noted.
1. Storage migration prompts (portable import or AppData). **Not reachable in WPF**, because the host forces `resolveContainedStorage`, which always returns `needsPortableMigration:false` (`WS\core\launcher-storage.js:44`). The portable branch writes an empty `settings.json` when it is missing (`WS\main.js:1887-1891`).
2. Preview-image resolver and the `preview-image` protocol (`:1904-1938`).
3. `createWindow({preparePanel:false})` (`:1941`).
4. `retiredFrontMenuRecovery.restore()` (`:1942-1943`).
5. `ensureRuntimeResources` → `<root>\Runtime\<version>` (`:1945`). Every file in `Reference\runtime-resources\manifest.json` is size- and SHA256-verified and atomically copied. Under WPF older versions are **not** pruned (`WS\core\runtime-resources.js:28-47`).
6. `prepareBlacktopPanel()` (F8 listener, auto-open detector) (`:1946`).
7. Create `VideoOverlay`, with `GameWindow.exe`, `GameVolume.exe` and `ControllerSkip.exe` (`:1947-1952`).
8. `jukeboxSession.restore()` (`:1958`).
9. If the game is **not** running: clear the conversion cache, run `cleanupDiagnostics` and run `cleanupTemporaryFiles` (`:1961-1965`).
10. Pick the jukebox library root, create `JukeboxManager`, and clear the launch cache if the cache setting is off (`:1966-1992`).
11. Create the player-research, multi-config and camera managers. Then `cameraManager.namesSession.restore()` (`:1993-2012`).
12. `scoreboardLayouts.restore()`, `highschoolDigitProbe.restore()`, `ballScaleRecovery.restore()` (`:2013-2018`).
13. `scalerManager.load()` + `recover(gameRoot)` (not caught: a failure here fails startup) (`:2019-2023`).
14. Create the unlocks, debug, presentation and crowd helpers, and publish restored states (`:2024-2044`).
15. `cameraManager.ensureInstalled()` (`:2045`).
16. `ModProfileManager`; `SharedPresentationSession.restore()` (`:2046-2051`).
17. `BlacktopSession` (outfit carry PowerShell) (`:2052-2057`).
18. `balls()`, `ballSelectionManager.cleanup()`, `liveSelector.recover()` (`:2058-2060`).
19. Construct `VersionProxySession` with `onStatus` / `onCleanup` (`:2061-2101`).
20. `modProfileManager.restoreIfSafe()` (an error is shown as session status "Mod-profile recovery needs attention: …") and `playerFolders().restore()` (`:2204-2207`).
21. `initializationFinished=true; releaseInitialization()` (`:2208`).
22. If the host was not lost: schedule an animation warm-up after **5000 ms**, start the observe-only probe, then `session.start(settings)` (the 1 s game monitor) (`:2209-2222`).

#### 3. Mechanism
- Settings live in `<root>\Data\settings.json`. `readSettings()` caches them. `writeSettings()` serializes writes through a promise tail and writes a temp file, fsyncs it, then renames it. `provider:'local'` and `startCameras:true` are always forced (`WS\main.js:810-844`).
- Every `send(channel,…)` also feeds `launcherHealth.observe` (`WS\main.js:846-849`).

#### 4. Build profiles
n/a

#### 5. Rust must match
- [ ] The UI cannot get bootstrap data until every startup recovery step has finished.
- [ ] Startup recovery order: front-menu → runtime resources → jukebox → caches/diagnostics (only when the game is not running) → camera names → scoreboards → ball scale → scaler → helpers → shared presentation → ball → mod profile → player folders.
- [ ] Runtime helpers are hash-verified before use. A mismatch fails startup with "Bundled runtime resource failed verification: <name>".

---

### 3. version.dll session loader: install, session files, handshake, cleanup

#### 1. User-visible behavior
Loader phases are pushed on `loader:status` with these messages (`WS\core\version-proxy-session.js`):

| phase | message | line |
|---|---|---|
| `preparing-loader` | "Checking the game build and preparing the local missing-asset route…" | 193 |
| `launching-game` | "Local provider ready. Launching NBA 2K27 without EAC…" | 300 |
| `waiting-for-game` | "Waiting for NBA 2K27 to load the verified session proxy…" | 375 |
| `loader-armed` | "Missing-asset route armed. Waiting 20 seconds for game startup to settle…" (or, with a playlist: "Waiting for verified file-manager readiness (read-only startup test)…") | 354, 349 |
| `activating-loader` | "Activating stock-first lookup for assets missing from the game manifest…" | 382 |
| `loader-active` | with a playlist: "Ready. Your selected playlist is active; unselected soundtrack songs are blocked."; otherwise: "Unique assets are active. Stock files stay stock except explicitly enabled launcher assets such as camera names." | 387-389 |
| `waiting-for-game-exit` | "Keeping game resources active until NBA 2K27 exit is confirmed." | 438 |
| `complete` | "NBA 2K27 closed. The local provider and session files were restored cleanly." | 309 |
| `loader-failed` | "Unique-asset route stopped safely: <err>" (background failure) or the raw error (preflight failure) or "Session cleanup needs attention: <err>" | 305, 317, 311 |
| `stopping-loader` / `stopped` | "Stopping the local unique-asset provider…" / "Unique-asset session stopped." | 480, 484 |

Preflight error strings (all fail **before** the DLL is copied):
- "NBA 2K27 is already running. Close it, then use this Play button so the loader can start first." (`:190`)
- "The launcher data location hint is missing or does not match its active library. Restart the launcher before Play." (non-portable only, `:201`)
- "NBA2K27.exe was not found." / "The configured mods folder was not found." / "The bundled session loader is missing." (`:216-218`)
- "The bundled session loader failed its integrity check." (`:222`)
- "A different version.dll is installed. Nothing was overwritten." (`:226`)
- "The mods folder needs at least one file from 1 byte to 64 MB for the safe startup check." (`:116`)
- "The local provider index changed during startup." (`:242`)
- "Prepared player selector is unavailable through the provider." / "The explicit override <p> was not found in the mods folder." / "Prepared scoreboard layout is missing." / "…unavailable through the provider." (`:252-271`)
- "Session configuration exceeds the native size limit." (`:296`)

Background (after Steam started) errors:
- "NBA 2K27 did not produce an armed report within 90 seconds." (`:377`)
- If the DLL's final report appears instead of the armed log, its text is shown verbatim, or "The session loader failed closed." (`:326-329`)
- "NBA 2K27 exited before the route activated." (`:357`)
- "The missing-asset route did not return an activation report." (`:384`)
- "The proxy rejected session activation." (`:386`)
- "NBA 2K27 exited before the missing-asset route finished activating." (`:331`)

#### 2. Timing and gating
- `start()` validates the inputs and rejects with "A unique-asset session is already active." if a session is running (`:185`). It checks that the game is not running (`:189`). Then, in order:
  1. Verify the bundled DLL hash.
  2. Verify that the installed `version.dll` is absent or a known launcher hash.
  3. Pick the preflight asset and hash it.
  4. Start the provider (startup timeout **30 s**, `WS\core\production-provider.js:7`).
  5. Query the asset size through the provider.
  6. Verify selectors, overrides and scoreboard layouts through the provider.
  7. Delete stale session files.
  8. Copy the DLL, then write the cfg.
  9. `emit launching-game`.
  10. `await launchGame()`: the callback arms helpers and starts Steam. See launch-play.md.
  11. Start the background `activateAndMonitor()` and return immediately (`:215-314`).
- `activateAndMonitor()` (`:374-402`):
  1. Wait up to **90 000 ms** for `armed`, polling **250 ms**. If the `report` file appears first, that is a fail-closed failure (`:322-337, 376`).
  2. `waitForStartupReadiness()` (`:339-360`):
     - Normal Play: a **20 000 ms** settle (`activationDelayMs` default, `:125`). The game is checked every ≤ **1000 ms**, and an exit throws.
     - With a playlist (`audioPolicy` set), it runs `NBA2K27AudioReadiness.exe --armed-report <armedPath>` instead, with a **65 000 ms** exec timeout (`WS\core\audio-readiness.js:10`). The helper itself loops for ≤ **60 000 ms** at **50 ms**, needing the manager pointer stable for **1000 ms** with stock default id `0xC7E442DC` at `manager+0x1E0` (`VP\audio_readiness.cpp:29,59-75`). The result must be `ready:true, memoryWrites:0, gameCalls:0` or it throws "Audio startup readiness did not pass." (`WS\core\audio-readiness.js:12`). The wiring is in `WS\main.js:1753-1756`.
     - `earlyActivation` (override `gooey_scenes/startup_boot_transition.iff`) skips the delay. It is never used in production (`:168, 340-345`).
  3. Delete the old trace. If the `report` file does not exist yet, write the trigger `trigger\r\n` (ASCII) (`:380-381`).
  4. Wait up to **180 000 ms** for the report (250 ms poll; game must stay running) (`:383`).
  5. The report must contain the line `LIVE_SESSION_ACTIVE=YES` (regex `^LIVE_SESSION_ACTIVE=YES\s*$`, multiline) (`:386`).
  6. `emit loader-active`. Then loop every **500 ms** while `gameMayBeRunning()`, sampling audio-route diagnostics every **2000 ms** (`:390-401`).
- **Game-exit proof** `gameMayBeRunning()` (`:410-434`):
  - Exact-handle presence `true` → running.
  - Otherwise a fresh process query. Running, or presence `true` on the re-read → running.
  - Presence `false` (verified handle exit) plus the fresh query saying absent → exited.
  - Without monitor evidence: exited only after **3 absent checks spanning ≥ 1000 ms**.
  - Any query error → treated as **running** (`processCheckError` is recorded).
- **Cleanup** (`cleanupOnce`, `:442-476`). Shared and retried: on failure `cleanupPromise` resets (`:404-408`).
  1. `waitForGameExit()` (500 ms poll).
  2. Archive logs (gzip) to `Logs\session-history`.
  3. `waitForGameExit()` again.
  4. `provider.stop()`. Returning `false` throws "Unique-asset provider cleanup is still pending."
  5. Delete the 5 session files. `.log`/`.trace` files are kept when evidence archiving failed.
  6. Delete `version.dll` only if `managedProxy` is set **and** its hash is in `KNOWN_PROXY_HASHES`.
  7. Loop: `waitForGameExit()` → `onCleanup()` (the main.js restoration list), retrying every 500 ms on `GAME_SESSION_ACTIVE` / `PROCESS_QUERY_FAILED`.
  8. `cleanupDiagnostics`.
  9. Rethrow any DLL-removal error.
- `stop()` sets `cancelled`, emits `stopping-loader`, awaits `cleanup()` (which still waits for game exit), waits ≤ **2000 ms** for the background task, then emits `stopped` (`:478-486`).

#### 3. Mechanism
**Files:**

| File | Location | Writer | Notes |
|---|---|---|---|
| `version.dll` | `<gameRoot>\version.dll` | launcher `fsp.copyFile(proxySource,…)` (`:297`) | Source `<root>\Runtime\<ver>\session-loader\version.dll` (`WS\main.js:730-734`). SHA256 must be `ECEF8C6C856A60AF3387BE50D76BD30B53A80F80F404476F5C31D5A5453E91DF` (`SCOREBOARD_TEXT_PROXY_SHA256`, `:15`, selected at `:222` because `scoreboardTextTest:true`). |
| `nba2k27_provider_audit.cfg` | **game folder** (always) | launcher, ASCII, CRLF, written **after** the DLL copy (`:276-299`) | The DLL reads it next to `NBA2K27.exe` (`VP\session_diagnostics.hpp:43`, `make_sibling_path` `VP\version_proxy.cpp:1374-1378`). |
| `nba2k27_version_proxy_audit.trigger` | **game folder** | launcher (`:381`) | The DLL deletes it after detection (`VP\version_proxy.cpp:2133`). |
| `nba2k27_version_proxy_armed.log` | `<root>\Logs\game-session\` | DLL | Path routed via `PORTABLE_LOG_DIR_HEX` (`VP\version_proxy.cpp:1369-1372`). The launcher redirects `paths.armed` to `dirname(audioDiagnosticsPath)\game-session` = `Logs\game-session` (`:207-213`, `WS\main.js:2063`). |
| `nba2k27_version_proxy.log` (final report) | `Logs\game-session\` | DLL | Contains `LIVE_SESSION_ACTIVE=YES/NO`. |
| `nba2k27_version_proxy_audit.trace` | `Logs\game-session\` | DLL | checkpoint lines `"<tick> <msg>"` (`VP\version_proxy.cpp:1395-1414`) |
| `last-loader-failure.json` | `Logs\` | launcher on background failure | schema `nba2k27-loader-failure-v1`, first 64 KiB of armed/trace/report (`:362-372`) |

**cfg keys, in write order** (`:276-295`):

```
PIPE=\\.\pipe\NBA2K27ModProvider-<launcherPid>-<uuid>
PATH=<logical path of preflight asset>        (prefers char/sig/png10000.iff; else first file 1 B..64 MiB, BFS, name-sorted)
SIZE=<bytes>
SHA256=<UPPERCASE hex>
PORTABLE_LOG_DIR_HEX=<each UTF-16 code unit of <root>\Logs\game-session as 4 uppercase hex digits>   (portable only)
OVERRIDE=… OVERRIDE_2=… OVERRIDE_3=… OVERRIDE_4=…   (production: only OVERRIDE=loc/english.iff when camera names installed, WS\main.js:1783)
SELECTOR_OVERRIDE=1                (when multi-config produced selectors, WS\main.js:1793)
SCOREBOARD_TEXT_TEST=1             (always in production, WS\main.js:1794)
AUDIO_SLOT_COUNT=71                (playlist)
AUDIO_ALLOWED_MAIN=<71 x 0/1> AUDIO_ALLOWED_PARK=<54 x 0/1> AUDIO_CUSTOM_MAIN=<71 x 0/1>
FREESTYLE_PLAYLIST=992 FREESTYLE_SELECTION=<71 x 0/1>   (usePlaylistInFreestyle)
<trailing empty line>
```

- Total size must be ≤ **2047 bytes** (launcher `:296`; the DLL rejects 0 bytes or > 2047, `VP\version_proxy.cpp:393-396`).
- The DLL fails closed on any invalid value: masks must be valid, `AUDIO_SLOT_COUNT` must be `71` when masks are present, and every custom bit must also be set in main. `FREESTYLE_PLAYLIST` requires custom == main, and `FREESTYLE_SELECTION` must hold 1-58 bits, all inside custom (`VP\version_proxy.cpp:430-470`).
- `PORTABLE_LOG_DIR_HEX` must decode to `<X:\…>\Logs\game-session`, where the root has regular `App`, `Data` and `Data\settings.json`. If not, diagnostics are disabled (no fallback once the key is present) (`VP\session_diagnostics.hpp:37-83`).

**DLL side (in game)** (`VP\version_proxy.cpp:2432-2439, 2100-2147`):
1. DllMain → `CreateThread(write_load_report)`.
2. Resolve the provider layout from the exe on disk (structural byte signatures) and check that there is no EasyAntiCheat module.
3. If OK, write the armed log with these keys:
   - `SCHEMA`, `PID`, `TARGET`, `PROXY`, `IDENTITY=PASS`, `EXE_TIMESTAMP`, `IMAGE_SIZE`
   - `ANTI_CHEAT=ABSENT`, `STRUCTURAL_RESOLVER=PASS`, `AUDIT_STATE=ARMED`
   - `GAME_MEMORY_WRITES=0`, `GAME_CODE_HOOKS=0`, `PROVIDER_REGISTRATION=OFF`, `ASSET_REPLACEMENT=OFF`
4. Poll for the trigger: **600 × Sleep(1000)**. If it never appears, the thread exits silently.
5. Delete the trigger and trace `MANUAL_TRIGGER_DETECTED`.
6. `run_registration_audit`: real-asset SHA256 check through the pipe.
7. `run_live_route` (only if the audit passed).
8. Write the final report.

If the resolver fails or EAC is present, it skips the armed log and goes straight to the audit/report (failure path). The launcher sees the report before the armed log and fails closed. The details of the live route and fallback are in `R\artifacts\wpf-version-dll-audit-20261017.md` rows 2-15.

**Foreign DLL policy.**
- Install only when `version.dll` is missing or its hash is one of the **40** `KNOWN_PROXY_HASHES` (`:18-59`). There is no backup of a foreign DLL.
- Remove it only if still managed and still a known hash (`:460-465`).
- DXGI/D3D12 add-ons are never touched (`:219-220`).

**Provider host.**
- `NBA2K27ProviderHost.exe` (from `Runtime\<ver>\provider-host`) is spawned. One stdin JSON line is sent: `{pipeName, token, modsRoot, aliases, mutableAliases}`.
- It must print a first stdout line containing `{"ready":true,…}` within **30 s**. Stop timeout is **5 s** (`WS\core\production-provider.js:7-8, 34-70`).
- Before Play, the build layout is checked with `NBA2K27ProviderResolver.exe <exe>` (timeout **30 s**). The result is cached in `<root>\Cache\provider-layouts\<TS>-<SIZE>.json`, keyed on exe path, size and mtime (`WS\core\provider-layout-resolver.js:61-130`, `WS\main.js:764-808`).
- If the resolver says anything other than `ready`, Play fails with "The current NBA 2K27 build is not safely resolved: <message>" (`WS\main.js:1740-1742`).

#### 4. Build profiles
- Bundled DLL profiles: original `6A974D13/368C3000` and Sep 11 `6AA2013D/36558000`. The provider route itself is profile-free (signature resolver).
- The freestyle profile and AudioReadiness are **orig + Sep11 only** (`VP\audio_readiness.cpp:14-17`).
- V2: Rust embeds the same DLL for legacy and a V2 DLL = WPF source + bridge hunks. See `R\artifacts\wpf-version-dll-audit-20261017.md` §0 and rows 4, 17 (freestyle missing on V2), D8 (fixed 20 s instead of AudioReadiness).

#### 5. Rust must match
- [ ] Copy the DLL, then write the cfg, then start Steam, in that order. Never start Steam before the cfg exists.
- [ ] The cfg is ASCII with CRLF, ≤ 2047 bytes, keys exactly as above, and `PORTABLE_LOG_DIR_HEX` uses UTF-16 code units as 4-hex.
- [ ] Wait ≤ 90 s for `AUDIT_STATE=ARMED`. A final report appearing first is a failure that shows its text.
- [ ] Write the trigger only after the 20 s settle (or a passing AudioReadiness), and only if the final report does not already exist.
- [ ] Wait ≤ 180 s for the report. Anything but `LIVE_SESSION_ACTIVE=YES` → "The proxy rejected session activation."
- [ ] A foreign `version.dll` blocks Play with "A different version.dll is installed. Nothing was overwritten." and is never deleted.
- [ ] The provider, session files and DLL are removed only after verified game exit. A failed cleanup is retried.
- [ ] After a background failure, the game keeps running unmodded, and cleanup still waits for its exit.

---

### 4. Game build detection (game-compatibility.js)

#### 1. User-visible behavior
- Settings → "Diagnostics & compatibility" shows `<TS:SIZE> · <message>` and a per-feature table: **Recognized** / **Unavailable** plus a reason (`WS\renderer\launcher-quality.js:15-16`).
- Messages:
  - "Compatibility checked against the native helper build guards."
  - "Unrecognized game update. Library management and recovery remain available."
  - "Choose the installed NBA 2K27 folder." (exe missing)
  - per-feature: "Build recognized; native safety checks still apply." / "This game build is not supported by this feature. No new edit will be attempted."
  - (`WS\core\game-compatibility.js:30-32`)
- A gated command on an unsupported build throws `"<Label>: <reason>"`, for example "Game session: This game build is not supported…" (`:34`).
- In the renderer, buttons for unsupported features (`launchGame`, `startSession`, `applyScaler`, the ball/arena/scoreboard menus, `openBlacktopRules`, `editBlacktopPlayer`) get `aria-disabled` and the reason as their title, and their clicks are swallowed (`WS\renderer\launcher-quality.js:19,22,25`).

#### 2. Timing and gating
- Runs before every IPC call that maps to a feature (`WS\main.js:17`). The result is cached on `[file, size, mtimeMs, ctimeMs]` (`:28`).
- The Settings card refreshes every **5000 ms** while the Settings page is active (`WS\renderer\launcher-quality.js:26`).

#### 3. Mechanism
- Reads the first 4096 bytes of `NBA2K27.exe`. It requires MZ, `PE\0\0`, machine 0x8664 and optional-header magic 0x20B. Identity is `TimeDateStamp(pe+8):SizeOfImage(pe+80)` as uppercase hex (`:7-11`).
- **Profiles:** `OLD='6A974D13:368C3000'`, `CURRENT='6AA2013D:36558000'` (`:3`).
- Feature → builds (`:6`):
  - session, roster, camera, arena, scoreboard, outfits, scaler: OLD + CURRENT
  - seasons, ball, rules, crowd, debug: CURRENT only
- Channel → feature mapping (`:12-23`):
  - `game:launch-without-eac` and `session:start` → session
  - `player:save`, `team:*save` and `arena:clone` → roster
  - `scaler:apply`, `camera:start` / `install-names`
  - `arena:select`, `ball:select`, `scoreboard:select`, `blacktop:apply`, `blacktop:rules-set`

#### 4. Build profiles
WPF knows only OLD and CURRENT (Sep 11). There is no V2 profile. Rust pins V2 (`R\artifacts\parity-score-20261017.md`: "Build resolver for game updates | MISS | port pins one exe hash").

#### 5. Rust must match
- [ ] An unknown exe identity keeps library management and recovery working, but refuses Play and edits with "<Feature>: This game build is not supported by this feature. No new edit will be attempted."
- [ ] The identity comes from the PE header (TimeDateStamp:SizeOfImage), cached by file stat.

---

### 5. Game window wait, volume and capture helpers (game-window.js, game-capture.js)

#### 1. User-visible behavior
- Capture status strings (`WS\core\game-capture.js`):
  - "Play starts the local game monitor. Ctrl+Shift+F12 captures a stuck loading screen." (`:24`)
  - "Waiting for this Play session’s game process." (`:82`)
  - "Game monitor ready. Press Ctrl+Shift+F12 if loading gets stuck." (`:58`)
  - "Game exited (0x%08x). Local diagnostics saved." (`:60`)
  - "No matching game process appeared within 90 seconds." (`:63`)
  - "Game snapshot saved locally." / "Capturing a small game snapshot; the game may pause briefly."
  - "Game snapshot unavailable (Windows error N). Limit: 3 snapshots per Play session."
  - "Start NBA 2K27 through Play first; a live game is required for a snapshot." / "A game snapshot is already in progress." / "Snapshot is still taking too long; check the local capture folder." (20 s)
- `Ctrl+Shift+F12` is registered after Steam starts (`WS\main.js:1813`) and is unregistered in close `prepare` (`:2326`).

#### 2. Timing and gating
- The capture monitor starts **before** Steam starts, inside `launchGame` (`WS\main.js:1808`), so even early exits record an exit code. A failure here only logs and never blocks launch.
- `stop()` sends `stop\n` and waits up to **17 000 ms** before killing (`game-capture.js:93`).
- `waitForGameWindow` spawns `GameWindow.exe --wait <launcherPid> <gamePath>`. It succeeds when stdout prints `{"event":"window-ready","pid":N}` and the helper exits with code 0. Timeout is **121 000 ms**. On cancel, stdin is closed and the helper is killed after 800 ms (`WS\core\game-window.js:3-17`).

#### 3. Mechanism
- `NBA2K27GameCapture.exe --watch <exe> <folder>` (from `Runtime\<ver>\diagnostics`).
  - stdout events: `attached-read-only{pid}` (sets `presence=true`), `game-exit{pid,value}` (sets `presence=false`, only for the attached pid), `game-not-found`, `dump-complete|auto-dump-complete{value}`, `dump-error|auto-dump-error{value}`.
  - stdin: `capture\n`, `stop\n`.
  - (`game-capture.js:37-89`)
- Folder layout: `Logs\game-captures\session-<ms>-<uuid>\{capture.json, watch.jsonl (≤ 1 MiB), exit.json, hang-[1-3].dmp}`.
  - `capture.json` schema `2k-game-capture-v1`, `maxDumps:3`, `maxDumpBytes:64 MiB`.
  - Retention: 2 old sessions are kept when a new one starts, so 3 including the new one. Only recognized files are deleted (`:6-20, 34-36`).
- `gamePresence()` is the **exact-handle exit oracle** used by the loader, the close guard and host-loss recovery (`:27-29`).
- Game exit also triggers `archiveSessionLogs` (`:61`).
- The native dump is described in `R\artifacts\game-capture-port-audit-20261017.md`: `MiniDumpWriteDump` with normal memory, thread info and unloaded modules, a 64 MiB / 15 s callback budget, and partial-then-publish. Dumps are excluded from support exports.

#### 4. Build profiles
Build-independent (works by exe path).

#### 5. Rust must match
- [ ] The monitor starts before Steam starts and is bound to this Play session's game.
- [ ] A capture or monitor failure is never treated as evidence that the game exited.
- [ ] At most 3 dumps per session, 3 retained sessions, never included in the support bundle. A capture times out after 20 s.

---

### 6. Restore on exit: game exit, launcher close, host loss

#### 1. User-visible behavior
- **Closing while the game runs** (last window X) (`WS\main.js:449-473`):
  1. Shows "Checking the game before closing…".
  2. If the game is present, shows the dialog **"Game session active"** / "Close NBA 2K27 first." / "Keep this window open so the launcher can finish restoring your game session." [OK]. The window stays open.
- **Closing with the browser roster editor open:** "Close launcher?" / "Closing the launcher disconnects the browser roster editor." [Keep open | Close launcher] (`:463`).
- **App quit (OS / explicit)** while the game runs: "Close NBA 2K27 first." / "The launcher must remain open to restore your game session." (`:2311-2317`).
- **Quit during init or launch:** "Wait for launcher initialization or game launch to finish before closing." (`:2309`).
- **While cleanup runs**, a modal shows "Closing launcher… Restoring session files and stopping helpers." It cannot be dismissed with Escape (`WS\renderer\launcher-closing.js:1-9`, `WS\main.js:2323`).
- **Cleanup failure:** "Launcher cleanup could not finish." / "Please try closing again. <error>". The window is recreated if needed (`:2300-2307`).
- If the process check fails: "Could not check the game session. Please close the game and try again." (`:469`).

#### 2. Timing and gating
- **Game-exit path:** `VersionProxySession.cleanupOnce` → `onCleanup` runs this list via `runRestorationSteps`, with `beforeEach` = fresh game check. If the game is live it throws `GAME_SESSION_ACTIVE` and the batch retries every 500 ms (`WS\main.js:2072-2100`). Order:
  1. Startup video window
  2. Outfit helper
  3. Blacktop ball
  4. Jukebox
  5. Startup video
  6. Camera names
  7. Scoreboards
  8. Front menu
  9. Scoreboard digits
  10. Ball scale
  11. Jukebox cache
  12. Playlist cache (only if `cachePreparedPlaylists===false`)
  13. Audio conversion cache (same condition)
  14. Mod profile
  15. Player folders
- Individual step errors are collected into an `AggregateError` after all steps have run. A safety-gate error aborts the batch immediately (`WS\core\session-restoration.js:3-13`).
- **Close path** (`before-quit` → `createExitHandler`) (`WS\core\exit-handler.js:5-34`, `WS\main.js:2294-2357`):
  - `canQuit` runs first. Then `prepare`, which stops these: animation preview, F12 hotkey, presentation import, auto-open, clothing cache, F8 panel, scaler, freestyle unlocks, camera (sync), session monitor.
  - Under WPF it then runs `cleanupNativeSession` (`:2238-2271`):
    1. `versionProxySession.stop()`.
    2. Restoration list: Startup video, Jukebox, Camera names, Scoreboards, Front menu, Scoreboard digits, Ball scale, Mod profile, Player folders, with `assertStopped` before each.
    3. Stop these helpers one by one, with a game check before each, throwing "`<name>` cleanup is still pending." if a stop returns `false`: arena, WNBA, ball, debug menus, presentation, crowd, outfit observer, F8 listener, camera controller, game monitor, audio conversion, Jukebox cache, roster server, probe, provider.
    4. Final `assertStopped`, then shared presentation restore.
  - The exit timeout is **15 000 ms**, but under WPF `forceOnTimeout=false`: it only logs "shutdown still pending" and **never force-exits** (`:2297-2299`).
  - The non-WPF (Electron) path uses `boundedShutdown` with per-helper timeouts (2-18 s). Not used under WPF.
  - `cleanupNativeSession` is wrapped in `shareCleanup`, so a successful close is not repeated on host EOF (`WS\core\shutdown-safety.js:9-17`).
- **Host-loss path** (`WA\Backend\host.cjs:70-78`, `WS\main.js:2272-2289`):
  - Triggered by stdin close, a stdout write error or the parent PID gone. Node rejects every pending host call, marks windows dead and emits `native-host-lost`.
  - main.js then stops the UI helpers and the session monitor, and runs `recoverNativeHost` (`WS\core\host-loss-cleanup.js:4-16`): loop every **1000 ms**:
    - `ready()` is true only when startup has settled, there is no launch in progress, no loader background task, presence is not `true` and a fresh query says not running.
    - Then `cleanupNativeSession()` → `app.finishNativeHostCleanup()`: release the owner pipe and `process.exit(0)`.
    - Failures retry forever. Progress goes to `Logs\native-host-recovery.jsonl`.
  - While the host is lost, Play is refused: "The launcher window host was lost. Session recovery is running; no new game launch will be sent." (`WS\main.js:499`).

#### 3. Mechanism
- `gameStopped()` = presence not true, **and** a fresh `processIsRunning` false, **and** presence still not true (`WS\core\shutdown-safety.js:3-7`).
- **`processIsRunning`** (`WS\core\session.js:8-44`):
  - Under WPF it calls the C# `process-running` host method (`Process.GetProcessesByName`, image name regex-validated, `WH\NativeProcessQuery.cs`).
  - Fallback: `tasklist /FO CSV /NH` with a 5 s timeout.
  - Results are cached for **500 ms** unless `{fresh:true}`. Errors throw `PROCESS_QUERY_FAILED` and are never treated as "not running".
- Every restore journal lives under `<root>\Data\…`. For example: `active-mod-profile.json` (`WS\main.js:2047`), `player-folder-session.json` (`:395`), `blacktop\scoreboard-layouts\`, `scaler\ball-session\`, `front-menu\`, `presentation\`, and `camera-tools\` for camera names.

#### 4. Build profiles
n/a

#### 5. Rust must match
- [ ] The X button and OS quit are refused while NBA2K27 is running (by fresh check or exact handle) with "Close NBA 2K27 first." The launcher stays open.
- [ ] Close is refused during initialization or an active launch.
- [ ] A cleanup failure keeps the app open, shows "Launcher cleanup could not finish.", and lets the user retry. There is no forced exit on timeout.
- [ ] Every restoration step runs, and the batch aborts the moment a fresh check sees the game.
- [ ] When the UI host dies, the backend keeps the provider and journals, waits for verified game exit, restores, then exits.
- [ ] The same cleanup is not repeated after it succeeded.

---

### 7. Launcher storage paths
WPF layout, `root` = the parent of `App` (`WH\App.xaml.cs:48`, `WS\core\launcher-storage.js:18-26`):

| Path | Contents |
|---|---|
| `App\` | Program (replaced by updates). `release.json` manifest. |
| `Data\settings.json` | Settings (`app.getPath('userData')` = `root\Data`, `WA\Backend\host.cjs:61`). |
| `Data\…` | Restore journals and state: `blacktop\`, `scaler\`, `unlocks\`, `camera-tools\`, `multi-config\`, `player-research\`, `front-menu\`, `presentation\`, `mod-library-tags.json`, `startup-timings.json`, `observe-only-lifecycle.json`, `startup-video-volume.json`. |
| `Library\Music\` | Jukebox (`library.json`, `archive-library\<sha16(modsRoot)>\{library.json,drafts,launch-ready-v1}`, `game-session\native-pcm-v1`). |
| `Library\Videos\`, `Library\Preview Packs\`, `Library\Outfits\outfit-presets.json` | Media and presets. |
| `Cache\` | `provider-layouts\`, `Blacktop\thumbnails\`, `Blacktop\fit-previews\`, `Blacktop\ball-thumbnails\`, `player-animation-preview\`, `Temp\` (Node TMP), `WebView2\` (C# WebView2 user data). |
| `Runtime\<version>\` | Hash-verified helper EXEs and DLLs copied from `App\Reference\runtime-resources`. |
| `Recovery\Updates\…`, `Recovery\PreviousApp\App` | Updater (see settings-support-updates.md). |
| `Logs\` | See settings-support-updates.md §Diagnostics for the file list. Includes `game-session\`, `session-history\`, `game-captures\`, `wpf-host.jsonl` (C#), `native-host-recovery.jsonl`, `launcher-shutdown.log`. |

The game folder only holds the session files during a session (`version.dll`, `.cfg`, `.trigger`), the `mods` junction swap (`.nba2k27-mod-launcher-default-mods` backup) and the player-folder hard links in `mods\char\sig`.

## Known divergences in current Rust port (brief check only)
- The close guard is a courtesy prompt: `close_policy.rs:8-10` says it "must never trap app exit". WPF **hard-refuses** to close while the game runs (§6).
- Any existing `version.dll` blocks Play. WPF overwrites its own 40 known hashes (`R\artifacts\wpf-version-dll-audit-20261017.md` row 21/D10).
- Rust uses a fixed 20 s settle even with a playlist. WPF used AudioReadiness (D8).
- The Rust V2 outfit worker depends on `LIVE_SESSION_ACTIVE`. The WPF outfit thread was independent (D2).
- Single instance: Rust `instance.rs` uses a mutex per root. There is no evidence that the cross-folder game-owner pipe (`2kLauncher-Wpf-GameOwner-v1`) is ported (**UNVERIFIED**, not grepped exhaustively).

## Open questions / UNVERIFIED
- The C# sources in `WH\` are assumed to be what built `WA\2k Launcher WPF.dll`. The version string matches, but no binary diff was done.
- `RendererMessageJson.Prepare` token rules (`WH\RendererMessageJson.cs`) were not read in detail.
- The `GameWindow.exe` / `GameVolume.exe` / `NBA2K27GameCapture.exe` native sources were not read. Their behavior is inferred from the JS contracts and the capture audit artifact.
