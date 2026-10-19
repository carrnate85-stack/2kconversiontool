# Outfits (apply + carry) — WPF reference
Source build: WPF quality-pass-qa-20261010 (frozen). Last researched: 2026-10-17.

Path shorthand: `W` = `nba2k27-launcher-wpf/release/quality-pass-qa-20261010/App`. All other paths are relative to
`C:\Users\carrn\CodexProjects\2kconversiontool`. The F8 bar, Arena, Scoreboard, Ball and Rules menus are covered in
`blacktop-overlay.md`. This file covers only the outfit parts of the overlay.

## Summary
WPF outfits run in **two separate stages that share one JSON file**:
1. **Selection-screen apply** (out of process). `NBA2K27Blacktop.exe` (source `nba2k27-roster-research/src/BlacktopTool.cs`)
   swaps the clothing-list pointer and count on each lineup player's PLAYER_DESCRIPTION, then sets a per-slot refresh byte.
   It is driven by `W/Reference/src/core/blacktop-manager.js` through the IPC calls `blacktop:scan` and `blacktop:apply`
   (`W/Reference/src/main.js:322-343`). Automatic random runs on a **100 ms main-process tick** (`core/blacktop-auto-open.js`).
2. **Match carry-over** (in process). After a verified apply with 3..7 pieces, the choice is written to
   `userData/blacktop/original-outfits.json.match-choices.json`. When the loader reaches `loader-active`,
   `session_saved_outfits.ps1` polls that file. It waits for 3 identical readiness snapshots 250 ms apart **inside the loaded
   match**, then writes a binary `BOP3` plan plus a `SAVED_OUTFITS_V1` trigger. The `version.dll` worker
   (`character_description_clone_sep11.hpp`) clones each description, expands its clothing list and pushes one STOCK
   create request per player so the game rebuilds the actor. This repeats every match: the script re-arms after 4×500 ms of
   stable selection screen.
3. The outfit creator, saved presets, `.2koutfit` share and the mannequin are launcher-side only. Nothing reaches the game
   except through stage 1.

**Most likely to break in a port:**
1. **Carry needs a separate, later moment.** Clothes are written at the selection screen, but carry-over only fires once the
   *match* is loaded and stable (3 snapshots, 250 ms apart). The plan is consumed **before** the trigger, so there is no
   retry. A port that tries to carry at selection, or retries after an uncertain result, diverges.
2. **Manager pointer form.** WPF uses `manager-0x78` for the full MANAGER_INSTANCE (active-list member and request field 3)
   and the interface pointer everywhere else (`character_same_outfit_rebuild_sep11.hpp:22-23,90`). The Rust V2 port
   missed this and its carry never fired (see Known divergences).
3. **Piece-count gates.** Selection apply accepts 2..7 pieces. Match carry only accepts **3..7 new pieces from a stock
   2-piece source** (`count==2` is checked at `session_saved_outfits.ps1:56` and in `expanded_copy_matches`). A
   2-piece result reports `unsupported-piece-count`, not an error.

## Key source map
| Concern | file:line |
|---|---|
| IPC: scan / apply / carry-status | `W/Reference/src/main.js:322-343` |
| IPC: saved presets list/save/delete/export/import | `W/Reference/src/main.js:306-321` |
| Preset store path (organized vs legacy) | `W/Reference/src/main.js:148-152` |
| BlacktopManager construction (helper path, journal path) | `W/Reference/src/main.js:295-305` |
| Auto-open + auto-random wiring (`prepareLineup`) | `W/Reference/src/main.js:874-899` |
| BlacktopSession (carry) construction | `W/Reference/src/main.js:2052-2057` |
| Carry start on `loader-active`, stop on `complete`/`stopped` | `W/Reference/src/main.js:2067-2071` |
| Carry stop at cleanup / exit | `W/Reference/src/main.js:2074` (onCleanup step "Outfit helper"), `:2260`, `:2347` |
| Outfit resolution, random modes, journal, apply, verify | `W/Reference/src/core/blacktop-manager.js:85-329` |
| Automatic random at selection | `W/Reference/src/core/blacktop-manager.js:211-253` |
| Selection tick (100 ms / 2000 ms) | `W/Reference/src/core/blacktop-auto-open.js:4,42-69` |
| Match-choice store (`blacktop-match-choices-v1`) | `W/Reference/src/core/blacktop-match-choices.js:6-33` |
| Carry child process + status strings | `W/Reference/src/core/blacktop-session.js:5-22` |
| Carry readiness / trigger / re-arm script | `W/Reference/runtime-resources/session-loader/session_saved_outfits.ps1:1-160` |
| BOP3 plan writer | `W/Reference/runtime-resources/session-loader/write_saved_outfit_plan.ps1:1-28` |
| Helper exe (scan/apply/original/detect/focus) | `nba2k27-roster-research/src/BlacktopTool.cs:175-487` |
| Plan parser (BOP1/2/3) | `nba2k27-version-proxy/src/character_saved_plan.hpp:1-46` |
| Per-match worker loop | `nba2k27-version-proxy/src/character_description_clone_sep11.hpp:371-443` |
| Stock create-request rebuild | `nba2k27-version-proxy/src/character_same_outfit_rebuild_sep11.hpp:1-133` |
| DLL thread start (`NBA2K27_SESSION_OUTFITS`) | `nba2k27-version-proxy/src/version_proxy.cpp:30-37,2048-2058` |
| Diagnostics dir the DLL and script share | `nba2k27-version-proxy/src/session_diagnostics.hpp:84-135` |
| Streetwear composition / random | `W/Reference/src/core/clothing-components.js:85-146` |
| Accessory slots / 7-piece math | `W/Reference/src/shared/accessory-slots.js:2-34` |
| Team clothing match | `W/Reference/src/core/clothing-team.js:1-17` |
| Mannequin random | `W/Reference/src/shared/mannequin-random.js:5-37` |
| Saved presets | `W/Reference/src/core/outfit-presets.js:8-85` |
| `.2koutfit` share | `W/Reference/src/core/outfit-share.js:5-39` |
| Outfit creator UI | `W/Reference/src/renderer/outfit-creator.js:1-196`, `W/Reference/src/renderer/index.html:150-179` |
| F8 outfit UI (row, random menu, presets, Random all) | `W/Reference/src/renderer/blacktop.js:64-428`, `W/Reference/src/renderer/blacktop-panel.html:12-18,38` |
| Random all (sequential) | `W/Reference/src/renderer/blacktop-batch.js:2-15` |
| Player dropdown | `W/Reference/src/renderer/blacktop-player-menu.js:1-46` |
| Prior V2 research | `nba2k27-launcher-tauri2/artifacts/outfits-v2-research-20261017.md` |
| Rust QA notes (sibling) | `side-session-notes/.../qa048-outfits-v2-random.md`, `outfits-v2-research.md` |

