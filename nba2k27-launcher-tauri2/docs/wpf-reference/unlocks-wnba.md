# Unlocks (WNBA in Freestyle, WNBA in Blacktop, Debug Menus): WPF reference
Source build: WPF quality-pass-qa-20261010 (frozen). Last researched: 2026-10-17.

Path abbreviations used in citations (all relative to `C:\Users\carrn\CodexProjects\2kconversiontool`):
- `W/` = `nba2k27-launcher-wpf/release/quality-pass-qa-20261010/App`
- `UH/` = `nba2k27-mod-launcher/unlocks-helper`
- `ART/` = `nba2k27-launcher-tauri2/artifacts`
- `SSN/` = `C:\Users\carrn\AppData\Roaming\Claude\side-session-notes\local_98bf0092-fe39-4ae3-abce-e88edad0a633` (absolute path; sibling-session data)

## Summary
- The Unlocks page has three toggles: **Unlock WNBA Teams in Freestyle** (saved setting, default **on**), **Unlock WNBA Teams in Blacktop** (saved setting, default **off**), and **Enable Debug Menus** (never saved, resets to off at every launcher start). Sources: `W/Reference/src/renderer/index.html:404,410,420`, `W/Reference/src/core/paths.js:24-25`, `W/Reference/src/core/debug-menus.js:9,19`.
- Nothing is written when a toggle changes. Toggling only saves the setting (or, for debug menus, sets an in-memory flag). All memory work is armed inside `launchGame` of the Play flow, just before `launchWithoutEac` (`W/Reference/src/main.js:1798-1806`).
- All three features use one C# helper, `NBA2K27FreestyleTeams.exe` (`W/Reference/runtime-resources/unlocks/`, 163,328 bytes, SHA-256 `B1EC5FAA…9067`; `W/Reference/runtime-resources/manifest.json:150-152`). Its source is `UH/*.cs`, and `Main` dispatches on `args[0]` (`UH/FreestyleTeams.cs:180`).
  - Freestyle and Debug run as **one-shot polled helper calls** (6 positional args, one JSON line on stdout).
  - Blacktop runs as **one long-lived child** (`blacktop-watch`) that writes NDJSON status lines and restores memory when its stdin reaches EOF.
- **The live Freestyle unlock is the `category-*` route** (`UH/TeamCategories.cs`): four static DWORDs in two 6-entry category-order tables, with page protection flipped to RW and back. It is **not** the mask=0x68400033/category=5 "combined list" route in `Run()` (`UH/FreestyleTeams.cs:122-179`). That route was withdrawn on 2026-09-20 (`nba2k27-mod-launcher/ABANDONED_FEATURES.md:54-59`), and the launcher JS never calls it (`W/Reference/src/core/freestyle-unlocks.js:34`).
- Blacktop does two things:
  - It swaps one vtable callback pointer (`BLACKTOP_DELEGATE` +0xD10 on the old build) to the stock Learn2K roster loader while the user is at an inactive Home/Quick Play menu.
  - At the Blacktop selection screen it adds a remote RX "bridge" plus a jump-table rel32 patch and a label-key swap (Euro→Women), so the **Y category button** cycles to WNBA (`UH/BlacktopTeams.cs`, `UH/BlacktopCategory.cs`).
- version.dll is **not involved**. `freestyle_build_profile.hpp` is the Freestyle *audio/playlist* redirect profile (`nba2k27-version-proxy/src/freestyle_build_profile.hpp:10-18`, used at `version_proxy.cpp:762-785`). `version_proxy.cpp` contains no WNBA or unlock logic (grep for `wnba|unlock` finds nothing).
- **Most likely to break in a port:**
  - (1) Using the withdrawn mask/category-5 write instead of the 4-DWORD category route.
  - (2) Skipping the strict "new session" bind validation (pid/created/basis vs the Play timestamp).
  - (3) Blacktop restore-on-exit. Restoration is driven by stdin EOF, the helper is never force-killed, and the exe is "pending" until the game closes.
  - (4) On V2 the old offsets and slot do not map 1:1: +0xD10 became +0xD30, and the default V2 Freestyle roster has 0 WNBA rows (`ART/wnba-v2-research-20261017.md:16-18`).
  - (5) Debug menus must never persist and must ask for confirmation in a dialog.

## Key source map
| Concern | file:line |
|---|---|
| UI markup (labels, help, status nodes) | `W/Reference/src/renderer/index.html:395-433` |
| Freestyle/Blacktop toggle handlers, status rendering | `W/Reference/src/renderer/app.js:2730-2750`, initial checked state `:543-544` |
| Debug toggle renderer | `W/Reference/src/renderer/debug-menus.js:1-15` (loaded `index.html:854`) |
| Preload bridge (IPC channel names) | `W/Reference/src/preload.js:25-27,34-36` |
| Settings defaults | `W/Reference/src/core/paths.js:24-25` |
| `settings:save` IPC (no unlock-specific validation) | `W/Reference/src/main.js:954-967` |
| IPC handlers `unlocks:*` | `W/Reference/src/main.js:111-121,128` |
| Manager construction (dirs, helper path) | `W/Reference/src/main.js:2024-2034` |
| Arm on Play / stop on launch failure | `W/Reference/src/main.js:1798-1823`, `:1829-1833` |
| Exit cleanup (before-quit) | `W/Reference/src/main.js:2294-2353`, `W/Reference/src/core/exit-handler.js:5-34` |
| Native-host-loss cleanup | `W/Reference/src/main.js:2238-2291`, `W/Reference/src/core/host-loss-cleanup.js:4-16` |
| Freestyle JS state machine | `W/Reference/src/core/freestyle-unlocks.js:1-51` |
| Blacktop JS child supervisor | `W/Reference/src/core/blacktop-unlocks.js:1-38` |
| Debug JS state machine | `W/Reference/src/core/debug-menus.js:1-72` |
| Child stop helper | `W/Reference/src/core/child-lifecycle.js:26-37` |
| Helper entry / dispatch | `UH/FreestyleTeams.cs:180` |
| Freestyle offsets, build select, snapshot | `UH/FreestyleTeams.cs:41-107` |
| Freestyle policy (masks, roster verifier) | `UH/FreestylePolicy.cs:7-37` |
| Category tables / allowlist / journal | `UH/CategoryPolicy.cs:4-24` |
| Category route (live Freestyle unlock) | `UH/TeamCategories.cs:15-107` |
| Blacktop roster-loader callback worker | `UH/BlacktopTeams.cs:1-122` |
| Blacktop Y-category bridge | `UH/BlacktopCategory.cs:1-68` |
| Debug menus helper | `UH/DebugMenus.cs:1-108` |
| Retired front-menu cleanup (not an unlock) | `W/Reference/src/core/retired-front-menu-recovery.js:1-27` |

---

### Freestyle: Unlock WNBA Teams in Freestyle

