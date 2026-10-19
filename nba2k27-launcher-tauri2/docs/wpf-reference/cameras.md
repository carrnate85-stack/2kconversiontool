# Cameras — WPF reference
Source build: WPF quality-pass-qa-20261010 (frozen). Last researched: 2026-10-17.

Path abbreviations used in citations (all relative to `C:\Users\carrn\CodexProjects\2kconversiontool`):
- `WR` = `nba2k27-launcher-wpf/release/quality-pass-qa-20261010/App/Reference`
- `CT` = `nba2k27-camera-tool/src/Program.cs`

**Source and binary match:**
- `WR/runtime-resources/camera-tools/NBA2K27CameraController.exe` has SHA-256 `97F04B68…519F`. This is identical to `nba2k27-camera-tool/dist/NBA2K27CameraController.exe` and `nba2k27-mod-launcher/runtime-resources/camera-tools/NBA2K27CameraController.exe`.
- That exe was built 6 s after `CT` was last changed (2026-10-04 17:09:32 → 17:09:38), so `CT` is the source of the shipped controller.
- The SHA in `CAMERAS-V41.md` (`1004A7CF…`) is an **older** build and is superseded.

## Summary
- **What it does:** a "Cameras" fold on the Presentation page edits 7 curated cameras. For each camera you can set: use/skip, name, height, zoom and order. A "Lock height and zoom" toggle is also available.
- **How it runs:** once the game process appears, the launcher starts a persistent helper, `NBA2K27CameraController.exe --managed-service`. The helper copies each selected camera into spare registry slots **63..71**. It points all **13 game-mode camera lists** at private expanded copies that list those slots.
- **Timing and gating:** the helper uses data writes only, with no code hooks. **There is no menu, game-mode or stage gate.** It installs once per game process, as soon as static tables validate. The install is all-or-nothing: any failure runs `RestoreAll`. It retries every 500 ms within a 20 s transient window. The JS side restarts the helper up to 2 times. After `LIVE` it never re-installs. A 200 ms maintain loop only re-asserts height/zoom.
- **Camera names:** names come from a session-scoped patched `loc/english.iff`. It is decoded from the installed game, written into the active mods folder before Steam starts, and force-served through version.dll `OVERRIDE`. It is removed after the game exits.
- **Most likely to break in a port:**
  1. **Adding a readiness or menu gate**, or retrying per write instead of all-or-nothing. The user memory confirms Rust 0.43/0.44 failed live for this reason. Rust 0.46 passed after copying WPF's model.
  2. **Write-protection handling.** The registry entry is written with plain `WriteProcessMemory`. Labels, availability bytes and mode descriptors use `VirtualProtectEx(PAGE_READWRITE)` → write → restore protection. Early `PAGE_WRITECOPY` pages broke Rust 0.45 (per user memory).
  3. **Order and selection persistence quirk.** `CameraOrder.IDs` stores **only selected IDs**, so unselected rows move to the end in curated order after reload. Each slot's first 8 bytes must be the **localization key** from `CameraNames.ini`. With the key missing, the managed service refuses that slot.

## Key source map
| Concern | file:line |
|---|---|
| Curated list, IDs, stock names | `WR/src/core/camera-manager.js:10-18` |
| INI parse/serialize, Nosebleeds migration | `WR/src/core/camera-manager.js:30-65` |
| Install/copy controller and INIs | `WR/src/core/camera-manager.js:113-136` |
| Read/save library, validation | `WR/src/core/camera-manager.js:138-192` |
| Spawn, 750 ms status poll, restart ×2 | `WR/src/core/camera-manager.js:242-267` |
| Stop (signal file, 2 s, kill) | `WR/src/core/camera-manager.js:283-298` |
| Status-file parser | `WR/src/core/camera-manager.js:77-90` |
| Name labels, keys, donors, max length | `WR/src/core/camera-localization.js:4-37` |
| english.iff patcher | `WR/src/core/camera-localization.js:13-62` |
| Session names prepare/restore | `WR/src/core/camera-names-session.js:1-53` |
| Session monitor (starts controller when game appears) | `WR/src/core/session.js:65-137`, wiring `WR/src/main.js:503-509` |
| IPC handlers | `WR/src/main.js:1184-1206` |
| Play: names prepare and forced override | `WR/src/main.js:1764-1783` |
| Cleanup on game end / launcher exit | `WR/src/main.js:2079`, `:2248`, `:2261`, `:2333`, `:2349` |
| Construct manager, startup recovery | `WR/src/main.js:2005-2012`, `:2045` |
| Camera UI markup | `WR/src/renderer/index.html:265-288` |
| Camera UI logic | `WR/src/renderer/app.js:335-413`, `:2443-2528` |
| Launch step "Camera unlock" | `WR/src/renderer/index.html:501`, `WR/src/renderer/app.js:289-290` |
| Managed service loop | `CT:944-994` |
| Transient/failure classification (20 s) | `CT:874-890` |
| Static-table discovery (only readiness check) | `CT:902-939` |
| Slot install / mode patch / RestoreAll | `CT:337-467` |
| Choice ordering from INI | `CT:891-901` |
| Template INIs | `WR/runtime-resources/camera-tools/CameraLibrary.ini`, `CameraPresets.ini`, `CameraProfiles.ini` |