---

### A. Selection-screen apply (F8 "Edit outfit")

#### 1. User-visible behavior
- F8 bar (`blacktop-panel.html:12-18`): **Player** dropdown (placeholder `Select players in game`), buttons **Edit outfit**,
  **Random player** (menu) and **Random all players**. Footer: `F8 to hide · Esc back to game` / `Edit on player selection ·
  Session-only outfits` (`:38`). Status line `#blacktopStatus` starts as `Reading selected players…` (`:28`).
- Each time the panel is shown it collapses to the compact bar and calls `reloadPlayers()`: it loads presets, then
  `scanBlacktop` (`blacktop.js:418-428`). The busy text is `Checking Blacktop players…`. There is **no periodic
  refresh** (`blacktop.js:429`).
- No players: `No players to show. Select players in Blacktop, then reopen this overlay.` (`blacktop.js:125`).
- The first player is auto-selected when the current key is gone (`blacktop.js:105`). The player key is `pid:slot:roster|name`
  (`:103`).
- Edit row: pickers for top, bottom, socks, shoes and accessories. Note text: `Compatible choices apply automatically.`
  (`:367`). Any change **auto-applies after 350 ms debounce** (`:326-329`). Changing the top resets the layer and, if the
  current bottom is not a separable streetwear bottom, snaps it to the top's own bottom (`:330-335`).
  - Shoe options: `Style’s original shoes`, `Player’s roster shoes` (`:217-218`). Socks: `Style’s original socks` (`:230`).
  - Hints: `Choose a compatible bottom for this top.`, `Choose a top and bottom.`,
    `Streetwear ready: choosing a piece applies this set. `, `New combination; check the fit in game.` (`:265-269`).
- Busy text `Applying outfit…` (`:325`). The status tooltip shows `Game apply: N ms; verification: M ms` (`:322`).
- Result messages (`applyReportMessage`, `blacktop.js:64-72`):
  - Not confirmed: `Outfit request sent, but the update was not confirmed. Keep player selection open, then reopen the overlay.`
  - 2 pieces: `Outfit changed at player selection, but match carry-over needs 3–7 verified clothing pieces.`
  - Other non-queued result: `Outfit changed at player selection, but match carry-over was not verified. Reopen the overlay before starting the match.`
  - Success with carry queued: empty string (the status line is cleared).
- Any thrown error: the snapshot is dropped, and the status shows `<error> Reopen the overlay to reconnect; original outfits are saved.`
  (`blacktop.js:409`).
- **There is no Restore button in this build.** `act()` is only ever called with `restoreOriginal=false` (`blacktop.js:317,329,338`).
  `canRestore` is computed (`blacktop-manager.js:204`) but no UI reads it. The manager's `restore:true` path still exists.
- Uniform-dependent wardrobe entries are disabled with: `This player's uniform is not loaded. Choose pants for now; independent
  uniform loading is not available yet.` or `The current uniform could not be verified. Choose pants for now.`
  (`blacktop-manager.js:131-135`).

#### 2. Timing and gating
- **Only one operation at a time**: `exclusive()` throws `An outfit operation is already running.` (`blacktop-manager.js:165-169`).
  The renderer also has a `busy` flag that makes the list inert (`blacktop.js:404-410`).
- **Token**: every scan creates a new `crypto.randomUUID()` token. Apply requires the same token and gameRoot, otherwise
  `Refresh Blacktop players before applying.` (`blacktop-manager.js:190,255`). The snapshot and token are cleared *before*
  the native apply (`:309`). The post-apply rescan gives a new token, so the apply response carries a fresh snapshot.
- Helper process: `execFile` with a **15000 ms timeout** and a 2 MiB stdout buffer (`blacktop-manager.js:172-174`). stderr
  becomes the error message.
- Helper apply flow (`BlacktopTool.cs:431-485`):
  1. `CheckGame`. Rejects any module whose name contains `easyanticheat` (`:197-199`). Requires PE stamp/size
     `0x6AA2013D/0x36558000` ("updated") or `0x6A974D13/0x368C3000` (`:203-204`). Requires the refresh signature
     `83 FA 1E 77 0E 89 D0 48 69 C0 B0 07 00 00 C6 44 01 30 01 C3` at RefreshRva (`:206`). Requires the selection screen,
     `[root]==BlacktopVtable && [root+0xEA8]==3` (`:207`), with message `Open the Blacktop player-selection screen first.`
  2. `VerifyRequest` re-scans and compares pid, start time, controller, slot, description, model, roster, expected
     pointer, count and uniform. Requires `uniformProtocol==5`. Requires refresh byte `controller+0x30+slot*0x7B0 == 0`,
     otherwise `A clothing refresh is already pending.` (`:358-389`).
  3. A composed list is allocated (see Mechanism), then `VerifyRequest` runs **again** (`:449`).
  4. Writes are done in order, with a readback after each (see Mechanism).
  5. Polls the refresh byte **30 × 100 ms (3 s)**, re-checking controller and description each time.
     `consumed=false` after 3 s is not an error, just an unconfirmed report (`:473-479`).