#### 1. User-visible behavior
- Page `#page-unlocks`, group "Team access", subtitle "Choose where WNBA teams are available.", badge "Applies next launch" (`W/Reference/src/renderer/index.html:399`). Category strip text: "Changes apply when you launch with Play" (`:397`).
- Card controls:
  - Checkbox `#unlockWnbaFreestyle` with label **"Unlock WNBA Teams in Freestyle"** (`:404`).
  - Help text: "Adds WNBA to the team categories in Freestyle. Availability varies by mode." (`:405`).
  - Status `<small id="freestyleUnlockStatus" role="status" aria-live="polite">` (`:406`).
  - WNBA logo image `../assets/wnba-logo-horizontal.png` (`:403`).
- Aside text: "WNBA choices save automatically and take effect on your next Play session." and "Custom rosters must contain WNBA teams for them to appear." (`:428-429`).
- Initial state: `checked = settings.unlockWnbaFreestyle === true` (`W/Reference/src/renderer/app.js:543`).
- On change (`app.js:2734-2741`):
  1. The input is disabled.
  2. `saveSettings({unlockWnbaFreestyle: enabled})` is called.
  3. On success the status text is cleared. On error the checkbox reverts (`!enabled`) and an error toast is shown.
  4. The input is re-enabled.
- Status rendering (`app.js:2730-2733`) shows `message`, except that the text is blank when:
  - phase is `saved` or `disabled`;
  - the message starts with "Saved";
  - the message is exactly "Unlock helper stopped safely. Restart the game before retrying."
- Status sources: the live push `unlocks:freestyle-status` (`app.js:2748`), plus one initial `invoke('unlocks:freestyle-status')` (`app.js:2750`). The default view, when no manager exists, is `{phase:'saved',message:'Saved — applies on your next launch.'}` (`W/Reference/src/main.js:128`).
- Status strings the user can see, from JS (`freestyle-unlocks.js`):
  - "WNBA unlock is off for this launch." (phase `disabled`, :23; hidden by the renderer)
  - "Waiting for NBA 2K27." (:25)
  - "The game did not start in time. Unlocks will retry on your next launch." (`timeout`, :32)
  - "Unlocks require a new game session started with Play." (bind validation failure, :37)
  - "Game session ended. Unlocks are saved for your next launch." (`saved`, :43; hidden)
  - "Freestyle unlock helper could not complete. Check that the game and launcher use the same permission level." (exec failure without stdout, :14)
  - "Invalid Freestyle unlock response." (:35)
- Status strings from the helper (`UH/TeamCategories.cs`):
  - "Waiting for the Freestyle team picker to initialize the category unlock." (`bound`, :73)
  - "WNBA category unlocked for this session. Team availability depends on the mode." (`applied`, :76,98)
  - "Team category navigation was changed outside this session. Restart the game before testing." (`conflict`, :66)
  - "An earlier category update was interrupted. Close the game before retrying." (:72)
  - "Use the launcher's without-EAC session for unlocks." (:63)
  - "Cannot access NBA 2K27. Use the same permission level for the game and launcher." (:64)
  - "More than one NBA 2K27 process is running." (:59)
  - "The running game does not match the launcher game folder." (:60)
  - "The combined-list experiment is active. Restart with this category test build." (:82)
  - Snapshot waits: "Waiting for Freestyle team selection." (`outside`, `UH/FreestyleTeams.cs:88,92`), "Waiting for the mixed NBA/WNBA roster." (`UH/FreestylePolicy.cs:26`), "Waiting for both national teams." (:27), "NBA, Classic and All-Time teams are not ready." (:28)
- In game, the result is that the Freestyle team picker's category cycle (previous/next) visits the native WNBA category (index 3). Before the unlock that category is skipped.

#### 2. Timing and gating
- **No write on toggle.** The comment says "Only Play arms a session. Merely opening the launcher/settings never writes." (`freestyle-unlocks.js:20`).
- Arming: `freestyleUnlocks.arm(settings.gameRoot, settings.unlockWnbaFreestyle===true)` runs inside `launchGame`, after `scalerManager.arm` and before `blacktopUnlocks.arm`, `debugMenus.arm` and `launchWithoutEac` (`W/Reference/src/main.js:1799-1806`). It is synchronous (not awaited).
- `arm()` calls `stop()` (which increments generation), resets session/applied, records `startedAt`, sets **deadline = startedAt + 120000 ms** and schedules a tick at 0 ms (`freestyle-unlocks.js:21-26`).
- Poll intervals (`freestyle-unlocks.js:27`): **500 ms** until bound, **1000 ms** while bound but not applied, **3000 ms** after applied. A tick after a `bound` result is scheduled at 0 ms (`:45`).
- Action chosen per tick (`:34`):

  | State | Action |
  |---|---|
  | not yet bound | `category-bind` |
  | bound, not applied | `category-apply` |
  | applied | `category-observe` |

- The deadline applies **only before bind** (`!this.session && now > deadline`, `:32`). After bind, the helper keeps polling for as long as the user takes to reach the Freestyle team picker.
- Bind validation (`:37`). The result is rejected with "Unlocks require a new game session started with Play." unless all of these hold:
  - `pid` is a safe positive integer;
  - `created` and `basis` are decimal strings;
  - `basis >= 65536`;
  - `created` (a FILETIME) is >= `fileTime(startedAt)` and <= `fileTime(now+1000)`.

  `fileTime(ms) = (ms + 11644473600000) * 10000` (`:5`). A game that was already running before Play therefore fails, which is an intentional fail-closed check.
- Phase handling (`:41-45`):

  | Result phase | Effect |
  |---|---|
  | `bound` | stores the session |
  | `applied` | sets `applied=true` |
  | `no-game` / `ended` with a session | stops; emits `saved` "Game session ended…" |
  | `no-game`, `outside`, `waiting`, `applied` | keep polling |
  | anything else (`conflict`, `error`, `write-failed`, `unsupported`) | **stops polling permanently** for this launch |

  The result is still emitted in every case.
- Serialization: ticks are chained on `this.queue`. A `generation` counter discards stale results after `stop()` (`:29-31,40,47,49`).
- Helper-side gating for `category-apply` (`UH/TeamCategories.cs:49-103`):
  - A per-journal-file named mutex `Local\2kLauncherCategories-<SHA256(upper(fullpath))>` is taken with `WaitOne(0)`. If it is busy, the phase is `waiting` "A category check is already running." (:53-55).
  - Exactly one `NBA2K27` process must be running, and its image path must equal `<gameRoot>\NBA2K27.exe` (:56-60).
  - For non-bind actions, pid, StartTime FILETIME and module base must all equal the bound session, otherwise `ended` (:62).
  - Write actions are rejected if any process name starts with `EasyAntiCheat` (:63).
  - The table state is checked: tables must be "Owned" (each entry Original or Enabled), and must equal Original unless the journal is the same session (:66).
  - `Build(tables)` verifies the build and code bytes, and the category page protection must be PAGE_READONLY (2) (:67,72).
  - `Snapshot(true)` requires the Freestyle picker to be live: mode hash `0xF54D5D12`, setup DWORDs `[0]=6, [12]=1, [16]=1, [20]=4`, picker vtable match, and a deep roster verify (:81; `UH/FreestyleTeams.cs:88-107`).
  - Stock mask/category must be untouched (mask `0x401033`, category < 5), and otherCategory must be < 5 (:82).
