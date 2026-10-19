# Arena, ball, scoreboard — WPF reference
Source build: WPF quality-pass-qa-20261010 (frozen). Last researched: 2026-10-17.

Path abbreviations used in citations:
- `W/` = `nba2k27-launcher-wpf/release/quality-pass-qa-20261010/App/Reference/src/`
- `WR/` = `nba2k27-launcher-wpf/release/quality-pass-qa-20261010/App/Reference/runtime-resources/`
- `ML/` = `nba2k27-mod-launcher/`
- `VP/` = `nba2k27-version-proxy/src/`
- `TA/` = `nba2k27-launcher-tauri2/artifacts/`
- `SN/` = `C:\Users\carrn\AppData\Roaming\Claude\side-session-notes\local_98bf0092-fe39-4ae3-abce-e88edad0a633\`

The WPF C# host does no arena/ball/scoreboard work. It runs the Electron-style `main.js` under Node (`nba2k27-launcher-wpf/worker/host.cjs`). A grep of `nba2k27-launcher-wpf/*.cs` finds no arena or scoreboard logic. All behavior below lives in JS managers plus three external helper EXEs.

## Summary
- All three pickers are **Blacktop-only presentation selectors**. You can change them from the launcher "Blacktop" page (`#page-blacktop`, "Scoreboard, arena & ball" card, `W/renderer/index.html:108-130`, driven by `W/renderer/blacktop-setup-picker.js`) or from the F8 compact bar (`W/renderer/blacktop-panel.html:21-23`, driven by `arena-picker.js`, `scoreboard-picker.js` and `ball-picker.js`).
- **Arena**: a long-lived C# helper (`NBA2K27Arena.exe`, source `ML/arena-helper/ArenaTool.cs`) is spawned lazily on the first arena pick. It takes ids on stdin and writes the arena mapping (two u32 ids) in the game image, plus a roster arena record for "template" arenas. It applies **only at Blacktop player selection**. The choice is kept **in memory only** (not persisted across launcher runs). Favorites are persisted.
- **Scoreboard**: `ScoreboardManager` persists `selection.json` and **polls every 3000 ms** by running the one-shot helper `NBA2K27Scoreboard.exe` (`ML/scoreboard-helper/ScoreboardTool.cs`). The helper rewrites three 12-byte property headers in the Blacktop package's cached scoreboard descriptor, but only while no scoreboard is active. Saved choices apply automatically at the next Blacktop entry. Nine layout-adapter IFFs are installed into Mods at Play and forced through version.dll (`SCOREBOARD_TEXT_TEST=1`).
- **Ball**: `BallSelectionManager` + `BallLiveSelector` drive `NBA2K27FreestyleTeams.exe ball-assets` (`ML/unlocks-helper/BallAssetService.cs`). The helper swaps the **Normal ball record filename** (model 0xCB5691BE) at a stable player selection and restores it once the ball unloads at a quiet menu. `requireExplicitBallSelection:true` resets any saved ball to "Current installed" at every launcher start, so a saved ball never auto-applies.
- **Top 3 port risks:**
  1. **Gating on the exact "player selection" state.** Arena and ball both require mode 0x92DF06DE for current *and* alternate, plus Blacktop stage == 3. Arena also checks the Blacktop vtable. Applying during loading or in a match is refused.
  2. **Re-apply and restore lifecycle across matches and game restarts.** The arena mapping persists across matches and re-applies on a new game process. The scoreboard keeps polling and re-applies. The ball restores after each use and re-prepares at the next selection.
  3. **The WPF ball mechanism is known wrong on V2.** The V2 audit proved that Normal-filename substitution feeds spectator props, not the court ball (`TA/ball-gameplay-v2-offline-audit-20261016.json` "confirmedFailure"). Do NOT port the filename swap. V2 uses GameData+DA4.

## Key source map
| Concern | file:line |
|---|---|
| Manager wiring (ball/arena/scoreboard singletons, IPC) | `W/main.js:206-268` |
| Ball manager options (`requireExplicitBallSelection:true`, helper = unlocks exe) | `W/main.js:215-228` |
| Arena helper and catalog paths (packaged = `runtime-resources/arena/`) | `W/main.js:229-235` |
| Scoreboard manager created lazily after `waitForInitialization`, `load()` then `start()` | `W/main.js:249-266` |
| Launch: restore, then `scoreboardLayouts.prepare`, then `balls().prepare`, then version proxy with `scoreboardTextTest:true` and `balls().arm` inside `launchGame` | `W/main.js:1716-1720, 1776-1805` |
| Launch-failure cleanup | `W/main.js:1813-1846` |
| Game-end cleanup (`onCleanup`: Blacktop ball, Scoreboards) | `W/main.js:2071-2081` |
| Launcher shutdown (`arenaManager.stop`, ball cleanup) | `W/main.js:2238-2262, 2335-2341` |
| Startup recovery (scoreboard layouts, ball scale, ball cleanup + live recover) | `W/main.js:2013-2018, 2058-2060` |
| Ball selection observation (from Blacktop detect loop) | `W/main.js:894`, `W/core/blacktop-auto-open.js:42-68` |
| Arena manager | `W/core/arena-manager.js:1-71` |
| Arena favorites | `W/core/arena-favorites.js:1-19` |
| Arena grouping/filter (shared) | `W/shared/arena-browser.js:1-20` |
| Arena F8 picker | `W/renderer/arena-picker.js:1-31` |
| Arena launcher picker (DEAD CODE, not loaded by any html) | `W/renderer/arena-launcher-picker.js` |
| Launcher Blacktop page picker (all three) | `W/renderer/blacktop-setup-picker.js:1-98` |
| Arena helper source | `ML/arena-helper/ArenaTool.cs:1-164` |
| Arena catalog (165 choices) | `W/data/blacktop-arenas.json`, packaged copy `WR/arena/blacktop-arenas.json` |
| Arena catalog generator input | `W/data/blacktop-arena-expansion.txt`, `ML/scripts/expand-blacktop-arenas.cjs` |
| Ball manager | `W/core/ball-selection-manager.js:1-147` |
| Ball live selector (helper client) | `W/core/ball-live-selector.js:1-91` |
| Ball helper service | `ML/unlocks-helper/BallAssetService.cs:1-142`, `ML/unlocks-helper/BallAssets.cs:1-111` |
| Ball offsets | `ML/unlocks-helper/BallSelector.cs:12-27, 119-121` |
| Ball exclusions | `W/data/ball-live-exclusions.json` |
| Ball thumbnails | `W/core/ball-thumbnails.js:1-19`, bundled `W/assets/blacktop-ball-previews/catalog.json` (468 items) |
| Ball F8 picker | `W/renderer/ball-picker.js:1-68` |
| Ball scale (recovery-only in this build) | `W/core/ball-scale-recovery.js`, `W/main.js:124-125, 2017-2018` (`ball-scale-session.js` / `ball-scale-archive.js` unused) |
| Scoreboard manager | `W/core/scoreboard-manager.js:1-102` |
| Scoreboard helper | `ML/scoreboard-helper/ScoreboardTool.cs:1-141`, `ScoreboardProfile.cs`, `ScoreboardCatalog.cs` |
| Scoreboard layout adapters (Mods install) | `W/core/scoreboard-layout-session.js:1-115`, `-archive.js`, `-patch.js`, `W/data/scoreboard-text-adapters.json` |
| Scoreboard catalog | `W/data/scoreboards.json:1-1243` |
| Scoreboard F8 picker / previews | `W/renderer/scoreboard-picker.js:1-111`, `W/renderer/scoreboard-preview-catalog.js:1-49` |
| version.dll scoreboard forced overrides | `VP/scoreboard_text_override.hpp:1-17`, `VP/version_proxy.cpp:425-429, 795-806` |
| version.dll selector override (player selector, related flag) | `VP/selector_override.hpp:1-12`, `VP/version_proxy.cpp:419-423, 738-740` |
| Session config lines (`SCOREBOARD_TEXT_TEST=1`) and layout presence check | `W/core/version-proxy-session.js:160-177, 264-272, 286-288` |

---

### Arena picker

#### 1. User-visible behavior
- **F8 bar** (`W/renderer/blacktop-panel.html:21`): label "Arena". Button text is `<label> ▾`, or `Default ▾` if the selected id is unknown (`arena-picker.js:12`). The button's `title` and `aria-label` carry the status message. `data-phase` = phase.
  - Clicking opens `#arenaMenu`, placed under the header (`arena-picker.js:25-26`). It also calls `sizeBlacktopPlayerMenu(7)`, and `sizeBlacktopPlayerMenu(0)` on close.
  - Search placeholder is `Search team, court or arena file…` (`:27`). It matches label, id, file, floorFile and group, after NFKD normalisation with diacritics stripped (`arena-browser.js:13-16`).
  - Filter tabs: `All`, `★ Favorites`, `NBA`, `WNBA`, `Historic`, `Gyms`, `Outdoor`, `Other` (`arena-picker.js:10`, `arena-browser.js:2`).
  - Count line: `N arena`/`N arenas` (`:15`).
  - Each row has a choice button (label, plus ` · Needs work` if disabled, with the group as a small line) and a star (`☆`/`★`). Star aria-label: `Add X to favorites` / `Remove X from favorites`.
  - Disabled choices cannot be clicked. The row tooltip is `caution || file || 'Uses Day / Night'` (`:18`).
  - Empty state: `Star an arena to find it here.` (Favorites tab with none starred) or `No matching arenas.`, plus a `Show all arenas` button that clears the search and goes to the All tab (`:16`).
  - Picking calls `selectArena(id)`, re-renders and closes the menu. On error, the message goes into the note and the menu stays open (`:19`).
  - The menu closes on Escape, an outside click, panel shown, or panel collapsed (`:27-29`).
  - If `arenaStatus()` fails, the button reads `Arena unavailable` (`:29`).
- **Sort order**: `default` first, then label with `localeCompare` numeric (`arena-browser.js:17`).
- **Grouping rules** (`arena-browser.js:3-12`):
  - `default` → Outdoor.
  - `arena_300..317_int`, `arena_wnba*` or `arena_mycourtw_int` → WNBA.
  - `551..649`, `800`, `924` or `arena_era*` → Historic.
  - `0..31` or `arena_clippers2023_int` → NBA.
  - `park_*` or `*_ext` → Outdoor.
  - `729`, `906`, or a match on gym/facility/mycourt/scrimmage/hsmycareer/rechighschool/privatehome/animationlab → Gyms.
  - Anything else → Other.
  - Resulting counts in the catalog: NBA 31, WNBA 18, Historic 58, Gyms 15, Outdoor 8, Other 35.
- **Launcher Blacktop page** (`index.html:119-124`, `blacktop-setup-picker.js`): row "Arena". It shows the selected label and a short status, and a `Catalog` button opens a modal `<dialog>`.
  - Dialog eyebrow: `BLACKTOP PRESENTATION`. Title: `Arena catalog`. Close button: `Close`. Search placeholder: `Search arena catalog`.
  - Filters: `All`, `★ Favorites`, then the groups present. Grid of 24 per page, with `Previous` / `N–M of T` / `Next`.
  - Each tile has a star button, aria `Toggle X favorite` (`:85`). Empty text: `Star an arena to find it here.` / `No matching choices.` (`:87`).
  - While saving, the status shows `Saving <label>…` (`:54`).
- **Short status strings** on the launcher row (`blacktop-setup-picker.js:12-25`) for kind=arena:

  | Phase | Text |
  |---|---|
  | pending | `Waiting for Blacktop` |
  | applied | `Active` |
  | idle/other | `Choose for Blacktop` |
  | error | the raw message |
- **Default**: id `default`, label `Default · Day / Night` (`W/data/blacktop-arenas.json:4-7`). Initial manager state is `{selected:'default', phase:'idle', message:'Uses the game’s Day / Night choice.'}` (`arena-manager.js:9`).
- **Catalog**:
  - 165 choices, 11 `disabled:true`: lobby/scene candidates `arena_allstarteamup_lobbyint`, `arena_matchmaking_lobbyint`, `arena_matchup_lobbyint`, `arena_provinggrounds_lobbyint`, `arena_provinggroundsa_int`, `arena_provinggroundsb_int`, `arena_reccenter_lobbyint`, `main_menu_int`, and `park_streetball_a/b/c`.
  - 6 use a template: Brickley's Gym, Casual Corner 3v3, Casual Corner 5v5, Clippers 2023, Historic Lakers, Historic Warriors / Oracle Arena.
  - `tested:true`: `default`, `arena_brickleyfacility_int`, `arena_700_int`.
  - NOTE: the packaged helper copy `WR/arena/blacktop-arenas.json` differs in exactly one field. `arena_reccenter_lobbyint` is `disabled:false` there and `disabled:true` in `W/data` (line 247). The UI uses `W/data` (`arena-manager.js:4`). The helper reads the packaged copy only for id lookup and its own disabled check.
- **Choice fields**: `id`, `label`, `arenaId` (u16 or null), `file`, `floorFile`, `templateArenaId` (700 when present), `tested`, `disabled`, `caution`.

#### 2. Timing and gating
- **Helper lifetime**: not started at launcher start or at Play. `ArenaManager.start()` runs on the first `select()` (`arena-manager.js:18-50`). After that the helper lives for the whole launcher run. It survives game exits: on `game.HasExited` it calls `Close()`, resets `applied=-1`, and re-attaches to the next `NBA2K27` process with the same desired id (`ArenaTool.cs:135, 139`).
  - It stops only on launcher shutdown or host-loss cleanup (`main.js:2257, 2340`, 2500 ms bound).
- **Helper loop** (`ArenaTool.cs:133-160`): about 500 ms per iteration. On an exception it emits `error`, sleeps 1000 ms, then 500 ms more.
  - With no game running, `Connect` throws `Waiting for NBA 2K27.`. This is emitted as **phase `error`**, so selecting with no game running shows an error status that keeps retrying every ~1.5 s.
- **Selection gate** (`ArenaTool.cs:91-92`, `Selection()`), all of:
  - `*(base+BlacktopRva) == base+BlacktopVtable`
  - `*(u32)(base+BlacktopRva+0xEA8) == 3`
  - for both ModeCurrent and ModeAlternate: index `< 128` and `ModeTable[index*64] == 0x92DF06DE`
- If the selection gate fails, the helper emits `pending` / `Return to player selection to prepare this arena.`, sleeps 500 ms and retries (`:141`). It never writes outside selection.
- **Re-checks**: `Selection()` is re-checked after `ValidateArena` (`:144`, `Selection changed; retrying.`) and twice inside `WriteFields` (`:52, :57`).
- **Once applied** (`applied==rev && previous==id`), the loop only re-emits `applied` / `Ready for the next Blacktop match.` (`:156`). It does not re-verify or rewrite per match. The mapping stays in place for every later match in that game process.
- **New selection**: `revision` increments on every valid stdin line (`:131`, lines over 128 chars or unknown ids are ignored). A change triggers a re-apply at the next selection. Before re-applying, the previous template record is restored (`RestoreTemplate`, `:142`).
- **Default**: on each loop, `Restore()` puts the mapping back to Stock if owned, then `RestoreTemplate()`.
  - If template restore needs selection and is not at selection → `pending` / `Default mapping restored. Leave the match to restore the temporary arena record.`
  - Otherwise → `applied` / `Uses the game's Day / Night choice.` (`:136`).
- **EOF (stdin closed by `stop()`)**: restores the mapping immediately. For a template record, it waits for 4 consecutive `Selection()` checks 250 ms apart, or game exit, before restoring (`:83-90`). The launcher waits `stopTimeoutMs=1500` with `forceKill:false` (`arena-manager.js:7, 64`).
  - If the helper has not exited, the manager emits `pending` / `Arena restoration is still pending; close NBA 2K27 before changing arenas.`. Further `start()` calls then throw `Arena restoration is still pending. Close NBA 2K27 before changing arenas.` (`:19, 66`).
- **Health**: `setInterval(checkHealth, 1000)` (`healthIntervalMs=1000`, `arena-manager.js:7, 43`). If the PID dies: `error` / `Arena helper disappeared before applying the selection. Select the arena again.` (`:16`).
- **Stdout filter**: the manager accepts helper lines only if `value.selected === state.selected` and phase ∈ {applied, pending, error} (`:32`). Stale lines for an old id are dropped.
- **Ordering vs other features**: independent of Play. No launch-time step exists for arena in WPF.

#### 3. Mechanism
- **Process access**:
  - Requires exactly one `NBA2K27` process whose `MainModule` equals `<gameRoot>\NBA2K27.exe`.
  - Refuses if any process named `EasyAntiCheat*` is running: `Use the launcher's without-EAC session.`
  - `OpenProcess(0x438)` (0x410 for inspect) (`ArenaTool.cs:93-97`).
- **Build detection** (`:98-100`), PE TimeDateStamp and SizeOfImage:

  | Build | TimeDateStamp | SizeOfImage | Name in code |
  |---|---|---|---|
  | updated | 0x6AA2013D | 0x36558000 | `updated` |
  | older | 0x6A974D13 | 0x368C3000 | — |

  Anything else → `Unsupported game build.`
- **Anchors**: six code anchors per build (`:101-105`). A mismatch → `Game build changed; arena picker disabled.`
- **RVAs per build** (`:22-37`):

  | Name | older | updated |
  |---|---|---|
  | RosterRootRva | 0x9A43EB8 | 0x9A47EB8 |
  | MappingRva | 0x78771F0 | 0x787AFD0 |
  | BlacktopRva | 0xAEB29D0 | 0xAEB69D0 |
  | BlacktopVtable | 0x78AABA8 | 0x78AEA48 |
  | ModeCurrent | 0xAC73E28 | 0xAC77E28 |
  | ModeAlternate | 0xAC73E24 | 0xAC77E24 |
  | ModeTable | 0xAC73660 | 0xAC77660 |
- **Mapping write** (`:20, 73-81, 153-154`):
  - Stock 20 bytes at `base+MappingRva` = u32 {0x3B9, 0x3BA, 0x38B, 0x2C5, 0x2C5}, i.e. 953, 954, 907, 709, 709.
  - On attach, the bytes must equal Stock, else `Another arena override is active. Close the test or restart the game.` (`:106`).
  - Page must be image-type (0x1000000), allocation == module base, protect == 2 (READONLY).
  - Write sequence: `VirtualProtectEx(8 bytes, PAGE_READWRITE=4)` → verify old==2 → re-read → write 8 bytes (target id in slots 0 and 1) → read back all 20 → restore the old protection in `finally`.
  - `owned` tracks the last written value for compare-and-swap.
- **Arena record validation** (`ValidateArena`, `:108-126`):
  - Roster root = `*(base+RosterRootRva)`, owner = `*(root+0xA8)`. The owner must be private (Type 0x20000) committed memory with no guard/noaccess, and `owner == allocation+0x50`.
  - Header at `owner+0x60`: `count` (1..2048) and `table` pointer.
  - Record stride 0x1278. Arena id u16 at `+0x792`. Arena filename ASCII at `+0x7E` (38 bytes). Floor filename at `+0x7E+38` (40 bytes).
  - Requires exactly one record with `id==700 && file=="arena_700_int"`, and exactly one record matching (`arenaId`, `file`). For template choices the target is `arena_700_int`.
  - For non-template choices with `floorFile`, the record's floor must equal it.
- **Template arenas** (`:146-152`):
  - Require `arenaId==700`, the record floor `arena_700_int_floor`, and both `levels/<file>.iff` and `levels/<floor>.iff` in `<gameRoot>\manifest`.
  - The 78-byte name field is replaced with the new file and floor (`TemplateFields`: file < 38 chars, `[a-z0-9_]`, floor == file+"_floor"). Private RW memory is checked first.
  - The template is written **before** the mapping. Order: template fields, then mapping.
- **Restore**:
  - Mapping → Stock on `default`, EOF or helper finally.
  - Template → original 78 bytes, only at selection.
  - Nothing is written to the game folder. No hooks.
- **Persistence**:
  - Selection: **none**. In-memory `state.selected` only, reset to `default` at each launcher start (`arena-manager.js:9`). Confirmed by `SN/qa048-arena-defaults.md`: "kept the selection in memory only".
  - Favorites: `<userData>/blacktop/arena-favorites.json` (`main.js:212`, `arena-favorites.js:5`), a JSON array of ids plus a trailing newline. Writes are serialized through a promise queue, then temp file + rename.
  - Favorites read: `ENOENT` → `[]`. Non-array or parse error → throws `Could not read arena favorites. Your saved file was left unchanged.`. Unknown ids are dropped and duplicates removed on read.
  - `set(id, favorite)` with an unknown id or a non-boolean → `Invalid arena favorite.` (`:7-16`).
- **IPC**:
  - `arena:status` → view `{selected, phase, message, choices}`.
  - `arena:select(id)`: an unknown or disabled id throws `That arena is not available for testing yet.` (`arena-manager.js:49`). On success it immediately emits `pending` / `Preparing arena for the next match…` and writes `id\n` to the helper.
  - `arena:favorites`, `arena:favorite(id, bool)` → new id array.
  - Push channel: `arena:status`.
- **Diagnostics**: every status goes to `launcherHealth.observe('arena:status')` and `selectionStatusLog.record('arena', …)` (`main.js:233`).
- **Not part of the picker**: `arena:clone` / custom arena profiles (`main.js:1500`, `playerResearchManager.startAutomaticArenas` at `main.js:508`). These are a separate roster-editor feature.

#### 4. Build profiles
- WPF: two builds, 0x6A974D13/0x368C3000 (older) and 0x6AA2013D/0x36558000 (updated) (`ArenaTool.cs:99-100`).
- V2 (0x6AC53765 / 0x35BDE000): a separate Rust implementation exists (`arena_v2.rs`). The offline tests include `arena_v2_loading_match_main_and_quick_play_never_get_arena_edits_but_rearm_at_selection` and `arena_v2_template_requires_both_assets_and_orders_names_before_mapping` (`TA/arena-v2-offline-regressions-20261016.log`). V2 RVAs: see the `TA/game-v2-arena-*` files (not re-verified here).

#### 5. Rust must match
- [ ] Arena selection with no game running shows a non-success state and keeps retrying (WPF: phase `error`, message `Waiting for NBA 2K27.`, ~1.5 s). It never blocks the UI.
- [ ] No arena memory write happens unless current mode == alternate mode == 0x92DF06DE, Blacktop stage == 3 and the Blacktop object vtable matches. Otherwise the status is `pending` / `Return to player selection to prepare this arena.`, re-polled about every 500 ms.
- [ ] After `applied`, a second and third Blacktop match in the same game use the same arena without re-selection, and the helper does not re-write.
- [ ] After the game exits and is relaunched with the launcher still open, the last chosen arena re-applies at the first selection of the new process.
- [ ] Template arenas write the roster record name fields first, then the mapping. Changing away restores the template record only at player selection.
- [ ] Selecting `Default · Day / Night` restores the mapping to 953/954 at once. The status stays pending until the template record can be restored at selection.
- [ ] Disabled catalog ids cannot be selected (`That arena is not available for testing yet.`).
- [ ] Favorites are stored as a JSON array of exact catalog ids. Unknown ids are dropped on read, and a corrupted file is left untouched with the error `Could not read arena favorites. Your saved file was left unchanged.`.
- [ ] The `★ Favorites` filter shows `Star an arena to find it here.` when empty. Default sorts first, then numeric label order.
- [ ] Parity note: WPF did NOT persist or auto-apply the arena at launch. The Rust QA 0.48 auto-apply of a saved arena is a deliberate extension (`SN/qa048-arena-defaults.md`).
- [ ] On launcher close, the mapping is restored and the helper is not force-killed while a template record is in use.

---

### Ball picker

#### 1. User-visible behavior
- **F8 bar** (`blacktop-panel.html:23`): label "Ball". The button reads `<label> ▾`, default `Current installed ▾` (`ball-picker.js:7`).
  - Click → `BlacktopOverlay.expand(true,'ballMenuButton')`. The catalog is built detached and attached only after the expansion is acknowledged (`:16-35`). If the expansion returns false, it closes.
  - Dialog: eyebrow `BLACKTOP`, title `Ball catalog`, button `Back to game` (`:42-43`).
  - Search placeholder: `Search ball names or categories`. Button: `Show selected` (jumps to the selected item's page, `:45`).
  - Category buttons: `All` plus every group present. Grid of 24 per page.
  - Tile art: `CURRENT` for `current`. Otherwise a bundled thumbnail `PreviewImageUrl('ball', thumbnail)`, falling back to `ballThumbnail(id)` and then `●`. `Loading…` shows while a game preview loads (`:52-54, 61`).
  - The selected tile shows a `Selected` badge.
  - Empty state: `No balls match those filters.` / `Try another name or show every category.` / `Reset filters` (`:57`).
  - Footer: status message (or `N balls`) plus paging.
  - Clicking a tile optimistically sets `pending` / `Selecting <label>…`, closes the catalog and calls `selectBall(id)` (`:55`). A failure shows phase `error` with the message.
  - If status fails, the button reads `Ball unavailable` and is disabled (`:66`).
- **Launcher page row** "Ball" (`index.html:125-129`) with a `Catalog` dialog (`blacktop-setup-picker.js`). Short status strings for kind=ball (`:12-25`):

  | Phase | Text |
  |---|---|
  | pending | `Waiting for safe selection` |
  | loading | `Waiting for ball load` |
  | applied | `Ball loaded` |
  | restoring | `Restoring original ball` |
  | restored | `Original ball restored` |
  | current | `Current installed ball` |
  | saved | `Saved for next launch` |
  | ready | `Ready for Blacktop` |
  | unavailable | `Not available for live swap` |
  | incompatible | `Stock ball changed; swap paused` |
  | error | the raw message |
- **Catalog** (`ball-selection-manager.js:66-73`):
  - Built from the `<gameRoot>\manifest` lines matching `^balls/ball_[a-z0-9_.-]+\.iff$`, lowercased and deduped, minus the 43 ids in `W/data/ball-live-exclusions.json`. The exclusion reason is "No unique matching stock model in the read-only live ball registry (450 records, 2026-09-26)", game build `6AA2013D-36558000`. Note that the exclusions include `nba_nba_official` and `official_default` themselves.
  - The first entry is always `{id:'current', label:'Current installed', group:'Installed'}`. The rest are sorted by group, then label, then id.
  - Cached by manifest size+mtime. A manifest that is missing, a symlink, empty or over 64 MiB → `The NBA 2K27 ball catalog is unavailable.`
- **Labels and groups** (`describeBall`, `:26-35`): the prefix `ball_(gameplay_)?(official_)?` is stripped. The first token maps to a group:

  | Token | Group | Token | Group |
  |---|---|---|---|
  | allstar | All-Star | gem | Gems |
  | animal | Animals | holiday | Holiday |
  | camo | Camo | nba / wnba | League |
  | cloth | Materials | pattern | Patterns |
  | country | Countries | season | Seasons |
  | emoji | Emoji | solid | Colors |
  | event | Events | team | Teams |

  Anything else → `Other`. Words are title-cased, and NBA/WNBA are uppercased.
- **Default**: `current`. Initial state is `{selected:'current', phase:'saved', message:'Current installed ball will be used.'}` (`:61`).
- **Saved-state messages** (`ball-live-selector.js:10`):
  - `current`: `Current installed ball will be used.`
  - otherwise: `Saved for the next Play session. Choose a ball at Blacktop player selection and wait for Ready before starting.`
  - If `liveSubstitutionEnabled` were false: `Live ball swaps are temporarily paused while Blacktop stability is investigated. Your choice is saved, but the installed ball stays unchanged.` (`ball-selection-manager.js:21`). Not active in this build (`main.js:220` sets true).

#### 2. Timing and gating
- **Launcher start**:
  - `ballSelectionManager.cleanup()` restores any legacy loose-file journal.
  - `liveSelector.recover(gameRoot)` runs the legacy `ball-recover` (8000 ms timeout). Then, if `<blacktop>/ball-asset-session.json` exists, it starts the helper in `recover` mode (`main.js:2058-2060`, `ball-live-selector.js:27-31`).
  - A recover result other than restored/ended/no-game → `blocked=true`, and no new selections are applied in that run.
- **load()**: with `requireExplicitBallSelection:true`, any saved non-`current` selection is **reset to `current` and persisted** (`ball-selection-manager.js:64`, comment at `main.js:218-219`). The ball NEVER auto-applies from a previous launcher run. A pick made before Play in the same run does carry into that Play.
- **Before Play**: `select(id)` persists, then (no session yet) emits saved status (`:81-87`).
- **At Play**:
  - Stage `preparing-ball` / `Preparing the selected Blacktop ball…` runs `balls().prepare()`. It cleans up, re-validates the selection against the catalog, sets the session and emits saved status. It installs no overrides (returns empty `forcedOverrides`) (`main.js:1779-1780`, `ball-selection-manager.js:120-125`).
  - Inside `launchGame`, just before Steam launch, `balls().arm(gameRoot)` runs (`main.js:1805`). This stops any previous helper and spawns `NBA2K27FreestyleTeams.exe ball-assets <root> <dir>/ball-asset-session.json <fileTime(now)> <selected> session` (`ball-live-selector.js:32-37, 64-68`).
- **Helper attach** (`BallAssetService.cs:63-81`):
  - Polls every 250 ms for up to **120000 ms** for the game (`pending` / `Waiting for the new Play session.`).
  - Rejects a process created before `minimumBirth` (`Ball selection requires a new game started with Play.`).
  - Rejects a different exe path, more than one process, EAC, or `start_protected_game` (`BallSelector.cs:118`).
  - Timeout → `error` / `No new game session found. No ball assets changed.`
- **Main loop**: 100 ms sleep (`:136`). `desired` is frozen per iteration (later stdin commands are queued).
  - Prepare/apply requires `s.selection` (current==alternate==0x92DF06DE, stage==3, active byte==0, kind==0; `BallAssets.cs:17`).
  - It also requires `unloaded` (reference count 0 and the 48-byte renderer resource all zero, `:19`) and the native ball canonical == Normal (0xCB5691BE; 0 and 0xDC1BFEC5 canonicalise to Normal, `:16`).
  - `AssetStable` takes **4 snapshots 100 ms apart** before every write (`BallAssets.cs:96-102`).
- **Status after prepare**: `ready` / `Ready for Blacktop. Start the match when ready.` (`BallAssetService.cs:120`).
- **During the match**:
  - Once the ball resource is loaded: `applied` / `Selected ball loaded by the game. Original restores after Blacktop unloads.`
  - While loading/unloading: `loading` / `Waiting for the game to finish loading or unloading the ball.`
  - If used and `gameMode != 6`: `restoring` / `Return to the main menu and wait for Original ball restored before another mode.` (`:104-107`).
- **Restore trigger** (`RestoreNow`, `BallAssets.cs:37-38`): (used ∨ choice changed ∨ stopping ∨ recovering ∨ main menu) ∧ quiet menu ∧ unloaded.
  - Quiet menu = selection, or main menu (mode 0x6509837B or 0x079964C0) with current==alternate, plus active==0 and kind==0.
  - After restore: `restored` / `Original ball restored. The saved choice is ready for the next Blacktop setup.`
- **Next match**: after a restore at selection, the next loop iteration (still at selection, unloaded) **re-prepares the same choice automatically**. So every Blacktop match gets the ball, with a restore/re-arm cycle at selection between matches.
- **Choice not ready to prepare**: if not at a stable selection, the status is `pending` / `Return to Blacktop player selection and wait for Ready before starting the match.`, or `restored` / `Original ball restored. Your choice prepares at the next Blacktop player-selection screen.` (`:112`).
- **Choice change while a ball is owned**: `pending` / `Choice saved for the next match. The current ball stays unchanged until it unloads.` (`:103`). The launcher side emits `pending` / `Choice saved. It will prepare at Blacktop player selection once the previous ball is unloaded.` and writes the id line to stdin only if it changed (`ball-live-selector.js:74`).
- **Incompatible**: at selection, unloaded, but the native ball is not Normal → `incompatible` / `The game selected a different stock ball model. Live swaps cannot prepare in this game process; use Current installed or restart NBA 2K27 before retrying.` (`:109-110`).
- **Unavailable choice**: a choice missing from the live registry → `unavailable`, retryable with another choice in the same game (`AssetChoiceUnavailable`, `:15-20`). Other errors mark that choice rejected until a different choice is made (`rejectedChoice`, `:111, 128, 133`).
- **Selection observations**: `BlacktopAutoOpen` polls detect every 100 ms on selection, 2000 ms otherwise (`blacktop-auto-open.js:43, 62`). It forwards `{selection}` to `observeBlacktop`, which only calls `liveSelector.set(selected, active)`. These observations are **advisory only**. They never trigger a restore (`ball-live-selector.js:75`, `ball-selection-manager.js:131-144`).
- **Stop** (`ball-live-selector.js:79-89`): stdin end, then wait 1200 ms. The helper is never force-killed. If it is still alive: `restoring` / `Ball cleanup continues until a safe main menu or game exit.`. The helper keeps waiting for a quiet unloaded menu, emitting `restoring` / `Leave the match and wait at the main menu to restore the original ball, or close the game.` (`BallAssetService.cs:102`).
- **Game end**: the helper emits `ended` / `Game closed; temporary ball substitution is gone.`. `onCleanup` runs `ballSelectionManager.cleanup()` (`main.js:2075`).

#### 3. Mechanism
- **Helper**: `WR/unlocks/NBA2K27FreestyleTeams.exe` (`main.js:222`), entry `ball-assets` (`ML/unlocks-helper/FreestyleTeams.cs:180`).
  - Args: `ball-assets <gameRoot> <journal> <minimumBirthFileTime> <choice> session|recover`.
  - Stdin: one logical path per line (`current` or `balls/ball_*.iff`, max 160 chars, no `..`).
  - Stdout: JSON lines `{phase, message, selected, applied, state}`. Valid phases: pending, ready, loading, applied, current, restoring, restored, ended, unavailable, incompatible, error (`ball-live-selector.js:8`).
  - Mutex `Local\2kLauncherBallAssetExperiment`. If it is already held → `Another ball observer is restoring an earlier session. Close that game before launching again.`
- **Build**: updated build only, 0x6AA2013D/0x36558000 (`BallSelector.cs:121`), else `Ball selection does not support this game update.`. `AssetBuild` checks extra anchors at 0x21BFED8, 0x21BE198, 0x21BE1A0, 0x31436C0, 0x31436D0 and the string `NBA_MainMenu_PlayNow` at 0x77F9368 (`BallAssets.cs:54-66`).
- **RVAs** (`BallSelector.cs:13-25`):

  | Name | Value |
  |---|---|
  | RegistryRva (table ptr + count at +8, entries `{model u32, index u32}`) | 0xAA81400 |
  | RecordHolderRva (records = `*(holder+0x18)+0xF4D28`) | 0xAA51F68 |
  | Record stride | 0x1A0 |
  | Filename offset in record | 0x9C |
  | ModeTable | 0xAC77660 |
  | ModeCurrent | 0xAC77E28 |
  | ModeAlternate | 0xAC77E24 |
  | BlacktopStage | 0xAEB69D0+0xEA8 |
  | Settings (kind byte at +0, game mode at +0xA04, native ball at +0xDA4) | 0xAACB9A0 |
  | Renderer (resource = +0x28+index*48) | 0xAA7B078 |
  | References (`refs = *(i32)(base+0xAC76B98+0x18+index*4)`) | 0xAC76B98 |
  | Active byte | 0xAACC310 |
- **Write**: the *only* game write is the ASCII filename field of the Normal record (model 0xCB5691BE). `ball_gameplay_official_NBA_NBA_Official.iff` is replaced by the source record's filename, zero-padded, and must fit in verified zero space (`BallAssets.cs:22-30, 103-111`). No protection change, allocation or hook.
- **Journal**: written durably **before** the write (phase `prepared`, then `armed` after the write, then `restored`) (`BallAssetService.cs:117-119`).
  - File: `<userData>/blacktop/ball-asset-session.json`, plus the log `…json.log.jsonl` (truncated at 1 MiB).
  - Schema 1 fields: `pid`, `birth`, `imageBase`, `image`, `holder`, `records`, `address`, `allocation`, `index`, `sourceModel`, `logical`, `phase`, `used`, `original`, `replacement` (`BallAssets.cs:42-45`).
- **Persistence**:
  - Selection: `<userData>/blacktop/ball-selection.json` = `{"schema":"blacktop-ball-selection-v1","selected":"<id>"}` (`ball-selection-manager.js:80`, `main.js:217`). It is reset to `current` on each launcher start (see section 2).
  - Legacy loose-file session: `<userData>/blacktop/ball-selection-session/session.json`, schema `blacktop-ball-selection-session-v1`. Recovery only. `installLoose` is never called in this build (`ball-selection-manager.js:97-119`).
- **Thumbnails** (`ball-thumbnails.js`):
  - The bundled `W/assets/blacktop-ball-previews/catalog.json` (468 items, `thumbnail` path) is used first.
  - Otherwise `store/thumbnails/<candidate>.iff` is resolved via the `previewCandidates` heuristics (`ball-selection-manager.js:36-44`). It is decoded with `Nba2kVczDecoder.exe` (20000 ms timeout), the single DDS is extracted, and it is converted with texconv to PNG (15000 ms). The PNG must be ≤512×512 and ≤2 MiB.
  - Disk cache: `cachePath('Blacktop','ball-thumbnails')`. Queue concurrency 4, max 64 pending (returns `{state:'busy'}`), in-memory LRU of 128.
- **IPC**: `ball:status` (view with choices), `ball:select(id)`, `ball:thumbnail(id)` → `{state:'ready', dataUrl}` / `{state:'missing'}`. Push: `ball:status`.
- **Ball scale (97%)**: only `BallScaleRecovery.restore()` runs, at startup, launch and cleanup. It restores `<userData>/scaler/ball-session/session.json` (schema `ball-scale-session-v1`, comment "Recovery only for 0.35.115-test.1"). `BallScaleSession`/`buildScaledBall` (factor 0.97) are not wired (`main.js:124, 2017`, `ball-scale-recovery.js:2`).

#### 4. Build profiles
- WPF: updated build 0x6AA2013D only.
- **V2: the WPF mechanism is invalid.** The live read-only snapshot showed the court (gameplay) ball is model 0x42A7A5EF, row 0, `ball_gameplay_official_DEFAULT.iff`. The Normal model 0xCB5691BE is a *prop* used by spectators. Conclusion: "Normal filename substitution was consumed by spectator props, not the separate actual court ball" (`TA/ball-gameplay-v2-offline-audit-20261016.json`, `confirmedFailure`).
- The V2 replacement writes one DWORD at GameData owner+0xDA4 (native setter 0x026FA830), with 4 observations ≥100 ms apart at selection, an empty native ball manager, and both resources unloaded (same file, `capability`). Modules: `ball_gameplay_v2.rs`, `ball_gameplay_runtime.rs`, `ball_gameplay_native.rs`.

#### 5. Rust must match
- [ ] A ball picked in a previous launcher run is NOT applied. At startup the selection is `Current installed` (WPF resets the saved file to `current`).
- [ ] A ball picked before Play in the same run is applied in that Play session, without re-picking, at the first stable Blacktop selection.
- [ ] No write happens unless at Blacktop player selection (mode 0x92DF06DE ×2, stage 3), the ball is fully unloaded and 4 consecutive observations 100 ms apart agree.
- [ ] Status reaches `Ready for Blacktop…` before the user starts. During the match it shows `applied`. After the match it restores at a quiet unloaded menu and automatically re-prepares at the next selection, so match 2 also uses the ball.
- [ ] Changing the ball mid-match does not touch memory until the current ball unloads. The status shows "Choice saved for the next match…".
- [ ] Selection observations from the detect loop never trigger a restore by themselves.
- [ ] A durable journal is written before any game write. A launcher restart with a live game recovers (restores) only for the exact same pid + birth + image base.
- [ ] Stopping never force-kills the helper. If restore is pending, the UI shows "Ball cleanup continues until a safe main menu or game exit."
- [ ] Catalog = manifest `balls/ball_*.iff` minus the exclusions, `Current installed` first, grouped as in `describeBall`. Page size 24, `Show selected` button.
- [ ] Use the V2 GameData+DA4 mechanism, not the Normal-record filename swap (the WPF swap only changes spectator props on V2).

---

### Scoreboard picker

#### 1. User-visible behavior
- **F8 bar** (`blacktop-panel.html:22`): label "Scoreboard". The button reads `<label before ' · '> ▾`, default `Default ▾` (`scoreboard-picker.js:12-13`).
  - Opening it closes the arena menu if open (`:34-35`). Same overlay-expansion pattern as the ball picker.
  - Dialog: eyebrow `BLACKTOP`, title `Scoreboard catalog`, `Back to game`. Search placeholder: `Search scoreboard names`.
  - Filters: `All`, `Classic`, `Broadcast`, `Eras`, `Experimental` (only those present). Group rule: `tested===false` → Experimental, `era-*` → Eras, `nativeController==='SCOREBUG_MAIN_GAME'` → Broadcast, else Classic (`:6, 69`).
  - Tiles show a preview image from `scoreboard-preview-catalog.js` (`assets/scoreboard-previews/<scene>-asset.png`) or `PREVIEW UNAVAILABLE`, plus a caption (e.g. `Ringers vs Ballers · illustrative offline render`).
  - Choosing optimistically sets `pending` / `Selecting <label>…`. On error it reverts to the prior selection with the message, or `Could not save scoreboard choice.` (`:49-57`).
  - If status fails, the button reads `Scoreboard unavailable` and is disabled.
  - When the panel is shown, the F8 picker re-requests status (`:110`). This triggers `refreshStatus()` (see section 2).
- **Launcher page row** "Scoreboard" (`index.html:113-118`). Short status for kind=scoreboard:

  | Phase | Text |
  |---|---|
  | pending | `Waiting for Blacktop` |
  | applied | `Active` |
  | idle | `Stock scoreboard` |
  | ready | `Ready` |
  | saved | `Saved` |
  | error | the raw message |
- **Catalog**: 21 entries in `W/data/scoreboards.json`, ids at lines 6-222. The picker allowlist removes `mycareer-story` and `flashback-openrun`, leaving **19 selectable** (`scoreboard-manager.js:6-8`).

  | Group | Selectable choices |
  |---|---|
  | Classic | default, simple, team-colors, proving-grounds, mypark, euro, mycourt-scrimmage*, mypark-simple*, proam-spectator*, w-scrimmage* |
  | Broadcast | world-full, proving-grounds-full, broadcast |
  | Eras | era-1980, era-1990, era-2000, era-2010 |
  | Experimental | highschool (`High school · Experimental`), shut-it-down* (`Shut It Down · Experimental`) |

  \* = `requiresLayoutAdapter:true`.
- Catalog note (`scoreboards.json:1242`): "Default, Simple, Broadcast and 1980s verified."
- **Default**: `default` (`Default`, resource 0x39264490, file `2kwscorebuggenericreduced.iff`). Initial state: `{selected:'default', phase:'idle', message:'Uses the stock Blacktop scoreboard.'}` (`scoreboard-manager.js:23`).
- **Saved-choice load** (`:29-38`):
  - Migrations: `mycareer-story` → `broadcast`, `flashback-openrun` → `default`.
  - A saved non-default choice gives `pending` / `Saved — waiting for Blacktop.`
  - Unreadable file → message `Saved scoreboard choice could not be read; using Default.`
- **Select**: writes the file, then `pending` / `Saved — preparing for the next Blacktop match.` (`:63`).

#### 2. Timing and gating
- **Manager lifetime**: created lazily on the first `scoreboard:status`/`select` IPC, after `waitForInitialization()`. The launcher Blacktop page calls `scoreboardStatus()` at renderer load (`blacktop-setup-picker.js:95`), so in practice this happens at launcher startup.
- **Polling**: `start()` → `tick()` (`main.js:262`). `tick()` reschedules itself every **3000 ms**, or **0 ms** if the revision changed during the tick (`scoreboard-manager.js:98`). It runs regardless of whether the game is running.
- **Tick early-exit** (`:74`): if `phase==='error'` (polling stops; **no implicit retry after an error**), or `selected==='default' && phase!=='pending'` (stock already confirmed).
  - So a non-default choice is polled forever, including after `applied`. This is how it re-applies on the next match and in a new game process (the property resets when the game restarts).
- **Layout gate** (`:79-82`): for choices with `requiresLayoutAdapter` and `!scoreboardLayouts.ready` → `pending` / `Launch the game through this test launcher to prepare the scoreboard.` (no helper call). `ready` becomes true only after `ScoreboardLayoutSession.prepare` succeeds at Play (`scoreboard-layout-session.js:112`), and false on restore.
- **Helper invoke**: `NBA2K27Scoreboard.exe apply <id> <gameRoot> <dir>/last-change.json`. `execFile` timeout **8000 ms**, maxBuffer 65536 (`:15-22`).
  - Responses: `pending` / `applied` / `error` (plus `ready` for `check`).
  - Helper messages, in gate order (`ScoreboardTool.cs`):
    - no game: `pending` / `Saved — waiting for NBA 2K27.` (`:73`)
    - more than one game: `error` / `More than one NBA 2K27 process is running.`
    - Blacktop package slot (id 0x0A2ADB76 in the 128×24 table at PackagesRva) not present: `pending` / `Enter Blacktop selection to prepare this choice.`
    - package still loading (byte +16 != 0): `pending` / `Waiting for Blacktop to finish loading.` (`:100-102`)
    - no changes needed: `applied` / `Active in this match` (scoreboard active and matching), `applied` / `Ready for the next match` (none active), or `pending` / `Pending — next Blacktop match` (active, different) (`:119-122`)
    - a scoreboard is active (`ActiveRva` or `OtherActiveRva` non-zero) and changes are needed: `pending` / `Pending — leave the match to change scoreboard.` (`:123`)
    - written: `applied` / `Ready for the next match` (`:136`)
- **Transient tolerance**: `error` / `Unrecognized resource table.` is shown as `pending` / `Waiting for game scoreboard resources.` for up to **45000 ms** before surfacing as an error (`scoreboard-manager.js:85-91`).
- **Error recovery**: `refreshStatus()` runs on every `scoreboard:status` call, only when phase is `error` and not busy. It runs the read-only `check`.
  - If the check returns `applied`, the state becomes `applied`.
  - If it returns `ready`: `error` / `Preflight now passes. Select this scoreboard again to apply it; no change was made.` (`:40-52`). The user must re-select. Selecting bumps the revision and restarts ticking.
- **Concurrency**: the helper holds the mutex `Local\2kLauncherScoreboardPreparation` with `WaitOne(0)`. Contention → `pending` / `Waiting for scoreboard preparation.` (`ScoreboardTool.cs:139`). Manager selects are serialized through the `this.saves` promise chain.
- **Match start / next match**: writes happen only while no scoreboard is active, i.e. between matches at Blacktop selection. A pick made mid-match stays `pending` until the user leaves the match. There is no live HUD reload (`ML/SCOREBOARD-TEST-README.md:24-29`).
- **Restore**: no explicit memory restore. Changes are session-only (they clear on game exit). Selecting `Default` writes the stock profile back at the next safe point.

#### 3. Mechanism
- **Build detection** (`ScoreboardTool.cs:80-98`): same two PE stamps as arena, with 11 code anchors per build. Failure → `This game update is not supported by the scoreboard picker.` / `This game build is not supported by the scoreboard picker.`
- **RVAs** (`:21-34`):

  | Name | older | updated |
  |---|---|---|
  | ActiveRva | 0xA907560 | 0xA90B540 |
  | OtherActiveRva | 0xA8E2660 | 0xA8E6640 |
  | ResourcesRva | 0xBE01608 | 0xBE05908 |
  | ResourceVtable | 0x807F878 | 0x80837B8 |
  | ResourceMethod | 0x506F510 | 0x5071E10 |
  | PackagesRva | 0xA8F89A0 | 0xA8FC980 |
- **Resource validation** (`:46-69`):
  - Hash table at `*(ResourcesRva)`: count at +0xA0 (power of two, ≤1M), buckets at +0xA8. Bucket = `0x988E62C4 & (count-1)`.
  - Rows with ids 0x988E62C4 / 0x7DE5F718; the owner must have +0x54 == 0xDCE5273B with a verified vtable and method.
  - Registry: version u32 must be 1525, then 1525 entries of 260 bytes (`id` + UTF-16 filename).
  - Requires the stock id 0x39264490 → `2kwscorebuggenericreduced.iff` and exactly one entry for the wanted id with its expected filename (`ScoreboardCatalog.Files`).
- **Package walk** (`:103-116`):
  - Package header: node, storage, used (≤capacity≤300). Nodes are 0x88 bytes in a BST keyed by u32 id. Find the node id == 0x39264490 (left/right at +0x78/+0x80).
  - The property list starts at `node+0x58`, sentinel `node+8`, 96-byte entries with next at +0x50, at most 128 entries.
- **Profile plan** (`ScoreboardProfile.cs:149-173`): exactly three properties.
  - Resource: key 0xFE11D138 (Name) or 0x45E79640 (Resource).
  - Class: key 0x2C8A369B.
  - Mode/handler: key 0xFEC8E933 or 0xC63510F0.
  - Each must have scalar type `(u32@+4 & 15)==5`.
  - New headers (first 12 bytes): resource = key `Name` if wanted==stock else `Resource`, value = wanted id. Class value = profile[0]. Mode = key profile[1], value profile[2].
  - Profiles: Blacktop {0x2A22DFCC, 0xFEC8E933, 0xAFA29832}, main-game {0x3A09A326, …}, eras/handler {0x75AFE63A, 0xC63510F0, 0x759F1AB6} (`ScoreboardCatalog.cs:4`).
- **Write**:
  - Before writing, a receipt is saved to `<userData>/blacktop/scoreboard/last-change.json`: `{schema:"scoreboard-picker-rollback-v2", pid, started, changes:[{address, before(base64 12B), after}]}`, flushed (`:126-128`).
  - Then `Stable()` re-reads every remembered snapshot and `Inactive()` is re-checked.
  - Each change: verify the 96 bytes are unchanged and still inactive → `WriteProcessMemory` 12 bytes → read back 96 (`:129-135`).
  - Must be private RW memory (Type 0x20000, Protect 4), 4-byte aligned (`:124`).
- **Layout adapters** (Mods install at Play) — `ScoreboardLayoutSession`:
  - At Play stage `preparing-scoreboards` / `Preparing scoreboard data bindings…` (`main.js:1776-1777`): restore the previous session.
  - Refuse if any target already exists in Mods: `A custom scoreboard mod is installed; it was left untouched.`
  - Build the 9 layouts from `scoreboard-text-adapters.json`: mycourt-scrimmage, mypark-simple, proam-spectator, flashback-aau, flashback-highschool, flashback-openrun, shut-it-down, w-scrimmage, triple-threat; assets `gooey/<file>.iff`.
  - Each layout is a fixed set of 8-byte edits on one `.VCUIELEMENT` member, bound to the member's `sourceSha256` (`scoreboard-layout-patch.js:6-17`). Six `loc/english.iff` text targets are verified read-only first (`scoreboard-layout-archive.js:7-14`).
  - Write the journal `<userData>/blacktop/scoreboard-layouts/session.json` (`{schema:1, modsRoot, files:[{asset, hash}]}`), then create the files with the exclusive `wx` flag. Set `ready=true`.
  - Cache: `<root>/cache/<id>.iff` + `bundle.json` (schema 2, identity = sha of entries, archive size/mtime, decoder stat, layouts, targets).
  - Restore removes only files whose sha matches. It runs on launch failure, game-end `onCleanup` and launcher shutdown, and refuses while the game is running (`scoreboard-layout-session.js:86-101`).
- **version.dll**:
  - The session writes `SCOREBOARD_TEXT_TEST=1` into the loader config. `scoreboardTextTest:true` is hardcoded at `main.js:1794`, and `version-proxy-session.js:264-272` verifies every layout file exists in Mods and that its provider size matches (`Prepared scoreboard layout is missing.` / `…unavailable through the provider.`).
  - The proxy then treats these exact leaf names (optionally prefixed `gooey/`) as forced overrides: `mycourtscrimmagescorebug.iff`, `myparkscorebugsimple.iff`, `proamspectatorscorebug.iff`, `scorebugfamilyflashbackaau.iff`, `scorebugfamilyflashbackhighschool.iff`, `scorebugfamilyflashbacknbaopenrun.iff`, `shutitdownscorebug.iff`, `thewscrimmagescorebug.iff`, `triplethreatonlinescorebug.iff` (`VP/scoreboard_text_override.hpp:9-15`, `VP/version_proxy.cpp:804`, compiled only under `NBA2K27_SCOREBOARD_TEXT_TEST`).
  - Any other value than `1` makes config parsing fail (`version_proxy.cpp:426`).
  - `HIGHSCHOOL_DIGIT_TEST=7` / `scorebug_highschool.iff` is a separate probe, not enabled at Play in this build (`main.js:1794` passes no `highschoolDigitTest`).
- **selector_override.hpp is NOT scoreboard-related**: it maps `char/signatureconfigs_all.iff`, `char/signatureconfigs_editplayer.iff` and `char/fpplayer_global.iff` (the player selector configs) when `SELECTOR_OVERRIDE=1` (`VP/selector_override.hpp:4-9`, `version-proxy-session.js:247-253`). It is listed here only because the session requires `scoreboardTextTest` whenever `selectorOverride` is on (`version-proxy-session.js:175`).
- **Persistence**: `<userData>/blacktop/scoreboard/selection.json` = `{"schema":1,"selected":"<id>"}`, written via `.tmp` + rename (`scoreboard-manager.js:57-60`, `main.js:254`).
- **IPC**: `scoreboard:status` (returns `refreshStatus()`), `scoreboard:select(id)`. Unknown id → `Unknown scoreboard choice.`. Push channel: `scoreboard:status`.

#### 4. Build profiles
- WPF: older 0x6A974D13 and updated 0x6AA2013D.
- V2: an independent Rust module (`scoreboard_v2.rs` etc.) with RVAs packageTable 0x0A98CC40, active 0x0A99B850, otherActive 0x0A976900, resourceRoot 0x0BE95238 and typedResourceVtable 0x08101C38.
- The V2 audit ships **5** layout files (mycourt, mypark-simple, proam, shut-it-down, w-scrimmage), not 9, with 23 fixed 8-byte edits and 6 english targets. Catalog 21 recognised / 19 selectable, same migrations (`TA/scoreboard-v2-offline-audit-20261016.json`). Live gameplay was not verified.

#### 5. Rust must match
- [ ] A saved non-default scoreboard is loaded at launcher start (with migrations) and shows `Saved — waiting for Blacktop.`. It applies automatically at the next Blacktop selection, with no user action.
- [ ] The status poll runs every 3000 ms (0 ms right after a selection change), continues after `applied`, and re-applies in a second match and after a game restart.
- [ ] No write while a scoreboard is active (in match). The status is `Pending — leave the match to change scoreboard.`, and the change lands after leaving.
- [ ] Before the Blacktop package is loaded: `Enter Blacktop selection to prepare this choice.`. While it loads: `Waiting for Blacktop to finish loading.`
- [ ] `Unrecognized resource table.` is tolerated for 45 s as `Waiting for game scoreboard resources.`
- [ ] After an error, polling stops. Re-opening status runs a read-only check. If the check passes, the UI says `Preflight now passes. Select this scoreboard again to apply it; no change was made.` and the user must re-select.
- [ ] Layout-adapter choices show `Launch the game through this test launcher to prepare the scoreboard.` unless the layouts were installed at this Play.
- [ ] Layouts install with exclusive create, refuse to overwrite an existing user mod, and are removed (hash-checked) at game end and on launch failure.
- [ ] A rollback receipt is written before the 12-byte header writes. Each write is verified by a 96-byte readback.
- [ ] `mycareer-story` and `flashback-openrun` are not selectable. Saved values migrate to `broadcast` / `default`.

---

## Cross-cutting: modes, match start, restore summary
| | Arena | Ball | Scoreboard |
|---|---|---|---|
| Applied | live, at Blacktop selection | live, at Blacktop selection (after Play only) | live, between matches when the Blacktop package is loaded |
| Launch-time step | none | `prepare` + `arm` helper | install 9 layout IFFs + `SCOREBOARD_TEXT_TEST=1` |
| Persisted selection | no (memory, launcher-run lifetime) | file, but reset to `current` every launcher start | yes, auto-applies |
| Favorites | yes (`arena-favorites.json`) | no | no |
| Next match | same mapping persists | restore after use, re-prepare at selection | keeps polling, re-applies if needed |
| Game restart (launcher open) | helper re-attaches, re-applies | helper ends; needs a new Play | poll re-applies |
| Restore | mapping on default/EOF; template only at selection | at quiet unloaded menu; never force-killed | session-only; `default` writes stock back |
| Modes | Blacktop selection gate (mode 0x92DF06DE); effect on other modes not checked in WPF (**UNVERIFIED**) | Blacktop only; restore requested if used and gameMode != 6 | Blacktop package (0x0A2ADB76) cached descriptor only |

## Known divergences in current Rust port (brief, from notes)
- Rust 0.48 persists `arenaId`/`scoreboardId` in Settings and queues saved arena + scoreboard once after the first `loader-active` (`provider_launch.rs:401 queue_saved_choices`, `SN/qa048-arena-defaults.md`). Arena auto-apply is an extension beyond WPF. Favorites are stored at `Data/arena-favorites.json` (`backend.rs:1213`), not `userData/blacktop/`.
- V2 ball uses the gameplay GameData+DA4 runner, not the WPF filename swap (`TA/ball-gameplay-integration-review-20261016.json`).
- `TA/feature-gap-inventory-20261017.md:60-67`: arena, favorites, saved defaults, ball and scoreboard all UNV. Arena (AAU Gym) was live in QA0.34 but failed at selection in 0.48. Ball and scoreboard failed in 0.48 and are retryable in 0.49. Top priority (+2.7) is a live test at selection, a re-apply on match 2, and saved defaults (`:123`).

- 0.50 catalog pass (2026-10-17, see `blacktop-catalogs.md`):
  - Arena rows use ArenaBrowser group, sort and terms, with all 165 listed (F8 shows `· Needs work`).
  - Ball rows use describeBall labels/groups and the group/label/id sort.
  - Scoreboard rows carry captions.
  - The launcher `Catalog` opens a port of the WPF dialog (`frontend/BlacktopCatalog.tsx`), whose footer shows WPF's idle/saved messages.
  - The saved ball resets to current each launcher start.
  - Still divergent:
    - Game-decoded ball thumbnails (`ball:thumbnail`) are not ported.
    - The launcher row short status (sr-only + title) is not shown.
    - The stock ball's data label stays `Currently installed`, displayed as `Current installed`.

## Open questions / UNVERIFIED
- Whether the arena mapping (953/954 slots) also affects non-Blacktop modes while applied. WPF only gates *writes* on selection, and the mapping stays in place afterwards. **UNVERIFIED.**
- The meaning of the five stock mapping ids (953, 954, 907, 709, 709). Only the first two are rewritten. Not found in WPF sources.
- `<userData>` resolution under the WPF host (`app.getPath('userData')` with `launcherStorage` organized/portable modes) was not traced here.
- The arena catalog drift between `W/data` and `WR/arena` (`arena_reccenter_lobbyint` disabled flag) looks accidental. The UI copy (disabled) wins.
- `arena-launcher-picker.js` is not loaded by any HTML in this build (it appears only in `release.json`). Treat it as dead code. The live launcher UI is `blacktop-setup-picker.js`.