---

### Camera setup UI
#### 1. User-visible behavior
- **Location:** a `<details id="presentationCamerasSection" open>` fold on the **Presentation** page (`index.html:265`). It shows the summary "Cameras" / "Choose and arrange cameras for Play" and a pill `#cameraCount` "N SELECTED" (`:266`).
  - Note: `cameras-polish.css` targets `#page-cameras`, an older standalone page. The live styling is `presentation-combined.css:13-23`.
- **Toolbar** (`index.html:268-271`): **"Retry cameras"** (`#retryCameras`, hidden) and **"Stop & restore"** (`#stopCameras`, hidden). The toolbar collapses when both are hidden (`presentation-combined.css:16`).
- **List columns:** "Use | Camera | Height | Zoom | Order" (`index.html:274`). Each row (`app.js:354-379`) has:
  - a checkbox
  - a name text input plus a **↺** reset-name button. Locked cameras show **—** and both controls are disabled.
  - height and zoom number inputs (min −100, max 100, step 1)
  - **↑ / ↓** order buttons. The first ↑ and last ↓ are disabled.
  - Name `maxLength` = `nameMaxLength`, with a tooltip of "N characters maximum" or "Stock camera name is locked". The reset tooltip is "Reset to <defaultName>". The placeholder is the default name.
- **Side card "Camera setup"** (`index.html:278-283`):
  - Toggle **"Lock height and zoom"**, small text "Reapply saved values while the game is running." (`#lockCameraValues`)
  - Box **"In-game camera names"** with status `#cameraNamesStatus` (initially "Checking loc/english.iff…") and an **"Install names"** button. The button is hidden when the names are app-managed, which is the case in WPF.
  - Button **"Save camera setup"**
- **Status card** `.camera-status` (`#cameraState`, `#cameraMessage`, `#cameraDot`) has class `hidden` in the markup (`index.html:284`). **No renderer code removes `hidden`**, and `.hidden{display:none!important}` (`styles.css:306`). So in this WPF build **the status card is never visible**. Only the Retry and Stop buttons react to the state. Its text is still kept up to date (`app.js:391-393`).
- **Button visibility** (`app.js:390-395`):
  - "Stop & restore" is shown while `STATE ∈ {WAITING, STARTING, LIVE}`.
  - "Retry cameras" is shown **only when `STATE === 'ERROR'`**.
- **Names status text** (app-managed branch, `app.js:399-400`):
  - conflict → "Another English text mod is present; it will not be overwritten."
  - installed → "Camera names active for this game session; cleaned up after exit."
  - otherwise → "App-managed camera names apply automatically on Play. Restart the game after updating."
- **Editing** only changes local state. The selected count updates live (`app.js:2443-2452`). ↺ restores `defaultName` locally (`:2455-2460`). ↑/↓ swaps rows locally (`:2462-2476`). **Nothing is persisted until "Save camera setup".**
- **Save** (`app.js:2478-2487`): the button shows "Saving…" and calls `camera:save`.
  - On success, toast: "Camera setup saved. Names apply after restarting the game; live height and zoom update automatically."
  - On a validation error, an error toast shows the error message.
- **Stop & restore** (`:2488-2495`): toast "Camera controller stopped and restoration requested."
- **Retry cameras** (`:2512-2520`): the button shows "Retrying…". It calls `camera:start` with the current UI setup, which saves first and then spawns the controller.
- **Launch checklist** step "Camera unlock" / "Starts after the game appears" (`index.html:501`):
  - `active` when the session is `game-running` and the camera is not yet started.
  - `done` when `session.cameraState === 'started'` (`app.js:289-290`).
  - Note: `started` means "controller spawned", not `LIVE`.
