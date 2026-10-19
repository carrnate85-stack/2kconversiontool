# Settings, theme, support bundle, diagnostics and launcher updates — WPF reference
Source build: WPF quality-pass-qa-20261010 (frozen). Last researched: 2026-10-17.

Path abbreviations (relative to `C:\Users\carrn\CodexProjects\2kconversiontool`):
- **WA** = `nba2k27-launcher-wpf\release\quality-pass-qa-20261010\App\`
- **WS** = `WA\Reference\src\`
- **R** = `nba2k27-launcher-tauri2\`

The storage layout is in `architecture.md` §7.

## Summary
- **Settings page** (`#page-settings`) holds five cards: Launcher updates, Diagnostics & compatibility (injected by `launcher-quality.js`), Game installation, Launch performance and Support. The theme switch (Light/Dark) sits at the bottom of the sidebar and is stored only in renderer `localStorage`.
- **Settings** are persisted in `<root>\Data\settings.json` through one serialized, atomic writer. `provider:'local'` and `startCameras:true` are forced on every read and write. Retired keys are deleted on read.
- **Support bundle** is a redacted JSON file (not a ZIP): at most the 40 newest `.json/.jsonl/.log/.txt/.ini` files in `Logs\`, the last 256 KiB of each, with paths, user profile and secrets redacted. Game dumps are never included.
- **Launcher updates** are offline. The user picks an extracted release folder. It is hash-verified against its `release.json`, staged, and applied on exit by a detached updater runner that swaps `App` ↔ `Recovery\PreviousApp\App` under a journal and rolls back on failure.
- **Top 3 port breakers:**
  1. The update swap must happen **after** the launcher's own session cleanup and with no game or launcher running. The runner waits up to 120 s for both parent PIDs to exit, holds the game-owner pipe, and re-checks with `tasklist`.
  2. Support-bundle redaction (known paths, `C:\Users\<name>`, `*path/root/file` keys become `<redacted-path>\leaf`, secret keys become `<redacted>`). Size caps: 40 files and 256 KiB per file, tail only.
  3. Diagnostic cleanup must only delete exactly named generated files, keep `STATE=FAILED|INCONCLUSIVE` logs for 7 days, and never run while the game is running.

## Key source map
| Concern | file:line |
|---|---|
| Settings page markup | `WS\renderer\index.html:455-492` |
| Sidebar nav (Settings is `data-page="settings"`) | `WS\renderer\index.html:41-53` |
| Theme switch | `WS\renderer\theme.js:1-26` |
| Settings defaults and normalization | `WS\core\paths.js:4-64` |
| Settings validation | `WS\core\paths.js:66-80` |
| read/write settings | `WS\main.js:810-844` |
| `settings:save` / `settings:choose` | `WS\main.js:954-967, 1016-1030` |
| Renderer settings handlers | `WS\renderer\app.js:541-548, 596-621, 2531-2536, 2729, 2767-2772, 2815-2816` |
| Support bundle | `WS\core\support-bundle.js:1-50`, `WS\main.js:1031-1038`, `WS\renderer\app.js:2497-2501` |
| Launcher health (helpers + performance) | `WS\core\launcher-health.js:1-25`, `WS\main.js:52-55, 171-177` |
| Diagnostics card UI | `WS\renderer\launcher-quality.js:1-27` |
| Game hang capture UI | `WS\renderer\app.js:2502-2511`, `WS\main.js:1039-1041` |
| Panel and selection diagnostics | `WS\core\panel-diagnostics.js:5-24`, `WS\core\selection-status-log.js:8-23` |
| Diagnostic cleanup | `WS\core\diagnostic-cleanup.js:16-53` |
| Session log archive | `WS\core\session-evidence.js:15-45` |
| Temporary-file cleanup | `WS\core\temporary-cleanup.js:5-31` |
| Update service | `WA\Backend\app-updates.cjs:4-43`, `WA\Backend\host.cjs:63-67, 96-98` |
| Update runner | `WA\Backend\app-update-runner.cjs:1-35` |
| Update transaction (apply/recover) | `WA\Backend\app-update-transaction.cjs:1-61` |
| Package manifest and verify | `WA\Backend\app-package.cjs:1-36`, `WA\release.json` |
| Update IPC | `WS\main.js:178-189` |
| Update UI | `WS\renderer\launcher-updates.js:1-24` |
| Storage resolution and migrations | `WS\core\launcher-storage.js:28-152`, `WS\core\launcher-portable-migration.js:43-100`, `WS\main.js:1866-1903` |

---

### 1. Settings page and settings.json

#### 1. User-visible behavior
- **Page layout** (`WS\renderer\index.html:455-492`), in DOM order. The diagnostics card is inserted as the second child (`WS\renderer\launcher-quality.js:11`).
  1. **Launcher updates.** Hidden until build info loads (§5).
  2. **Diagnostics & compatibility** (§3).
  3. **Game installation**, badge `#validationLabel` showing "GAME READY" (green) or "CHECK PATHS" (amber) (`WS\renderer\app.js:248-249`; initial text "NOT CHECKED").
     - Description: "The launcher uses this folder for the game executable, mods and verified runtime integration."
     - Field "NBA 2K27 folder" (`#gameRoot`), **Browse**, **Save settings**.
     - **Open launcher folder** with the storage path shown beside it (initially "Checking storage location…").
  4. **Launch performance.** Switch "Cache prepared playlists" (see `launch-play.md` §5).
  5. **Support**, badge "Redacted".
     - "Export recent launcher diagnostics without game paths, profile paths, usernames, or secrets."
     - **Export support bundle**, with status "Nothing is sent automatically."
     - **Capture game hang** and **Open diagnostics**, with status "Ctrl+Shift+F12 captures a stuck game while it is still running."
     - Note: "Snapshots can briefly pause the game and may contain private memory. Stored locally in Logs/game-captures; never uploaded or included in support exports. Keeps three sessions, up to three 64 MB snapshots each."