- Once applied, there is **no re-apply on later visits**. The tables are static image data and persist for the process lifetime (`UH/TeamCategories.cs:79-80`). Observe confirms `Enabled` each 3 s. If the tables have changed, it reports `conflict` "Category navigation changed after applying. No automatic overwrite." (:75), and JS then stops.
- Helper timeout: `execFile` with `timeout: 8000`, `maxBuffer: 16384`, `windowsHide: true` (`freestyle-unlocks.js:13`). A non-zero exit that has stdout is still parsed (`:14`).

#### 3. Mechanism
- CLI (`freestyle-unlocks.js:12`):
  ```
  NBA2K27FreestyleTeams.exe <action> <gameRoot> <userData>\unlocks\team-categories-session.json <pid|0> <created|0> <basis|0>
  ```
  Exactly 6 args; anything else gives "Invalid category unlock request." (`UH/TeamCategories.cs:51`). Actions: `category-bind|category-inspect|category-apply|category-observe|category-restore`. The launcher uses only bind/apply/observe.
- Output: one JSON line, for example `{"phase":"bound","message":…,"writes":0,"pid":N,"created":"<FILETIME>","basis":"<base>"}` (`:73`). Exceptions become `{"phase":<p>,"message":<m>}` with exit 0. An unknown exception becomes `{"phase":"error","message":"Unlock helper stopped safely. Restart the game before retrying."}` with exit 1 (`UH/FreestyleTeams.cs:180`).
- Process access: `OpenProcess(0x438)` (VM_OPERATION|VM_READ|VM_WRITE|QUERY_INFORMATION) for writes, `0x410` for reads (`UH/TeamCategories.cs:64`). Identity is re-checked before every write with `GetProcessTimes` (exit==0 and creation==bound) (`UH/FreestyleTeams.cs:35`).
- Build selection uses PE `TimeDateStamp` (`pe+8`) and `SizeOfImage` (`pe+80`) (`UH/FreestyleTeams.cs:56-77`):

  | Build | TimeDateStamp | SizeOfImage | Note |
  |---|---|---|---|
  | original | `0x6A974D13` | `0x368C3000` | |
  | "updated" (Sept 11) | `0x6AA2013D` | `0x36558000` | |
  | anything else | | | `unsupported` "This game update is not supported by the Freestyle unlock." |

- Code-byte guards (`Build`, `UH/FreestyleTeams.cs:80-81`), updated-build RVA / expected bytes:
  - `0x20F086B` / `488B0546769507488B80A8000000`
  - `0x35F9020` / `5657534883EC304889CE8B05000CB607`
  - `0x27BC130` / `4157415641554154565755534883EC78`
- Data guards: mask pages at `FsMasks` must equal `{0x400031,0x400002,0x48000000,0x20400030,0x400000}` (`UH/FreestylePolicy.cs:9`; `UH/FreestyleTeams.cs:83`). The previous/next tables must equal the expected cycles (`:85`). Category and mask addresses must be inside the image allocation (`:86`).
- Category tables (`UH/CategoryPolicy.cs:4-12`):

  | | previous table | next table |
  |---|---|---|
  | Original | `{4,0,1,3,2,5}` | `{1,2,4,3,0,5}` |
  | Enabled | `{4,0,1,2,3,5}` | `{1,2,3,4,0,5}` |

  Updated-build RVAs: `FsPrevious=0x7A2FE20`, `FsNext=0x7A2FE40` (`UH/FreestyleTeams.cs:62-63`). Page `0x7A2F000` (4096 bytes).
- Write allowlist, exactly 4 DWORDs, as (RVA, original, enabled), updated build (`UH/CategoryPolicy.cs:11`):
  - `(0x7A2FE2C,3,2)`
  - `(0x7A2FE30,2,3)`
  - `(0x7A2FE48,4,3)`
  - `(0x7A2FE4C,3,4)`

  Old build: `0x7a2bebc/0x7a2bec0/0x7a2bed8/0x7a2bedc`. `WriteCategory` rejects anything outside this list (`UH/TeamCategories.cs:28-32`).
- Write order (`UH/TeamCategories.cs:85-98`):
  1. Save the journal with phase `prepared`.
  2. Run `Build()` and `Snapshot(true)` again; they must equal the earlier snapshot exactly, rows included.
  3. Set the journal to `protecting`, then call `VirtualProtectEx(page, 4096, PAGE_READWRITE)`. The returned previous protection must be 2.
  4. Re-snapshot and confirm the tables are Original.
  5. Set the journal to `writing`.
  6. For each change: read the value (it must still be the original), write the enabled value, verify by readback, and increment `writes`.
  7. In a `finally`, restore protection to PAGE_READONLY (2).
  8. Verify page protection is 2 and the tables equal Enabled. Run `Build(Enabled)` and `Snapshot(true)`; they must be unchanged.
  9. Set the journal to `applied`.
  10. Return `{phase:"applied",writes:4,bytesChanged:16,hooks:0,codeWrites:0}`.
- Failure rollback (`:99-103`):
  - If protection had been touched, run `RestoreCategories`, which writes the changes in reverse back to their originals under a temporary RW protect (`:33-48`). If the restore itself fails, the journal is set to `failed` with "Category restoration could not be verified. Close the game before retrying."
  - If protection was not touched, the journal is set to `restored`.
- Journal: `<userData>\unlocks\team-categories-session.json`, written atomically (`.tmp` + `File.Replace`/`Move`, flushed) (`UH/TeamCategories.cs:21-26`).
  - Schema: `{schema:1,pid,writes,created,basis,kind:"wnba-category",image,phase}` (`UH/CategoryPolicy.cs:22-24`).
  - Valid phases: `prepared|protecting|writing|applied|restored|failed` (`:20`).
  - The file is limited to 8192 bytes. Reparse points (links) anywhere on the path are refused (`UH/FreestyleTeams.cs:108`).
- Snapshot structures (updated build) (`UH/FreestyleTeams.cs:61-75,89-107`):
  - Roster root `[base+0x9A47EB8]`; owner = `[root+0xA8]`; header at `owner+0x80` = `{count u32, table u64}`.
  - Record stride `0x1550`.
  - Team id `u16 @ +0xF20`; kind = `u32 @ +0x123C >> 26`; name UTF-16 at `+0x2FA` (48 bytes) (`UH/FreestylePolicy.cs:17,21`).
- Roster verifier (`UH/FreestylePolicy.cs:13-29`), all of which must hold:
  - WNBA kind-29 ids are exactly {300..311, 315, 316, 317} (15 clubs);
  - national kind-22 ids are {69, 361};
  - kind 0 count == 30;
  - kind 1 > 0 and kind 27 > 0.

  So the unlock only proceeds on a **mixed NBA+WNBA roster that is already loaded**. It never loads or injects rows (`ART/wnba-freestyle-default-roster-audit-20261017.md:14,20`).