- **Validation messages** (`camera-manager.js:165-174`, `camera-localization.js:32-33`):
  - "The camera setup must contain all seven cameras."
  - "The camera order is invalid."
  - "Select between one and nine cameras." (effectively 1..7)
  - "Camera height and zoom must be whole values from -100 to 100."
  - "Camera <id> name must be 1–<max> printable English characters." (ASCII 0x20–0x7E, trimmed)
  - "<name> is locked to its stock name."

#### 2. Timing and gating
- There is **no enable/disable switch** for cameras. `startCameras: true` is forced in settings (`main.js:825,835`, `session.js:69`). If at least one camera is selected, the controller runs on every Play.
- **Saving while the game runs:**
  - Selection, order and names are **not** applied live. The installed choices are fixed for that game process (`CT:321`, `CT:973-974`). Names need a game restart.
  - Height, zoom and lock **are** picked up live: the controller polls `CameraLibrary.ini` mtime every 200 ms and calls `ConfigureManagedOverrides` (`CT:986`).
  - **With lock off, new H/Z values are NOT written live.** `SetLock(false)` does not apply them (`CT:324`). They are only written once at install (`Enable → ApplyOverrides`, `CT:341,343`). With lock on, they are written every 200 ms.

#### 3. Mechanism: files
All files live in `<userData>/camera-tools/` (`main.js:2006`):
- `NBA2K27CameraController.exe`: copied from the template when its SHA-256 differs (`camera-manager.js:115-117`).
- `CameraProfiles.ini` and `CameraPresets.ini`: copied only if missing (`:118-120`). These are used only by the manual WinForms tool UI. **The managed service does not read them.** Only `LibraryIni` and `CameraNames.ini` are used (`CT:891-901`, `CT:384`).
- `CameraLibrary.ini`: copied from the template if missing (`:122-126`). Template contents (`WR/runtime-resources/camera-tools/CameraLibrary.ini`):
  ```
  [Unlocks] 51=1 53=1 54=1 52=1 57=1 59=1 55=1
  [CameraOrder] IDs=51,53,54,52,57,59,55
  [UnlockValues] <id>=1,1   (height,zoom)
  [Managed] LockValues=0  NosebleedsAdded=1
  ```
  Saved keys (`camera-manager.js:176-190`):
  - `[Unlocks] id=1`; deselected IDs are **deleted**
  - `[UnlockValues] id=h,z`
  - `[Names] id=name` and `[DisplayNames] id=name`
  - `[CameraOrder] IDs=<selected ids only, in UI order>`
  - `[Managed] LockValues=0|1`
  - Note: the save is a plain `writeFile` (not atomic), while the migration write is atomic.
  - Defaults: missing `LockValues` is added as `0` (`:129-130`). The **controller's** own default when the key is missing is `"1"` (`CT:897`).
  - The Nosebleeds migration (`:55-65`) runs once (guarded by `Managed.NosebleedsAdded`). It inserts 55 after 59 and sets `Unlocks 55=1` and `UnlockValues 55=1,1`.
- `CameraNames.ini`: `[LocalizationKeys] id=<16-hex key>`, rewritten by `seedKeys` (`camera-names-session.js:13`, `camera-localization.js:63`).
- `camera-status.ini`: written by the controller (`CT:863-873`). Fields: `STATE=`, `GAME_PID=`, `CAMERAS=<ids>`, `MESSAGE=`, `UPDATED=<ISO UTC>`. Written via `.tmp` + copy.
- `camera-stop.signal`: written by the launcher to stop the controller (`camera-manager.js:287`).
- `names-session.json`, `names-cache/`, `legacy-camera-names.iff`: name session files (see the Camera names section).
- Diagnostics: `logs/camera-service-history.json`. It keeps the last 80 rows, deduplicated on state|pid|message, and is ≤128 KB before rewrite (`camera-manager.js:277-281`).

#### 4. Build profiles
- Same UI for every build.