- **Browse** opens a folder dialog titled "Choose the NBA 2K27 game folder" ("Choose the live mods folder" for `modsRoot`). The default path is the current value. It only fills the input; it does not save (`WS\main.js:1016-1030`, `WS\renderer\app.js:2531-2536`).
- **Save settings** sends `{gameRoot (trimmed input), modsRoot (current), modsProfiles (current, plus modsRoot if missing)}`. It then re-bootstraps the provider list and shows the toast "Settings saved.", or an error toast (`WS\renderer\app.js:596-621`).
- **Open launcher folder** opens `<root>` (organized mode) in Explorer (`WS\main.js:1860-1861`).
- **Errors from `settings:save`:**
  - "Finish or cancel the video import before changing its profile or playback setting." (while a video import is busy and the patch touches `modsRoot`, `gameRoot` or `startupVideoEnabled`)
  - "Invalid playlist cache preference." / "Invalid presentation preference." (non-boolean)
  - "This launcher library is still stored inside the current NBA 2K27 folder. Finish its AppData migration before changing the game path." (game-placement storage only)
  - (`WS\main.js:955-959`)
- Other preference switches live on other pages but write to the same file:
  - `unlockWnbaFreestyle` and `unlockWnbaBlacktop`: Unlocks page (`WS\renderer\index.html:404,410`)
  - `autoOpenBlacktop`: Blacktop page (`:145`)
  - `usePlaylistInFreestyle`: Jukebox page (`:238`)
  - `startupVideoEnabled`: Home (`:101`)
  - Profiles: Profiles page
  - All are filled from `fillForm()` (`WS\renderer\app.js:541-548`).

#### 2. Timing and gating
- `settings:save` writes immediately. If `cachePreparedPlaylists===false` and the game is not running, it clears the playlist launch cache and the conversion cache right away (`WS\main.js:961-964`).
- A changed `gameRoot` resets the provider-layout state to "Game path changed; waiting to rescan." (`:965`).
- `fillForm()` fires the window event `launcher:settings-refreshed`, which refreshes the diagnostics card (`WS\renderer\app.js:542`, `WS\renderer\launcher-quality.js:23`).

#### 3. Mechanism
- **File:** `<root>\Data\settings.json` (`WS\main.js:690-692`).
  - Read: `JSON.parse`, merged onto `defaults()`. ENOENT or SyntaxError falls back to defaults. Other errors throw (`:812-831`).
  - Write: serialized promise tail → temp `settings.json.<pid>.<uuid>.tmp` (`wx`) → write + `fsync` → rename. Pretty-printed with a trailing newline (`:833-844`).
  - Every read and write forces `provider:'local'` and `startCameras:true`.
- **Defaults** (`WS\core\paths.js:13-30`):

  | key | default |
  |---|---|
  | `gameRoot` | `C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27` |
  | `modsRoot` | `<gameRoot>\mods` |
  | `modsProfiles` | `[{name:'Default mods', path:<modsRoot>}]` |
  | `provider` | `'local'` (forced) |
  | `startCameras` | `true` (forced) |
  | `usePlaylistInFreestyle` | `true` |
  | `autoOpenBlacktop` | `true` |
  | `unlockWnbaFreestyle` | `true` |
  | `unlockWnbaBlacktop` | `false` |
  | `blacktopPresentation` | `{drone:false, dj:false, mc:false}` |
  | `blacktopCrowdEnabled` | `true` |
  | `startupVideoEnabled` | `false` |
  | `cachePreparedPlaylists` | absent, read as **true** (`!==false`) |
  | `jukeboxLibraryRoot` | set once at first start (`WS\main.js:1966-1980`) |

