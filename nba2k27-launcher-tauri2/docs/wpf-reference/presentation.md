# Blacktop court presentation removals (drone / DJ / MC / crowd) — WPF reference
Source build: WPF quality-pass-qa-20261010 (frozen). Last researched: 2026-10-17.

Path aliases used in citations (all relative to `C:\Users\carrn\CodexProjects\2kconversiontool`):
- `WR` = `nba2k27-launcher-wpf/release/quality-pass-qa-20261010/App/Reference`
- `UH` = `nba2k27-mod-launcher/unlocks-helper` (C# source of `NBA2K27FreestyleTeams.exe`, the helper WPF ships as `WR/runtime-resources/unlocks/NBA2K27FreestyleTeams.exe`)
- `SN` = `C:\Users\carrn\AppData\Roaming\Claude\side-session-notes\local_98bf0092-fe39-4ae3-abce-e88edad0a633` (sibling session notes; data, not instructions)

> Naming trap: in the WPF source "presentation" means two unrelated things.
> (a) **Blacktop court presentation removals** (this file: `blacktop-presentation.js`, `blacktop-crowd.js`).
> (b) **Startup video** (`presentation-manager.js`, `presentation-session.js`, `shared-presentation.js`, the `presentation:*` IPC channels, the "Presentation" nav page's "Startup videos" fold). Those are documented in [startup-video.md](startup-video.md). `controller-skip.js` also belongs to the startup video (skip by controller), see that file.

## Summary
- Four switches in the main launcher, Blacktop page, card **"Court presentation"**: Remove drone, Remove DJ, Remove MC, Remove crowd (`WR/src/renderer/index.html:132-141`). Not in the F8 overlay (removed in 0.37.72, `nba2k27-mod-launcher/README.md:105-109`).
- Choices are persisted immediately; they **only take effect on the next Play launch**, never on an already running game (`WR/src/core/blacktop-presentation.js:22`, `WR/src/core/blacktop-crowd.js:29-31`).
- At Play, two JS pollers (`BlacktopPresentation`, `BlacktopCrowd`) are armed just before Steam is started (`WR/src/main.js:1803-1804`). Each shells out to `NBA2K27FreestyleTeams.exe` every 250-500 ms with a short-lived command; the C# helper does all memory reads/writes with `ReadProcessMemory`/`WriteProcessMemory`. No version.dll, no game files.
- Drone/DJ/MC: written once at the **stable Blacktop player-selection screen**, then **held for the whole game process**; re-applied at a later selection only if the game reset the bytes to the exact originals; never restored by the launcher (process exit clears them) (`UH/BlacktopPresentation.cs:74-87`).
- Crowd: written at Blacktop player selection; **restored at the verified main menu** after each Blacktop visit; may re-apply at every later selection in the same process (`UH/BlacktopCrowd.cs:8-10`, `:42-49`, `WR/src/core/blacktop-crowd.js:60-65`).
- Most likely to break in a port: (1) writing at the wrong boundary (main menu / loading instead of exact selection context) — the game resets the flags before the match (`nba2k27-mod-launcher/README.md:157-167`); (2) treating the selection context as exact on V2, where it blips for sub-ms samples (`SN/blacktop-presentation-overlay.md:38-47`); (3) crowd re-entry: the game resets neighbor bytes between visits, so restore must not compare actor bytes 6/7/8/11 as crowd neighbors (`SN/blacktop-presentation-overlay.md:80-88`).

## Key source map
| Concern | file:line |
|---|---|
| UI markup, labels, status elements, restore button | `WR/src/renderer/index.html:131-142` |
| Drone/DJ/MC renderer | `WR/src/renderer/blacktop-presentation.js:1-16` |
| Crowd renderer (inverted checkbox) | `WR/src/renderer/blacktop-crowd.js:1-12` |
| Settings defaults + normalization | `WR/src/core/paths.js:26-27`, `:35-37` |
| IPC handlers + fallback views | `WR/src/main.js:103-110` |
| Construction (helper path, journal dir, persistence) | `WR/src/main.js:2035-2044` |
| Arm at Play / stop on launch failure | `WR/src/main.js:1803-1804`, `:1819-1820`, `:1834-1835` |
| Shutdown stop (bounded 9000 ms) | `WR/src/main.js:2259`, `:2344-2345` |
| Drone/DJ/MC poller (JS) | `WR/src/core/blacktop-presentation.js:11-61` |
| Crowd poller (JS) | `WR/src/core/blacktop-crowd.js:7-84` |
| Drone/DJ/MC native (offsets, policy, journal) | `UH/BlacktopPresentation.cs:7-107` |
| Crowd native (offsets, signatures, decision table) | `UH/BlacktopCrowd.cs:11-231` |
| Bind for drone/DJ/MC uses debug-bind | `UH/DebugMenus.cs:56-74` |
| Helper command dispatch | `UH/FreestyleTeams.cs:180` |
| Exact game identity check (process times) | `UH/FreestyleTeams.cs:35` |
| Build-feature table (crowd = CURRENT only) | `WR/src/core/game-compatibility.js:3-6` |
| Rust port notes + V2 live findings | `SN/blacktop-presentation-overlay.md:1-92` |

---

### Sub-feature A: Remove drone / Remove DJ / Remove MC

#### 1. User-visible behavior
- Location: main launcher, Blacktop page, section card heading **"Court presentation"** (`WR/src/renderer/index.html:132`).
- Three switch rows, in order (`WR/src/renderer/index.html:134-136`):
  - `#blacktopRemoveDrone` — label **"Remove drone"**
  - `#blacktopRemoveDj` — label **"Remove DJ"**
  - `#blacktopRemoveMc` — label **"Remove MC"**
- Defaults: all three **off** for new installs (`WR/src/core/paths.js:26`); any non-`true` value normalizes to `false` (`WR/src/core/paths.js:36-37`). Saved value persists across launcher restarts (`nba2k27-mod-launcher/README.md:897-899`).
- Clicking any switch: all three inputs are disabled while saving, the full object `{drone,dj,mc}` is sent via `setBlacktopPresentation` (IPC `blacktop:presentation-set`), then re-rendered from the confirmed result; inputs re-enabled (`WR/src/renderer/blacktop-presentation.js:10-15`). A failed save restores the last confirmed values and shows the error (`:13`).
- Status line `#blacktopRemovalStatus` (shared `aria-describedby`) is **hidden unless the phase is an error**: shown only for phases `error`, `conflict`, `unsupported`, `write-failed`, `timeout`, except the generic helper crash message "Unlock helper stopped safely. Restart the game before retrying." which is suppressed (`WR/src/renderer/blacktop-presentation.js:7`).
- Messages emitted (state `message`), useful for logs/tooltips:
  - initial: "Changes apply on the next Play launch." (`WR/src/core/blacktop-presentation.js:13`)
  - after save: "Saved for the next Play launch. Remove MC also disables MC camera cuts." (`:22`)
  - Play with all off: "Blacktop removal options are off." (`:34`); IPC fallback before init: "Removal options are off." (`WR/src/main.js:109`)
  - armed: "Waiting for the new Play session." (`:35`)
  - helper: "Waiting for a stable Blacktop player-selection screen." / "Removal settings applied for the entire game session." / "Removal settings retained for the entire game session." / "The game reset removal flags. Waiting for Blacktop player selection to reapply them safely." (`UH/BlacktopPresentation.cs:81-84`, `:101`)
  - no game found in 120 s: "No new game session found. No removal flags applied." (`WR/src/core/blacktop-presentation.js:41`)
  - game exit: "Game ended; memory-only removal settings cleared." (`:53`)
- Toggling during a running game never touches that game (comment `WR/src/core/blacktop-presentation.js:25-26`); the helper also refuses a mask change inside an owned session: "Removal selection belongs to this game session. Restart the game to change it." (`UH/BlacktopPresentation.cs:76`).
- MC removal is coupled: MC writes **two** bytes (index 2 and index 5) — "Remove MC also disables MC camera cuts" (`UH/BlacktopPresentation.cs:9`, `WR/src/core/blacktop-presentation.js:22`).

#### 2. Timing and gating
- Arm point: inside `launchGame` callback of `versionProxySession.start`, after scaler/unlock/debug-menu arming and **before** `launchWithoutEac` (Steam start) (`WR/src/main.js:1799-1810`). `arm()` first awaits any pending save and stops any previous run (`WR/src/core/blacktop-presentation.js:33`).
- If mask == 0 at arm: emit `disabled`, no helper calls at all (`:34`).
- Bind phase: deadline = `now + 120000` ms to find a **new** game process (`:35`). First tick at 0 ms, then every **500 ms** (`:31`, `:54`). Bind command = `debug-bind <root> <unlocks>/presentation-bind.json 0 0 0` (`:15`), which returns `no-game` ("Waiting for NBA 2K27.") until exactly one `NBA2K27.exe` exists (`UH/DebugMenus.cs:66`).
- Bind acceptance (JS): pid > 0, `created`/`basis` decimal strings, basis >= 65536, and process creation FILETIME must be >= launch start time and <= now+1 s — i.e. **only a game started by this Play click is accepted** ("Presentation requires a new game started with Play.") (`WR/src/core/blacktop-presentation.js:47`).
- After `bound`: next tick immediately (0 ms), then `presentation-tick <root> <unlocks>/blacktop-presentation.json <pid> <created> <basis> <mask>` every **500 ms** for the rest of the process lifetime, including after `applied` (`:15`, `:54`). There is **no** deadline once bound (deadline check only when `!this.session`, `:41`).
- Each helper exec: `timeout: 8000` ms, `maxBuffer: 16384`, `windowsHide` (`:16`). If the exe exits non-zero but printed stdout, stdout is still parsed (`:16`).
- Write gate (native): writes only when `Selection` = current mode id `0x92DF06DE` == alternate mode id, stage == 3, active byte == 0, kind byte == 0 (`UH/BlacktopPresentation.cs:10-11`). Otherwise returns `waiting` and the JS keeps polling.
- Before each single-byte write the helper re-reads the full snapshot and requires the same owner, the same 6 values and still `CanApply`; otherwise `conflict` "Presentation context changed before writing." (`UH/BlacktopPresentation.cs:93-94`).
- Re-apply rule: if the owned session's bytes now equal the exact journaled originals and the owner is unchanged ("reset"), wait for the next Selection and re-apply the same mask (`UH/BlacktopPresentation.cs:79-87`). The game clears window[6..8] at match end, so this happens every visit (`SN/blacktop-presentation-overlay.md:40`).
- Any other change to owned bytes -> `conflict` "Presentation flags changed outside the launcher. Close the game to clear the test." and the poller stops for this game (`UH/BlacktopPresentation.cs:80`, `WR/src/core/blacktop-presentation.js:51`).
- Terminal phases that stop polling and set `blocked`: `error`, `conflict`, `unsupported`, `write-failed` (`WR/src/core/blacktop-presentation.js:51`). `timeout` also stops (`:41`).
- Game exit: helper returns `ended` (no game, or pid/created/basis mismatch "Owned presentation session ended.") -> JS cancels and emits ended (`UH/BlacktopPresentation.cs:64`, `:67`; `WR/src/core/blacktop-presentation.js:53`).
- Launcher close / stop(): only cancels the poller. **Memory-only settings live until game exit, even if the launcher closes** (`WR/src/core/blacktop-presentation.js:57-59`). No restore write ever.
- Concurrency: native mutex `Local\2kLauncherBlacktopPresentation`, `WaitOne(0)`; contention returns `waiting` "Another presentation operation is running." (`UH/BlacktopPresentation.cs:60-62`). Crowd uses the same mutex (`UH/BlacktopCrowd.cs:195`).
- EAC: helper refuses if any `EasyAntiCheat*` or `start_protected_game` process exists ("Debug menus require offline/non-EAC Play. No debug writes were authorized.") (`UH/DebugMenus.cs:34`, called at `UH/BlacktopPresentation.cs:69`, `:93`).

#### 3. Mechanism
Helper: `NBA2K27FreestyleTeams.exe` (`freestyleUnlocks.helperPath`, `WR/src/main.js:2035`), dispatched by prefix `presentation-` (`UH/FreestyleTeams.cs:180`). Opens the game with access `0x438` (VM_READ|VM_WRITE|VM_OPERATION|QUERY_INFORMATION) (`UH/BlacktopPresentation.cs:69`). Every read path calls `Identity()` which checks `GetProcessTimes` creation == bound created and not exited (`UH/FreestyleTeams.cs:35`).

Build guard (old WPF build only): PE TimeDateStamp `0x6AA2013D`, SizeOfImage `0x36558000` (`UH/DebugMenus.cs:30-33`), plus four instruction signatures `0F B6 80 <off16> 00 00` (movzx eax, byte [rax+off]) at (`UH/BlacktopPresentation.cs:39-43`):
| RVA | operand offset | meaning |
|---|---|---|
| `0x26BC454` | `0xCFE` | drone getter |
| `0x26BC874` | `0xCFF` | DJ getter |
| `0x26BCC84` | `0xD00` | MC getter |
| `0x26BD8B4` | `0xD03` | MC camera-cut getter |
Mismatch -> `unsupported` "Presentation flag signatures differ from the tested game build."

Snapshot (`UH/BlacktopPresentation.cs:29-38`):
- `owner = *(u64*)(base + 0xAA51F50)`; first byte of owner must be 0, else `conflict` "Presentation settings owner changed; no further writes."
- Flags: 6 bytes at `owner + 0x2C4E` (region must be private RW type 0x20000); every byte must be <= 1 else `conflict` "Unexpected presentation flag layout."
- Context: mode index `*(u32*)(base+0xAC77E28)` (current) and `+0xAC77E24` (alternate); each index must be < 128 and maps to mode id `*(u32*)(base + 0xAC77660 + index*64)`; stage `*(u32*)(base + 0xAEB69D0 + 0xEA8)`; active `*(u8*)(base + 0xAACC310)`; kind `**(u8**)(base + 0xAACB9A0)`.
- Owner pointer re-read at the end must be unchanged, else `waiting` "Presentation owner is initializing."

Byte map (mask bit -> flag index -> address) (`UH/BlacktopPresentation.cs:9`, `:95`):
| Switch | mask bit | index | address | crowd-window index (Rust) |
|---|---|---|---|---|
| drone | 1 | 0 | owner+0x2C4E | window[6] |
| DJ | 2 | 1 | owner+0x2C4F | window[7] |
| MC | 4 | 2 | owner+0x2C50 | window[8] |
| MC (camera cuts, coupled) | 4 | 5 | owner+0x2C53 | window[11] |
Target value is always **1** (= removed). Window mapping confirmed by `SN/blacktop-presentation-overlay.md:13`.

Write order and journal (`UH/BlacktopPresentation.cs:85-102`):
1. Build journal `{schema:1,pid,created,basis,image,owner,mask,original[6],expected[6],phase:"prepared"}` and save atomically (temp + `File.Replace`/`Move`, `Flush(true)`) to `%userData%/unlocks/blacktop-presentation.json` **before** any write (`:44-48`, `:86-88`; path `WR/src/core/blacktop-presentation.js:15`, dir `WR/src/main.js:2035`).
2. For each selected index in order 0,1,2,5 whose value differs: re-read + verify, `WriteProcessMemory` 1 byte, read back (`:91-98`).
3. Final readback must equal `expected`; then journal `phase:"applied"` (`:99-100`).
4. Failure: journal `phase` = `failed` if any write was attempted (-> `write-failed` "Presentation write stopped. Close game to clear changes; recovery retained.") else `restored` (`:102`).
- An existing journal for the same exact game with a phase other than `applied`/`restored` -> `conflict` "An interrupted presentation write needs a game restart." (`:72`).
- Journal load validation: size <= 8192, schema 1, mask 0..7, 6-byte arrays of 0/1, `expected` must equal original with the mask applied (`:49-55`).
- Restore: **none by design**. Settings key: `blacktopPresentation: {drone,dj,mc}` in launcher settings (`WR/src/main.js:2036`).

#### 4. Build profiles
- WPF: only `6AA2013D:36558000` (what WPF calls `CURRENT`, `WR/src/core/game-compatibility.js:3`). The older `6A974D13:368C3000` build is rejected by `DebugBuild` (`UH/DebugMenus.cs:32`). Note `game-compatibility.js` has no explicit row for drone/DJ/MC; the guard lives only in the helper.
- WPF's last successful removal was on `6AA2013D`; WPF cannot do it on the newer V2 game update (`SN/blacktop-presentation-overlay.md:12-13`).
- V2 equivalent (Rust): `src/presentation_v2.rs`, `presentation_v2_native.rs`, `presentation_v2_runtime.rs`; same window layout, selection signature `0x92DF06DE` stage 3 still valid on V2 (`SN/blacktop-presentation-overlay.md:13-18`). V2 getter research: `nba2k27-launcher-tauri2/artifacts/game-v2-presentation-getters-20261016.txt`, `game-v2-presentation-dj-mc-getters-20261016.txt`, `game-v2-presentation-mc-coupling-20261016.txt`.

#### 5. Rust must match
- [ ] Labels exactly "Court presentation", "Remove drone", "Remove DJ", "Remove MC"; fresh install = all off.
- [ ] Toggling saves immediately (atomic), disables the switches while saving, and on failure shows the error and reverts to the last confirmed values.
- [ ] Toggling while a game is running does not write to that game; the change applies on the next Play.
- [ ] With all three off, Play starts no presentation helper/poller at all.
- [ ] Only a game process created after the Play click is bound; if none within 120 s, status shows "No new game session found. No removal flags applied." and nothing is written.
- [ ] No write at the main menu, during loading or during a match: writes only at Blacktop player selection (mode `0x92DF06DE` both slots, stage 3, active 0, kind 0) — debounced on V2.
- [ ] Remove MC writes both MC bytes (window[8] and window[11]); drone = window[6]; DJ = window[7]; value 1.
- [ ] Durable journal written before the first byte; each byte re-verified immediately before writing and read back after.
- [ ] After a match the game resets the bytes to originals; at the next Blacktop selection in the same process the same mask is re-applied (second match still has no drone/DJ/MC).
- [ ] Unexpected values / owner change -> stop for this game with a visible error; never overwrite foreign data.
- [ ] Never restores; closing the launcher leaves the bytes until game exit.
- [ ] Polling cadence ~500 ms; each helper/native call bounded (~8 s).
- [ ] Status text hidden unless error/conflict/unsupported/write-failed/timeout.
- [ ] Refuses when EasyAntiCheat / start_protected_game is running.

---

### Sub-feature B: Remove crowd

#### 1. User-visible behavior
- Fourth switch in the same card: `#blacktopCrowdEnabled`, label **"Remove crowd"** (`WR/src/renderer/index.html:137`). **Inverted binding**: the setting is `blacktopCrowdEnabled` (true = crowd kept); the checkbox shows `checked = enabled === false` and saves `!checked` (`WR/src/renderer/blacktop-crowd.js:6`, `:10`).
- Default: `blacktopCrowdEnabled: true` -> "Remove crowd" unchecked (`WR/src/core/paths.js:27`, normalization `:35`).
- Separate status line `#blacktopCrowdStatus` and a hidden secondary button `#blacktopCrowdRestore` **"Restore original crowd setting"** (`WR/src/renderer/index.html:140-141`).
- Status line is hidden when phase is `configured` or the message is one of: "Crowd control leaves the original setting unchanged.", "Unlock helper stopped safely. Restart the game before retrying.", "Game ended; memory-only crowd changes cleared."; otherwise **every** crowd message is shown (unlike drone/DJ/MC, waiting/applied messages are visible) (`WR/src/renderer/blacktop-crowd.js:6`).
- Restore button visible only when `canRestore` = owned session && applied && not blocked (`WR/src/core/blacktop-crowd.js:19`; render `WR/src/renderer/blacktop-crowd.js:6`). Clicking it = `save(true)` (re-enable crowd) (`:11`).
- While saving: checkbox and restore button disabled (`WR/src/renderer/blacktop-crowd.js:9`).
- Messages: "Crowd removal saved for the next Play launch." (`WR/src/core/blacktop-crowd.js:12`, `:31`); "Waiting for a new Play session and Blacktop player selection." (`:39`); "No new game session found. No crowd change attempted." (`:47`); helper: "Waiting for Blacktop player selection.", "Stand crowd and all five Blacktop standing-crowd presets prepared for this visit.", "Crowd removal retained for this Blacktop visit.", "Original crowd settings restored after leaving Blacktop.", "Crowd restoration waits until Blacktop ends at the main menu.", "Both crowd settings were already off; no change owned.", "The game already restored the original crowd settings." (`UH/BlacktopCrowd.cs:181`, `:207-223`).
- Each crowd state is also logged to the selection status log as `{selected: 'original'|'remove', ...state}` (`WR/src/main.js:2043`).

#### 2. Timing and gating
- Arm at Play right after drone/DJ/MC (`WR/src/main.js:1804`). If crowd is kept (`enabled`), emit configured and run nothing (`WR/src/core/blacktop-crowd.js:38`).
- **Arm throws** if the previous visit still owns unrestored changes: "The previous Blacktop crowd visit is still awaiting safe restoration." (`:36`) — this aborts the launch (thrown inside `launchGame`, caught at `WR/src/main.js:1815-1822`).
- Bind: deadline 120000 ms; `crowd-bind` every 500 ms; same "new game since Play" acceptance, plus `writes===0` ("Crowd control requires a new game started with Play.") (`WR/src/core/blacktop-crowd.js:39`, `:47`, `:51`). The deadline applies only to finding the game, not to time spent in menus afterwards (comment `:45-46`).
- After bound: `crowd-tick` every **500 ms**, or every **250 ms** while changes are owned (`applied`) (`:66`).
- Apply gate: exact Selection context (`0x92DF06DE` both, stage 3, active 0, kind 0) (`UH/BlacktopCrowd.cs:20`). Restore gate: exact **MainMenu** context: mode `0x6509837B` or `0x079964C0` (both slots equal), stage 1, active 0, kind 0 (`UH/BlacktopCrowd.cs:23`). Never writes during loading/gameplay (`UH/BlacktopCrowd.cs:212-213`).
- Settle: before the first write of a transaction the helper sleeps **250 ms** and re-captures; any difference -> `waiting` "Crowd preparation settling: <what> changed before writing; no write attempted." (`UH/BlacktopCrowd.cs:160-162`).
- Decision table (`UH/BlacktopCrowd.cs:42-49`): not owned + selection + not removed -> apply; owned + main menu -> restore (or "reset" if the game already restored); owned + selection + bytes re-set by game -> reapply; owned + removed -> observe ("retained"); stop requested while not at menu -> wait-exit.
- Auto-restore at main menu ends the **visit**, not the session monitor: a later Blacktop selection in the same game applies again (`WR/src/core/blacktop-crowd.js:60-65`).
- Turning removal off (enable crowd) during a game: `stop()` sends `crowd-restore`; if not at the menu the helper returns `waiting` and the JS keeps a **restore-only** monitor running until the helper verifies the main menu (`WR/src/core/blacktop-crowd.js:69-82`). Newly disabling crowd (choosing remove) during a running game never attaches/writes (`:29-31`).
- Failure phases (`error`,`conflict`,`unsupported`,`write-failed`) set `blocked` and stop (`:6`, `:57`). `ended` clears session (`:59`).
- Launcher shutdown: `blacktopCrowd.stop()` bounded to 9000 ms (`WR/src/main.js:2345`); in native-host cleanup it runs after the game is confirmed stopped (`:2259`).

#### 3. Mechanism
Helper commands `crowd-bind|crowd-tick|crowd-restore <root> <unlocks>/blacktop-crowd-session.json <pid> <created> <basis>` (`WR/src/core/blacktop-crowd.js:14`, `UH/BlacktopCrowd.cs:191`). Bind opens with `0x410` (read), others `0x438` (`UH/BlacktopCrowd.cs:205`).

Build guard (`UH/BlacktopCrowd.cs:12-18`, `:86-103`): PE timestamp `0x6AA2013D`, image size `0x36558000`, getter bytes at `0x26BAF80` (full hex at `:18`), plus 17 code signatures (table anchor `0x24C8D45`, row arithmetic `0x28D8AB5 imul rcx,rcx,0xC810`, Blacktop preset `175+index` at `0x32BF569`, match-size `0..4` checks, etc.). Mismatch -> `unsupported` "Crowd control does not support this game build." / "Crowd preload code differs from the verified game build."

Data (`UH/BlacktopCrowd.cs:13-17`, `:108-130`):
- Global: `owner = *(u64*)(base+0xAA51F50)` (same owner as drone/DJ/MC); 16-byte window at `owner+0x2C48`; **stand crowd flag = window[1] = owner+0x2C49**; must be <= 1.
- Preload table: `table = *(u64*)(base+0xAACAFE8)`; records **175..179** (the five stock Blacktop match-size presets); row = `table + index*0xC810 + 8`; first u32 of row must equal index (else waiting until initialized, or conflict if owned); front window 24 bytes at `row+0x7428` (must have [0]==0, [1]==0x4C, [8]<=1); **standing-crowd flag = front[8] = row+0x7430**; ambient 24 bytes at `row+0x73F8` checked ([1]==0x4D, [8]<=1) but never written.
- Removed state = window[1]==0 and all five front[8]==0 (`:72`).
- Up to 6 single-byte writes, value 0 to remove; restore writes journaled originals (`:16`, `:165-179`). Order: stand flag first, then presets 175..179.
- Per write: full re-read, data must be byte-exact (owner, table, whole window, every preset front+ambient) and context exact + still Selection/MainMenu; write; readback exact (`:169-178`).
- Journal `%userData%/unlocks/blacktop-crowd-session.json`, schema 3: `{schema,pid,created,basis,owner,image,phase,timestamp,size,original,expected:0,records:[{index,owner,original[24]}×5]}`; phases `prepared|partial|applied|restoring|restored|failed` (`:52-57`, `:136-158`). Saved before writes and after completion; a `waiting` interruption mid-transaction after some writes -> `partial` (still owned, retried at selection or restored at menu) (`:41`, `:183-186`). Legacy schema 1/2 journals in the same live game -> `conflict` "Legacy or failed crowd changes require a game restart." (`:209`).
- Known V2 port issue (Rust, fixed 0.49): the game resets window[1] and drone/DJ/MC bytes between visits; WPF crowd restore compared preset fronts and window, Rust must treat window bytes 6/7/8/11 as game-owned booleans (`SN/blacktop-presentation-overlay.md:80-88`). Note WPF `DataExact` compares the **whole** 16-byte window (`UH/BlacktopCrowd.cs:76`) but WPF re-captures a fresh snapshot each tick, so it only compares within a transaction.

#### 4. Build profiles
- WPF: `crowd: ['Crowd removal',[CURRENT]]` = `6AA2013D:36558000` only (`WR/src/core/game-compatibility.js:6`).
- V2 (Rust): `presentation_v2*.rs`; V2 Main menu reads stage **0** (not 1) so restore needed `stage 0|1` (`SN/blacktop-presentation-overlay.md:15-16`, `:22-23`). V2 crowd research: `nba2k27-launcher-tauri2/artifacts/game-v2-crowd-*-20261016.txt`.

#### 5. Rust must match
- [ ] Label "Remove crowd"; default unchecked (crowd kept); stored inverted as `blacktopCrowdEnabled`.
- [ ] With crowd kept, Play runs no crowd helper.
- [ ] Applies only at exact Blacktop player selection after a 250 ms settle re-capture; writes stand flag + all five Blacktop presets 175-179 to 0.
- [ ] Restores originals only at the verified main menu (V2: stage 0 or 1), never during loading/gameplay.
- [ ] After restore at the menu, the next Blacktop selection in the same game removes the crowd again (match 2, 3 ... all crowd-free).
- [ ] Unchecking "Remove crowd" during a match does not write; it restores at the next main menu. "Restore original crowd setting" button appears only while changes are owned.
- [ ] A Play launch while a previous visit still owns unrestored crowd changes fails with "The previous Blacktop crowd visit is still awaiting safe restoration."
- [ ] Poll 500 ms (250 ms while owned); 120 s to find the new game process.
- [ ] Crowd status messages (waiting/applied/restored) are visible in the card; "configured" and "unchanged original" messages are hidden.
- [ ] Shares a cross-process lock with drone/DJ/MC so the two never write concurrently.

---

## Known divergences in current Rust port (brief, from sibling notes; not re-verified)
- Rust 0.37-0.46 never requested removals; settings import of WPF `blacktopPresentation`/`blacktopCrowdEnabled` added later, only on first-run import (`SN/blacktop-presentation-overlay.md:8-11`, `:33-36`).
- 0.47 live: 3 of 4 actor writes landed (window[11] = 0) due to V2 context blips; fixed with 5-sample majority debounce and bounded actor retry (max 3 per game) (`SN/blacktop-presentation-overlay.md:38-47`). WPF has no debounce and no retry limit (it simply re-polls every 500 ms).
- 0.48 crowd returned on match 2 (`nba2k27-launcher-tauri2/artifacts/feature-gap-inventory-20261017.md:69`); 0.49 fix pending live check (`SN/blacktop-presentation-overlay.md:80-92`).

## Open questions / UNVERIFIED
- Exact game semantics of index 3 and 4 in the 6-byte flag block (owner+0x2C51/0x2C52) — not touched by WPF; **Not found in WPF sources**.
- Whether WPF ever showed drone/DJ/MC status anywhere besides the error line — only the error line found (`WR/src/renderer/blacktop-presentation.js:7`).
- F8 overlay had Presentation switches only in 0.37.71 and they were removed in 0.37.72 (`nba2k27-mod-launcher/README.md:105-120`); the frozen build has none.