- Restore: **the launcher never calls `category-restore`**. `FreestyleUnlocks.stop()` only cancels polling (`freestyle-unlocks.js:49`). The tables live in the process image and disappear when the game exits (`UH/TeamCategories.cs:34-35`, comment). A stale journal from a previous process is ignored because `Same()` compares pid, created, basis and image (`UH/CategoryPolicy.cs:19`).
- Legacy/withdrawn route, still present in the exe and unused by the JS:
  - `bind|inspect|apply|blacktop-check` through `Run()`, with mutex `Local\2kLauncherFreestyle-<sha>`. It writes `FsMask=0x68400033` then `FsCategory=5` and journals in the same team-categories file shape (`FreestyleJournal`) (`UH/FreestyleTeams.cs:122-179`; `UH/FreestylePolicy.cs:8,39-42`).
  - It was withdrawn because "The category-5 combined list must not be used by automatic launches" (`nba2k27-mod-launcher/ABANDONED_FEATURES.md:56-58`).
  - **Note:** `ART/wnba-v2-research-20261017.md:10` describes the WPF Freestyle mechanism as the mask/category-5 write. That describes the withdrawn route. The shipped WPF JS uses `category-*` (`freestyle-unlocks.js:34`).

#### 4. Build profiles
- WPF supported old `0x6A974D13/0x368C3000` and updated `0x6AA2013D/0x36558000` (`UH/FreestyleTeams.cs:59-60`).
- On the old build the stock Freestyle roster was already mixed: 403 rows, 30 NBA + 15 WNBA (`ART/wnba-v2-research-20261017.md:14,116`).
- V2 (`0x6AC53765/0x35BDE000`):
  - Category tables exist (`freestyle_v2.rs` PREVIOUS `0x7A9FB10`, NEXT `0x7A9FB30`, PAGE `0x7A9F000`).
  - The default V2 Freestyle roster (NBA league = `TKU_DELEGATE` → `learn2k_roster.iff`) is **334 rows, 0/15 WNBA**, so the category route alone exposes an empty category (`ART/wnba-freestyle-default-roster-audit-20261017.md:12-19`; `ART/wnba-v2-research-20261017.md:118`).
  - V2 has a stock WNBA league path: front-end byte `0xA313DD9`, toggled by stock action `0x39D0E30`, makes Freestyle use `PlayNow_InitGameMode(7+isWnba)` → `TKU_WNBA_DELEGATE` → `learn2k_wnba_roster.iff` (`ART/wnba-v2-research-20261017.md:83-90`; `SSN/wnba-v2-research.md:10-11`).
  - Whether that roster is mixed is **UNVERIFIED** (live probe `tools\Read-WnbaRouteState.ps1` has not been run; `ART/wnba-v2-research-20261017.md:119,165-176`).
  - Stock V2 selectors are exclusive (NBA-only or WNBA-only); no mixed asset name exists (`ART/wnba-stock-roster-selector-audit-20261017.md:73,79-99`).
  - The 0.41 sibling-thunk writer (TKU +0xD30 → WNBA thunk) crashed and is withdrawn (`ART/wnba-v2-research-20261017.md:20-23`).

#### 5. Rust must match
- [ ] Toggling "Unlock WNBA Teams in Freestyle" only persists `unlockWnbaFreestyle` (boolean); no helper or memory work runs until Play.
- [ ] A fresh settings file defaults `unlockWnbaFreestyle` to `true`.
- [ ] A failed save reverts the checkbox and shows an error toast.
- [ ] With the toggle off, Play emits phase `disabled` "WNBA unlock is off for this launch." and the status line stays blank.
- [ ] Arming happens inside the launch callback, before the game process is started, and the 120 s deadline counts from arm time.
- [ ] A game process whose creation FILETIME predates arm time is rejected ("Unlocks require a new game session started with Play.") and nothing is written.
- [ ] Polls at 500 ms (unbound), 1000 ms (bound), 3000 ms (applied); keeps polling indefinitely after bind while phase is `outside`/`waiting`/`no-game`.
- [ ] `conflict`/`error`/`write-failed`/`unsupported` stops all further attempts for that launch.
- [ ] No write happens unless the loaded roster passes the 15-WNBA/2-national/30-NBA/Classic/All-Time verifier.
- [ ] Exactly 4 DWORDs change; page protection is RO before and after; each write is read back.
- [ ] The journal phase sequence is `prepared`→`protecting`→`writing`→`applied`, written atomically, before each step.
- [ ] Game exit after `applied` emits phase `saved` "Game session ended. Unlocks are saved for your next launch."
- [ ] Never writes the mask=0x68400033/category=5 combined list.
- [ ] Refuses writes when any `EasyAntiCheat*` process exists, when >1 game process is running, or when the image path differs from `<gameRoot>\NBA2K27.exe`.

---

### Blacktop: Unlock WNBA Teams in Blacktop
(The Blacktop overlay/F8 panel UI is covered in `blacktop-overlay.md`. This section covers only the WNBA unlock.)

#### 1. User-visible behavior
- Checkbox `#unlockWnbaBlacktop` with label **"Unlock WNBA Teams in Blacktop"** (`W/Reference/src/renderer/index.html:410`).
- Help text: "Use Y to cycle to WNBA. Custom rosters must include WNBA teams." (`:411`).
- Status node `#blacktopUnlockStatus` (`:412`).
- Initial checked state: `settings.unlockWnbaBlacktop === true` (`app.js:544`). Default `false` (`paths.js:25`).
- On change (`app.js:2742-2747`):
  1. The input is disabled.
  2. `saveSettings({unlockWnbaBlacktop: checked})` is called.
  3. On success the status is cleared.
  4. On error the checkbox reverts to the saved value and **the error text is written into the status node** (not a toast, unlike Freestyle).
- Status comes from push events only (`unlocks:blacktop-status`, `app.js:2749`). There is **no** status invoke for Blacktop: no IPC handler and no preload method (`preload.js:25` only defines `onBlacktopUnlockStatus`).
- Status strings, from the helper:
  - "Waiting for a new launcher-managed game session." (`UH/BlacktopTeams.cs:87`)
  - "WNBA roster loader armed from Home|Quick Play for the next Blacktop entry." (phase `ready`, :111)
  - "N WNBA teams are available through Blacktop's Y category button." (phase `applied`, `UH/BlacktopCategory.cs:66`)
  - "This loaded roster exposes only N usable WNBA team(s); at least two are required. Your roster is left unchanged." (phase `unavailable`, :56)
  - "Blacktop unlock session ended." / "Blacktop unlock stopped." (`ended`, `UH/BlacktopTeams.cs:116,93`)
  - Conflicts: "Blacktop callback is already modified." (:97); "An existing WNBA category test must finish before starting this launcher." (:99)
- Status strings from JS (`blacktop-unlocks.js`):
  - "Blacktop unlock helper finished restoration." (:23,33)
  - "Blacktop unlock restoration is still pending; close NBA 2K27 before retrying." (phase `pending`, :34)
  - "Blacktop unlock restoration could not be confirmed. …" (:23)
  - "Blacktop unlock helper stopped unexpectedly (exit N). … Restart the game before retrying." (:24)
  - "Blacktop unlock helper failed: <err>" (:20)
  - "Blacktop unlock restoration is still pending. Close NBA 2K27 before starting another session." (thrown from `arm`, :9)