#### 5. Rust must match
- [ ] The editor always shows exactly 7 rows, in saved order, followed by unselected cameras in curated order (51, 53, 54, 52, 57, 59, 55).
- [ ] Saving persists the order of **selected** cameras only. After a reload, unselected cameras appear after the selected ones.
- [ ] Saving with 0 selected is refused with "Select between one and nine cameras."
- [ ] H/Z outside −100..100, or non-integer, is refused with the exact message.
- [ ] Side (59) and Nosebleeds (55) names are locked. ↺ is disabled and shows "—".
- [ ] "Retry cameras" is visible only in `ERROR`. "Stop & restore" is visible in WAITING/STARTING/LIVE.
- [ ] Name reset (↺) and H/Z edits are two separate actions. Neither persists without Save.

---

### Controller lifecycle and activation (the important part)
#### 1. User-visible behavior: controller STATE and MESSAGE values
| STATE | MESSAGE | source |
|---|---|---|
| WAITING | "Waiting for NBA2K27.exe." | `CT:947,952` |
| WAITING | "NBA 2K27 detected; waiting for its camera registry. Last check: <detail>" | `CT:887` |
| ERROR | "Camera data did not become ready within 20 seconds. Close NBA 2K27, launch it again, then retry cameras. Last check: <detail>" | `CT:888` |
| ERROR | "Camera compatibility check failed; nothing was left applied. <detail>" | `CT:889` |
| ERROR | "Select at least one camera in the launcher." | `CT:973` |
| LIVE | "<N> camera(s) mapped to data-enabled slots 63-<62+N> (no hooks)." plus per-camera notes, plus " H/Z overrides locked." or " Start a new session." | `CT:335` |
| GAME_CLOSED | "NBA 2K27 closed; original camera data restored." | `CT:991` |
| STOPPED | "Camera controller stopped; original camera data restored." | `CT:991` |
| (no file) | parsed as STOPPED "Camera controller is stopped." | `camera-manager.js:84-87` |

The UI also knows `STARTING`, but the controller never writes it.

#### 2. Timing and gating (verified against source)
- **When it starts:**
  - Play → `launchStage('preparing-session', 'Starting the local game and camera monitor…')` → `session.start(settings)` (`main.js:1764-1766`). This runs **before** camera names, scoreboards, ball and version-proxy arming.
  - `SessionManager` ticks every **1000 ms** (`session.js:85-94`) and checks `NBA2K27.exe` via tasklist or a native probe (result cached 500 ms, `session.js:13`).
  - **When the process first appears**, it calls `cameraManager.start(modsRoot)` once (`session.js:127-130` → `main.js:506-507`).
  - So the controller is spawned **after the game process exists**, typically within ≤1 s of process creation. No menu or window readiness is involved.
- **Spawn** (`camera-manager.js:242-267`):
  - `ensureInstalled()` runs first. The old stop and status files are deleted.
  - Then `spawn(controller, ['--managed-service', statusPath, stopPath, String(process.pid)], {cwd: managedRoot, windowsHide, stdio:'ignore'})`.
  - A **750 ms** interval reads `camera-status.ini` and publishes `camera:status`.
  - The first publish happens after a **250 ms** delay.
- **Auto-restart** (`camera-manager.js:252-261`): when the child exits, it restarts after **750 ms** only if all of these hold:
  - not an intentional stop
  - last STATE ≠ `GAME_CLOSED`
  - `restartAttempts < 2`
  - the game is still running
  - `restartAttempts` resets to 0 whenever a publish sees `LIVE` (`:271`).