- **Normalization on every read** (`WS\core\paths.js:32-64`):
  - Delete retired keys: `usePlaylistInBlacktop`, `blacktopPlaylistId`, `blacktopMusicVolume`, `blacktopCourtSpeaker`, `blacktopCourtSpeakerStrength`, `startupSplashEnabled`, `startupTeamSplashEnabled`, `hideWhatsNewPanel`, `autoUnlockMultiConfigPlayers`, `replaceFrontMenuPlayers`, `frontMenuPlayerMappings`, `frontMenuPlayerConfigs`, `frontMenuActiveConfig`, `unlockGameRules`.
  - Coerce the presentation and crowd booleans.
  - Rebuild `modsProfiles`: the default `<game>\mods` first, then saved profiles, then the active root. Paths are de-duplicated case-insensitively, and names are trimmed to 80 characters.
- **Validation** (`WS\core\paths.js:66-80`):
  - Returns `{gameRoot, gameExe, modsRoot: 'file'|'directory'|'missing'|'other', ready, gameExe path, normalizedModsRoot}`.
  - `ready` = gameRoot is a directory, `NBA2K27.exe` is a file, and modsRoot is a directory.

#### 4. Build profiles
n/a

#### 5. Rust must match
- [ ] A missing or corrupt `settings.json` loads defaults without crashing. Writes are atomic and serialized.
- [ ] Retired keys are dropped, `provider` and `startCameras` are forced, and profiles are de-duplicated with the default profile first.
- [ ] Browse only fills the field; Save persists it and shows "Settings saved.".
- [ ] The badge shows "GAME READY" only when the folder, exe and mods folder all exist.
- [ ] A Rust port that reads WPF settings must accept these keys and defaults (`R\src\settings_compat.rs` maps them).

---

### 2. Theme

#### 1. User-visible behavior
- A "Light"/"Dark" segmented control is appended to the bottom of the sidebar (`role="group"`, `aria-label="Appearance"`), with sun and moon icons and `aria-pressed` on the active choice (`WS\renderer\theme.js:9-23`).
- The default is **dark**. The roster workspace (`?workspace=roster`) is always dark and has no switch (`:2`).

#### 2. Timing and gating
- Applied before `DOMContentLoaded` (`document.documentElement.dataset.theme`), so there is no flash (`:6-7`).
- Syncs across windows through the `storage` event (`:24`).

#### 3. Mechanism
`localStorage['mod-launcher-theme']` = `'light'` or `'dark'`. It is not in settings.json. Failures are ignored (`:3-5, 19`). Styles key off `[data-theme]`.

#### 4. Build profiles
n/a

#### 5. Rust must match
- [ ] Dark by default. The choice persists per user and applies before first paint.

---

### 3. Diagnostics & compatibility card (launcher health)

#### 1. User-visible behavior
- The card "Diagnostics & compatibility" has a **Refresh** button and a summary line `<TS:SIZE> · <message>` (`WS\renderer\launcher-quality.js:10,15`). Sections:
  - `<details>` "Feature compatibility": table Feature / Support (Recognized | Unavailable) / Details.
  - Helper table: Helper / State / Latest report / Updated (local time, or "Not observed"). Caption: "Latest reported helper states, with timestamps—not a guarantee that a pending effect is already visible in-game."
  - `<details>` "Performance measurements": "Host startup, shutdown cleanup, and catalog click-to-visible time. A regression needs at least three samples per build and a median increase above 30% and 100 ms. QA tests are kept separate." Table Operation / Samples / Latest / Median / Previous build median / Trend ("Slower—investigate" | "Collecting samples" | "Recorded").
  - **Copy diagnostic report**, which reports "Diagnostic report copied. Local paths are redacted; nothing was uploaded." (`:12`).
- Helper rows (`WS\core\launcher-health.js:4`):
  - Game session, Session loader, Cameras, Scaler, Arena, Ball, Scoreboard
  - Outfit carry-over, Crowd removal, Drone / DJ / MC
  - Freestyle unlock, Blacktop teams, Debug menus
  - An unobserved helper shows status Unknown, phase `not-observed` and "No status received this launcher session." (`:23`).

#### 2. Timing and gating
- Auto-refreshes every **5000 ms** while the Settings page is active, and after `launcher:settings-refreshed` (`WS\renderer\launcher-quality.js:23,26`).
- Helper state comes from every `send()` push (`WS\main.js:846-848`) and the ball/arena/scoreboard publishers (`:225, 233, 258`).