- Manager post-apply: rescans and compares the actual items with the requested items (`blacktop-manager.js:314-326`).
  `exactMatch` requires `consumed && pointerMatches && uniformMatches`, the same pid/started, and identical item arrays.
- **Re-apply on later visits:** a selection-screen apply is **not** re-run on later lineups or matches. Clothes stay on that
  description object until the game rebuilds it. Carry into the match is stage C.

#### 3. Mechanism
- Helper CLI: `NBA2K27Blacktop.exe <scan|apply|original|detect|focus> <gameRoot> <layouts.json> [request.json]`
  (`blacktop-manager.js:172-173`, `BlacktopTool.cs:391`). Layout file: packaged `resources/roster-research/blacktop-preset-layouts.json`,
  dev `src/data/blacktop-preset-layouts.json` (`blacktop-manager.js:59-63`). It is a list of **1055** outfit groups
  (counted 2026-10-17). The helper also loads `streetwear-components.bin` when it is newer than the JSON, otherwise the
  JSON (`BlacktopTool.cs:435-436`).
- Old-build RVAs (updated `6AA2013D` vs original `6A974D13`), from `BlacktopTool.cs:175-189`:
  BlacktopRoot `0xAEB69D0`/`0xAEB29D0`, BlacktopVtable `0x78AEA48`/`0x78AABA8`, PreviewModelVtable `0x6778FC8`/`0x6775FA8`,
  PreviewDescriptionVtable `0x682D858`/`0x682A858`, RosterRoot `0x9A47EB8`/`0x9A43EB8`, Lineup `0x9BBF0A0`/`0x9BBB0A0`,
  Refresh `0x3ACB20`/`0x3ACA20`.
- Read path (`ReadPlayer`, `BlacktopTool.cs:301-342`):
  - controller = `[root+0x1080]`. Must have `[controller+0x18]==1`, otherwise
    `Blacktop models are not ready. Select your players first.` (`:345-347`). Up to 10 slots are scanned.
  - Slot entry = `controller+0x20+slot*0x7B0`. `[entry]` = model (CONTAINER), `[entry+8]` = description.
  - Description fields: `+0xB8` roster, `+0xC8` clothing list pointer, `+0xD0` count (2..7), `+0xF0` uniform context.
  - Roster fields: `+0x28`/`+0x00` first and last name (40 chars), `+0x110` cyberface u16, `+0x60` team pointer.
    Team table = `[[[RosterRoot]+0xA8]+0x88]`, stride `0x1550`. Team id u16 is at `team+0xF20`, team name at `team+0x2FA`.
  - Each player's wardrobe group is found by testing every layout record's basis against the live pointer (`ValidateLayout`,
    `:258-267`). If none match: `Loaded outfit table does not match a verified preset layout.`
- Composed list (`CreatePrivateList`, `:234-256`): `VirtualAllocEx` 4096 bytes RW, then protected **PAGE_READONLY**.
  These pages are **never freed** ("Retain immutable pages until game exit"). Layout:
  - Header of 10 u64 at offset 0: magic `C27CLOF2` (`0x32464F4C43373243`; `C27CLOF1` is a legacy 2-piece), pid, started
    ticks, description, model, roster, sourcePointer, count, streetIndex, rosterShoes(0/1).
  - Pointer array at `+128`, UTF-16 names at `+256 + i*512`.
  - The returned target pointer is `allocation+128`.
  - `PrivateList` recognizes these pages on later scans (MEM_PRIVATE, 4096 bytes, PAGE_READONLY, matching header) and
    reports them as the `custom` outfit (`:208-233`).
- Write order (`BlacktopTool.cs:452-472`), each step with a readback:
  1. If a uniform is added: `desc+0xF0 = targetUniform` **first**.
  2. `desc+0xC8 = target pointer` and `desc+0xD0 = count`, written as one 12-byte write.
  3. If the uniform is removed: `desc+0xF0 = 0` **after** the source update.
  4. Refresh byte `controller+0x30+slot*0x7B0 = 1`. The game clears it when it has consumed the change.
- Journal (`blacktop-manager.js:283-301`): `userData/blacktop/original-outfits.json`, keyed `pid:started:description:roster`
  with value `{pointer,name,slot,uniform,count,composeReservations?,managedUniform?}`. It is written atomically
  (tmp + fsync + rename) **before** any memory write. It is never overwritten for a key once present, so the original
  survives repeated applies and launcher restarts.
- Compose budget: **256 composed lists per game session**. The reservation is counted before the native call, even if the
  call fails. Over budget: `This game session has reached its custom-outfit limit. Restart the game to clear temporary clothing lists.`
  (`blacktop-manager.js:288-295`).
- Request JSON (`blacktop-manager.js:302-308`):
  `{pid,started,controller,slot,description,model,roster,expected,expectedCount,target,targetCount,top,bottom,streetId,shoe,sock,layer,accessory,accessories,items,uniformProtocol:5,expectedUniform,targetUniform}`.
  It is written `wx` to `<statePath>.<uuid>.request.json` and deleted in `finally`.
- Helper report: `{consumed,pointerMatches,countMatches,uniformMatches,composed,uniformAdded}` (`BlacktopTool.cs:480-482`).
  The manager adds `timings{nativeApplyMs,verifiedScanMs}` and `matchCarryover` =
  `queued` (exact match and 3..7 items, or restore) | `unsupported-piece-count` (exact but 2 items) | `not-verified`
  (`blacktop-manager.js:316-326`).
- No-op shortcut: if the target equals the current list and uniform, the helper is not called. The manager returns
  `unchanged:true` and still **records the match choice** when the items verify (`blacktop-manager.js:275-282`).
- Diagnostics: each apply appends to `Logs/blacktop-selections-<version>-<pid>.jsonl` as category `outfit`
  (`main.js:334-340`, `core/selection-status-log.js:9`).

#### 4. Build profiles
- WPF supports only `6A974D13/368C3000` and `6AA2013D/36558000` (September build) (`BlacktopTool.cs:203-204`). Version 2
  is refused with `Unsupported game build; outfit controls disabled.`