- **Controller loop** (`CT:944-994`):
  1. Runs while the stop file is absent **and** the parent launcher PID is alive (`CT:950`).
  2. If there is no `NBA2K27` process: WAITING, sleep **400 ms**. If the game was seen before, leave the loop and report GAME_CLOSED (`CT:952`).
  3. **`waitingSince` = the time this PID was first seen** (`CT:953`).
  4. **Read-only preflight** `DiscoverManaged(true)` (`CT:954-963`). This is the **only readiness check**. It validates static tables and nothing else (`CT:902-939`):
     - `MainModule` base is readable.
     - No module named `*easyanticheat*`, `*easy_anticheat*` or `beclient*` is loaded, else "Use the supported offline without-EAC launch option."
     - `OpenProcess(QUERY|VM_READ)` succeeds, else "Camera data could not be opened."
     - MZ/PE64 header is valid.
     - Build is `6A974D13/368C3000` or Sep 11 `6AA2013D/36558000`, else "Unsupported camera build."
     - On Sep 11: 5 RIP-relative code anchors must match (`CT:921-923`).
     - Broadcast registry entry 10: class == 8, and both parameter pointers == `base+broadcastParamRva`.
     - All **13 mode descriptors** are readable. Each `count` is in 1..64, `ptr` is inside the image, and every ID is ≤ 71.
     - **It does not read the current mode pointer, any menu state, a stage flag or a gameplay flag.** `CurrentModePointerRva 0x0AAE11A0` is used only by the manual `HiddenCameraOverride` tool class (`CT:202,270`), never by the managed service.
  5. **Write connection** `DiscoverManaged(false)` adds `VM_WRITE|VM_OPERATION` and repeats the same validation (`CT:964-972`).
  6. Reads choices from the INI. With zero choices: ERROR, exit 3 (`CT:973`).
  7. `new MenuCameraUnlock` → `SetTargets` → `ConfigureManagedOverrides` → `SetEnabled(true)` → `Enable()`. This is **one pass**, all-or-nothing (`CT:974`, `CT:337-342`).
  8. If `Enable` fails, `RestoreAll` has already run inside `Enable`. The failure is classified, and the loop sleeps **500 ms** and repeats from step 2 (`CT:975-982`).
  9. On success: `sawGame=true`, STATE LIVE. Then an inner loop runs every **200 ms** while the stop file is absent, the parent is alive and the game is alive (`CT:983-987`):
     - re-read INI H/Z and lock if the mtime changed
     - `Maintain()`: re-apply H/Z overrides if locked, and rewrite the legacy H/Z step clones when H/Z changed (`CT:335,436-440`)
     - rewrite the LIVE status
     - **No re-install and no re-check of modes. Nothing reacts to menus, matches or Quick Play.**
  10. After the loop, the `using` block disposes the Connection → `MenuUnlock.Dispose()` → **`RestoreAll()`** (`CT:63,467`). Final STATE is STOPPED if the stop file exists, otherwise GAME_CLOSED if the game was seen, otherwise STOPPED (`CT:991`).
  11. Any exception: STATE ERROR with the message, exit 5 (`CT:993`).
- **Transient vs. fatal** (`CT:874-884`):
  - **Transient** messages (WAITING until 20 s after `waitingSince`, then ERROR):
    - "Camera data could not be opened."
    - "Camera mode unavailable."
    - "Camera mode list validation failed."
    - "Camera ID … registry/label read …" and "Camera ID … source-label read …"
  - **Everything else is fatal immediately**: STATE ERROR, the controller **exits with code 4**, and the JS restart (≤2) applies. Fatal examples:
    - unsupported build
    - "Camera reference validation failed."
    - "Broadcast registry validation failed."
    - "Camera mode ID validation failed."
    - any **write** failure, e.g. "slot registry write", "slot label write/protection", "availability write/protection", "runtime availability write/protection"
    - "camera eligibility descriptor moved/changed"
    - signature mismatches
    - "verified app camera-name key is missing"
    - "A game-mode camera list could not be expanded; all changes were restored."
  - Each restart is a new process, so `waitingSince` and the 20 s window restart too.
- **Stop paths:**
  - UI "Stop & restore" → `camera:stop` (`main.js:1198-1201`) → `stop()`: write `camera-stop.signal`, wait **2000 ms** for exit, otherwise `kill()` and wait 1000 ms (`camera-manager.js:283-291`).
  - Launcher quit → `stopSync()` writes the signal file only (`main.js:2333`). Then `boundedShutdown('camera controller', …, 3500)` (`:2349`), or under the WPF native host, `cleanupNativeSession` → `stop()` after confirming the game has stopped (`:2261`).
  - If the launcher dies, the controller sees its parent PID gone and restores (`CT:950,984`).
- **Game exit:** the inner loop ends. RestoreAll runs (harmless on a dead process) and STATE becomes GAME_CLOSED. There is no restart. The session monitor resets `cameraStarted=false` and emits "NBA 2K27 closed. Local loader remains armed for the next normal launch." (`session.js:132-136`). The next game process triggers `cameraManager.start` again.
- **Retry button** → `camera:start` with payload → `saveLibrary(payload)` then `start()` (`main.js:1193-1197`). If the child is still running, `start` just returns the view (`camera-manager.js:244`).

#### 3. Mechanism: memory writes (`MenuCameraUnlock`, `CT:289-468`)
- **RVAs** (`CT:291-293`, Sep 11 overrides `CT:317`):