#### 3. Mechanism
- **Status classification** of the phase string (`WS\core\launcher-health.js:17`), checked in this order:
  1. /error|failed|unsupported|attention|blocked/ → **Failed**
  2. exactly off|inactive|disabled|idle|closed|stopped|restored|configured|saved → **Inactive**
  3. /pending|wait|prepar|arm|start|load|restor|monitor/ → **Pending**
  4. /active|running|applied|ready/ → **Active**
  5. otherwise **Unknown**

  Messages are redacted and capped at 1000 characters.
- **Performance metrics:** `startup` (WPF host ready ms, sent by C# `MainPageReady`), `shutdown` (`Date.now()-shutdownStartedAt`, recorded in `app.exit`), and `catalog.ball|scoreboard|arena|outfit|rules` (renderer `launcher:performance`, which only accepts names starting with `catalog.`) (`WS\core\launcher-health.js:5`, `WA\Backend\host.cjs:65,109`, `WS\main.js:53-54,177`).
- Stored in `Logs\launcher-performance.json` `{schema:1, samples:[{name, ms, version, lane, at}]}`. Up to the last 300 samples are kept. Files over 256 KiB are ignored. Each sample must be 0-600000 ms.
- `lane` = `qa` when `WPF_SMOKE=1`, else `normal`. The median uses the last 20 samples of the current version against the most recent other version (`:8-21`).
- The report also includes `compatibility = GameCompatibility.inspect(gameRoot)` (`WS\main.js:171-174`). **Copy** puts `redactValue(report)` on the clipboard, using the known paths gameRoot, modsRoot, storage root and userData (`:176`).

#### 4. Build profiles
Compatibility rows: see `architecture.md` §4.

#### 5. Rust must match
- [ ] Helpers show last state, message and time. Unobserved helpers say "No status received this launcher session."
- [ ] Performance regression = at least 3 samples in both builds, median more than 30% higher **and** more than 100 ms higher.
- [ ] The copied report is redacted.

---

### 4. Support bundle and diagnostics files

#### 1. User-visible behavior
- **Export support bundle**:
  - Save dialog titled "Export redacted support bundle", default name `2k-launcher-support-<version>.json`, filter "2k Launcher support bundle (*.json)", overwrite confirmation (`WS\main.js:1034`).
  - While running, the button reads "Exporting…".
  - On success: "Saved N diagnostic file(s)[; M trimmed to the safe size limit]." plus the toast "Redacted support bundle exported." (`WS\renderer\app.js:2497-2501`).
  - Cancelling the dialog returns null and changes nothing.
- **Capture game hang** / **Open diagnostics**: see `architecture.md` §5. Open diagnostics opens `Logs\` (`WS\main.js:1041`).

#### 2. Timing and gating
On demand only. Nothing is uploaded.

#### 3. Mechanism
- **Bundle** (`WS\core\support-bundle.js:42-48`):
  - Source: `Logs\` (top level only, not recursive).
  - Files: extensions `.json`, `.jsonl`, `.log`, `.txt`, `.ini`, newest first by mtime, at most **40** (`MAX_FILES`).
  - Each file contributes its last **256 KiB** (`MAX_FILE_BYTES`). It is JSON-parsed and redacted if possible, otherwise redacted as text. Entry fields: `{name, modifiedUtc, originalBytes, truncated, content}`.
  - Top-level shape:
    - `{schema:'2k-launcher-support-v1', createdUtc, launcherVersion}`
    - `privacy: {pathsRedacted:true, secretsRedacted:true, maxDiagnosticBytes, maxFiles}`
    - `system: {platform, arch, osRelease, electron, node}`
    - `settings`: the safe subset only. `gamePathConfigured`, `modsPathConfigured`, `profileCount`, `provider`, `startCameras`, `cachePreparedPlaylists`, `unlockWnbaFreestyle`, `unlockWnbaBlacktop`, `autoOpenBlacktop` (`:30-34`)
    - `validation` (redacted), `diagnostics[]`
  - Written as pretty-printed UTF-8 JSON.
  - Because the bundle is top-level only, `game-session\`, `session-history\` and `game-captures\` are excluded.
- **Redaction** (`:5-29`):
  - Known paths (userData, gameRoot, modsRoot, each profile path, in `\`, `/` and `\\` variants, longest first) → `<redacted-path>`.
  - The home directory → `<user-profile>`. Regex `[A-Za-z]:\\Users\\[^\\\s"']+` → `<user-profile>`.
  - `token|secret|password|authorization|cookie|api_key = value` → `$1<redacted>`.
  - Keys matching /(token|secret|password|credential|authorization|cookie|key)$/i → `<redacted>`.
  - Keys matching /(path|root|directory|folder|file|target)$/i holding an absolute path → `<redacted-path>\<leaf>`.
- **Logs\ files** written by WPF (top level unless noted):

  | File | Source |
  |---|---|
  | `wpf-host.jsonl` | C# host events (`WH\App.xaml.cs:143-146`) |
  | `native-host-recovery.jsonl` | `WA\Backend\host.cjs:10`, `WS\main.js:2280` |
  | `launcher-shutdown.log` | `WS\main.js:2233-2237` |
  | `launcher-performance.json` | §3 |
  | `blacktop-panel-<ver>-<pid>.jsonl` | F8 panel trace, ≤ 1 MiB per process (`WS\core\panel-diagnostics.js:6-23`) |
  | `blacktop-selections-<ver>-<pid>.jsonl` | ball/arena/scoreboard/outfit/crowd/WNBA status, ≤ 1 MiB (`WS\core\selection-status-log.js:8-23`) |
  | `blacktop-wnba-status.json` | `WS\main.js:2031-2032` |
  | `video-overlay.jsonl` | `WS\main.js:1949,1952` |
  | `camera-service-history.json` | `WS\main.js:2010` |
  | `jukebox-audio-route.json` | `WS\main.js:2063` |
  | `last-loader-failure.json` | `WS\core\version-proxy-session.js:369` |
  | `game-session\` | DLL logs, outfit worker logs and plans |
  | `session-history\<sha256>--<name>.gz` | archived session logs, newest kept until > **200** files or > **32 MiB** (`WS\core\session-evidence.js:15-45`) |
  | `game-captures\session-*\` | dumps (`architecture.md` §5) |

- **Diagnostic cleanup** (`WS\core\diagnostic-cleanup.js:16-53`). Runs at startup when the game is not running (`WS\main.js:1963`) and around each loader session (`WS\core\version-proxy-session.js:212, 474`). It **skips completely if the game is running**. Steps:
  1. Archive first (session-evidence).
  2. In `gameRoot` and `Logs\game-session`, delete only names matching the owned outfit-log regex or `nba2k27_version_proxy(_armed|_audit)?.(log|trace)`.
  3. A `.log` ≤ 1 MiB that contains `STATE=FAILED|INCONCLUSIVE` and is younger than **7 days** is kept: moved from the game folder into `game-session`, or left in place there.
  4. Retention:
     - `game-session` owned files: newest **20** or **5 MiB**, max age **7 days**.
     - `blacktop-panel-*.jsonl`: newest **5** or **5 MiB**, max age 7 days.
  5. No recursive deletes; links are refused.
- **Temporary cleanup** (startup only, `WS\main.js:1964`; `WS\core\temporary-cleanup.js:5-31`). It deletes flat folders older than **24 h** matching:
  - `Data\blacktop\thumbnail-cache\decode-XXXXXX`
  - `Data\blacktop\fit-preview-temp\fit-XXXXXX` and `Data\blacktop\fit-preview-cache\fit-XXXXXX`
  - `Data\jukebox\artwork-<n>-<n>`
  - `Data\jukebox\game-session\native-pcm-v1\<hash>.wav.tmp` files older than 24 h

  The root is `app.getPath('userData')`. In the WPF layout the music session is `Library\Music\…`, so the jukebox patterns here are legacy paths (**UNVERIFIED** whether they ever match under WPF).

#### 4. Build profiles
n/a

#### 5. Rust must match
- [ ] The export has a save dialog with name `2k-launcher-support-<version>`. Cancel does nothing. The result message gives the file count and trimmed count.
- [ ] Only bounded recent log tails are included, with paths, usernames and secrets redacted. Dumps are never included.
- [ ] Settings appear in the bundle only as booleans and counts, never paths.
- [ ] Generated diagnostics are cleaned only when the game is not running. Failed reports are kept 7 days.

Rust note: `R\src\support_bundle.rs:19-22` uses 128 files, 512 KiB per file, 8 MiB total and depth 12, and produces a ZIP (`R\artifacts\parity-score-20261017.md`: "Support bundle ZIP"). WPF was a single JSON with 40 files × 256 KiB, top level only. Document the divergence or match it.

---

### 5. Launcher updates and rollback

#### 1. User-visible behavior
- **Card "Launcher updates"** (`WS\renderer\index.html:457-466`, `WS\renderer\launcher-updates.js`):
  - The badge `#launcherBuildLabel` shows `<version> · <channel>` (for example `0.1.0-wpf.24 · Main`). This also replaces the sidebar version text.
  - "Built <locale date>". `<details>` "What changed" lists `release.json` notes.
  - Text: "Extract your downloaded launcher release, then choose its folder to update. The previous version is kept for rollback."
  - Buttons:
    - **Install update folder**: folder dialog "Choose downloaded launcher update folder" (`WS\main.js:183`).
    - **Restore previous version**: labelled "Restore <prev version>" when `Recovery\PreviousApp\App\release.json` exists; otherwise disabled.
- **Flow:**
  1. On pick, the status reads "Checking update…" with an indeterminate progress bar. Then progress pushes "Checking update files…" / "Preparing update…" with completed/total (updated every 20 files).
  2. Success: the review box shows `<version> · <channel>` and notes, with status "Verified and ready. Install and restart when ready."
  3. Buttons **Install and restart** / **Cancel**. Cancel → "Update canceled." A cancelled pick → "Update selection canceled."
  4. Install → status "Finishing session cleanup, then installing and restarting…". The launcher quits after **100 ms** (`WS\main.js:188`).
  5. After the restart, the status shows `last-result.json`'s message: "Launcher update completed." / "An interrupted update was recovered." / the error (`WS\renderer\launcher-updates.js:13`).
- **Errors** (`WA\Backend\app-updates.cjs`, `WA\Backend\app-package.cjs`, `WA\Backend\host.cjs:97`, `WS\main.js:179`):
  - "Close NBA 2K27 before updating the launcher."
  - "Wait for launcher initialization or game launch to finish."
  - "An update is already being prepared."
  - "Choose a downloaded update, not the running App."
  - "Choose the original download, not update working files."
  - "An interrupted update needs recovery. Use Recovery/Updates/Runner/Recover launcher.cmd."
  - "This folder is not a supported launcher update."
  - "Invalid update file list."
  - "Update package exceeds its size limit."
  - "Update is missing a launcher component."
  - "An App update cannot contain user data."
  - "Update contains missing or unexpected files."
  - "Update verification failed: <path>"
  - "Linked update paths are not supported."
  - "Choose an update first."
  - "Wait for update preparation to finish."
  - "Finish or cancel the pending launcher update before starting the game." (Play while an update is pending)
- If the runner cannot be spawned on exit: dialog "Update could not start" / <error> / "The current launcher is still installed. Reopen it to retry." (`WA\Backend\host.cjs:66`).

#### 2. Timing and gating
- `assertClosed` is required before choosing, during prepare (twice), before install, and in `startOnExit`. It requires:
  1. `assertUpdateReady`: initialization finished, no launch in progress, and no loader background task.
  2. A fresh native check that `NBA2K27.exe` is not running.

  (`WA\Backend\host.cjs:96-98`, `WS\main.js:179`)
- **Install** sets `installRequested=true`, then `app.quit()`. This runs the normal close guard and full session cleanup (architecture §6). Only in `app.exit(0)`, after cleanup, does it call `launcherUpdates.startOnExit(parentPid)` (`WA\Backend\host.cjs:63-67`).
- **Runner** (`WA\Backend\app-update-runner.cjs:13-28`):
  1. Validate `request.json`: schema 1, exactly 2 parent PIDs, created within **300 000 ms**.
  2. Wait until both parents (WPF exe pid, node pid) have exited, polling **250 ms**, deadline **120 000 ms** ("Launcher cleanup has not finished. Open the launcher and try again.").
  3. Acquire the game-owner pipe.
  4. `tasklist` (timeout 10 s) must show no `NBA2K27.exe`, `2k Launcher.exe` or `2k Launcher WPF.exe` ("Close NBA 2K27 and every launcher window before updating.").
  5. `recover()`, then `apply()` if nothing needed recovery.
  6. Write `last-result.json`.
  7. Relaunch `App\2k Launcher WPF.exe`, also after a failure.
- Moves retry on EPERM/EACCES/EBUSY up to 6 times with a 100·n ms back-off (`WA\Backend\app-update-transaction.cjs:5-10`).

#### 3. Mechanism
- **Locations** (`app-update-transaction.cjs:11`):
  - `updates` = `<root>\Recovery\Updates`
  - `staged` = `…\Updates\Staged\App`
  - `previous` = `<root>\Recovery\PreviousApp\App`
  - `displaced` = `…\Updates\Displaced\App`
  - `journal` = `…\Updates\transaction.json`
- **Prepare** (`WA\Backend\app-updates.cjs:15-35`):
  1. If the picked folder contains `App\release.json`, use `App`.
  2. `verify(candidate)`.
  3. Copy to `Updates\Incoming-<uuid>` (no overwrite) and verify again.
  4. Copy `node.exe`, `app-package.cjs`, `app-update-transaction.cjs`, `app-update-runner.cjs` and `game-owner.cjs` into `Updates\Runner\`, and write `Recover launcher.cmd` = `"%~dp0node.exe" "%~dp0app-update-runner.cjs" --recover`.
  5. Replace `Staged\App` (after verifying the old one), atomically.
  6. Set `pending=true` and `choice={version, channel, notes}`.
- **Rollback button** = `prepare(Recovery\PreviousApp\App)`, the same flow (`WS\main.js:186`).
- **startOnExit:** write `Updates\request.json` `{schema:1, parents:[wpfPid, nodePid], created}` and spawn `Runner\node.exe app-update-runner.cjs` detached, hidden and unref'd (`app-updates.cjs:37-43`).
- **Apply** (journaled, `app-update-transaction.cjs:13-38`):
  1. Verify staged and current App.
  2. Refuse if a journal or `Displaced` exists.
  3. Write journal `prepared`.
  4. Move `PreviousApp` → `Displaced` (if it exists).
  5. Move `App` → `PreviousApp`. Journal `old-moved`.
  6. Move `Staged` → `App`, then verify. Journal `committed`.
  7. Delete `Displaced` and the journal.

  On any error, undo the moves in reverse and delete the journal.
- **Recover** (`app-update-transaction.cjs:39-60`):
  - `committed`: verify App and drop `Displaced`.
  - Otherwise: put the new App back to staged if needed, restore `PreviousApp` → `App`, and `Displaced` → `PreviousApp`. "Conflicting rollback folders; no files were removed." if both exist.
- **Package verify** (`WA\Backend\app-package.cjs:22-35`):
  - `release.json` ≤ 8 MiB, `schema:1`, `product:'2k-launcher-wpf'`, version regex `^\d+\.\d+\.\d+-wpf\.\d+$`, channel `Main|QA`, notes ≤ 1000 characters each.
  - 4-10000 files. Safe names (no `\`, no `:`, no reserved DOS names, no trailing dot or space). Total ≤ **4 GiB**.
  - Must include `2k launcher wpf.exe`, `2k launcher wpf.dll`, `backend/node.exe` and `backend/host.cjs`.
  - Must contain no `data|library|cache|logs|recovery|runtime` paths.
  - The exact file set must match the manifest. Every file's size and SHA256 must match. No symlinks anywhere up the path.
- **Build info** (`app-updates.cjs:8-14`): `{current:{version, channel, notes, builtAt} from App\release.json, previous:{version, channel}, result: last-result.json, busy, pending, choice}`. Fallback: "This build predates the update catalog."

#### 4. Build profiles
n/a

#### 5. Rust must match
- [ ] Updates are offline (pick an extracted folder) and verified file-by-file against a manifest before staging. There is no network.
- [ ] Install happens only after normal session cleanup, with no game or launcher running. The swap is journaled, recoverable, and keeps the previous App for rollback.
- [ ] A failed update relaunches the old app and shows the failure in Settings.
- [ ] Play is refused while an update is pending.

Rust status: **missing** (`R\artifacts\parity-score-20261017.md`: "Launcher update / rollback | 2 | MISS").

---

### 6. Storage resolution and migrations

#### 1. User-visible behavior
- **Under the WPF host, migrations never prompt.** `host.cjs` overrides `resolvePortableStorage` with `resolveContainedStorage(root)` (`WA\Backend\host.cjs:87-88`). That returns `organized/portable` with `needsPortableMigration:false`, or `blocked` with these errors (`WS\core\launcher-storage.js:29-46`):
  - "Portable storage is not a regular directory|file: <path>"
  - "Portable import is incomplete. The existing library was preserved." (if `Data\.portable-import-pending` exists)
  - "Portable settings are invalid."

  A blocked storage throws at startup (`WS\main.js:38`).
- **Electron-era prompts** (kept in source, not reachable under WPF; `WS\main.js:1866-1903`):
  - "How should this launcher folder start?" [Import previous library | Start fresh | Cancel]
  - "Launcher library copied into this portable folder."
  - "Launcher library moved to AppData."

#### 2. Timing and gating
- At startup, before the main window exists. Import is refused while the game is running ("Close NBA 2K27 before importing the previous launcher library. The original data remains untouched.").

#### 3. Mechanism
- **Portable import** (`WS\core\launcher-portable-migration.js:43-100`):
  1. Stage into `<root>\.portable-import-<uuid>`, skipping Chromium caches and `runtime-resources` (or `Cache`/`Runtime` for organized sources).
  2. Rebase JSON paths.
  3. Write `portable-import-manifest.json`.
  4. Create `Data\.portable-import-pending`.
  5. Commit each file with `COPYFILE_EXCL` and verify by SHA256. Conflicting different files abort.
  6. Commit settings last.
  7. Remove the pending marker.

  The source is never modified.
- Legacy sources: `%APPDATA%\nba2k27-mod-launcher` (settings.json at its root), `%APPDATA%\2k Launcher`, and `<game>\2k Launcher` via the pointer `%APPDATA%\nba2k27-mod-launcher-location.json` (+ `.ini` native hint `[Launcher]\r\nRoot=<root>`) (`WS\core\launcher-storage.js:5-12, 48-112, 144-152`).
- The folder mapping from the legacy layout to the organized layout: `videos → Library\Videos`, `jukebox → Library\Music`, `diagnostics → Logs`, `runtime-resources → Runtime`, `blacktop\preview-packs → Library\Preview Packs`, `blacktop\outfit-presets.json → Library\Outfits`, `blacktop\thumbnail-cache → Cache\Blacktop\thumbnails`, … (`WS\core\launcher-storage-migration.js` `destinationFor`).

#### 4. Build profiles
n/a

#### 5. Rust must match
- [ ] A portable root is validated (regular directories, no links, valid settings JSON). An incomplete import blocks startup instead of starting empty.
- [ ] Any migration copies and verifies, and never deletes the source. (Rust status: "WPF data migration | PART" — outfits, previews and advanced settings are not migrated, `R\artifacts\parity-score-20261017.md:98`.)

---

## Known divergences in current Rust port (brief)
- No launcher updater or rollback (MISS).
- The support bundle is a ZIP with different caps (`R\src\support_bundle.rs:19-22`).
- The data migration from WPF is partial.
- Not checked: whether Rust has a diagnostics & compatibility card with helper states and performance regression tracking.

## Open questions / UNVERIFIED
- The theme CSS tokens (`release-theme.css`, `styles.css`) were not catalogued here; see the UI reference.
- The `temporary-cleanup.js` jukebox patterns refer to `Data\jukebox\…`. Under the WPF layout, music is under `Library\Music`, so they may never match.
- The Developer diagnostics page (`#developerNav`, hidden) is reached only from Help → "Open session diagnostics" (`WS\renderer\app.js:3015-3019`). Its probe and bridge-audit tools are developer-only and not documented here.

## Ctrl+Shift+F12 hang capture: Rust findings (2026-10-17, QA 0.50)

- **WPF mechanism:** WPF registered the chord with `RegisterHotKey(mainHwnd, 273, MOD_CONTROL|MOD_SHIFT|MOD_NOREPEAT, VK_F12)` at Play.
  - Sources: `nba2k27-launcher-wpf\NativeWindow.cs:335-336`, via `NativeBridge.cs:137,209-214`; Electron's `globalShortcut` in `Reference\src\main.js:1813`; unregistered at `main.js:2326`.
  - Failure was not surfaced: `console.warn` only.
- **0.50 live result:** the Settings button worked, but the in-game chord produced nothing.
- **Probe (2026-10-17, game closed):** Ctrl+Shift+F12 could be registered, so it is not reserved or permanently held. Delivery could not be tested from a non-interactive shell.
- **Likely causes:**
  - The game, while foreground, suppresses application hotkeys (raw input with `RIDEV_NOHOTKEYS`).
  - Another app owns the chord while the game runs (error 1409).
- **Rust 0.51 fix (`src\game_capture.rs`):**
  - Keep `RegisterHotKey` (WPF parity).
  - Also poll `GetAsyncKeyState` every 20 ms while the exact game process is foreground. This is the same mechanism WPF's in-game F8 watcher used (`blacktop-overlay.md` §F8).
  - One press fires once, de-duplicated over 1.5 s.
- **Visible state:**
  - Settings > Support shows `Ctrl+Shift+F12: Waiting for game / Registered / Held by another app`, the last press time and its source, via channel `launcher-tools-hotkey`.
  - Every state change and press is logged to `Logs\game-capture-hotkey.log`, so the next live test shows which path fired or why neither did.

## Build resolver: Rust per-feature relocation (2026-10-18)

- **WPF:** the provider resolver uses compiled-in masked signatures and unique clusters, and is a launch gate (`main.js:1740`). Every other feature was blocked on unknown builds by the `game-compatibility.js` allow-list.
- **Rust:** `src/feature_relocation.rs` plus `assets/v2-relocation-catalog.json`.
  - The catalog has 199 anchors from 11 feature modules' own proofs, each unique in the verified build and with RIP/rel32 fields masked.
  - Anchors are relocated by a unique search, and 53 globals are re-derived from relocated displacements.
- **UI:** the per-feature table is in Settings > Advanced, "Game build compatibility".
- **Admission:** on an unrecognized update, only mod loading is admitted (WPF resolver plus 8 ABI anchors). Every other feature shows "Game updated: X needs a new mapping (…)".
- **Report:** `artifacts\game-update-resilience-20261018.md`.