- Diagnostics: every status is also recorded to the selection-status log under kind `wnba` (`W/Reference/src/core/selection-status-log.js:14`) and written to `<logs>\blacktop-wnba-status.json` as `{time,version,...state}` (`W/Reference/src/main.js:2028-2033`).
- In game:
  - The user goes to Home or Quick Play, so the loader arms.
  - The user enters Blacktop; the stock Learn2K roster loads.
  - At team selection, pressing **Y** cycles the third (otherwise unused) category, now labelled Women, which lists the WNBA clubs.

#### 2. Timing and gating
- Arming: `await blacktopUnlocks.arm(gameRoot, unlockWnbaBlacktop===true)` runs right after the Freestyle arm, inside `launchGame` (`W/Reference/src/main.js:1801`).
- `arm` first runs `stopOwned()` on any previous child. If the previous child does not exit, it throws (`blacktop-unlocks.js:9`). With the toggle off it returns without spawning (`:10`).
- Spawn: `spawn(helper, ['blacktop-watch', gameRoot, String(fileTime(now))], {windowsHide:true, stdio:['pipe','pipe','pipe']})` (`:11`). The third arg is the earliest acceptable game creation FILETIME.
- Helper wait loop: up to **120 s** for exactly one `NBA2K27` process whose `StartTime` FILETIME is >= earliest, polling every **250 ms** (`UH/BlacktopTeams.cs:87-92`). On timeout, or on stdin EOF, it emits `ended` "Blacktop unlock stopped." (:93).
- Startup checks:
  - image path match (:94);
  - no `EasyAntiCheat*` (:95);
  - `OpenProcess(0x438)` (:96);
  - `BtBuild` (:97);
  - callback slot currently stock (:97);
  - category dispatch table stock (:99).
- Main loop: **every 100 ms** while not stopped and the game has not exited (`:101-114`):
  - Read mode hashes `a` (current) and `b` (alternate) and selection stage `[base+BtRoot+0xEA8]`.
  - **Arm condition** `BtCanArmMenu` (`:69-81`, policy `:10-14`):
    - `a == b`, active == 0, kind == 0;
    - old build: `a == 0x6509837B` (Home) and stage == 0;
    - updated build: (Home and stage 0|1) or (`a == 0x079964C0` Quick Play and stage == 1), plus re-read verification of the inactive byte `[base+0xAACC310]` and the owner kind `[[base+0xAACB9A0]]`.
    - It is evaluated **twice** before arming (:106,109).
  - On arm: `BcRestore()`, then swap the callback to the loader, set `armed=true`, `entryDeadline = now + 60 s`, and emit `ready` (:109-112).
  - **Release**: when armed and (`RosterEntryComplete` [a == b == `0x92DF06DE` and stage == 3] or the 60 s deadline has passed), swap the callback back to the original and clear armed (:108; `UH/BlacktopPresentation.cs:8`; `UH/BlacktopTeams.cs:15`). Stages 1 and 2 are deliberately *not* treated as entry (:103-104).
  - When not armed: `BcTick()` (category bridge). A `waiting` stop inside `BcTick` triggers `BcRestore()` and the loop continues; other stops propagate (:113).
- Re-arming happens on every return to an eligible Home/Quick Play menu (the loop is continuous). There is therefore re-apply on every Blacktop entry within the same game process.
- Category bridge timing (`UH/BlacktopCategory.cs:45-67`):
  - Active only while `BcSelection()` holds: mode `0x92DF06DE` for both, BtRoot vtable match, stage 3 (:17).
  - Leaving selection immediately runs `BcRestore()` (:46).
  - Roster re-validation is throttled to **1 s** while the roster identity is unchanged (:51-52).
  - It waits for labels to initialize (label id nonzero) (:60).
- Stop: `blacktopUnlocks.stop()` ends the child's stdin (EOF). The helper's stdin thread sets `btStop` (`UH/BlacktopTeams.cs:85`) and the loop exits, so restoration runs in `finally` (:117-120).
  - JS waits **1500 ms** (`stopTimeoutMs`) with `forceKill:false` (`blacktop-unlocks.js:6,32`; `child-lifecycle.js:26-37`).
  - If the child has not exited by then, JS publishes `pending` and returns false. **It never kills an armed worker** (comment `blacktop-unlocks.js:27`).
- Stop is also called on: launch failure (`W/Reference/src/main.js:1817,1832`); before-quit (`boundedShutdown('blacktop unlock helper', …, 2500)`, `:2342`); native-host-loss cleanup (`['WNBA',…]`, `:2257`).
- Stdout guard: if more than 16384 buffered bytes arrive without a newline, JS ends stdin (`blacktop-unlocks.js:15`).

#### 3. Mechanism
- CLI: `NBA2K27FreestyleTeams.exe blacktop-watch <gameRoot> <earliestFileTime>`. Exactly 3 args; anything else gives "Invalid Blacktop session request." (`UH/BlacktopTeams.cs:83`). Output is NDJSON lines `{phase,message}`, flushed per line (:32). The final return value is also printed by `Main`.
- **Roster-loader callback swap** (`UH/BlacktopTeams.cs:22-45`). Updated-build RVAs:

  | Name | RVA |
  |---|---|
  | `BtSlot` | `0x7461D38` |
  | `BtOriginal` | `0x2342E00` (a `ret`) |
  | `BtLoader` | `0x2344400` (Learn2K loader thunk) |
  | `BtRoot` | `0xAEB69D0` |
  | `BtVtable` | `0x78AEA48` |
  | `BtDelegate` | `0x9AAB730` |
  | `BtDelegateVtable` | `0x7461028` |

  Old build: `0x745e1d8 / 0x2340ca0 / 0x23422a0 / …` (:22-23).
  - The slot is `BLACKTOP_DELEGATE` vtable +0xD10 (`ART/wnba-v2-research-20261017.md:11,44`).
  - The write is an 8-byte absolute pointer (`base+BtLoader` ↔ `base+BtOriginal`). It is allowlisted (:34-37).
  - Protect sequence: verify the current value equals `from`; the region must be image-backed and PAGE_READONLY; `VirtualProtectEx(8 bytes, RW)`; write and read back; restore the old protection in `finally` (:38-45).
- `BtBuild` validation (:46-68):
  - PE stamp and size match.
  - The on-disk exe header equals the loaded one (or the identity fields match).
  - Native spans are compared byte-for-byte with the on-disk exe through the section table. Updated build (RVA, length): `BtOriginal`/16, `BtLoader`/26, `0x32C51D0`/0xCB, `0x22E09F0`/0xFC, `0x27BC3D0`/0x80.
  - `BtOriginal` first byte is `0xC3`, and `[base+BtDelegate] == base+BtDelegateVtable`.