- V2 equivalents (verified on disk, from `outfits-v2-research-20261017.md` §2): root `0xAF46870`, lineup `0x9C496C0`,
  model vtable `0x67DD188`, description vtable `0x68920C8`, private description `0x68923C8`, owned list `0x68A2498`,
  refresh anchor `0x3B2050`. Slot stride `0x7B0` and fields +B8/+C8/+D0/+F0/+0x30 are unchanged.

#### 5. Rust must match
- [ ] Apply with a stale token or after another apply/scan fails with `Refresh Blacktop players before applying.` and writes nothing.
- [ ] A second apply while one is running fails with `An outfit operation is already running.`
- [ ] The original pointer/count/uniform is durably journaled before the first memory write and never overwritten for that key.
- [ ] A uniform add is written before the list swap. A uniform removal is written after it. The refresh byte is last.
- [ ] Apply refuses when the refresh byte is already non-zero (`A clothing refresh is already pending.`).
- [ ] The refresh byte is polled for at most 3 s (30×100 ms). `consumed=false` gives the "not confirmed" message, not a crash.
- [ ] Composed lists are read-only pages never freed during the session, capped at 256 per game session.
- [ ] A 2-piece result reports `unsupported-piece-count` with the exact WPF message. 3..7 pieces gives `queued`.
- [ ] Choosing a piece auto-applies after 350 ms. Opening the overlay rescans once. No periodic rescans while open.
- [ ] Uniform-requiring wardrobe options show the WPF "Choose pants" reason and cannot be applied.

---

### B. Random player / Random all / automatic random

#### 1. User-visible behavior
- **Random player** opens a menu row under the compact bar (only in compact mode) with four buttons: `Random streetwear`,
  `Random team clothes`, `Random all`, `Random accessories` (`blacktop.js:340-366`, labels `:351`). The menu has 2 columns
  under 700 px width, otherwise 4. Escape or a click outside closes it. Each button calls
  `applyBlacktop({randomFit:true, randomMode})` for the visible player.
- **Random all players** (`blacktop.js:413-417`, `blacktop-batch.js:2-15`). It is disabled unless every player has a
  `male`/`female` pool and no error (`blacktop.js:73-79`). While running, the button reads `Applying i/N…` and the status
  `Randomizing i/N: <name>…`. Players are applied **sequentially** with `{randomMode:'all', rosterShoes:true}`.
  - Before each player it checks the signature `[slot,name,cyberface]`. If it changed: `Lineup changed after N outfits; stopped.`
  - It stops at the first unconfirmed report: `Stopped after N confirmed outfits; refresh was not confirmed for <name>.`
  - Precondition failure: `Refresh players; every selected player must have a verified clothing fit.`
- **Automatic random** has no UI and **no setting**. It is not gated on `autoOpenBlacktop`, because `prepareLineup` is always
  passed (`blacktop-auto-open.js:47`, `main.js:897`). Each new lineup occupant gets one `randomMode:'all'` outfit with
  roster shoes as soon as the lineup is stable.

#### 2. Timing and gating
- Tick (`blacktop-auto-open.js:42-69`): the default delay is **2000 ms** and **100 ms while `selection===true`**. Each tick
  runs `NBA2K27Blacktop.exe detect` (15 s timeout). Ticks never overlap: the next one is scheduled in `finally`.
- `detect` output (`BlacktopTool.cs:392-419`):
  `{selection,lineupReady,requiredPlayers,selectedPlayers,playersReady,lineupPlayers:[{slot,description,roster}],session:"pid:started",foreground}`.
  - `selection` means `[root]==vtable && [root+0xEA8]==3`.
  - `playersReady` needs required players in 2..10 and even, `[lineup+4]==[lineup+8]`, controller ready, the loaded model
    count equal to `[lineup+0xB8]`, and every value re-read unchanged after the slots are read.
- **Stability:** `prepareLineup` runs only when this tick's signature `JSON.stringify([session,lineupPlayers])` equals the
  previous tick's signature, so it needs 2 consecutive identical reads about 100 ms apart (`blacktop-auto-open.js:54-57`).
- `autoRandomSelection` (`blacktop-manager.js:211-253`):
  - Validates ≤10 entries, slots 0..9, positive description and roster, unique slots. Otherwise emits `invalid-lineup`.
  - A new `session` clears the seen set, retry timer and throttle.
  - Prunes seen keys (`slot:description:roster`) that left the lineup. Returns if `busy` or all are seen.
  - **Throttle:** at most one attempt per lineup signature per **1000 ms** (`:223-226`).
  - Inside `exclusive`, scans and requires the scanned pid:started and player set to equal the observation, otherwise
    `scan-mismatch`.
  - For each unseen entry: skip if the player has an error or no pool. **If the journal already has a manual entry, mark it
    seen and skip ("Existing manual choices win, including after a launcher restart").** Otherwise mark it seen
    **before** the write (`:243`), then apply. After an unverified report it stops the loop and never retries
    (`:247-248`).
  - Status events are throttled to one per phase+message per **10 s** (`:159-164`). They go to the selection log as
    `{selected:'automatic',phase,message}` (`main.js:302`).
- Manual vs automatic precedence comes from the journal: any manual apply creates a journal key, and auto-random skips any
  keyed player.

#### 3. Mechanism
`resolveOutfit` with `randomFit` (`blacktop-manager.js:85-96`):
- `streetwear`: `randomStreetwear()` over the catalog with presets containing any team clothing removed (`streetOnlyCatalog`, `:94`).
- `all`: `randomStreetwear()` over the full catalog, including team and NBA/WNBA pieces.
- `team`: `randomTeamClothes()` (`:22-33`) picks presets whose top and every team piece match the player's teamId/teamName
  via `clothing-team.js`. If there are none, it picks one of the player's own stock 2-piece wardrobe outfits. If there are
  none of those either: `No compatible team clothing is available for this player.`
