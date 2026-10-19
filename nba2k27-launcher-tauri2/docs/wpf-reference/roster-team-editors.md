# Roster and team editors — WPF reference
Source build: WPF quality-pass-qa-20261010 (frozen). Last researched: 2026-10-17.

Path aliases used in citations (all relative to `C:\Users\carrn\CodexProjects\2kconversiontool`):
- `W/` = `nba2k27-launcher-wpf/release/quality-pass-qa-20261010/App/Reference/`
- `RR/` = `nba2k27-roster-research/src/` (C# source of `NBA2K27RosterResearch.exe`, built by `nba2k27-roster-research/build.ps1`)
- `ART/` = `nba2k27-launcher-tauri2/artifacts/`
- `NOTES/` = `C:\Users\carrn\AppData\Roaming\Claude\side-session-notes\local_98bf0092-fe39-4ae3-abce-e88edad0a633\` (absolute)

## Summary

- The WPF host ran the **unchanged Electron-era `main.js`** under a shim (`nba2k27-launcher-wpf/release/quality-pass-qa-20261010/App/Backend/host.cjs:1-3`, `:139`). So every roster behavior below comes from `W/src/main.js`, `W/src/core/*.js` and `W/src/renderer/*.js`. `app.getPath('userData')` is `<root>\Data` (`Backend/host.cjs:62`).
- **All game I/O is live process memory.** Nothing edits the roster save file on disk. A separate C# helper, `NBA2K27RosterResearch.exe` (`W/src/main.js:754-758`), does every read and every write. It runs once per operation as a child process and prints one JSON report on stdout. Each write is one helper process: re-check expected values → write one field → read it back → on any failure, roll back in reverse order. The only on-disk roster artifact is a *read-only* export of a saved roster, taken over the game's TCP roster service (`native-roster-*`).
- After every Save, WPF tells the user "**Save the roster in-game to keep the changes**" (`W/src/renderer/app.js:1074`). The launcher never persists a roster itself.
- In the frozen build the editor UI is a **local HTTP page opened in the user's default browser**. It is not a launcher window. The main window's Players/Teams nav calls `roster:open` → `RosterWebServer` → `shell.openExternal` (`W/src/renderer/app.js:1956`, `W/src/main.js:475-485`, `W/src/core/roster-web-server.js`).
- **Top 3 port risks:**
  1. **Attach/readiness.** There is no menu gate for ordinary reads and writes. Readiness means "the helper finds exactly one validated player database in memory". Auto-attach retries at most every 15 s and only succeeds when the scan returns `live-roster` players. There is also a one-shot "database not found" refresh-and-retry inside Save.
  2. **Identity guards and write order.** Every save passes the *expected* old values: name, cyberface, positions, jersey, potential raw, height/wingspan raw, rating `expected`, birth values and so on. The helper rejects the save if any of them changed. Writes are field-by-field compare-and-write with read-back, and rollback runs in reverse order on failure. The renderer then **always rescans** and rebuilds its state from the fresh scan.
  3. **Season operations** suspend the whole game process (`NtSuspendProcess`) with a 2000 ms budget, and they are only allowed on three specific menu IDs. Structural edits and imports write recovery JSON *before* writing memory.

## Key source map

| Concern | file:line |
|---|---|
| Roster IPC registration + global gate (compat, cleanup, transfer-busy, season gate) | `W/src/main.js:390-393` |
| Sender allowlist / owner window | `W/src/main.js:396-397` |
| Browser editor server start + open | `W/src/main.js:475-485`; `W/src/core/roster-web-server.js:3-58` |
| Browser RPC bridge (token, ping 5 s) | `W/src/renderer/roster-browser-bridge.js:1-27` |
| Workspace switcher + auto-attach loop (5 s) | `W/src/renderer/roster-workspace.js:1-47` |
| Auto-attach service (15 s throttle) | `W/src/core/roster-auto-attach.js:4-36`; wired `W/src/main.js:1209-1221` |
| Scan (read live roster) | `W/src/core/player-research.js:666-716`; helper `RR/Program.cs:2496-2550` |
| DB locate (region scan) | `RR/Program.cs:1615-1656`, constants `RR/Program.cs:376-414` |
| Learned catalog persist | `W/src/core/player-research.js:612-664` |
| Player save (JS guard + args + report check) | `W/src/core/player-research.js:856-929`; retry `:511-527`; IPC `W/src/main.js:1431-1437` |
| Player save (helper writes/rollback) | `RR/Program.cs:585-789` |
| Rating field validation / max | `W/src/core/player-field-edits.js:3-15` |
| Bio (birthday / years pro) plan | `W/src/renderer/player-bio.js:10-23` |
| Team save | `W/src/core/player-research.js:718-754`; `RR/Program.cs:791-931`; IPC `W/src/main.js:1438-1444` |
| Uniform edit / link | `W/src/core/player-research.js:756-854`; IPC `W/src/main.js:1488-1499` |
| Custom arena clone + auto re-populate | `W/src/core/player-research.js:931-1003`; `W/src/main.js:505-509`, `:1500-1505` |
| Staff read/save | `W/src/core/team-staff.js:57-94`; `RR/TeamStaff.cs`, `RR/TeamStaffPatch.cs` |
| Rotations read/save | `W/src/core/team-rotations.js:7-32`; `RR/TeamStaff.cs` (rotation branch), `RR/TeamRotations.cs` |
| Seasons read/save/structure | `W/src/core/player-season-service.js`; `RR/SeasonStats.cs:51-74`; `RR/SeasonStructure.cs` |
| Season import | `W/src/core/player-season-import-service.js`, `player-season-import-plan.js`; `RR/SeasonImportCommit.cs`, `RR/SeasonImportModel.cs` |
| Season write serialization gate | `W/src/core/player-season-operation-gate.js:4-13` |
| Game build gate per channel | `W/src/core/game-compatibility.js:3-35` |
| Player DNA / Team DNA | `W/src/core/player-dna.js`; `W/src/core/team-dna.js`; IPC `W/src/main.js:1351-1378`, `:1461-1477` |
| 2KDB / 2KAuction | `W/src/core/twokdb-browser.js`, `twokdb-import.js`, `twokauction-import.js`; apply `W/src/renderer/twokdb-apply.js`, `W/src/renderer/app.js:1027-1055` |
| Basketball-Reference stats | `W/src/core/basketball-reference-stats.js`; IPC `W/src/main.js:1382-1386` |
| 2K23 catalog | `W/src/core/player23-catalog.js`; IPC `W/src/main.js:1387-1401` |
| Editor presets | `W/src/core/editor-presets.js`; `W/src/renderer/editor-presets.js`; IPC `W/src/main.js:1456-1460` |
| Mapped roster transfer (.2kroster JSON) | `W/src/core/roster-transfer.js`, `roster-transfer-session.js`; IPC `W/src/main.js:1323-1350` |
| Native saved-roster export | `W/src/core/native-roster-{reader,connection,export,file,bundle}.js`; IPC `W/src/main.js:1300-1322`; UI `W/src/renderer/native-roster-export.js` |
| Player editor open/save UI | `W/src/renderer/app.js:927-1083` |
| Team editor open/save UI | `W/src/renderer/app.js:1136-1407` |
| Staged edits / reset / review | `W/src/renderer/player-edit-session.js`; `W/src/renderer/editor-review.js`; `W/src/renderer/import-review.js` |
| Field schemas | `W/src/data/player-field-schema.json` (455 fields), `W/src/data/staff-field-schema.json`, `W/src/data/staff-controls.json` (175) |

## Service inventory

"Mem" = touches game process memory. R = read, W = write. All helper calls use `execFile(helperPath, args, {windowsHide:true, timeout:30000})` unless stated otherwise.

| Service (file) | IPC / RPC name | What it does | Mem | Game build gate | Busy / concurrency | Persisted files |
|---|---|---|---|---|---|---|
| PlayerResearchManager.scan (`core/player-research.js:666`) | `player-research:scan` (`scanPlayerResearch`), `player-research:status`, `roster:bootstrap` | Helper with no args → full live directory: players, teams, arenas, uniforms, origins, ratings, audio | R | none (scan) | `scanRevision` discards stale results (`:667,696,707`) | `Data/player-research/learned-player-catalog.json` (`main.js:1998`) |
| RosterAutoAttach (`core/roster-auto-attach.js`) | `roster:auto-attach` (`rosterAutoAttach`) | Poll: game running? → scan at most every 15 s | R | none | single in-flight `pending` (`:11-15`) | — |
| editPlayer (`core/player-research.js:856`) | `player:save` (`savePlayer`) | Guarded write of info, ratings, bio, audio, From | W | `roster` OLD+CURRENT (`game-compatibility.js:14`) | season gate (`player-season-operation-gate.js:4`) | — |
| editTeam (`:718`) | `team:save` (`saveTeam`) | name/city/abbr/state/logo/roster type/historic year/arena ptr (+ floor clone) | W | `roster` | season gate | `recovery/team-edit-backups/team-<guid>.json` (floor path only, `RR/Program.cs:873-878`) |
| editTeamUniform (`:756`) | `team-uniform:save` (`saveTeamUniform`) | 52 uniform controls, colors, packed flags | W | `roster` | season gate + renderer `teamUniformBusy` | — |
| linkTeamUniform (`:815`) | `team-uniform:link` (`linkTeamUniform`) | One 8-byte slot pointer to an unreferenced same-team uniform | W | `roster` | season gate | — |
| cloneArena / applyAutomaticArenas (`:931`, `:960`) | `arena:clone` (plain ipcMain, `main.js:1500`) | Clone donor arena → reserved ID; profile re-applied after each roster reload | W | `roster` | `arenaAutoPromise` singleton | `Data/player-research/custom-arena-profiles.json` (`main.js:1999`) |
| TeamStaffService (`core/team-staff.js`) | `team:staff`, `team:staff-save` | Read 32 staff slots; save one staff record (456 B) under byte masks | R/W | `roster` (save only) | `busy` | `recovery/staff-edit-backups/staff-<ms>-<uuid>.json` |
| TeamRotationService (`core/team-rotations.js`) | `team:rotations` {read, save} | 48×5 u16 rotation block | R/W | `roster` (save) | `busy` | `recovery/rotation-edit-backups/rotations-<uuid>.json` |
| PlayerSeasonService (`core/player-season-service.js`) | `player:seasons` {read, save, add-older, remove-oldest} | Season rows, awards; structural add/remove | R/W | `seasons` **CURRENT only** (`game-compatibility.js:6,16`) | `busy`, `revision` | `recovery/season-edit-backups/` (structure, written by helper) |
| PlayerSeasonImportService (`core/player-season-import-service.js`) | `player:seasons` {teams, review-import, apply-import, cancel-import} | Snapshot → plan → native commit | R/W | `seasons` | `busy`, `revision`, `pending` | `recovery/season-imports/` (helper) |
| BasketballReferenceStats (`core/basketball-reference-stats.js`) | `player:reference-stats` | HTTPS fetch + parse totals | no | — | `busy`; 5 s min spacing; 10 min cache, 4 entries | — |
| Player23Catalog (`core/player23-catalog.js`) | `player23:catalog` {status, search, player, selection, import} | Bundled/imported 2K23 player + season catalog | no | — | `player23CatalogBusy` | `<storage>/Library/Player Imports/<sha>.2k23catalog`, `selected.json` |
| Player DNA (`core/player-dna.js`) | `player-dna:transfer` {players, copy, import, export, 2kdb} | `.2kplayer` v1/v2 portable data | R (copy reads seasons) | — | `playerDnaBusy` | user-chosen file |
| twokdb-browser (`core/twokdb-browser.js`) | `player-dna:transfer` {action:'2kdb', url} | Sandboxed window scrapes 2KDB / 2KAuction player JSON | no | — | module `busy` | — |
| Team DNA (`core/team-dna.js`) | `team-dna:transfer` {import, export} | `.2kteam` portable data | no | — | `teamDnaBusy` | user-chosen file |
| EditorPresets (`core/editor-presets.js`) | `editor:presets` {list, read, save} | signature / equipment / coaching presets | no | — | `busy` (save) | `<storage>/Library/editor-presets/<uuid>.json` |
| RosterTransferSession (`core/roster-transfer-session.js`) | `roster:transfer` {status, cancel, export, preview, apply} | Mapped-field `.2kroster` export/preview; **apply disabled** | R | `roster` (apply) | `busy`; blocks all other roster IPC while busy | user-chosen file |
| NativeRosterExport (`core/native-roster-export.js`) | `roster:native-export` {list, export} | TCP 127.0.0.1:7928 read of saved roster slots 1–100 → `2KRSTR2` file | no (socket) | exe SHA-256 pin | `nativeRosterExportBusy` + class `busy` | user-chosen file (hard-link publish) |
| RosterArtwork (`core/roster-artwork.js`) | `player-research:artwork` | Portrait/logo from game archives → 512 px PNG; manual overrides | no | — | `playerArtBusy` (choose); queue of 2 | `Data/player-editor-artwork.json`; cache `Player Editor/art-cache` |
| PlayerHeaderLabels (`core/player-header-labels.js`) | `player-research:header-labels` | Archetype label from `loc/english.iff` | no | — | single pending | cache `Player Editor/header-labels` |
| BadgeIcons (`core/badge-icons.js`) | `player-research:badge-icons` | Badge icon PNGs | no | — | — | cache `Player Editor/badge-icons` |
| player-audio-preview (`core/player-audio-preview.js`) | `player-research:preview-audio` | Name clip → Ogg/Opus (worker thread, 20 s timeout) | no | — | module `active` | — |
| PlayerAnimationPreview (`core/player-animation-preview.js`) | `animation:{open,warm,catalog,request,close,progress-status}` | External worker process renders animations | no | — | max 8 sessions, 64 warm owners | animation cache |
| exportCatalog (`core/player-research.js:1005`) | `player-research:export` | JSON `nba2k27-player-asset-catalog-v2` | no | — | — | user-chosen file |
| RosterWebServer (`core/roster-web-server.js`) | `roster:open`, `roster:launcher` | Local HTTP server + token for browser editor | no | — | serial RPC queue (`tail`) for non-animation calls | — |

`roster-research` runtime folder (`W/runtime-resources/roster-research/`) contains `NBA2K27RosterResearch.exe` (this area), plus `NBA2K27Blacktop.exe`, `NBA2K27F8Watcher.exe`, `blacktop-preset-layouts.json` and `streetwear-components.bin` (Blacktop and outfits areas, not covered here).

---

### A. Editor hosting: browser workspace and IPC gate

#### 1. User-visible behavior
- In the launcher's main window, clicking the **Players** or **Teams** nav item does not switch pages. It calls `openRosterEditor(page)` (`W/src/renderer/app.js:1956`, preload `W/src/preload.js:21`). The main process starts the server and calls `shell.openExternal(url)`, and the URL ends with `?workspace=roster&section=players|teams#<64-hex token>` (`W/src/main.js:475-485`, `W/src/core/roster-web-server.js:13`).
- The page title is `2k Roster Editor` (`W/src/renderer/roster-workspace.js:3`). The section switcher `#rosterSectionSwitch` sets the heading to `Player Editor` / `Team Editor` (`:11`). While a player or team editor is open, switching sections shows the toast `Close the current editor before switching sections.` (`:6`).
- The startup text is `Loading Roster Editor…` / `Opening the player workspace.` (`:20-21`).
- The bridge adds a status banner: `Local editor · Keep the launcher open`. If a ping fails it becomes `Launcher disconnected · Reopen the editor from the launcher. Unsaved fields remain here.` (`W/src/renderer/roster-browser-bridge.js:24-25`).
- A failed RPC fetch throws `Launcher disconnected. Keep the launcher open, then reopen the roster editor from it.`. A non-JSON reply throws `Editor session expired. Reopen the editor from the launcher.` (`:9-10`).
- The **launcher button** `#rosterLauncherButton` calls `openLauncher` → `roster:launcher`, which restores or focuses the main window or re-creates it (`roster-workspace.js:16-17`, `main.js:474`).
- Closing the browser tab with unsaved edits triggers the browser's standard beforeunload prompt (`roster-workspace.js:43-46`).
- Closing the launcher while the browser editor exists asks `Closing the launcher disconnects the browser roster editor.` (`W/src/main.js:463`).

#### 2. Timing and gating
- Server: `listen(0,'127.0.0.1')`. If the OS picks port 43110 it closes and retries (`roster-web-server.js:10-11`). Timeouts: `requestTimeout=15000`, `headersTimeout=10000` (`:9`).
- RPC rules: only `POST /rpc` with matching `Origin`, `X-Roster-Token` (timing-safe compare) and `Content-Type: application/json`. Body ≤ 2 MiB, `args.length ≤ 1`, and the method must be in the `METHODS` allowlist (`:3`, `:30-36`).
- **All non-animation RPCs are serialized** on one promise chain `this.tail` (`:40`). A Save therefore can never overlap another browser RPC. Animation RPCs bypass the queue.
- Every roster channel goes through `registerRoster` (`W/src/main.js:390-393`), which checks in this order:
  1. sender allowlisted (`rosterOwner`, `:397`)
  2. not `nativeClosing`
  3. `assertGameFeature(channel,args)`, the build gate (`main.js:17`)
  4. sender re-checked, and `nativeClosing` re-checked
  5. if a roster transfer session is busy, every channel except `roster:transfer`, `roster:launcher`, `roster:bootstrap`, `animation:close|warm|progress-status` throws `Wait for the roster import to finish, or stop it from the import panel.`
  6. `seasonOperationGate.run` (see F).
- Ping every 5000 ms from the bridge (`roster-browser-bridge.js:25`). Animation preview owners expire after 120000 ms, swept every ≤ 5000 ms (`roster-web-server.js:5,21`).
- Confirmation dialogs: for browser callers (`event===rosterWebEvent`), structural season edits and season import use `request.confirmed===true` instead of a native dialog (`W/src/main.js:1414`, `:1423`). The single-row season save **always** shows a native message box, even for browser callers (`:1427-1429`).

#### 3. Mechanism
- `index.html` is served with `roster-browser-bridge.js` injected before `theme.js` (`roster-web-server.js:54`). The bridge replaces `window.modLauncher` with a Proxy. Any `modLauncher.X(arg)` becomes `POST /rpc {method:'X',args:[arg]}`, and `on*` listeners become no-ops (`roster-browser-bridge.js:21`).
- `METHODS` maps names to IPC channels, e.g. `savePlayer→player:save`, `rosterAutoAttach→roster:auto-attach`, `saveTeamUniform→team-uniform:save` (`roster-web-server.js:3`).
- **Not in `METHODS`:** `linkTeamUniform` (`team-uniform:link`), `cloneArena` (`arena:clone`) and `rosterTransfer` (`roster:transfer`). So in the browser workspace those calls return `404 Unsupported editor operation.`. They only worked from the in-window renderer path.
- The token lives in `sessionStorage['2k-roster-session']`, and the hash is stripped from the URL after it is read (`roster-browser-bridge.js:4-6`).
- Static file allowlist regex: `roster-web-server.js:52`. CSP is at `:27`.

#### 4. Build profiles
Hosting is build-independent. V2: unknown/NA.

#### 5. Rust must match
- [ ] Every editor RPC that can write runs strictly one at a time: no overlapping saves, even across tabs.
- [ ] An RPC without the per-launch token, or from a foreign Origin, is rejected (403) before any game access.
- [ ] Disconnecting the backend shows the exact disconnected banner text and keeps unsaved fields.
- [ ] Switching sections while an editor is open is refused with `Close the current editor before switching sections.`
- [ ] While a roster import/transfer is busy, every other roster call fails with `Wait for the roster import to finish, or stop it from the import panel.`
- [ ] Closing the browser tab or the editor with staged edits prompts before discarding.

---

### B. Live attach and scan (memory read)

#### 1. User-visible behavior
- Status text before any scan: `Start NBA 2K27 and refresh to read the live player directory.` (`W/src/core/player-research.js:483-486`). During a scan: `Reading the live player directory…` (`:668`). The renderer shows `Reading the live player and team directories…` and the buttons show `Reading…` (`W/src/renderer/app.js:1451-1453`).
- Success message: `<N> current live records, <M> retained NBA/WNBA records, and <K> unloaded stock/mod cyberfaces cataloged.` (`player-research.js:702`). The renderer toast is `Live player directory refreshed without game-memory writes.` (`app.js:1456`).
- Errors: if the message matches `NBA 2K27 is not running` the UI shows `Error: NBA 2K27 is not running.`; any other failure shows `Error: Could not refresh players and teams. Please try again.` (`app.js:1443-1448`). Button labels are reset to `Refresh live players` / `Refresh live teams` (`:1465-1466`). In the HTML they read `Refresh players` / `Refresh teams` (`W/src/renderer/index.html:345,375`).
- Auto-attach notice `#rosterAutoAttach` texts:
  - `Waiting for NBA 2K27. The roster will load automatically.`
  - `Waiting for the game roster to finish loading…`
  - `Waiting for a readable roster. You can also use Refresh live players.`
  - on success: `Live roster ready.`
  - if the RPC fails: `Waiting for the launcher connection. Keep the launcher open.`
  
  Sources: `W/src/core/roster-auto-attach.js:19,23,33`, `W/src/renderer/roster-workspace.js:25,34,37`.
- Rows from a retained catalog (`cataloged-roster`, `cataloged-team`, `mods-only`, stock asset) are listed, but their editors open read-only (see C1).
- Activating the Players or Teams section while `status==='idle'` triggers a refresh (`roster-workspace.js:13`).

#### 2. Timing and gating
- **Renderer loop** (`roster-workspace.js:26-40`):
  - First attempt 500 ms after load, then every **5000 ms**.
  - An attempt is skipped (rescheduled) while a player or team editor is open, while a scan is running, or while the startup overlay is visible.
  - The loop stops permanently after the first `ready`. It is a one-time attach, not a periodic redraw (comment `:22`).
  - If an editor opened while the call was in flight, the ready result is dropped and it retries in 5 s (`:33`).
- **Backend throttle** (`roster-auto-attach.js:16-34`):
  - If `NBA2K27.exe` is not running (`processIsRunning`, `main.js:1213`), it returns waiting and resets `attached=false`.
  - If already attached and the view is `ready`, it returns the cached view with no rescan.
  - It rescans at most every **15000 ms** (`nextAttempt`) and never while `status==='scanning'`.
  - Success requires `result.status==='ready'` **and** at least one player with `source==='live-roster'`. Scan exceptions are swallowed: "The process can exist before its roster is readable".
- **There is no menu or screen gate on scan.** The only readiness test is structural: the helper must find exactly one validated player database (`RR/Program.cs:1615-1656`). If it doesn't: `A unique validated NBA 2K27 player database was not found. Wait at the full main menu and refresh again.` (`RR/Program.cs:2534`). It also needs at least 1 arena (`:2537`) and at least 100 decoded players (`:2545`).
- Helper process: timeout 30000 ms, maxBuffer 16 MiB (`player-research.js:671-676`).
- **Concurrency:** each scan bumps `scanRevision`. A finishing scan whose revision is stale returns the current state without overwriting it (`:667,696,707`). A successful player save also bumps the revision (`:917`), so an older in-flight scan cannot clobber the just-saved identity.
- Every write IPC (`player:save`, `team:save`, `team-uniform:save|link`, `arena:clone`) **rescans after writing** and returns `{report, playerResearch}` (`main.js:1431-1444`, `:1488-1505`).

#### 3. Mechanism
- Helper scan (`RR/Program.cs:2528-2550`): `OpenProcess(QUERY_INFORMATION|VM_READ)` (`:69-76`), then the PID. The PID is taken from `--pid` or the single `NBA2K27` process; with none it fails `NBA 2K27 is not running.`, with more than one `More than one NBA 2K27 process is running.` (`:1605-1613`).
- Then: LocateDatabase → ReadArenas → ReadUniforms → ReadTeams → ReadPlayers → ReadPlayerAudio → ReadPlayerRatings → ReadPlayerOrigins → WriteReport.
- **LocateDatabase** (`RR/Program.cs:1615-1656`): walks `VirtualQueryEx` regions where:
  - `MEM_COMMIT`, not NOACCESS/GUARD
  - size 0x800000..0x8000000
  - u64 at base+0x60 = player count 100..8192
  - u64 at base+0x68 == base+0x680
  - the first 3 records have a valid UTF-16 name
  
  It requires **exactly one** match.
- Constants (`RR/Program.cs:376-414`):

  | Record | Offsets |
  |---|---|
  | player | stride 0x4F8; last name +0, first name +0x28 (UTF-16, 0x28 bytes each); appearance ptr +0x78; weight float +0x108; cyberface u16 +0x110; jersey +0x115 bits 3-9; positions +0x12D (bits 0-2 primary, 3-5 secondary); potential +0x45D; cached overall +0x1AC |
  | team | dir base+0xD0/0xD8, stride 0x1550; name +0x2FA; city +0x32C; cityShort +0x350; stateShort +0x35E; logo +0x1AE; arena ptr +0xEE0; uniqueId +0xF20; roster type +0x123F bits 2-6; historic year +0x154A bits 3-9; uniform slots +0xDE0 ×32 |
  | uniform | dir base+0x290/0x298, stride 0x170 |
  | arena | dir base+0xB0/0xB8, stride 0x1278; name +0; file +0x7E; floor +0xA4; id +0x792 |

- Ratings, bio and audio use the roster root at **image+0x9A47EB8** (`RR/Program.cs:669,1943,1998`). That is the CURRENT build RVA. `BlacktopTool.cs:187` shows the OLD build value `0x9a43eb8`, but Program.cs hard-codes the CURRENT one.
- Report schema `nba2k27-live-player-research-v1` with `writes:0`. JS refuses anything else: `The read-only roster helper returned an invalid safety report.` (`player-research.js:686-688`).
- The JS merge (`:670-705`) runs in parallel with the helper:
  - reference face-ID catalog
  - `<gameRoot>/manifest` cyberfaces + uniform bases
  - loose mod cyberfaces
  - `data/player-catalog-seed.json`
  - the learned catalog
  - `data/team-catalog-seed.json`
  
  Live teams win by `teamId`. Catalog-only teams are tagged `cataloged-team` with `recordAddress:''` (`:184-212`).
- **Learned catalog**: after every successful scan, all `live-roster` + `cataloged-roster` players and all teams are written to `Data/player-research/learned-player-catalog.json`. Schema `nba2k27-learned-player-catalog-v1`, compact JSON, written to a temp file with `wx`, fsync'd, then renamed (`:612-664`). The record key is `normalizedName|cyberfaceId|recordIndex` (`:180-182`).
- Player identity used for every edit: `recordIndex` + `cyberfaceId` + `firstName` + `lastName`, plus `source==='live-roster'` (`:493-499`). Team identity: `recordIndex` + `teamId` + `name`.

#### 4. Build profiles
- Scan is pattern-based and runs on OLD (`6A974D13:368C3000`) and CURRENT (`6AA2013D:36558000`) (`W/src/core/game-compatibility.js:3`). The ratings, audio and bio sub-reads use the CURRENT root RVA only.
- V2 (`PE 0x6AC53765/0x35BDE000`): the Rust port uses its own `roster_v2_access`/`roster_v2_readonly` snapshot (`NOTES/roster-v2-readonly-wiring.md`).
  - Proven on V2: the four owner directories (players 0x10/0x4F8, arenas 0x60/0x1278, teams 0x80/0x1550, uniforms 0x290/0x170), plus staff +0x180/0x1C8 and season stats +0x1D0/0x40 (`ART/editor-services-v2-research-20261017.md:11-15`).
  - player+0x60 → team pointer is proven live on V2 (`:98`).
  - The V2 projection still leaves `audio`, `archetypeKey`, `heightRaw`/appearance and `fromIndex` null (`:22`).

#### 5. Rust must match
- [ ] With no game running, auto-attach shows `Waiting for NBA 2K27. The roster will load automatically.` and does not scan.
- [ ] With the game running but no readable roster, the backend rescans at most once per 15 s. The UI retries every 5 s and shows `Waiting for a readable roster. You can also use Refresh live players.`
- [ ] Auto-attach declares ready only if at least one live (non-catalog) player was decoded, and then it stops polling.
- [ ] Auto-attach never re-renders while a player or team editor is open.
- [ ] A scan finishing after a newer scan or save never overwrites newer state.
- [ ] Every successful write is followed by a full rescan, and the UI re-renders from that rescan, not from local guesses.
- [ ] Rows that are not from the current live roster stay visible but cannot be edited.
- [ ] Manual refresh failure says exactly `Error: NBA 2K27 is not running.` or `Error: Could not refresh players and teams. Please try again.`

---

### C. Player save (live memory write)

#### 1. User-visible behavior
- The editor modal title is the player's name, with an identity line `Record #<n> · <address>` (`W/src/renderer/app.js:937-940`).
- Editable controls:
  - First/Last name, Cyberface, Jersey (`00` → raw 100)
  - Position and Secondary from `['PG','SG','SF','PF','C','N/A']` (`:915`)
  - From (origins dropdown; first option keeps the current college or `Unassigned / unavailable`)
  - Height as ft + in
  - Wingspan in inches
  - Weight
  - Potential
  - Birth date, Years pro
  - Full-name audio ID
  - Rating tabs
- Disabled cases:
  - Every control, when the player is not `live-roster`.
  - Audio ID, when `audio.fullNameId` is not an integer.
  - From, when `fromIndex` is not an integer or no origins exist.
  - Birth date and years pro, unless `bioLayoutVerified===true` (`:983-986`).
- Boundary text (editable): `Edits are read back and verified, with rollback on failure. Only the player-specific full-name audio ID is editable; shared name mappings remain read-only. Save the roster in-game to retain changes.`. For a non-live player: `This is not a live roster record. Its known data is shown for reference, but editing is disabled until that exact roster is loaded and refreshed.` (`:988-990`).
- **Save & verify** (`#savePlayerEdit`) is refused with these messages, in order (`app.js:1057-1066`):
  - `Refresh live players and reopen this player before saving again.` (when `editorNeedsRefresh`)
  - `Wait for the Player DNA operation to finish.`
  - `Apply selected seasons or discard the season import before saving player values.`
  - `Save or reset your edited season rows first using Save season.`
- While saving, the button reads `Saving…` and the status line reads `Saving player changes and verifying them…`.
- On success: re-render, the editor closes, and the toast says `Player saved and verified. Save the roster in-game to keep the changes.` (`:1071-1074`). On failure the editor stays open with edits kept, and the error goes to both the status line and a toast.
- Payload-level errors (thrown in the renderer, `app.js:997-1025`):
  - `Birthday cannot be cleared. Enter a complete date or reset the field.`
  - `Enter years pro, including 0 for a rookie.`
  - Measurement errors: `Enter whole feet and inches from 0 to less than 12.`, `Height is outside the supported game range.` (`W/src/renderer/player-measurements.js:6-10`).

#### 2. Timing and gating
- Build gate: the `roster` feature must be supported (OLD or CURRENT exe identity, read from the first 4 KB of `<gameRoot>/NBA2K27.exe`, cached by size/mtime/ctime). Otherwise the error is `Roster / Team editor: This game build is not supported by this feature. No new edit will be attempted.` (`W/src/core/game-compatibility.js:12-34`).
- The season gate makes `player:save` exclusive with other writers and with season edits. If one is already running: `Wait for the current roster edit or season import to finish.` (`W/src/core/player-season-operation-gate.js:8-12`).
- **No menu gate** for player saves.
- **Retry on DB-not-found** (`player-research.js:511-527`):
  1. If the helper error matches `unique validated NBA 2K27 database was not found`, rescan once.
  2. If the rescan fails: `The live roster database was temporarily unavailable. Wait for the roster screen to finish loading and try again.`
  3. If the guarded fields of the player changed between the original and the refreshed scan: `The live roster changed while the editor was open. The player list was refreshed; reopen the player before saving.`
  4. Otherwise retry the edit once. If it fails the same way again: `The live roster database is still loading. Wait a moment and try again; the player list has already been refreshed.`
- The helper's "changed" errors are mapped to `This player or roster changed since it was loaded. Refresh live players and reopen the player before saving again. Your staged edits have not been retried.` (`player-research.js:887-889`).
- Helper timeout: 30000 ms.

#### 3. Mechanism
- **JS side (`player-research.js:856-929`):**
  - The input is validated (`validatePlayerEditInput`, `:308-362`). Names are ≤ 19 characters, missing numbers are never treated as zero, and at least one of first/last name must be set.
  - The live player must match on recordIndex, cyberface, first and last name, and must have integer positionId/secondaryPositionId/jerseyNumberRaw/potentialRaw/heightRaw/wingspanRaw and a finite weight. Otherwise: `The live <field> value is unavailable; this player was left untouched.`
  - Potential encoding: unchanged → keep `potentialRaw`; else `potential<=25 ? 0 : min(255,(potential-25)*3-1)` (`:876-878`).
  - Height/wingspan raw = `round(cm*100)`.
  - Ratings: `ratingEdits` entries are `{index, expected, value}`, checked against `live.ratingsRaw[index]===expected`. Any mismatch: `Player fields changed. Refresh before saving.` (`W/src/core/player-field-edits.js:11`). Unchanged entries are dropped. The rest are sent as `--rating-edits i:exp:val;…`.
  - `maximum(f)` = −1 (read-only) for `PotentialRaw` or `readOnly`; else `min(2^bits-1, f.max ?? (money?200000000 : choices?len-1 : badges?5 : tendencies?100 : 2^bits-1))` (`player-field-edits.js:3`).
  - Renderer rating encoding (`W/src/renderer/player-ratings.js:131-137`):
    - attributes shown 25–99 → raw `v<=25?0:min(255,(v-25)*3-1)`; display `min(110,25+floor((raw+1)/3))` (`:6`)
    - money in millions with 6 decimals
- **Helper (`RR/Program.cs:585-789`):**
  1. Opens the process with VM_WRITE|VM_OPERATION (`:69-76`), locates the DB, and reads the 0x4F8-byte record.
  2. Checks each expected value against memory: last/first name, cyberface, positions, jersey, potential raw, appearance height/wingspan. Failure: `The selected player changed before Save; refresh the roster and try again.` (`:683-693`).
  3. Audio: the expected full-name ID must equal i16 at +0x2E8 (`:674-679`).
  4. From: the origin directory index must match and be selectable (`:647-655`).
  5. Ratings/bio need `BioLayoutVerified` and an unchanged record. The rating owner slot is `[[image+0x9A47EB8]+0xA8]+0x18`, and it must still equal the player table on every write (`:657-671`, `:732`).
  6. **Write order** (`:699-722`):
     1. last name (0x28 B)
     2. first name (0x28 B)
     3. weight (float)
     4. cyberface (u16)
     5. jersey (packed u16)
     6. positions byte
     7. potential byte
     8. appearance (height u16 + wingspan u16 at `[+0x78]`)
     9. *then* From pointer at +0xB0 (8 B)
     10. bio patches
     11. one op per changed rating **byte**, refused if it overlaps any earlier op
     12. *last*, audio i16 at +0x2E8
     
     All 8 base ops are always written, even if their values are unchanged.
  7. For each op: re-check origin stability and the rating owner, compare memory with `Before` (`A player field changed before its write.`), write, then read back (`A player-field read-back check failed.`) (`:728-739`).
  8. Final verification: name/cyberface/audio identity, and bio re-read (`:741-752`).
  9. On failure: reverse-order rollback. A field is skipped if it no longer holds our `After` value. The error message is suffixed with ` All applied fields were rolled back.` or ` Rollback verification failed.` (`:755-772`).
  10. Report: `nba2k27-player-edit-v1`, `writes=operations.Count`, `ratingWrites`, `bioWrites`, `readBackVerified:true`, `rollbackOnFailure:true`, `gameCodeHooks:0` (`:774-786`).
- JS report check: `writes === (editAudio?9:8) + ratingWrites + (editFrom?1:0) + bio.writes` and `ratingWrites ≤ 4×edits`. Otherwise: `The player editor failed its guarded write report checks.` (`player-research.js:893-900`).
- After success the JS patches its cached state with the committed values (only if the pid and playerTable are unchanged) before the IPC handler rescans (`:917-927`).
- **Undo:** there is no game-side undo after a verified save. "Reset staged edits" only reverts UI fields (see D). Rollback exists only inside a failing helper transaction.

#### 4. Build profiles
- OLD + CURRENT per the gate. The bio, ratings and audio paths depend on the CURRENT RVAs (`0x9A47EB8`, `PlayerOrigins.cs:42` setter at `0x2DABB48`).
- V2 port (`NOTES/qa048-roster-v2-saves.md`):
  - Writable: first/last name, cyberface (GraphicId), positions, jersey, birth date, years pro, potential, editable rating fields.
  - Refused: weight (float 0x108 unproven), height/wingspan (appearance pointer unproven), audio, From.

#### 5. Rust must match
- [ ] Save sends expected-old values for every guarded field, and the backend refuses if any differ, without writing.
- [ ] Field writes are compare-before → write → read-back, one field at a time. Any failure rolls back already-applied fields in reverse order and reports whether rollback verified.
- [ ] Changed rating bytes are written as single-byte ops and never overlap identity fields.
- [ ] `database not found` causes exactly one refresh plus retry, with the exact three messages above.
- [ ] After a verified save the editor closes, the list re-renders from a fresh scan, and the toast says `Player saved and verified. Save the roster in-game to keep the changes.`
- [ ] Save is refused while a Player DNA dialog, a pending season import or unsaved season rows exist (exact messages above).
- [ ] Bio fields are disabled unless the bio layout is verified for that player. Clearing the birthday is refused.
- [ ] Potential raw encoding: `0` if ≤ 25, else `(p-25)*3-1` capped at 255. It is unchanged if the displayed potential is unchanged.

---

### D. Player studio UI: tabs, staged edits, review, undo

#### 1. User-visible behavior
- Tabs (`W/src/renderer/index.html:653-667`): Player info, Attributes, Development, Draft info, Injuries, Tendencies, Play types, Badges, Hot zones, Signature, Contract, Equipment, Stats, Awards, Player DNA. Empty tabs show `No editable fields are available in this section yet.` (`W/src/renderer/player-studio.js:22`).
- Draft and Injuries are read-only, with the note `Read-only raw game data. Calendar encoding and injury lifecycle need verification before editing can be enabled.` (`W/src/renderer/player-ratings.js:122`).
- Hero header (`player-studio.js:38-58`):
  - position | #jersey, height ft′in″, weight lb, From, years pro, age, personality, archetype, overall
  - unmapped values show `—` with the tooltip `Not mapped in the live roster yet.`
  - artwork buttons read `Add portrait` / `Change portrait` and `Add team logo` / `Change logo`
  - a chosen image is stored in the app with the message `Artwork saved in the app. Game files are unchanged.`
- Staged edits (`W/src/renderer/player-edit-session.js`):
  - Status reads `Unsaved changes` / `No staged changes` / `Import verified · Other edits unsaved` / `Import verified · Save roster in-game`.
  - Each changed field gets a per-field `↶` reset button.
  - `Reset staged edits` asks `Reset all staged edits to the values loaded when you opened this player? Saved game changes are not undone.` (`:41`).
  - `Review changes` opens a Field/Before/After table with per-row Remove, titled `Review player changes` and noted `Review only. No game data is written here. Remove a row to discard just that staged edit.` (`W/src/renderer/editor-review.js:6`).
  - A quick-switch select (`Choose a loaded player`) lists live players.
- Leaving with edits asks `Discard unsaved player and season changes?`. During a save or DNA operation: `Please wait for the current editor operation to finish.` (`player-edit-session.js:23-26`).
- Measurements are display units only; cm are kept internally. If the user doesn't touch the ft/in or inch fields, the original cm value is preserved exactly (`W/src/renderer/player-measurements.js:1-11`).

#### 2. Timing and gating
- Baseline is captured on the `player-editor:open` event. After a verified import (`player-editor:committed`), only the committed fields get a new baseline (`player-edit-session.js:43-63`).
- The import-review dialog closes itself as "not accepted" if the editor reopens or commits during review (`W/src/renderer/import-review.js:23-25`).

#### 3. Mechanism
- Pure renderer state. The rating schema is `W/src/renderer/player-rating-schema.js`, generated from `W/src/data/player-field-schema.json`: 455 fields by group — stats 28, attributes 53, tendencies 99, personality 16, badges 53, contract 20, appearance 5, signature 65, hotzones 14, equipment 82, playtypes 5, development 6, draft 3, injuries 6.
- Each field carries `{key,label,group,offset,shift,bits,rva,[choices|money|max|readOnly]}`.
- Presentation grouping: `W/src/renderer/player-editor-layout.js:3-58`. Personality pairs: Friendliness, Ego, Motivated (`:50-54`).

#### 4. Build profiles
UI only. V2: same UI; fields with null V2 values are blank and disabled (`NOTES/qa048-roster-v2-saves.md`, browser-adapter).

#### 5. Rust must match
- [ ] Reset only reverts staged UI values. Its confirm text says saved game changes are not undone.
- [ ] Review dialog rows can be removed individually, and nothing is written from it.
- [ ] Untouched height/wingspan round-trip to the identical cm value (no ft/in rounding drift).
- [ ] Draft and Injuries tabs are read-only, with the exact note.

---

### E. Player DNA, 2KDB/2KAuction import, editor presets

#### 1. User-visible behavior
- **Player DNA tab** sections (`W/src/renderer/player-dna.js:6`): Player info, Appearance, Full-name audio ID, Attributes, Development, Tendencies, Play types, Personality, Badges, Hot zones, Signature, Equipment, Contract & preferences, Stat highs, Season history (review selected years).
- Actions:
  - Export: `Player DNA exported. No portraits, game files, addresses, or local paths are included.`
  - Import from file
  - `Copy from another player`
  - `Import ratings from 2KDB or 2KAuction`
- Imports are staged through an ImportReview dialog: Field/Before/After, `N changed · M unchanged`, Cancel / Stage changes (`W/src/renderer/import-review.js:14-27`).
- **2KDB/2KAuction:**
  - The user pastes a link. A sandboxed 1100×800 window titled `2KDB — read player data` / `2KAuction — read player data` opens (`W/src/core/twokdb-browser.js:11`).
  - Status: `Opening the ratings source. Let the selected player page load; close its window to cancel.`
  - The import review uses the action button `Apply & verify` and the note `These selected fields will be written and read back for verification. Other fields stay unchanged. …` (`W/src/renderer/app.js:1032`).
  - **This writes immediately through `savePlayer`.** It does not just stage.
- **Presets** bar (`W/src/renderer/editor-presets.js:5-17`):
  - Controls: name input (≤ 60), `Save preset`, a list, `Stage preset`.
  - Messages: `Preset saved in Library/editor-presets.`, `Preset staged. Review, then Save & verify.`
  - Kinds: `signature`, `equipment`, `coaching` (staff).

#### 2. Timing and gating
- 2KDB window: polls every 1500 ms for up to 120000 ms (`twokdb-browser.js:18-22`). On timeout: `2KDB import closed or timed out. No player values were changed.`. Only one at a time: `A ratings import is already open.`
- 2KDB apply is refused if the editor needs a refresh: `Refresh live players and reopen this player before importing again.` (`app.js:1030`).
- After an apply, if the fresh player is not found: `The import was saved, but the updated player could not be reloaded. Refresh live players and reopen the player before another edit.`. Save is then disabled (`:1037`).
- `player-dna:transfer` is blocked by the season gate while a write or season op is running (`player-season-operation-gate.js:10`).

#### 3. Mechanism
- `.2kplayer`:
  - Shape: `{format:'2kplayer',version:1|2,game:'NBA2K27',schemaId,name,info,fields,[seasonHistory]}`. `schemaId` = SHA-256 of the schema JSON. Older compatible IDs come from `data/player-schema-legacy-ids.json` (`W/src/core/player-dna.js:3-8`).
  - Size ≤ 256 KB. Info rules are at `:9-13`.
  - `fromPlayer` requires a `live-roster` player with full `ratingsRaw`. It skips birthDate/yearsPro unless `bioLayoutVerified` (`:38-48`).
  - Version 2 carries season history, read through the helper (`W/src/main.js:1359-1362`).
- 2KDB extraction parses `self.__next_f.push([1,"…"])` JSON literals and never executes site code (`W/src/core/twokdb-import.js:6-15`). Mapping (`:21-67`):
  - attributes 25–99 → raw `(v-25)*3`
  - tendencies 0–100
  - badges 0–5
  - hot zones 0–2
  
  The network allowlist is `2kdb.net`, `img.2kdb.net`, `challenges.cloudflare.com` (2KAuction: `2kauction.com` + cloudflare). Downloads and permissions are denied, and navigation is locked to the source origin (`twokdb-browser.js:8-16`).
- `twokdbApply.prepare` builds the full `savePlayer` payload from the live player plus imported fields. From is applied only on a unique case-insensitive origin match (`W/src/renderer/twokdb-apply.js:3-40`).
- Presets:
  - File: `<storage or userData>/Library/editor-presets/<uuid>.json`, ≤ 32 KB, written to a `wx` temp file, fsync'd, renamed. Linked folders are refused (`W/src/core/editor-presets.js:15-22`).
  - Values are validated against the schema group and `maximum`.
  - Coaching presets exclude Identity/Contract groups, text, faceId, soundId and wage (`:5`).

#### 4. Build profiles
File-only, except copy-with-seasons (helper) and apply (player save). V2: DNA on V2 tabs is refused (`NOTES/qa048-roster-v2-saves.md`).

#### 5. Rust must match
- [ ] DNA export contains no addresses, team links, file paths or process IDs. Import rejects a different schemaId with `This player DNA uses a different field schema. Export it again with this launcher version.`
- [ ] The 2KDB/2KAuction window is sandboxed to the allowlisted hosts, times out after 120 s, and leaves values unchanged on close.
- [ ] 2KDB apply shows Before/After review with `Apply & verify`, then performs a guarded player save and re-baselines only the imported fields.
- [ ] Presets stage values only. Writing still requires Save & verify.

---

### F. Seasons, awards, stats import, season import, 2K23 catalog

#### 1. User-visible behavior
- The Stats/Awards tabs load on open: `Open Stats to load season history.`, `Loading season history…`. Buttons: `Refresh seasons`, `Add older season`, `Remove oldest season`, `Import stats`, `Save season`, `Save awards` (`W/src/renderer/player-seasons.js`).
- Success: `Season saved and verified. Save the roster in-game to retain it.`. Cancel: `Save canceled. Your edits are still here.`
- A shared season row warns: `This season history is shared by N player entries in the currently loaded roster. Saving will update all N linked entries. Other roster files will not be changed.` (`W/src/core/player-season-stats.js:26`).
- Native confirm dialogs:
  - **Save season history?**, buttons Cancel / Save season (`W/src/main.js:1427-1429`)
  - **Change season history?**, detail `Only this player is affected. Save the roster in-game to retain the change. Recovery snapshots are kept in the launcher folder.`, buttons Cancel / Add older season | Remove oldest season (`:1424`)
  - **Import selected seasons?**, buttons Cancel / Apply selected seasons (`:1415`)
- The structure confirm text ends with `Stay at the main menu or roster editor, outside gameplay. Use a backup roster. The game will pause briefly while this change is verified.` (`W/src/core/player-season-service.js:38`).
- Import stats (Basketball-Reference): `Paste an NBA player page URL. Select regular-season years and normal destination teams below. Ratings and awards stay unchanged.` (`W/src/renderer/player-stats-import-ui.js`). The import message says to keep the roster at the roster-edit player list (or inactive Home / Quick Play) (`W/src/core/player-season-import-service.js:79`).

#### 2. Timing and gating
- Build gate: `seasons` = **CURRENT only** for save/apply-import/add-older/remove-oldest (`W/src/core/game-compatibility.js:6,16`).
- `PlayerSeasonOperationGate` (`player-season-operation-gate.js:4-13`):
  - Writers are `player:save`, `team:save`, `team-uniform:save`, `team-uniform:link`, plus season actions save/add-older/remove-oldest/review-import/apply-import.
  - While busy, any writer, any `player:seasons` call and `player-dna:transfer` throws.
- Single-row save: one 64-byte write. **No process suspend** (`RR/SeasonStats.cs:58-69`).
- Structural add/remove and import commit: `NtSuspendProcess` on the game. A verification that exceeds **2000 ms** throws `Season verification took too long.` (`RR/SeasonStructure.cs:77-80`, `RR/SeasonImportModel.cs:194-197`, `RR/SeasonImportCommit.cs:102-108`). Dispose always resumes.
- Import commit **menu gate** (`RR/SeasonImportCommit.cs:45-50`, `RR/SeasonImportModel.cs:31-38`):
  - current == alternate menu, active byte 0, settings kind 0, **and** (id `0x6509837B` & stage 0|1, or `0x079964C0` & stage 1, or `0x0B888544` & stage 0).
  - Error: `Stay at the NBA 2K27 roster-edit player list, Home or Quick Play menu before importing seasons. Close in-game player-edit/save dialogs. No gameplay/loading writes are allowed.`
  - Menu statics (CURRENT): table 0xAC77660, current 0xAC77E28, alternate 0xAC77E24, settings 0xAACB9A0, active 0xAACC310, stage 0xAEB69D0+0xEA8. Identity anchors: `NBA_MainMenu_PlayNow` at 0x77F9368, plus code at 0x31436D0 and 0x35BC270 (`:79-80`).
- Basketball-Reference: 15000 ms socket timeout, 20000 ms abort, ≥ 5000 ms between requests, 10 min cache (4 entries), ≤ 2 redirects, 4 MiB cap. HTTP 403/429 → `Basketball-Reference blocked this request (HTTP n). Try again later. No stats were imported.` (`W/src/core/basketball-reference-stats.js:45-62`).

#### 3. Mechanism
- Helper `--season-stats <base64 JSON>` handles read, save and structure (`RR/SeasonStats.cs:51-74`). A token (SHA-256 of pid, root, owner, table, stats, player, slot, index, raw, links) must match before a write.
- Season directory: i16[33] at player+0x130 (25 regular slots). It refuses players with a non-zero private-history pointer at +0xE0.
- Season records are 64 B in the owner+0x1D8 table (count +0x1D0). Layout:
  - team pointers +0 / +8
  - year−1900 at +0x11
  - free-chain next at +0x12
  - refcount at +0x3C bits 13-20
  
  Sources: `W/src/core/player-season-import-plan.js:6-43`, `ART/editor-fields-v2-research-20261017.md:152-160`.
- Stat bitfields: `W/src/core/player-season-stats.js:5-11`. Awards bits: `W/src/renderer/player-season-awards.js:2-14`.
- Save validation: gs ≤ gp; dd/td ≤ gp; made ≤ attempts; 3PT ≤ FG. Points ≠ 2·FGM+3PM+FTM gives a warning only (`player-season-stats.js:47-59`).
- Import (`player-season-import-service.js`):
  1. `--season-import-snapshot` (read-only, `writes:0`, 32 MiB buffer)
  2. JS `plan` builds the full after-image of players/stats/pool, never evicts history, and keeps within 25 regular slots (`player-season-import-plan.js:52-109`)
  3. user confirm
  4. `--season-import` with JSON on stdin (≤ 64 KiB), `expectedDigest` = SHA-256(after players+stats+pool)
  5. the helper writes recovery JSON into `backupRoot` before writing; the JS verifies `afterDigest`, `allocated`, `originYear` and that the backup is inside `backupRoot` (`:73-90`)
- Recovery folders: `recovery/season-edit-backups`, `recovery/season-imports` in organized storage, else `Data/season-edit-backups`, `Data/season-imports` (`W/src/main.js:1409`, `:1420`).
- 2K23 catalog: bundled `assets/player23-catalog/players.2k23catalog`, or an imported one stored as `<root>/Library/Player Imports/<sha256>.2k23catalog` with a `selected.json` pointer. Files are read stably (size/mtime/ctime must not change during the read), max 32 MB (`W/src/core/player23-catalog.js:72-95`).

#### 4. Build profiles
- WPF: CURRENT only.
- V2 (`ART/editor-fields-v2-research-20261017.md:142-215`): all 20 totals and 12 awards match. Pool callbacks: old {0x2EB59E0, 0x2EB59B0, 0x2F9A7A0, 0x2F9A760} → V2 {0x2EEB980, 0x2EEB950, 0x2FD0870, 0x2FD0830}. V2 menu statics: table 0xAD07408, current 0xAD07BD0, alternate 0xAD07BCC, settings 0xAB5CE20, active 0xAB5D790, stage 0xAF46870+0xEA8.
- Live run 1 PASS for directory and refs (`ART/editor-services-v2-research-20261017.md:99`). The V2 SafeMenu IDs are still unobserved.

#### 5. Rust must match
- [ ] A single season row save writes exactly 64 bytes after token re-check, reads back, and restores on failure. There is no process pause.
- [ ] Add-older, remove-oldest and import pause the game process, finish verification within 2000 ms or roll back, and always resume.
- [ ] Import is refused outside the allowed menus with the exact message. A recovery file is written before any memory write.
- [ ] Remove-oldest is refused for shared seasons with `Shared-season removal is not supported yet.`. Add-older is refused at slot ≥ 24 or year ≤ 1900.
- [ ] Season operations never overlap player/team saves or DNA transfers.
- [ ] Basketball-Reference requests are rate-limited to one per 5 s and cached for 10 min.

---

### G. Team editor: info save, arena/floor, custom arena

#### 1. User-visible behavior
- Team list columns: record #, name/city, Team ID, roster type/historic year, arena (ID/file), logo. A live row's tooltip is `Click to view and edit this live team`. Otherwise it is `Load this team’s roster and refresh to edit it` (`W/src/renderer/app.js:1079-1129`).
- Editor tabs: info, uniforms, arena, staff, rotations, dna. The main **Save & verify** button is hidden on uniforms/staff/rotations/dna (`app.js:1150-1162`).
- Roster type select shows 32 entries, `n — Name`, from `Normal, Historic, User, Online, All-Time, Classic, WNBA, G League, International, College, High School, Special, MyTEAM, Blacktop, Practice, Unused`, else `Unknown / reserved` (`app.js:1131-1134`, `:1233-1239`).
- Boundary text (live): `Save rechecks this team ID and original name, verifies every field, and rolls back all applied fields if any check fails.` (`:1294-1296`).
- While saving the button reads `Verifying & saving…`. On success: `Saved and verified all <n> fields for team ID <id>.`, and the editor closes (`:1331-1361`).
- Custom arena: donor list (populated arenas), target list (IDs 996–1045, labeled `Available` or the existing name; prefers 999). The defaults for an empty target are name `Arena Test`, file `arena_<id>_int`, floor `arena_<id>_int_floor` (`:1270-1298`). The button reads `Create, verify & link` / `Cloning & verifying…`. Success: `Custom arena ID <id> created, linked, and saved for automatic startup.` (`:1300-1329`).

#### 2. Timing and gating
- `roster` build gate on `team:save` and `arena:clone`. Season gate on `team:save`.
- A team is editable only if it is `live-team`. Messages: `That team is retained from another roster. Load its roster and refresh before editing it.`, `The selected team changed in the launcher. Refresh and try again.` (`W/src/core/player-research.js:722-724`).
- **Automatic arena re-apply** (`player-research.js:954-1003`):
  - Started by the session start callback (`W/src/main.js:505-509`) and after a clone.
  - Loop while `NBA2K27.exe` is running (tasklist, 5000 ms timeout, `:42-48`).
  - No profiles → sleep 1000 ms.
  - Otherwise run `--arena-status`. For each profile whose target isn't populated with the same name/file/floor, run `--clone-arena --populate-only`. Then sleep **750 ms**.
  - Status messages:
    - `N custom arena profile(s) monitored · K restored after a roster reload`
    - `Waiting for the arena table after a roster transition: …`
    - `Custom arena monitoring stopped when NBA 2K27 closed.`

#### 3. Mechanism
- Team helper `--write-team` (`RR/Program.cs:791-931`):
  - Validates the team and arena directories and requires the team uniqueId and name to match.
  - The arena ID must resolve to exactly one record.
  - **Write order** (`:861-870`): name (48 B UTF-16) → city (36) → cityShort (14) → stateShort (14) → mainLogo (72) → roster-type byte → historic-year u16 → arena pointer (8).
  - Before every op, the team header, arena header and the whole expected team record are re-compared.
  - Each write is read back, then the full record is compared at the end.
  - Rollback is in reverse order.
  - JS expects `writes==8`, or 9 with the floor path (`player-research.js:749`).
- Floor change (`--floor-file`, `RR/Program.cs:839-855`, `:873-878`): the floor must exist in the live arena catalog (`Choose a floor from the current live arena catalog.`). If the donor's floor differs, an unused arena slot is cloned with the new floor and ID (`TeamArenaPair.Target`).
- **Recovery JSON `team-<guid>.json` (schema `nba2k27-team-recovery-v1`) is fsync'd to `recovery/team-edit-backups` before any write** (`main.js:1440`).
- Arena clone `--clone-arena` copies the donor record over the target and patches name/file/floor/id, plus team+0xEE0 unless `--populate-only`. Expected report: `nba2k27-arena-clone-v1`, `recordsWritten` 2 (1 when populate-only), read-back and rollback (`player-research.js:560-582`).
- Mods assets `levels/<stem>.iff` must exist under `modsRoot` (`:550-558`).
- Profiles live in `Data/player-research/custom-arena-profiles.json`, schema `nba2k27-custom-arena-profiles-v1` (`:540-548`).

#### 4. Build profiles
- OLD + CURRENT gate.
- V2: team text fields are writable. Roster type (V2 reads 6 bits at 0x123C>>26; legacy wrote 5), historic year and arena/floor are **refused** (`NOTES/qa048-roster-v2-saves.md`). team+0xEE0 is unmapped on V2 (`ART/editor-services-v2-research-20261017.md:44`).

#### 5. Rust must match
- [ ] Team save re-verifies team ID and original name, writes the 8 fields in the listed order with read-back, and rolls all of them back on any failure.
- [ ] The floor change path writes a recovery file before any memory write.
- [ ] Custom arena profiles are re-applied automatically after roster reloads every 750 ms while the game runs, and the loop stops when the game exits.
- [ ] Catalog (non-live) teams open read-only with the exact boundary message.

---

### H. Team uniforms (edit, link) and uniform transfer

#### 1. User-visible behavior
- Each owned uniform slot has its own **Save & verify** button (`W/src/renderer/team-uniforms.js:305`). The button reads `Verifying & saving…`. Success: `Saved and verified uniform settings for <file> (slot <n>). Save the roster in-game to retain it.` (`W/src/renderer/app.js:1363-1386`).
- Unknown enum values show as `Unknown (n) · unchanged` (disabled option) (`team-uniforms.js:173`).
- Available uniforms (unreferenced, same teamId, stock art present) can be added to an empty slot. The button reads `Adding & verifying…`. Success: `Added and verified <file> in slot <n>. …` (`app.js:1388-1407`).

#### 2. Timing and gating
- `roster` gate. Season gate. Renderer `teamUniformBusy` makes the list inert while saving.
- Drafts for other slots are preserved across the post-save rescan (`app.js:1372-1381`).
- Link requires a `<gameRoot>/manifest` entry for the base filename. Otherwise: `The stock art resource for <file> is unavailable; the jersey was not linked.` (`player-research.js:829-832`).

#### 3. Mechanism
- `--write-team-uniform`:
  - Expected values for headband home/away/logo, 21 color fields, 4 packed flag words (+0x160/164/168/16C) and 28 packed fields (`player-research.js:12-41`, `:756-813`).
  - Report schema `nba2k27-team-uniform-edit-v4`, `writes===52`, `physicalWrites` 0–25.
  - The helper limits each flag word to an editable mask (`RR/Program.cs:1058-1070`).
- `--link-team-uniform`: one 8-byte pointer write at team+0xDE0+slot·8. Report `nba2k27-team-uniform-link-v1`, `physicalWrites===1`, `restoredToNull===false` (`player-research.js:833-852`).
- Team DNA uniform transfer matches destination uniforms by `{typeId, sideId, designationId}` (`W/src/renderer/team-uniform-transfer.js:8-9`).

#### 4. Build profiles
- OLD + CURRENT.
- V2: all 52 uniform controls are writable (UNIFORM serializer 0x687590 proof). Slot linking is unported (`NOTES/qa048-roster-v2-saves.md`).

#### 5. Rust must match
- [ ] A uniform save is refused if any expected color, flag word or packed field differs from the loaded value (`The selected uniform … changed in the launcher. Refresh and try again.`).
- [ ] Unknown enum values are displayed and left unchanged, never coerced.
- [ ] Linking writes one pointer, only into an empty slot, only for an unreferenced uniform with the same teamId and an existing stock art resource.

---

### I. Team staff

#### 1. User-visible behavior
- Read status: `Stage mapped settings, then Save & verify. IDs and assignment links stay unchanged; save the roster in-game to retain edits. Raw attribute and badge levels are shown without tier conversion.` (`W/src/core/team-staff.js:73`).
- Saving: `Saving and verifying…`, then `Saved & verified. Save the roster in-game.` (`W/src/renderer/team-staff.js`).

#### 2. Timing and gating
- Read requires the live team and `state.status==='ready'`. Otherwise: `Load this team’s roster and refresh live teams first.`
- If `manager.state` changed during the read: `The live roster changed during inspection. Refresh and try again.` (`team-staff.js:62-68`).
- Save requires the same `state` object as the read snapshot, matching `expected` values, and a configured `backupRoot` (`:76-83`).
- `roster` gate on `team:staff-save` only. Staff save is **not** in the season-gate writer list (`player-season-operation-gate.js:4`).

#### 3. Mechanism
- The helper validates five SHA-256 code spans before any read; mismatch → `This game's staff layout is unsupported. No staff data read.` (`RR/TeamStaff.cs:26-29`).
  - The spans are 0x6B5900/17580, 0x2EA5C10/46, 0x2F49F60/5, 0x64F840/12 and 0x2EDFA80/16.
- Owner = `[[image+0x9A47EB8]+0xA8]`. Staff table at owner+0x180, stride 456. Team staff slot pointers at team+0xF70 ×32. The slot is checked as staff+0x182 & 0x1F, plus the reciprocal team link at staff+0x18.
- The JS `patch` sets bits only via `staff-controls.json`. The helper re-checks that no byte outside `TeamStaffPatch.Masks` changed: `Staff IDs, roles and links are not editable.` (`RR/TeamStaffPatch.cs:4-7`).
- Recovery file `staff-<ms>-<uuid>.json` (schema `nba2k27-staff-recovery-v1`) is written with `wx` and fsync'd before `--team-staff-write` (`team-staff.js:85-86`).
- Report `nba2k27-team-staff-write-v1`, `readBackVerified`, `writes` 1..456.

#### 4. Build profiles
- The hash spans pin one build. Assumed CURRENT, **UNVERIFIED** which build the hashes match.
- V2: every field bit is proven from the StaffData serializer at 0x6BBA00. Systems are signed 4-bit (15 → −1). Shared coaches: save only records whose +0x18 is the edited team; live run 1 found 6 shared (`ART/editor-fields-v2-research-20261017.md:42-104`, `ART/editor-services-v2-research-20261017.md:100-106`).

#### 5. Rust must match
- [ ] Only mask-permitted bits may change. IDs, roles and the +0x18 link are never written.
- [ ] A recovery file is written and fsync'd before the staff write.
- [ ] A save based on a stale read snapshot is refused with `Refresh this team’s staff before saving.`

---

### J. Team rotations

#### 1. User-visible behavior
- Messages: `Reading rotations…`, `Segment appearances (not minutes): `, `Rotation changes staged. Review, then Save & verify.`, `Lineup range staged…`, `Saving and verifying rotations…`, `Saved & verified. Save the roster in-game.`
- Validation: `Choose a source and a valid 1–48 segment range.`, `The source lineup must contain five distinct assigned players.` (`W/src/renderer/team-rotations.js`).

#### 2. Timing and gating
- Read requires the live team. The helper also requires 5–64 members whose player+0x60 equals this team (`RR/TeamRotations.cs:9-17`).
- Save requires the same `state` and `token`, plus a backup root (`W/src/core/team-rotations.js:20-23`). If nothing changed: `{saved:true, writes:0}` with no helper call.
- Extra code-hash gate: 0x2F50D70/215 and 0x3299990/0x4DA (`RR/TeamStaff.cs:30-33`).

#### 3. Mechanism
- Block: team+0x1360, 480 bytes = 48 segments × 5 u16 player UniqueIds (player+0x128).
- Each edited segment must hold 5 distinct member IDs (`team-rotations.js:6`, `RR/TeamRotations.cs:20-25`).
- The write passes `--team-digest`/`--member-digest`, and the helper re-checks both plus the `before` bytes.
- Recovery file `rotations-<uuid>.json` is written before the write (`team-rotations.js:25`). Report `nba2k27-team-rotations-write-v1`, writes 1..240.

#### 4. Build profiles
- Single build via hash.
- V2: the layout is unchanged. team+0x154C bit 0 = has advanced rotation. When the bit is clear the block is empty and the game uses auto rotation. WPF never set the bit. Recommendation: allow saves only for flagged teams (`ART/editor-fields-v2-research-20261017.md:106-140`).

#### 5. Rust must match
- [ ] Edited segments need five distinct players from this team. Untouched segments pass through unchanged.
- [ ] A recovery file exists before any rotation write. The save is refused if team or member digests changed.

---

### K. Team DNA

#### 1. User-visible behavior
- Section checkboxes (`W/src/renderer/index.html:780-786`):
  - Team info (checked)
  - Arena & floor (checked)
  - Coaches & staff settings (checked)
  - Uniform settings (matching destination slots) (checked)
  - Rotations (matching destination players) (unchecked)
- Buttons: `Import .2kteam`, `Export .2kteam`.
- Export message: `Team DNA exported. Team ID, staff IDs and assignments are not included.`
- After import: `Staged Team DNA from <name>. Destination Team ID <id> is unchanged. Review Team info and Arena, then Save & verify; save each imported staff member and uniform in its own tab.` (`W/src/renderer/app.js:1171-1218`).

#### 2. Timing and gating
`teamDnaBusy`, and it does not run while `teamUniformBusy`. Import only stages values. Each tab must then be saved on its own.

#### 3. Mechanism
- Shape: `{format:'2kteam',version:1,game:'NBA2K27',name,info,arena,staff[≤32],rotations(48×5 names)|null,uniforms[≤128]}`, ≤ 512 KB. Text limits: name 23, city 17, cityShort 6, stateShort 6, mainLogo 35 (`W/src/core/team-dna.js:5-18`).

#### 4. Build profiles
File-only. V2: unknown.

#### 5. Rust must match
- [ ] Import never writes. It stages each section into its tab, and unresolved arena/staff/uniform matches stay unchanged with warnings.

---

### L. Roster transfer (.2kroster mapped JSON) and native saved-roster export

#### 1. User-visible behavior
- Mapped transfer:
  - export dialog `Export mapped roster data` (`Roster.2kroster`)
  - preview dialog `Preview roster import`
  - **apply throws** `Whole-roster writes are disabled pending roster identity, ownership and recovery validation. Export and preview are read-only.` (`W/src/main.js:1330-1332`)
  - the coverage warning says it is a partial export and not a backup (`W/src/core/roster-transfer.js:23`)
- Native export UI (`W/src/renderer/native-roster-export.js`):
  - `Reading saved NBA rosters… Keep the game at the main menu.`
  - `No saved NBA rosters found. Save a roster in-game, then try again.`
  - `Exported <name> (<x> MB). Native import remains unavailable; cross-account compatibility is unverified.`

#### 2. Timing and gating
- Native export `verify()` (`W/src/core/native-roster-connection.js:5-19`):
  - exactly one `NBA2K27` process (via PowerShell `Get-Process`, 10 s timeout), else `Open one NBA 2K27 game at the main menu before exporting a roster.`
  - netstat shows the game PID LISTENING on **TCP 0.0.0.0:7928**, else `The game’s native roster service is not ready. Stay at the main menu.`
  - exe SHA-256 == `0ca3734d217d62197a2a88ea7485fc8f761c8b6862d3bce44616f5bf75e97c89`, else `Native roster export has not been verified for this game build.`
- `list` reads slots 1..100 and re-verifies the session afterwards.
- `export` re-verifies the session, re-reads slot metadata before and after the blob read, and compares the stamp. Mismatch: `Saved roster changed during export. Try again after saving finishes.` (`W/src/core/native-roster-export.js:7-22`).
- Each socket read times out after **15000 ms** (`W/src/core/native-roster-reader.js:24`).

#### 3. Mechanism
- Protocol (`native-roster-reader.js`):
  1. connect to 127.0.0.1:7928
  2. send `ROSTERTOOL`
  3. expect an 8-byte ack `8d921c1500000000`
  4. request a 12-byte packet: BE opcode `0x9198dc77` (metadata) or `0x3f683266` (blob), BE length 4, LE slot
  5. replies `0x90a3aa2b` / `0xeb553eb0`; ignore empty control packets `0xff1bebf9` (≤ 128)
  
  This is a read-only allowlist with no write opcodes.
- File: `2KRSTR2\n` magic, metadata JSON ≤ 4096 with SHA-256 in the header, then the `BNH(`/`BNDL` bundle 4 KiB–16 MiB (`W/src/core/native-roster-file.js`, `native-roster-bundle.js`).
- Publish: `wx` temp file, fsync, `fs.link` to the target (never overwrite: `That file already exists. Export again with a new filename.`), unlink temp (`W/src/main.js:1313-1320`).

#### 4. Build profiles
- Native export: pinned to one exe hash (WPF build).
- V2: **blocked**. The hash differs and the protocol is unproven (`ART/editor-services-v2-research-20261017.md:25,40`).

#### 5. Rust must match
- [ ] Whole-roster apply stays disabled with the exact message. Preview and export stay read-only.
- [ ] Native export never overwrites an existing file. It requires a single game process listening on 7928 and an allowlisted exe hash.

---

### M. Artwork, header labels, audio, animation, badges, accessory/fit (brief)

- Portrait source is `player_images/chr_g{PhotoId:0000}.iff`, then `chr_r…_a1.iff`. Logo source is the team `mainLogo` if it starts with `logo<digit>`, else `logo{teamId:000}`. Logo background special case: `logo000phi_1978`.
- Decode path: VCZ decoder → exactly one DDS (≤ 4096²) → texconv 512×512 PNG. Results are cached by SHA-256 key, the in-memory cache holds 32, the disk cache is pruned to 128 MiB (`W/src/core/roster-artwork.js`, `W/src/main.js:1234-1272`).
- Manual overrides are images ≤ 16 MB resized to ≤ 512, stored as data-URLs in `Data/player-editor-artwork.json` (≤ 32 MiB).
- Header archetype label: player `archetypeKey` (16 hex) resolved in `loc/english.iff` `TEXT.VCLOCALIZEDATA` (`W/src/core/player-header-labels.js`).
- Audio preview banks: auto (commentary, then PA), `commentary`, `pa`; ID 0..32767. It uses the CRC64 archive index `0A` and `newaudio/{players,paplayers}/directory_listing.adb2`. Only mono Opus configs 0x78/0x70 are accepted, remuxed to Ogg in a worker with a 20 s timeout (`W/src/core/player-audio-preview.js`).
- Animation preview runs an external worker (startup timeout 1800000 ms, request 120000 ms). Max 8 sessions; payloads are served via `/animation-payload/<48hex>` (`W/src/core/player-animation-preview.js`, `W/src/main.js:400-445`).
- Accessory controls / fit preview (`W/src/renderer/accessory-controls.js`, `fit-preview-ui.js`, `W/src/core/accessory-rig-fit.js`) are presentation only within the player studio and are not covered in depth here.

Rust must match:
- [ ] Artwork never writes game files. Manual art is stored only in the app library.
- [ ] Audio preview reads stock archives only, and fails with `No clip available for this sound ID in …` when none exists.

---

## Live memory vs save file: summary

| Data | Where WPF edits | Persisted by |
|---|---|---|
| Players, teams, uniforms, arenas, staff, rotations, seasons | Live process memory via helper (WriteProcessMemory) | User must **save the roster in-game** |
| Learned catalog, arena profiles, artwork overrides, presets, 2K23 catalogs | Launcher `Data`/`Library` files | Launcher (atomic temp + rename) |
| Saved roster file | Read-only TCP export | Never written back (import disabled) |
| Recovery snapshots | `recovery/{team-edit,staff-edit,rotation-edit,season-edit}-backups`, `recovery/season-imports` (organized storage), else under `Data` | Written **before** the memory write (team floor path, staff, rotations, season structure/import). Plain player/team/uniform saves have **no** recovery file; they rely on in-transaction rollback only |

There is no UI to restore from recovery files. **Not found in WPF sources:** a restore command or IPC that consumes them.

## Known divergences in current Rust port (from session notes, not re-verified in code)

- V2 tabs: weight, height/wingspan, audio and From are refused. Team roster type, historic year and arena/floor are refused. Uniform linking and Player DNA are refused (`NOTES/qa048-roster-v2-saves.md`).
- V2 reads require the game to have been launched by this launcher (the binding comes from the launch journal). WPF scanned any running game (`NOTES/roster-v2-readonly-wiring.md`).
- The V2 save uses a durable intent journal (`Data/roster-v2-write-journal.json`). Launch preflight blocks while a journal is pending. WPF had no journal for player saves (`NOTES/qa048-roster-v2-saves.md`).
- After a V2 save only the saving tab stays current; other tabs must Refresh. WPF rescanned globally after every write (`NOTES/qa048-roster-v2-saves.md`).
- Rust source files present: `nba2k27-launcher-tauri2/src/roster_native.rs`, `roster_v2_access.rs`, `roster_v2_readonly.rs`, `roster_v2_write.rs`, `player_dna.rs`, `team_dna.rs`, `editor_presets.rs`, `reference_stats.rs`, `season_math.rs`, `player_transaction.rs`, `team_transaction.rs`, `uniform_transaction.rs` (not reviewed line by line).

## Open questions / UNVERIFIED

- Which exe build (OLD or CURRENT) the staff/rotation hash spans in `RR/TeamStaff.cs:26-33` match. **UNVERIFIED**; probably CURRENT, since the RVA 0x9A47EB8 is CURRENT-only per `RR/BlacktopTool.cs:187`. That would make the `roster` gate's OLD support partial (scan, base player/team writes) on OLD.
- Whether the in-window (non-browser) roster page was ever reachable in the frozen WPF build. `app.js:1956` always redirects Players/Teams to the browser when not in `workspace=roster`. So `team-uniform:link`, `arena:clone` and `roster:transfer` (absent from `METHODS`) look unreachable from the shipped UI. **UNVERIFIED** live.
- V2 SafeMenu IDs for season import are unobserved (capture B in `ART/editor-fields-v2-research-20261017.md:223-227`).
- `badge-icons.js` internals were not reviewed.