- **Y-category bridge** (`UH/BlacktopCategory.cs`). Updated build:

  | Name | Value |
  |---|---|
  | `BcTable` | `0x73BA190` (6 × rel32 jump table) |
  | `BcLabel` | `0xAC6C6B0` |
  | `BcContinue` | `0x22E0A4B` |
  | `BcEuro` (label key) | `0x6FC058B0442652CD` |
  | `BcWomen` (label key) | `0x0F39A44C3426C962` |

  Sources: `UH/BlacktopTeams.cs:27-29`; `UH/BlacktopCategory.cs:10-11`.
  - Bridge (once per process): `VirtualAllocEx` 4096 bytes, MEM_COMMIT|RESERVE, RW. It tries addresses from `base+0x40000000` to `base+0x50000000` in 64 KB steps (:37).
  - Bridge code is `BE 00 00 00 20` (`mov esi,0x20000000`) followed by `E9 rel32` → `BcContinue` (:40). The memory is then set to PAGE_EXECUTE_READ (0x20) and `FlushInstructionCache` is called (:41).
  - The bridge is **never freed** ("Never free code that another game thread could still be finishing", :43).
  - Patch: table entry 3 (`BcTable+12`, 4 bytes) is set to `rel32(bridge - (base+BcTable))`, with a 4-byte RW protect, readback, and the old protection restored (:19-25,42).
  - Label: verify `[BcLabel] == 0xBA3C982D`, `[+4] == 0`, `[+8] == Euro`; write `+8 = Women` (:26-30). The label area must be writable (Area check `true`).
  - Order on apply: label first, then dispatch (:63-64). Restore (`BcRestore`): dispatch back to the original table, then (in `finally`) the label back to Euro (:31-34).
  - Roster gate: count the kind-29 rows in the loaded table. Proceed only if 2..32 distinct ids, each in 0..1023 (:15,55-56). Otherwise the phase is `unavailable` and nothing is written.
- Final cleanup in `finally` (`UH/BlacktopTeams.cs:117-120`): if the game is still alive, run `BcRestore()`; then, if armed and the slot still holds the loader, swap it back. The handle is then closed. If the game has already exited, nothing is needed because everything is in-process memory. The RX bridge leaks by design.
- No journal file for Blacktop. State is held only in the helper process.
- `blacktop-check` (`UH/FreestyleTeams.cs:139-143`) is a read-only verify action. **Not used by the WPF JS** (grep finds no caller).

#### 4. Build profiles
- WPF supported old `0x6A974D13/0x368C3000` and updated `0x6AA2013D/0x36558000` (`UH/BlacktopTeams.cs:48`). The updated-build arm boundary adds the Quick Play/stage-1 case (:8-14).
- V2 equivalents (`ART/wnba-v2-research-20261017.md:16,36-51,96-110,146-163`):
  - Slot = `BLACKTOP_DELEGATE` vtable **+0xD30** = `0x74CDE88`, stock `ret` `0x236E510`, general thunk `0x236FB30`. V2 +0xD10 is an unrelated `xor eax,eax; ret` getter.
  - Category switch `0x230BDD0`, table `0x7425F4C`, case-3 `0x230BEA4`, continuation `0x230BE2B`, label `0xACFC4D0`. The case layout is identical to WPF's.
- **Caveat:** on V2 the general Learn2K roster is very probably NBA-only (334 rows, 0 WNBA), so a literal port loads no WNBA teams. The WNBA thunk `0x236FF70` is acceptable only if `learn2k_wnba_roster.iff` is proven mixed. This is **UNVERIFIED** (`ART/wnba-v2-research-20261017.md:153-155`).
- Fallback: a user-loaded custom roster containing WNBA would need only the category bridge, which is also **UNVERIFIED** (`:163`).

#### 5. Rust must match
- [ ] Toggle persists `unlockWnbaBlacktop` (default `false`); a save error is shown inline in `#blacktopUnlockStatus` and the checkbox reverts.
- [ ] With the toggle off, Play spawns no Blacktop helper.
- [ ] The worker ignores any game process created before arm time and gives up after 120 s with `ended`.
- [ ] The callback is armed only at inactive Home (stage 0, or 0|1 on the updated build) or Quick Play (stage 1), never during stages 1/2 of Blacktop entry.
- [ ] The callback is restored at selection stage 3 or 60 s after arming, whichever comes first.
- [ ] Re-arms on every later return to Home/Quick Play within the same process.
- [ ] Bridge, dispatch patch and label swap are applied only while at selection stage 3, and are restored as soon as selection is left.
- [ ] If the roster has fewer than 2 distinct WNBA clubs, emit `unavailable` with the count and write nothing.
- [ ] Every pointer/rel32/label write is verified by readback, and page protection is restored in a `finally`.
- [ ] Shutdown sends a graceful stop (EOF/cancel), waits about 1.5 s, and **never force-kills**. If the worker is still alive, show `pending` "Blacktop unlock restoration is still pending; close NBA 2K27 before retrying."
- [ ] Starting a new Play while a previous Blacktop worker is still restoring fails with "Blacktop unlock restoration is still pending. Close NBA 2K27 before starting another session."
- [ ] Writes `blacktop-wnba-status.json` in the logs dir with every status.

---

### Debug Menus: Enable Debug Menus (Experimental)

#### 1. User-visible behavior
- The group is headed "Experimental", subtitle "Advanced controls. Use with care.", badge "Resets on app launch" (`index.html:417`).
- Checkbox `#enableDebugMenus` with label **"Enable Debug Menus"** (`:420`).
- Help text: "Opens built-in debug and restricted menus when starting the game with Play." (`:421`).
- Note: "**Steam Offline Mode required** May cause instability or affect saves. Use a backup or test roster. Always off when the launcher opens; this choice is never saved." (`:422`).
- Status `#debugMenuStatus`, initial text "Debug menus are off." (`:423`).
- Turning it on (`W/Reference/src/main.js:113-121`) shows a modal warning dialog:
  - title "Experimental debug menus";
  - message "Steam must be in Offline Mode.";
  - detail "Experimental: exposes built-in debug menus and restricted options. These options may cause instability or affect saves. Use a backup/test roster. Only use the launcher’s offline/non-EAC Play flow. This toggle resets to off whenever the launcher opens.";
  - buttons `['Cancel','Steam is offline — enable']`, default and cancel = 0.

  Anything other than button 1 returns the unchanged view, and the renderer re-renders the box unchecked (`W/Reference/src/renderer/debug-menus.js:6,11`).
- After confirming, the status reads "Experimental — enabled for the next Play launch. Steam must be in Offline Mode." (phase `armed`, `W/Reference/src/core/debug-menus.js:28`).
- Other statuses:
  - "Waiting for a new offline Play session." (:34)
  - "Experimental debug menus enabled. Reopen Pause if needed." (`UH/DebugMenus.cs:102`)
  - "Debug menus disabled. Reopen Pause to refresh." (:102)
  - "Game ended; memory-only debug changes are gone." (`debug-menus.js:54`)
  - "Debug features did not initialize in time. No further debug writes will be attempted." (`timeout`, :41)
  - "Debug menus require offline/non-EAC Play. No debug writes were authorized." (`UH/DebugMenus.cs:34`)
  - "Debug menus do not support this game update." (:32)
  - "Re-enabling debug menus in the owned Play session." (`debug-menus.js:26`)