- `accessories`: `randomAccessories()` (`:108-130`) requires a current streetwear outfit, otherwise
  `Choose a streetwear outfit first, then add random accessories.` It adds 1..min(3, 7−base, free slots) items, one per
  slot. No room: `No compatible accessory slots are available for this outfit.`
- `randomStreetwear(catalog,pool,random,accessories,rosterShoes)` (`clothing-components.js:125-146`):
  - Presets are filtered by pool, `separable`, not `manualOnly`, and not `accessory-test-`.
  - Compatibility needs ≤1 shoe (`rosterShoes`) or exactly 1, ≤1 sock, and a total ≤7.
  - It picks a random top (unique by top), bottom, sock and shoe (`'roster'` when `rosterShoes`), then calls
    `composeStreetwear`. A random value outside [0,1) throws `Invalid random source`.
  - Accessories are carried from the current outfit unless given (`blacktop-manager.js:93`).
- Pool (`playerClothingPool`, `blacktop-manager.js:34-41`) is `female` if all streetwear-like wardrobe names start with
  `female_`, `male` if none do, otherwise null. A null pool makes random unavailable.
- Team matching (`clothing-team.js:3-11,14-16`) uses a `team_<ddd>` numeric tag when present, otherwise the 3-letter code
  table (teamId→`phi,mil,…`), the team name, or the city list. `isTeamClothing` also flags `nba`/`wnba` tokens and the
  NBA/WNBA nickname list.

#### 4. Build profiles
Same as A (old builds only in WPF). The random logic is pure JS and build-independent. It needs only the scanned wardrobe
plus `data/streetwear-components.json` (schema `nba2k27-clothing-components-v1`, 16221 presets, 196 additional shoes;
counted 2026-10-17).

#### 5. Rust must match
- [ ] Random player offers four modes with the exact labels. `team` falls back to a stock wardrobe outfit and errors only when both are empty.
- [ ] `Random streetwear` never picks a preset containing team/NBA/WNBA clothing. `Random all` may.
- [ ] Random all runs strictly one player at a time and stops on a lineup-signature change or the first unconfirmed report, with WPF's messages.
- [ ] Random all and auto-random use `rosterShoes:true` (shoes unchanged).
- [ ] Auto-random runs from a backend tick (100 ms on selection, 2000 ms elsewhere) after 2 identical lineup reads, whether or not the overlay is open.
- [ ] Auto-random never touches a player that has a journal entry (manual choice), including after a launcher restart.
- [ ] Auto-random marks a player seen before writing and never re-applies after an uncertain result. It throttles to 1 attempt/s per lineup.
- [ ] A new game session (pid:started) resets the seen set.

---

### C. Match carry-over ("Saved outfits apply once per Blacktop match")

#### 1. User-visible behavior
- Carry status line `p.blacktop-session-status` sits under the overlay status (`blacktop.js:80-84`). Warnings and errors
  are also copied into the main status. The default state message is `Saved outfits apply once per Blacktop match.`
  (`blacktop-session.js:5`).
- Phases and messages (`blacktop-session.js:13`):
  `waiting` → `Waiting for your saved outfits and Blacktop match…`; `applying` → `Loading your saved outfits…`;
  `applied` → `Outfit data applied. Clothing may take a moment to appear.`; `stopped` → `Outfit session ended.`;
  `warning:<text>`; `error:<text>`. A non-zero exit with no prior error shows the stderr tail or
  `Outfit session stopped before confirmation.`
- Script error texts (`session_saved_outfits.ps1`): `Game process unavailable`, `Unexpected game path`, `Game build changed`,
  `Duplicate player`, `Outfit worker not armed`, `Outfit rebuild stopped; see session report`,
  `Outfit confirmation timed out`, `Outfit session limit reached; restart the game.` Warning:
  `Some outfits have an unsupported piece count and were skipped.` (once per run).

#### 2. Timing and gating
- **Start:** `BlacktopSession.start(gameRoot)` runs when the version-proxy session publishes `loader-active` (`main.js:2069`).
  `started` makes it once per launcher session (`blacktop-session.js:7-8`). **Stop** on loader `complete`/`stopped`
  (`main.js:2070`), on the cleanup step `Outfit helper` (`main.js:2074`), on the shutdown observer (`:2260`), and at app exit
  with a bounded **2500 ms** wait (`:2347`).
- Child: `powershell.exe -NoProfile -ExecutionPolicy Bypass -File session_saved_outfits.ps1 -GameRoot <root> -ChoicesPath
  <userData>/blacktop/original-outfits.json.match-choices.json -DiagnosticsDir <Logs>/game-session` (`blacktop-session.js:10`,
  `main.js:2053-2055`).
- Per-match loop (`session_saved_outfits.ps1:85-158`):
  1. Prints `OUTFITS:waiting`.
  2. Every **250 ms** it reads the choices file. Each player that is fresh (choiceId not yet consumed), currently in the
     lineup (`InLineup`: exactly one `[Lineup+0xC0+8i]==roster`), and has 3..7 items gets a `Ready()` key. Any not-ready
     player throws `Match loading`, which resets the key.
  3. Breaks after **3 identical keys** (a 0.5 s minimum window, `:114-118`).
  4. Waits up to **10 s** (250 ms poll) for the DLL log `nba2k27_saved_outfits_<pid>[_matchN].log` to contain
     `STATE=ARMED` and `PID=<pid>`.
  5. Freezes the validated JSON and runs `write_saved_outfit_plan.ps1`, which writes `<prefix>.plan` CreateNew.
  6. **Marks the frozen choiceIds consumed before triggering** (`:132-138`, ledger `<choices>.consumed.json`, bounded to
     10000 ids, pid+started scoped).
  7. Creates `<prefix>.trigger` (CreateNew) containing exactly `SAVED_OUTFITS_V1`. Prints `OUTFITS:applying`.
  8. Polls the log every **1 s for up to 650 s**: `STATE=REBUILT` prints `OUTFITS:applied`. `FAILED|INCONCLUSIVE` throws.
  9. Re-arm: waits for **4 consecutive Selection() samples at 500 ms**, increments matchNumber and goes back to step 1
     (`:151-157`).