| Table | Original 6A974D13 | Sep 11 6AA2013D |
|---|---|---|
| Camera registry (64 B entries, IDs 0..71) | `0x098A4F80` | `0x098A8FA0` |
| Label table (4 B per ID) | `0x06BBE8E0` | `0x06BC18E0` |
| Static availability mask (1 B per ID) | `0x06BBEA00` | `0x06BC1A00` |
| Runtime availability (1 B per ID) | `0x0A5E9D00` | `0x0A5EDD00` |
| Mode-list table (13 × 16 B descriptors {ptr u64, count i32}) | `0x098A4E00` | `0x098A8E20` |
| Press source params | `0x098F6E70` | `0x098FAE90` |
| Full Court source params | `0x098FCD60` | `0x09900D80` |
| Beluba source params | `0x099016F0` | `0x09905710` |
| Broadcast / ball-tracking params | `0x098ECD10` | `0x098F0D30` |
| Parameter record size | `0x4990` | same |

- **Registry entry layout used:**
  - `+0x00` 8-byte identity / localization key
  - `+0x08` class (int)
  - `+0x10`/`+0x18` eligibility descriptor pointers
  - `+0x20` zoom (int32), `+0x24` height (int32)
  - `+0x28` factory
  - `+0x30`/`+0x38` primary and secondary parameter pointers
- **Slot mapping:** the camera at selected index `i` goes to slot `63+i`, with at most 9 (`CT:297,340`). The order comes from `CameraOrder.IDs` filtered by `Unlocks=1`, followed by any other selected supported ID in the order 51, 53, 54, 52, 57, 59, 55 (`CT:891-896`).
- **`InstallSlot(slot, target)`** (`CT:344-360`). **Write order per slot:**
  1. **Read** the original slot entry, the target entry, the slot label, the static mask and the runtime mask. Read the source label from `LabelDonorFor(target)`: **51 → 38, 57 → 37**, all others use their own ID (`CT:385`).
  2. Build the behavior entry (`BuildBehaviorEntry`, `CT:391-417`). This allocates private RW clones of the parameter records (`VirtualAllocEx`). Stock records are never edited.
     - **53 (Press 2K17):** Press 2K27 (ID 54, class 3) tracking with 2K17 framing (−1350, 400, 0, FOV 45). Two clones in a 0xA000 allocation.
     - **51 (Broadcast Low 2K18) and 21:** Broadcast (ID 10, class 8) tracking. 51 gets X −2100, Y 450, Z 0, FOV 64.023117.
     - **52 (Beluba 2K17):** Side (ID 59) tracking with Beluba framing: X −1900, Y 400, Z 0, FOV 67.041786 (`CT:296,429-435`).
     - Each source record is signature-checked (floats within 0.01). A mismatch gives a "signature changed" failure.
  3. Copy entry bytes `+0x08..+0x40` from the behavior entry into a copy of the slot's original entry.
  4. Eligibility (`CT:370-382`): for 21/51/52/53/55, clone the 0x34-byte descriptor and set field +4 from 1 to 0 ("unrestricted", needed for Blacktop and other modes). Point `+0x10/+0x18` at the clone.
  5. Push a `SlotRecord` with the originals for restore.
  6. Write the localization key from `CameraNames.ini` to entry `+0x00`. **Required** in managed mode (`RequireManagedNames=true`, `CT:946,353`).
  7. **Writes, in order:** registry entry (64 B, plain WPM) → slot label (4 B, `WriteProtected`) → static availability = `0` (`WriteProtected`) → runtime availability = `1` (`WriteProtected`) (`CT:356-359`).
- **`WriteProtected`** (`CT:466`): `VirtualProtectEx(PAGE_READWRITE)` → WPM → `VirtualProtectEx(old)`.
- **After all slots**, `PatchMode(mode)` runs for **mode 0..12** (`CT:341,447-456`):
  - It builds a new ID list: stock IDs **minus the selected target IDs** and minus slots 63..71, then appends the slots in selection order.
  - If the list changed, it allocates a 0x1000 RW block, writes the list, then **writes the descriptor pointer, then the count** (both `WriteProtected`).
  - If a descriptor write fails partway, it rolls back the count and pointer and frees the block.