- The renderer hides only the generic "Unlock helper stopped safely…" message (`renderer/debug-menus.js:6`). Checkbox state follows `view.enabled`.

#### 2. Timing and gating
- Volatile: `this.enabled=false` at construction. The comment says "no settings file can enable this at application startup" (`debug-menus.js:8,19`). There is no settings key.
- `setEnabled(true)` before Play only sets the flag (phase `armed`). On Play, `await debugMenus.arm(gameRoot)` runs after the Blacktop arm (`W/Reference/src/main.js:1802`).
- `arm` sets **deadline = now + 120000 ms**. The deadline applies to everything until `applied` is reached (`!this.applied && now > deadline`), **including after bind**, unlike Freestyle (`debug-menus.js:33,41`).
- Poll: **500 ms** until applied, **3000 ms** after applied; 0 ms after `bound` (`:36,56`).
- Action per tick: unbound gives `debug-bind`; bound and not applied gives `debug-apply`; applied gives `debug-observe` (`:43`).
- Bind validation is the same as Freestyle (new process since arm, `basis >= 65536`) (`:46`).
- `blocked=true` on `conflict|write-failed|error|unsupported`. Once blocked, `stop()` will not attempt a restore (`:52,62`).
- **Live toggle-off during a session**: `setEnabled(false)` → `stop()` → cancel polling, then `debug-restore` writes the original snapshot values back (`:23,59-69`).
- **Live toggle-on again** after `restored` in the same owned session restarts polling with a fresh 120 s deadline (`:24-27`).
- Stopped on launch failure (`main.js:1818,1833`); on before-quit (`boundedShutdown('debug menu helper',…,9000)`, `:2343`); and in host-loss cleanup (`:2258`).

#### 3. Mechanism
- CLI:
  ```
  NBA2K27FreestyleTeams.exe debug-bind|debug-apply|debug-observe|debug-restore <gameRoot> <userData>\unlocks\debug-menu-session.json <pid> <created> <basis>
  ```
  (`debug-menus.js:11`; `UH/DebugMenus.cs:58`). `execFile` timeout 8000 ms.
- Global mutex `Local\2kLauncherDebugMenus` (not per-file) (`UH/DebugMenus.cs:61`).
- Build: **updated build only**, `0x6AA2013D/0x36558000` (`:32`).
- EAC check also covers `start_protected_game` (`:34`). The check is repeated before every write (`:87,91,95`).
- Structure (`:36-42`):
  - manager = `[base+0xA167348]`; table = `[manager+0xF8]`; count = `[manager+0x110]` (1..100000).
  - The entries are a sorted pointer array, searched by binary search on the u32 key at `*entry` (duplicate neighbours rejected) (`:12-20`).
- Keys `0x8EB3D32B`, `0x976369F2`, `0x5B4DD268` (`:9`). Each entry must have packed `(u32@+4 & 31) == 11` and `((>>5) & 0x3FFFFFF) == 1`, and a value byte `@+8` that is <= 1 (`:11`).
- Write: one byte, `1`, at `entry+8` for each key that is not already 1, re-validating the whole snapshot before each byte and reading it back (`:93-99`). Restore writes back the journaled original values (`:94`).
- Journal `debug-menu-session.json`:
  - Fields: `{schema:1,pid,created,basis,image,phase,timestamp:0x6AA2013D,size:0x36558000,snapshot:{manager,table,count,entries[3],values[3]}}` (`:28,44-55`).
  - Phases: `prepared|restoring|applied|restored|failed|ended`.
  - A journal for the same session in any other phase gives `conflict` "An earlier debug write was interrupted…" (`:76`).
  - With no game running, the journal is marked `ended` (`:66`).

#### 4. Build profiles
- WPF: updated build `0x6AA2013D` only. V2: **unknown** (not found in artifacts).

#### 5. Rust must match
- [ ] The debug toggle is never persisted and is always unchecked after launcher start.
- [ ] Enabling it shows the confirm dialog with the exact strings above; Cancel leaves it off.
- [ ] Enabling before Play writes nothing; status shows the `armed` text.
- [ ] Writes happen only to the 3 allowlisted feature bytes, only with no EAC/`start_protected_game` process.
- [ ] Gives up after 120 s if not applied, even when bound.
- [ ] Turning it off while the game runs restores the original bytes ("Debug menus disabled. Reopen Pause to refresh.").
- [ ] After any conflict/error, no restore write is attempted.

---

### Retired front-menu recovery (context only)
- This is **not an unlock.** It is cleanup for a retired "What's New"/main-menu replacement feature. It can delete only two known files whose hashes match, and only when the journal `<userData>\front-menu\whats-new-session.json` exists (`W/Reference/src/core/retired-front-menu-recovery.js:4-7,11-25`).
- Known files:
  - `gooey_scenes/mainmenu_2k27_whatsnew_description.iff` (sha256 `9f45…e5ea`)
  - `gooey/nba_2k27_mainmenu.iff` (sha256 `916a…0862`)
- It refuses to run while the game is running (`:15`), refuses symlinked paths (`:9`), and leaves externally changed files untouched (`:21`).
- When it runs: at startup (`W/Reference/src/main.js:1942-1943`); before the version.dll session starts on Play (`:1782`); on launch failure (`:1843`); in native cleanup steps (`:2081,2250`).
- `W/Reference/src/data/front-menu/` is an **empty directory**. `nba2k27-mod-launcher/front-menu-helper/` is also an **empty directory** (no sources).
- Retired settings keys deleted on load include `replaceFrontMenuPlayers`, `frontMenuPlayerMappings`, `frontMenuPlayerConfigs`, `frontMenuActiveConfig`, `hideWhatsNewPanel` and `unlockGameRules` (`W/Reference/src/core/paths.js:38`).

### Exit and host-loss handling (shared)
- `createExitHandler` (`W/Reference/src/core/exit-handler.js:5-34`):
  1. Prevents the default quit and shares one pending promise across clicks.
  2. `canQuit` refuses while the game runs ("Close NBA 2K27 first.", `main.js:2308-2318`).
  3. `prepare` runs with a 15000 ms race. On timeout it forces the quit, except under the WPF native host (`forceOnTimeout: !WPF_NATIVE_HOST`), where it keeps waiting and only logs (`main.js:2297-2299`).
- In `prepare`, the Freestyle manager is stopped synchronously (polling only). Blacktop and Debug are stopped with bounded timeouts of 2500 ms and 9000 ms (`main.js:2332,2342-2343`). Under the WPF native host, `cleanupNativeSession()` runs instead, with `assertStopped` before each stop (`main.js:2238-2271`).
- `createHostLossCleanup` loops every **1000 ms** until `ready()`: startup settled, not launching, and the game is not running (fresh check). Only then does it run cleanup; any failure is retried. Status is logged to `native-host-recovery.jsonl` (`host-loss-cleanup.js:4-16`; `main.js:2272-2281`). On host loss, Freestyle is stopped immediately (`main.js:2287`).