- `Ready(origin)` (`:47-62`). Old-build "updated" RVAs (`:30-38`): Settings `0xAACB9A0`, MatchTable `0xAACB9B0`,
  RecordClass `0x75C0D58`, ManagerClass `0x6820088`, PlayerClass `0x682C528`, DescriptionClass `0x682DB58` (private).
  Checks, in order:
  - `[[Settings]+0xA04]==6`.
  - Scans 30 records `MatchTable+i*0x50` with `[r]==RecordClass` and `[[r+0x30]+0x500]==roster`.
  - m=`[r+0x10]`: `[m]==ManagerClass`, `[roster+0x518]==m`, `[m+0xA0]==0`, `[m+0xB8]==0`.
  - player=`[m+0x98]`, d=`player+0xF20`: `[player]==PlayerClass`, `[d]==DescriptionClass`, `[d+0xB8]==d+0x378`,
    **`[d+0xD0]==2`**, `([d+0x7D]&1)==0`.
  - Reads 2 UTF-16 names from `[d+0xC8]`.
  - Exactly one match is required.
- Worker side (`character_description_clone_sep11.hpp:371-443`, `character_same_outfit_rebuild_sep11.hpp:57-131`):
  - Writes `STATE=ARMED`, then polls for the trigger every **500 ms with no limit** (session build). It deletes the
    trigger (single use), then reads and parses the plan.
  - **All** choices must resolve uniquely in the 30-record match table before any rebuild.
  - Players are processed sequentially (`STATE=RUNNING`). The first failure gives `STATE=INCONCLUSIVE` and the loop ends
    for good with "no retry or forced rollback" (`:417`).
  - Per player, after submitting the request it polls **240 × 250 ms (60 s)** for the queue to drain and a new PLAYER at
    `[manager+0x98]` (`same_outfit_rebuild_sep11.hpp:111-129`).
  - Between matches the worker waits for **4 × 500 ms selection samples** (`:430-439`), the same as the script.
  - Selection = `[0xAEB69D0]==0x78AEA48 && [0xAEB69D0+0xEA8]==3 && [[0xAACB9A0]+0xA04]==6`.
- The user is told to stay paused. The ARMED text is `Select outfits in launcher, then pause actual match and submit saved
  choices` (`:386`). REBUILD_PENDING says `stay paused while native model loads`. **The WPF script does not check pause
  state.** It only checks the readiness conditions above (**UNVERIFIED** whether pausing is actually required).

#### 3. Mechanism
- Match-choice file (`blacktop-match-choices.js:8-33`):
  `{schema:'blacktop-match-choices-v1', pid, started, players:[{choiceId(uuid), roster, slot, name, items[1..7], status:'preview-verified-not-applied-to-match'}]}`.
  - A different pid/started discards the old file.
  - Recording removes any entry with the same roster **or** slot (displaced occupants). A restore removes without
    re-adding. At most 10 players. Writes are atomic.
  - Item names: `NO SHIRT` or `^[A-Za-z0-9_]{1,255}$`.
  - Note: the research artifact calls this status "queued". The literal status is
    `preview-verified-not-applied-to-match`. "queued" is only the `report.matchCarryover` value.
- Plan `BOP3` (`write_saved_outfit_plan.ps1:14-23`, parser `character_saved_plan.hpp:11-45`):
  - Header: u32 magic `0x33504F42`, u32 pid, u32 count (1..10), u32 0, u64 process creation FILETIME.
  - Per player: u64 roster, u32 pieces (3..7), u32 0, then 7×512-byte UTF-16 slots (unused slots all zero).
  - Size is exactly `24 + count*(16+7*512)`.
  - The writer refuses any status other than `preview-verified-not-applied-to-match` or a pid/started mismatch
    (`Saved choices belong to another game session. Select outfits in the current game first.`).
- Per-player rebuild (`character_description_clone_sep11.hpp:191-351`, `character_same_outfit_rebuild_sep11.hpp`):
  1. Guards: exact build, no anti-cheat, `in_match`, the main thread id `[0x899C6D0]` is non-zero and is not the current
     thread, plus SHA-style byte fingerprints of clone/dispose/worker bodies (`:193-201`).
  2. Finds the record/manager/player for the roster (`:203-218`).
  3. Clones the description with `base+0x729250(source, heap=[0xA3801D8])`. The clone must have the private vtable
     `0x682DB58` (`:231-245`).
  4. Disposes the clone's list (`0x823930`), re-inits it with N pieces (`0x7AA3B0`), sets each name (`0x7AAB70`), then
     binds `clone+0xC8/+0xD0` (`0x17B970`) (`:250-280`).
  5. Rebuild (`same_outfit_rebuild::run`):
     - Verifies the idle manager: `[manager]==0x6820088`, `[manager-0x78]==0x681FCC8`, `[manager+0xA0]==0`,
       `[manager+0x18]==1`, `manager-0x78` is a member of the active list at sentinel `0xA37FFB0`, and `[manager+0xB8]==0`.
     - Verifies the queue `0xA380108` is initialized and empty.
     - Allocates a 0x40 packet via `0x4B219D0(0x40,heap,0,0xAD7FD911,0x673,8,true,false)`.
     - Fields: `[0]=0x67FBD48` (request vtable), `[1],[2]=packet`, **`[3]=manager-0x78`**, `[4]=context+0x18`,
       `[7]=tag|line<<32`.
     - Under `TryEnterCriticalSection(0xA380128)`, it re-verifies everything, sets `clone+0x7C |= 2`, stores the clone at
       `packet+0x28`, and inserts via `0x60BA00(0xA380110, packet)`.
     - Ownership then passes to the game. On a pre-transfer failure it frees the packet with `0x4B21C40`.
  6. Success needs the queue drained and a new PLAYER whose description names match the expected names.
  7. Logs: per player `nba2k27_saved_outfits_<pid>_player<i>.log`; per match `nba2k27_saved_outfits_<pid>[_matchN].log`
     with `SCHEMA=blacktop-saved-outfits-v1`, `STATE`, `REASON`, `PLAYERS_REBUILT`, `GAME_CODE_HOOKS=0`, `OBJECT_HOOKS=0`.