- **Any failure** of any slot or mode → `RestoreAll()`, with status "Camera ID X -> slot Y failed: <detail>" or the mode-list message. **All or nothing** (`CT:340-341`).
- **`RestoreAll`** (`CT:457-463`) restores mode descriptors (count then pointer) and frees their blocks, then native labels, then slots in reverse order (entry, label, static mask, runtime mask). Parameter clones are freed only if every slot was restored.
- **H/Z** (`CT:327-334,343,436-440`):
  - Overrides are written to slot entry `+0x20` (zoom) and `+0x24` (height).
  - For the cloned-parameter cameras, `ApplyMappedSteps` turns H/Z into Y and FOV in the clones: `Y = baseY + (h-1)*step`, `FOV = baseFov + (z-1)*step`. Steps: 53 → 22.22222 / 1.111111, 51 → 55.55556 / 4.444444, 52 → 88.88889 / 6.666667. The clone is rewritten only when the (h,z) state changes.
- `PatchNativeConvertedLabels` exists (`CT:386`) but is **never called**. It is dead code.

#### 4. Build profiles
- The WPF controller supports original `6A974D13:368C3000` and Sep 11 `6AA2013D:36558000` (`CT:917-918`).
- **V2** values come from the Rust source, not WPF (`nba2k27-launcher-tauri2/src/camera_v2.rs:18-25`): REGISTRY `0x0992C830`, MODES `0x0992C6B0`, LABELS `0x06C27DC0`, STATIC_AVAILABILITY `0x06C27EE0`, RUNTIME_AVAILABILITY `0x0A67C430`, image size `0x35BDE000`.
- Rust 0.46 passed live with WPF-model publication (user memory; `artifacts/parity-score-20261017.md:74`).

#### 5. Rust must match
- [ ] Publication starts as soon as the game process exists **and** the static tables validate. There is **no** check of current mode, menu, stage or window.
- [ ] All slots and all 13 mode lists go in one pass. Any failed write restores everything already written and leaves the game stock.
- [ ] Failed attempts retry every 500 ms. Only "could not be opened / mode unavailable / mode list validation / registry-label read" count as transient, within 20 s of first seeing the PID. Other failures go to ERROR at once.
- [ ] The helper is restarted at most twice (750 ms delay) while the game is running. The counter resets on LIVE.
- [ ] Per-slot write order: registry 64 B → label 4 B → static availability 0 → runtime availability 1. Label and availability writes use a temporary `PAGE_READWRITE` protection change.
- [ ] Per-mode order: new list into a private block → descriptor pointer → count. Restore order: count → pointer → free.
- [ ] Selected native IDs are removed from the mode lists and replaced by slots 63+i in user order.
- [ ] Once LIVE, the installation is never re-applied or re-checked across menus, matches, Quick Play or Main. Only H/Z is re-asserted every 200 ms when lock is on.
- [ ] Changing H/Z in the launcher while LIVE with lock **on** takes effect within about 200 ms. With lock off it does not take effect until the next session.
- [ ] Stop & restore restores all camera data within 2 s. Game exit reports GAME_CLOSED with no restart.
- [ ] Missing localization key for a selected camera means the slot is refused (fatal ERROR).
- [ ] Parameter records are cloned, never edited in place. Signature checks match within 0.01.

---

### Camera names (session-scoped english.iff)
#### 1. User-visible behavior
- Names, stock sources, localization keys and length limits (`camera-localization.js:4-12`). Max length = length of `donorSource` if present, otherwise of `source`.

| ID | Default name | Stock source string | Key | Donor key / donor string | Max | Locked |
|---|---|---|---|---|---|---|
| 51 | Broadcast Low 2K18 | Full Court | `070A02F4676F7A26` | `E2D6B673633FB485` "Pregame Shootaround" | 19 | no |
| 53 | Press 2K17 | Half Court | `724E93A6FB8B2389` | `B09970FB8F8E8E2A` "Coach View" | 10 | no |
| 54 | Press 2K27 | DO NOT USE THIS CAM | `B800A4C12E23B6BD` | — | 19 | no |
| 52 | Beluba 2K17 | Full Court Corner | `81C8B0C640438EFB` | — | 17 | no |
| 57 | Beluba 2K27 | Rail High | `2AFCCD8CA86B576C` | `C0E7B391A0EDA7AD` "Minigame Downtown" | 17 | no |
| 59 | Side | Side | `3B94D5AAE1F235DE` | — | 4 | **yes** |
| 55 | Nosebleeds | Nosebleeds | `7DE7AF563ED79DC4` | — | 10 | **yes** |

- Note: the default name "Press 2K17" (10 characters) exactly fills its 10-character donor slot.