## Known divergences in current Rust port (read-only check)
- Settings keys: Rust `Settings` has `unlock_wnba` and `unlock_wnba_freestyle` (camelCase `unlockWnba`, `unlockWnbaFreestyle`), both defaulting to **false** (`nba2k27-launcher-tauri2/src/backend.rs:53-54,93-94`). WPF uses `unlockWnbaBlacktop` (default false) and `unlockWnbaFreestyle` (default **true**). `settings_compat.rs:198-199` lists the WPF keys for import; whether they map onto the Rust fields was **UNVERIFIED**.
- UI: `frontend/UnlocksPanel.tsx` uses labels "WNBA teams in Blacktop", "WNBA teams in Freestyle" and "Enable debug menus", shows the Experimental group as "Unavailable", and says "Unavailable options stay disabled and never attempt a native write." (`:58,65,77-82,99`). WPF labels are "Unlock WNBA Teams in …" and "Enable Debug Menus".
- `blacktop_unlock_v2.rs` has `RUNTIME_MAPPED=false`, and its `CALLBACK_SLOT_CANDIDATE 0x74CDE68` / `STOCK_CALLBACK_CANDIDATE 0x23531A0` are the wrong slot. They should be `0x74CDE88` / `0x236E510` (`src/blacktop_unlock_v2.rs:11,16-17`; `ART/wnba-v2-research-20261017.md:181-185`; `SSN/wnba-v2-research.md:7-8`).
- `wnba_loader_v2.rs` has `RUNTIME_ENABLED=false` (the 0.41 sibling-thunk route is withdrawn) (`src/wnba_loader_v2.rs:18`).
- No Rust debug-menu writer was found (grep `debug_menu` in `src` shows nothing beyond UI/settings).

## 2026-10-18 QA077 comparison update

The user reports both WNBA modes worked in retained WPF. That is the behavior target. The older Rust state/offset statements above describe the 2026-10-17 snapshot; consult the newest QA handoff for current results.

Blacktop WPF throws `waiting` on roster movement during preparation (`UH/BlacktopCategory.cs:57`) and continues its 100 ms worker (`UH/BlacktopTeams.cs:113-114`). QA077 instead permanently holds when its pre-initialize checkpoint changes (`native/wnba_blacktop_category_core.hpp:212`, `native/wnba_blacktop_category_win.hpp:627`, within this Rust project). Retained uninitialized memory requires explicit state and original durable ownership proof before waiting can safely resume. See `ART/qa077-wpf-blacktop-waiting-comparison-20261018.md`.

Freestyle WPF keeps pre-effect readiness failures waiting, but touched/protection/write failures restore then stop (`UH/TeamCategories.cs:85-102`). Rust can spend its eight-attempt restored budget even with zero applied category words. Zero applied words alone does not prove no attempted protection/write or uncertain durable intent: track transaction stage explicitly. WPF omits selected/category/mask fields from its ending individual deep-snapshot check, but still checks them across transaction snapshots (`UH/FreestyleTeams.cs:26-27,105`, `UH/TeamCategories.cs:88,92,97`). Do not drop those proofs globally. See `ART/qa078-wpf-freestyle-entry-retry-field-comparison-20261018.md` for full source citations.

QA077 user run had 403 teams/30 NBA/15 WNBA clubs but no categories in either mode. Blacktop changed kind/game mode/active (CTX224) before initialization intent; Freestyle's changed snapshot field remains unknown. Neither justifies blanket context normalization. QA078 controller, self-owned Windows witness and transaction models are research only; no QA078 production build exists.

## Open questions / UNVERIFIED
- Whether the V2 `learn2k_wnba_roster.iff` (stock WNBA league Freestyle) is mixed NBA+WNBA. This decides both the V2 Freestyle category route and the Blacktop thunk choice (`ART/wnba-v2-research-20261017.md:172`). **UNVERIFIED**; needs the live read-only probe.
- Whether the V2 WNBA league front-end disables the Freestyle tile (`0x32F5EC0`/`0x32F6DD0`) (`ART/wnba-v2-research-20261017.md:94,144`). **UNVERIFIED**.
- The V2 debug-menu manager RVA and keys. Not found in WPF sources or artifacts.
- WPF Freestyle never restores the category tables during a session (no `category-restore` caller). That is fine for WPF, because the change is static per-process data, but a Rust port that adds a mid-session restore would be new behavior.
- Old-build WPF Freestyle live success was observed on a mixed (403-row) roster. Updated-build live success of the `category-*` route is stated only indirectly (`ART/wnba-freestyle-default-roster-audit-20261017.md:20`). No dedicated live log was located. **UNVERIFIED**.


## 2026-10-18 QA078 implementation update

Dev source now matches WPF's preparation-wait distinction more closely. Freestyle takes two complete deep snapshots before planning, fresh complete-context/profile/source/table-prefix checks before intent and every DWORD, then a complete deep snapshot and compact reproof after DWORD4 before Applied. Every ContextSnapshot field remains exact. A typed valid readiness difference before publication intent waits without journal growth. Once intent is attempted, any error conditionally restores only its exact owned prefix and permanently holds, as WPF stops after touched publication. See `src/feature_runtime.rs::publish_freestyle_or_restore`, `freestyle_tick`, and `src/freestyle_v2.rs::verify_compact` / `verify_deep_equal`.

This intentionally changes the old eight-deep-scan timing: an in-place row mutation during the four writes may be detected at the final deep proof, rather than between words. It retains WPF's three-deep temporal shape and adds fresh compact guards; it is not an equivalence claim for mutable-row detection timing. Foreign or partial DWORDs are never overwritten during recovery.

Blacktop now has an explicit retained-uninitialized preparation state, bounded to12 attempts/10 seconds from one original monotonic origin. Only typed completed context-change observations with exact retained generation can defer; no context value is normalized. Same original intent file identity/bytes/ordinal, pinned directories and twice-zero owned8192RW allocation are required before strict resume. Initialize-intent attempt is irreversible. Source/consent/identity loss, uncertain reads or attempted effects hold. Both core and Host enforce the deadline; post-seal-intent guards reprove exact code/chain/function/zero-padding, page phase, authority and selection before protection/flush/unwind effects. Active-route restore and process-lifetime allocation retention remain. See `native/wnba_blacktop_category_core.hpp` and `native/wnba_blacktop_category_win.hpp`.

Roster reading now groups the same validated403 Team id/kind/name fields through34 bounded64KiB-or-smaller table reads instead of1209 small reads, preserving all ending context/owner/table/region checks. Native retained rows contain those explicit fields; this is distinct from Freestyle's full raw-row byte equality. No global read bound was raised.

Validation/provenance: `artifacts/qa078-production-scope-and-native-validation-20261018.json`, `qa078-freestyle-independent-review.json`, `qa078-category-core-production-integration-final.json`, `qa078-host-seal-independent-review.json`. Final handoff/stage records determine the exact testable build. Live availability in both modes and persistence across Home/re-entry still require the next normal user Play session. No encrypted roster-index research asset was integrated.