- DLL: the shipped `W/Reference/runtime-resources/session-loader/version.dll` (SHA-256 `ECEF8C6C…91DF`) **does include**
  `NBA2K27_SESSION_OUTFITS`. The strings `SAVED_OUTFITS_V1` and `blacktop-saved-outfits-v1` are present (grep 2026-10-17).
  It is built by `nba2k27-version-proxy/build_session_outfits.ps1:12`. The outfit thread starts from `write_load_report`
  next to the normal provider work (`version_proxy.cpp:2048-2055`).
- Shared directory: the DLL resolves `<launcher root>\Logs\game-session\`, either from the portable config or
  `%APPDATA%\2k Launcher\Logs\game-session`, or `<gameDir>\2k Launcher\...` via `nba2k27-mod-launcher-location.ini`, or the
  legacy `%APPDATA%\nba2k27-mod-launcher\diagnostics\game-session` (`session_diagnostics.hpp:84-135`). The launcher passes
  `logsPath('game-session')`. **These must resolve to the same directory**, or the script never sees ARMED.
- Restore: none in the match. A rebuild cannot be undone ("no manual rollback"). Outfits are session-only. A game restart
  clears everything (new pid/started invalidates choices, ledger and journal keys).

#### 4. Build profiles
- WPF: script `BindBuild` accepts `6AA2013D/36558000` or `6A974D13/368C3000` (`session_saved_outfits.ps1:24-39`). The worker
  accepts only `6AA2013D/36558000` (`character_queue_clone_sep11.hpp:36-42`). Other builds fall back to
  `character_description_clone_audit::worker` (`version_proxy.cpp:2051-2053`). V2 is not supported by WPF.
- V2 address map: see `outfits-v2-research-20261017.md` §2. Manager vtables are `0x68844D8` (full) and `0x6884898`
  (interface, full+0x78). Active sentinel `0xA409E90`, queue `0xA409FE8`/lock `0xA40A008`/heap `0xA40A0B8`, main thread
  `0x8A1F320`. The stock producer allocation line is `0x67C` on V2 (sibling note "D9"), while the old build used `0x673`.

#### 5. Rust must match
- [ ] Carry does nothing at the selection screen. It fires only when every chosen, in-lineup player passes `Ready()` 3 times in a row at 250 ms.
- [ ] Only players with 3..7 pieces from a stock 2-piece source are carried. Others produce the one-time warning and are skipped, without failing the batch.
- [ ] Choices are consumed (persisted ledger) before the trigger. An uncertain or failed rebuild is never retried in that match or in later matches.
- [ ] Each choice is carried in one match only. A later match with the same player uses only a newly made choice (new choiceId).
- [ ] After REBUILT the carry re-arms only after 4 × 500 ms of stable selection screen.
- [ ] Choosing a different player for a slot removes the old occupant's pending choice (same slot or roster).
- [ ] Status strings match the six WPF phases exactly.
- [ ] Manager identity: the full object (`interface-0x78`) is used for the active-list membership check and request field `[3]`.
- [ ] Carry stops when the loader completes or stops and on launcher exit (≤2.5 s).

---

### D. Outfit creator, saved presets, share, mannequin (launcher page)

#### 1. User-visible behavior
- Blacktop page, section **Outfit creator** (`index.html:150-179`):
  - Controls: `Saved outfits` (first option `New outfit`), `Name` (maxlength 64, placeholder `Name your outfit`),
    `Clothing fit` (`Men`/`Women`), **Save outfit**, **View on mannequin**, and a **More ▾** menu with **Import outfit**,
    **Export outfit** and **Delete outfit**.
  - Piece cards: Top, Bottom, Socks and Shoes, each with a `Change ↗` button, plus an accessory card grid.
  - Footer: `Prepare all previews`, `Import preview ZIP`, `Cancel`.
- Status strings (`outfit-creator.js`):
  - Open: `Saved outfits apply when assigned to a player in F8.`
  - Unavailable record: `This saved outfit is unavailable in the current catalog. Choose replacement pieces and save to repair it.` (`:128-129`)
  - Save: `Saved “<name>”. Available below and in the F8 panel after Refresh players.` (`:177`)
  - Export: `Exported .2koutfit — ready to share.` or `Export canceled.` (`:183`)
  - Import: `Imported “<name>”. Saved without changing any player.` or `Import canceled.` (`:186-188`)
  - Delete confirm: `Delete “<name>” from saved outfits? This will not change a player’s current clothes.` then
    `Outfit deleted. Players were not changed.` (`:191-192`)
  - Discard guard: `Discard unsaved outfit changes?` (`:23`)
  - Pool change: `Choose pieces for this clothing fit, then save.` (`:157`)
  - Mannequin apply: `Mannequin selections copied into the outfit draft. Save to keep them.` (`:155`)
- Button gating: Save needs a name, a valid top+bottom and valid accessories. **Mannequin is enabled only for `male`**.
  Export needs a valid draft. Delete needs a saved record (`:15-21`).
- The picker note in the creator is `This edits your outfit draft only. Save it, then apply it to a selected Blacktop player.` (`:164`).
- F8 per-player preset bar: a select with placeholder `Choose a saved outfit…`, or `No saved outfits for this clothing fit`
  when empty, plus **Apply preset** (busy `Applying saved outfit…`). It lists only `available` presets for the player's
  pool (`blacktop.js:368-389`). Saving in the creator dispatches `blacktop-presets-changed`, which re-renders the F8 rows
  (`blacktop.js:85-87`).
- Mannequin dialog (`fit-preview-ui.js:6-25`): title `Clothing mannequin`, buttons `Use these choices`, `Random clothes`
  (tooltip `Replace top, bottom and socks; keep shoes unchanged.`), `Random accessories` (tooltip `Replace accessories with
  up to three items; keep clothes and shoes unchanged.`), and `Discard`.

#### 2. Timing and gating
- The preset store serializes writes through a promise tail (`outfit-presets.js:53-65`). A file larger than 1 MiB or
  invalid is refused and left untouched.
- Apply preset: main resolves the preset (`savedOutfits().resolve`) and converts it to a streetwear apply value
  (`main.js:328-331`). From there it uses the same path as A.
- Sender gating: list is allowed from launcher or panel. Save/delete/export/import are launcher only
  (`requireLauncherSender`, `main.js:306-321`).

#### 3. Mechanism
- Presets file: organized storage `storagePath('outfits','outfit-presets.json')`, legacy `storagePath('data','blacktop','outfit-presets.json')`
  (`main.js:150`). Schema `{schema:'nba2k27-outfit-presets-v1', outfits:[{id(uuid),name(1..64),streetId,bottom,pool,shoe?,sock?,layer?,accessories?|accessory?}]}`.
  Limit 1000 outfits (`outfit-presets.js:8-85`). `list()` marks a record `available:false` when `composeStreetwear` no
  longer accepts it.
- `.2koutfit` (`outfit-share.js`): JSON with schema `nba2k27-shared-outfit-v1|v2|v3` (v3 if accessories, v2 if a single
  accessory), `name`, `top:{asset,original[]}`, `pieces:{bottom,shoe?,sock?,layer?,accessory?|accessories?}`. Max 16 KiB.
  Import matches the top by **asset + exact original item list**, otherwise
  `This outfit’s top variant is not available in this catalog. Nothing was imported.`. It never contains UUIDs, paths or
  pointers. Save dialog default `Outfit - <name>.2koutfit` (`main.js:309-315`).
- `composeStreetwear` rules (`clothing-components.js:85-123`): the bottom swap requires a separable preset and a bottom
  seen in the same pool. Shoe `'roster'` removes the style's single shoe. A sock replaces the single sock or appends one.
  Accessories are normalized to one per slot in the fixed 19-slot order (`accessory-slots.js:2,17-21`). Final count must
  be **2..7**.
- 7-piece math for UI: `baseCount = extras+2 (+1 sock if style has none) (±layer)` (`accessory-slots.js:28-33`). The
  accessory limit is `min(3, 7-baseCount)` (`mannequin-random.js:29`).
- Mannequin random (`shared/mannequin-random.js`): **male pool only**.
  - `clothes` keeps shoes (resolving "style's original shoes" first), keeps ≤3 accessories, and picks a different top
    where possible, plus a random separable bottom and sock.
  - `accessories` shuffles slots and picks up to the limit, preferring new items.
  - The F8 overlay preview frame reuses these for its fake player (`blacktop-overlay-preview-frame.js:49-53`).
- Preview rendering (launcher only, no game writes): `clothing-thumbnails.js` (cache `Cache/Blacktop/thumbnails`),
  `clothing-cache-job.js` (4 workers, batches of 64, cancellable), and `fast-clothing-preview*.js`, which reads game IFFs
  with `clothing-item-table.js` (schemas `0x3A70F295`/`0x6E2FAD8B`) and `clothing-color-bindings.js` (magic `0x20E04CC8`)
  for the 3D mannequin (`main.js:153-170,344-370`).

#### 4. Build profiles
Build-independent (catalog and IFF reading only). **UNVERIFIED** whether V2 game-data IFFs still match the item-table and
color-binding schemas.

#### 5. Rust must match
- [ ] Preset JSON schema, 64-char name, 1000-item cap, and atomic write match WPF, so the files are interchangeable.
- [ ] `.2koutfit` v1/v2/v3 round-trip with WPF files. A 16 KiB cap. Top matched by asset + original list.
- [ ] Presets that no longer compose show `· unavailable` in the creator and are hidden in F8.
- [ ] The F8 preset dropdown filters by the player's clothing pool.
- [ ] Mannequin and its random buttons are male-only.

---

## Known divergences in current Rust port (checked 2026-10-17, read-only)
- **V2 carry gate** is now emitted (`src/provider_access.rs:296` `version2OutfitCarryMapped`), fixing artifact TL;DR #1.
- **+0x78 manager fix** landed late in Rust (sibling note "ROOT CAUSE FIXED"). WPF already had it
  (`character_same_outfit_rebuild_sep11.hpp:22-23,90`).
- **Auto-random** lives in the frontend (`App.tsx` effect, `Settings.autoRandomOutfits` default true, `src/backend.rs:61,100`).
  WPF has it in a main-process tick with **no setting**. `outfit_carry_v2::capabilities().automatic_randomization` is still
  `false` (`src/outfit_carry_v2.rs:31`).
- **Random team clothes is missing** in the Rust random strip (sibling note: "Team clothes not offered (V2 list has no team id)").
- **Random all** in Rust reports per-player `error` refusals and **continues**. WPF stops at the first unconfirmed player.
- The Rust carry readiness is 3 samples/500 ms in the launcher plus a native `StableBoundary<…,3,250>`. WPF is 3 samples at
  250 ms in the script only.

## Open questions / UNVERIFIED
- Whether the game must actually be paused for the rebuild. WPF only *tells* the user to stay paused and never checks.
- Whether `blacktop-preset-layouts.json` (1055 groups) still matches V2 wardrobe IFF layouts (artifact Unknown #1).
- The WPF behavior for 1v1 vs 5v5 lineups beyond "required players even, 2..10" was not separately tested in sources.
- `canRestore` / `restore:true` exist in the manager, but no WPF UI calls them in this build. Whether a Restore control is
  wanted in Rust is a product decision.
- The first match uses `nba2k27_saved_outfits_<pid>` and later matches `_match<N>`. The script and worker number matches
  independently, so a desync after a worker failure is possible (the worker returns forever after the first non-zero
  result while the script throws). **UNVERIFIED** in practice.