#### 2. Timing and gating
- **Play** (`main.js:1769-1774`): `preparing-names` "Checking the optional in-game camera names…" runs `readLibrary()` then `namesSession.prepare({gameRoot, modsRoot, names})`.
  - **A thrown error fails Play.** It is not caught. Examples: "A different loc/english.iff text mod is installed. It was left untouched; remove it from this profile to use camera names."
  - If the game is running: "Restart NBA 2K27 to apply its camera names before language data loads." (`camera-names-session.js:39,44`)
- If `installed`, Play passes `forcedOverrides=['loc/english.iff']` to version.dll (`main.js:1783`). Per the audit, this is the only production use of `OVERRIDE` (`artifacts/wpf-version-dll-audit-20261017.md:69`).
- **Restore** runs at launcher startup (`main.js:2012`), before each Play (`:1717`), on launch failure (`:1840`), on game-session cleanup (`:2079`) and on launcher exit (`:2248`). Restore refuses while the game is running.

#### 3. Mechanism
- `build()` (`camera-names-session.js:15-26`) works as follows:
  1. Find the manifest entry for `loc/english.iff`.
  2. Read the packed bytes from the game archive.
  3. Compute the cache identity `<sha256(packed)>-names-v3-<sha256(names)[0:16]>`.
  4. Decode with the VCZ decoder exe: args `archive offset size data/oodle/oo2core_9_win64.dll output`, 120 s timeout.
  5. Patch `TEXT.VCLOCALIZEDATA` in the zip (`camera-localization.js:44-62`):
     - Each key must be unique, 8-byte aligned and at `at%64 < 40` in the header blocks.
     - Its 4-byte string offset is at `block+40+slot*4`, in UTF-16 units from the pool, which is `ceil(u64@24/64)*64`.
     - The source must match the stock string exactly.
     - Shared-suffix strings are refused.
     - With a donor, the name is written into the donor's storage and the key's offset field is redirected to it.
  6. Re-verify, write the cache file, and delete the raw decoded file.
- `prepare()` (`:38-51`):
  1. Restore first.
  2. Re-seed `CameraNames.ini`.
  3. The existing target must be absent, a known legacy hash, or the template hash.
  4. Back up the original to `legacy-camera-names.iff`.
  5. Write the journal `names-session.json` `{schema:'camera-names-session-v1', modsRoot, target, before, hash}`.
  6. Check the target is unchanged.
  7. Write the target atomically (`tmp` + rename). Symlinks are refused (`:9-10`).
- `restore()` (`:27-37`): if the current hash is neither `hash` nor `before`, it refuses: "Camera name file changed outside the launcher; it was left untouched." Otherwise it deletes the target and the journal.

#### 4. Build profiles
- Names are built from whatever english.iff the installed build has, and fail closed if the source strings changed. They are not tied to a build.

#### 5. Rust must match
- [ ] Names are prepared before Steam starts, from the installed game's english.iff. A foreign `loc/english.iff` in the profile blocks Play with the exact message.
- [ ] The patched file is served via forced override and removed after the game exits or on next startup. An externally modified file is kept.
- [ ] Name length and charset validation match the table above. Locked names cannot change.

## Known divergences in current Rust port (brief; from sibling note `qa048-small-fixes.md`)
- Rust has a **visible** camera status card that polls every 1.5 s. WPF's card is hidden (`index.html:284`).
- Rust's "Retry cameras" appears when the phase is `stopped`, or when held and retryable. WPF shows it only when STATE is `ERROR`.
- Rust gives up after 10 fully restored publication attempts (`CameraRestoredLimit`). WPF's limit is time-based (20 s transient window per process) × 3 processes (1 + 2 restarts).
- Rust runs cameras in its in-process feature runner, not a separate exe. Stop is "applied on the next tick" rather than via a signal file.
- The 0.48 audit added a zero-selected refusal before Play. WPF refuses zero selected at **save**, and the controller exits with ERROR "Select at least one camera in the launcher." (`CT:973`).

## Open questions / UNVERIFIED
- **UNVERIFIED:** whether WPF users ever saw an ERROR → restart cycle in practice. `logs/camera-service-history.json` from a live WPF session was not examined.
- **UNVERIFIED:** whether the runtime availability byte (`RuntimeAvailabilityRva`) can be reset by the game on mode transitions. WPF never re-asserts it after LIVE, and Rust 0.46 live passed without re-asserting it.
- The meaning of mode indices 0..12 is not documented in the WPF sources. All 13 are patched, so no per-mode knowledge is needed.
