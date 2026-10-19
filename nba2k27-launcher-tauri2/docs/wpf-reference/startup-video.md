# Startup video (launch intro) — WPF reference
Source build: WPF quality-pass-qa-20261010 (frozen). Last researched: 2026-10-17.

Path aliases used in citations (all relative to `C:\Users\carrn\CodexProjects\2kconversiontool`):
- `WR` = `nba2k27-launcher-wpf/release/quality-pass-qa-20261010/App/Reference`
- `VH` = `nba2k27-mod-launcher/video-helper` (C# source of `ControllerSkip.exe`, `GameVolume.exe`, `GameWindow.exe`, shipped as `WR/runtime-resources/video/*.exe`)
- `VP` = `nba2k27-version-proxy/src`
- `HOST` = `nba2k27-launcher-wpf` (WPF C# host + `worker/host.cjs` Electron shim)

## Summary
- **What the frozen WPF build actually does:** an **external fullscreen MP4 player window** shown over the game after Play. It does **not** replace, patch or stage the game's native startup movies, and it does not touch game memory (`WR/src/renderer/app.js:157`; `nba2k27-mod-launcher/README.md:39-58`). The game loads independently behind it.
- Pieces: MP4 library (`WR/src/core/mp4-library.js`, `mp4-metadata.js`), overlay window (`WR/src/core/video-overlay.js`, `video-overlay-page.js`, `video-overlay.html/.css`, `video-overlay-preload.js`), three read-only/audio helpers (`GameWindow.exe` wait-for-window, `GameVolume.exe` volume follow + temporary game mute, `ControllerSkip.exe` XInput skip) wired in `WR/src/main.js:1947-1952`.
- Flow: Play -> (if enabled and a valid MP4 is selected) launch game -> wait (non-blocking) for a visible game window stable 250 ms -> frameless always-on-top fullscreen window plays the MP4 at the game's Windows mixer level, game audio temporarily muted -> ends on video end / click / Enter / Esc / Space / Xbox A, B, Start / error / 10m15s watchdog -> restore game audio -> focus the game.
- Everything native (`presentation-manager.js`, `presentation-session.js`, `native-movie-codec.js`, `movie-import*`, `startup-transition-builder.js`, version.dll `startup_movie_test.hpp`, `startup_presentation_sep11.cpp`, `startup_blank_accelerator_sep11.cpp`) is **retired/unwired** in this build (README 0.37.82: "Removed the old native movie import/conversion UI, movie replacement alias, memory-patching startup helper", `nba2k27-mod-launcher/README.md:50-52`). Only `shared-presentation.js` remains, for recovery of old temporary aliases. Documented below as history (Sub-feature D) so nobody re-ports it by mistake.
- `startup-splash.html/.css` is the **launcher's own** "Starting 2k Launcher" loading window, unrelated to the game intro (Sub-feature E).
- Most likely to break in a port: (1) showing the intro before the game window exists or blocking launch on it (WPF waits asynchronously, `WR/src/main.js:1811-1812`); (2) audio: start at 25 % (not 100 %), follow the game session level, temporarily mute only the game's audio sessions and always restore on every exit path (`WR/src/core/game-volume.js`, `VH/GameVolume.cs`); (3) skip input guards (700 ms opening guard, fresh press after neutral, 200 ms release) and not stealing/injecting input.

## Key source map
| Concern | file:line |
|---|---|
| Home "Startup video" toggle + selector | `WR/src/renderer/index.html:101` |
| Presentation page "Startup videos" fold, table, buttons, footnote | `WR/src/renderer/index.html:256-302` |
| Library rendering, actions, strings | `WR/src/renderer/app.js:118-180` |
| Toggle save + toasts | `WR/src/renderer/app.js:2773-2793` |
| Setting default `startupVideoEnabled:false` | `WR/src/core/paths.js:28` |
| IPC `presentation:*` handlers | `WR/src/main.js:969-1006` |
| settings:save guard while importing | `WR/src/main.js:954-957` |
| Library object construction + directory | `WR/src/main.js:80` |
| MP4 library (import/select/rename/remove/preview) | `WR/src/core/mp4-library.js:10-107` |
| MP4 validation (codecs, size, duration) | `WR/src/core/mp4-metadata.js:4-84` |
| Launch integration (check, skip, show after launch) | `WR/src/main.js:1688-1714`, `:1810-1812`, `:1830` |
| Overlay construction (helpers, onReturn, logs) | `WR/src/main.js:1947-1952` |
| Overlay window lifecycle | `WR/src/core/video-overlay.js:5-65` |
| Overlay page (volume, skip, error) | `WR/src/core/video-overlay-page.js:1-25`, `video-overlay.html:1-3`, `video-overlay.css:1` |
| Wait for game window | `WR/src/core/game-window.js:3-17`, `VH/GameWindow.cs:17-41` |
| Volume follow / temporary game mute | `WR/src/core/game-volume.js:6-52`, `VH/GameVolume.cs:92-196` |
| Controller skip | `WR/src/core/controller-skip.js:3-23`, `VH/ControllerSkip.cs:14-48` |
| Cleanup on game end / launcher exit | `WR/src/main.js:2074`, `:2246`, `:2346` |
| WPF host window shim (topmost/fullscreen/mute/file URLs) | `HOST/worker/host.cjs:33`, `:47`, `:54`; `HOST/NativeWindow.cs:270-287`, `:326-332`; `HOST/NativeBridge.cs:145`, `:154` |
| Legacy alias recovery | `WR/src/core/shared-presentation.js:42-74`, `WR/src/main.js:2049-2051` |
| Retired native route | `WR/src/core/presentation-manager.js`, `presentation-session.js`, `movie-import.js`, `native-movie-codec.js`, `startup-transition-builder.js`, `VP/startup_*.cpp`, `VP/startup_movie_test.hpp` |
| Launcher splash window | `WR/src/main.js:860-872`, `WR/src/renderer/startup-splash.html:1-20` |

---

### Sub-feature A: Startup video library (Presentation page)

#### 1. User-visible behavior
- Nav page **"Presentation"** (`WR/src/renderer/index.html:45`), intro "Manage cameras and startup videos for your next session." (`:70`). Fold `#presentationVideosSection`: title **"Startup videos"**, subtitle "Choose intro playback for Play", button **"Open folder"** (`:289-290`).
- Table header: "Video name" | "Length / size" | "Actions" (`:294`). Footer button **"+ Add MP4"** and footnote: "Fullscreen MP4 playback · Click or Xbox A/B to skip · H.264, AV1 or VP9 + AAC · Up to 10 minutes / 2 GB" (`:296-299`).
- Empty library text: "Add an MP4 to start your video library. Choose it on Play to use it at launch." (`WR/src/renderer/app.js:128`).
- Each row: poster badge (`2K<yy>` if the name contains e.g. "2K12"/"NBA 2K12", else "VIDEO"), name, meta "`<round(sec)>` sec · `<bytes>`", actions **Preview**, **Edit**, **Remove**; selected row has class `selected` (`:129-155`).
- Edit opens dialog `#presentationRenameDialog` ("Video name", maxlength 120, Cancel/Save) (`WR/src/renderer/index.html:257-263`; `app.js:145-147`, `:174-180`). Toast "Video renamed.".
- Remove asks `confirm('Remove "<name>" from the video library? The original imported file is not deleted.')` (`app.js:149`); deletes the library copy `mp4/<id>.mp4`; toast "Video library updated." If the selected video is removed, selection becomes none (`WR/src/core/mp4-library.js:90-92`).
- Preview: plays in the same overlay window immediately (no game wait, no game mute, focus returns to the launcher) (`WR/src/main.js:993-1000`; `video-overlay.js:42`, `:59`). Errors toast.
- Add MP4: file dialog title "Add startup MP4", filter "MP4 videos" (*.mp4) (`WR/src/main.js:978`). Progress panel `#presentationImportProgress` with bar, message and **"Cancel import"** (`index.html:293`; `app.js:2791-2792`). Progress messages: 0 % "Checking MP4…", 20 % "Copying MP4 into Library / Videos…", 100 % "MP4 added. No conversion needed." (`WR/src/core/mp4-library.js:60`, `:64`, `:72`). Toast "MP4 added to the library.". The first video added becomes selected if none is selected; **adding never enables startup playback** (`mp4-library.js:70`; `WR/src/main.js:983`).
- Detail line `#presentationMovieDetail`: selected-movie error, else (old MOV present) "Older game-format MOV videos are no longer used. Add the original MP4 files to this library.", else "Videos stay in Library / Videos. Playback does not replace or patch the game’s startup movies." (`app.js:157`).
- Error strings (`WR/src/core/mp4-library.js`, `mp4-metadata.js`): "Choose an .mp4 video. Native game .mov files are no longer supported."; "Choose a regular MP4 file smaller than 2 GB."; "Choose an MP4 up to 10 minutes long."; "Unsupported MP4 video|audio codec: X. Use H.264, AV1 or VP9 video with AAC audio (or no audio)."; "This MP4 has no supported video track."; "This is not a complete MP4 video."; "Close the video preview before changing the library."; "Close NBA 2K27 before changing startup videos."; "A video operation is already in progress."; "Wait for the video operation to finish."; "Video not found. Refresh the library."; "Enter a video name between 1 and 120 characters."; "The selected MP4 is missing. Add it again or choose another video."; "The MP4 library could not be read. Restore mp4-library.json before editing it."; "The video library must use regular folders, not links.".
- "Open folder" opens the library folder (creating it) (`WR/src/main.js:1002-1006`).

#### 2. Timing and gating
- All edits refused while the game runs or the preview is open (`mp4-library.js:38-41`); also refused while a Play launch is in progress ("Wait for game launch to finish.") (`WR/src/main.js:977`, `:980`, `:988`, `:995`).
- One operation at a time (`busy`) (`mp4-library.js:55`, `:80`). Import cancel via AbortController, checked after copy (`:68`). Play refused while importing: "Wait for the startup video import to finish or cancel it before Play." (`WR/src/main.js:1690`). Settings changes to `modsRoot`/`gameRoot`/`startupVideoEnabled` refused while importing (`:955`).
- If the mod profile changed during the file dialog: "The mod profile changed. Please add the video again." (`:981`).
- Validation runs at import, after copy, at select and at preview (`mp4-library.js:61`, `:66`, `:89`, `:105`), and at every view (`:48`).

#### 3. Mechanism
- Directory: organized storage `<root>/Library/Videos` else `%userData%/videos` (`WR/src/main.js:80`; `WR/src/core/launcher-storage.js:21`). Files `mp4/<uuid>.mp4`; manifest `mp4-library.json` (`mp4-library.js:16`, `:26`).
- Manifest schema: `{schema:1, selected:<uuid>|null, items:[{id, name, durationSeconds, format:"mp4", codec:"H.264"|"AV1"|"VP9"|joined, overlayReady:true, bytes}]}` (`mp4-library.js:28`, `:69`; `mp4-metadata.js:53`, `:82`). Validated: unique UUIDs, names 1-120 chars, selected must exist (`mp4-library.js:28`). Saved atomically (temp `wx` + rename) (`:31-37`).
- Import copies with `COPYFILE_EXCL` (no conversion); uncommitted copy removed on failure (`:65`, `:75`).
- MP4 checks (`mp4-metadata.js`): extension `.mp4`; regular non-link file 24 B..2 GiB; top-level boxes walked (<=10000); `ftyp` required; one `moov` <= 16 MiB; `mvhd` duration >0 and <= 600 s; every `vide` track sample entry in {avc1, avc3, av01, vp09}, every `soun` entry `mp4a`; at least one supported video track.
- `legacyPresent` = a `startup.mov` file exists in the library dir (old native format) (`mp4-library.js:51`).

#### 4. Build profiles
- Build-independent (no game memory). Works on any game build.

#### 5. Rust must match
- [ ] Labels: "Startup videos", "Choose intro playback for Play", "Open folder", "+ Add MP4", table headers, footnote, row actions "Preview / Edit / Remove", rename dialog "Video name".
- [ ] MP4 only (H.264/AV1/VP9 + AAC or silent), <= 2 GB, <= 10 min; copied once into Library/Videos/mp4/<uuid>.mp4; no conversion.
- [ ] First import auto-selects only when nothing is selected; importing never turns on the Startup video toggle.
- [ ] Remove confirm text exact; removing the selected video leaves no selection.
- [ ] All edits refused while NBA 2K27 runs, while previewing, or while launching.
- [ ] Old `startup.mov` present -> show the "Older game-format MOV videos are no longer used…" line; never convert/delete it.

---

### Sub-feature B: Play integration (enable toggle, selection, launch)

#### 1. User-visible behavior
- Home (Play) page row: label **"Startup video"** with checkbox `#startupVideoEnabled` and select `#presentationVideoSelect` (aria "Startup video") (`WR/src/renderer/index.html:101`).
- Default off (`WR/src/core/paths.js:28`). Checkbox is **disabled** unless the selected video validates (`movie.info`) or while importing (`app.js:158-159`). Toggle toasts: "Startup video enabled for the next launch." / "Startup video disabled for the next launch." (`app.js:2773-2779`); invalid type -> "Invalid presentation preference." (`WR/src/main.js:957`).
- Select lists library items; empty library shows one disabled option "No movie selected" (or "Current startup video" if a legacy movie is present) (`app.js:123-126`). Changing it selects the video; toast "Startup video selected." (`app.js:167-172`).

#### 2. Timing and gating
- At Play (`WR/src/main.js:1706-1714`): if `startupVideoEnabled===true`, read the library view; any error or no valid selected MP4 -> `console.warn('Startup video skipped: …')` and Play continues ("Optional media failures must not prevent the game from launching", `:1709-1712`). When disabled, the library is **not inspected at all** (`:1709-1711`).
- Before launching: `videoOverlay.finish('launch-failed')` closes any leftover window; legacy alias recovery runs (`:1713-1714`).
- After `launchWithoutEac()` returns, `showWhenGameReady` is called **without await** ("Do not block provider readiness while waiting for a visible game window", `:1811-1812`). Errors only logged.
- Launch failure -> `videoOverlay.finish('launch-failed')` (`:1830`). Game ended (session cleanup) -> `finish('game-ended')` (`:2074`). Launcher exit -> `finish('launcher-exit')`, bounded 2500 ms (`:2246`, `:2346`).

#### 3. Mechanism
- Setting key `startupVideoEnabled` (boolean) in launcher settings; selection lives in `mp4-library.json` `selected`.

#### 4. Build profiles
- Build-independent.

#### 5. Rust must match
- [ ] Toggle disabled unless a valid MP4 is selected; default off; toasts as above.
- [ ] Play with the toggle off never reads the video library.
- [ ] Missing/invalid video never blocks Play; it is skipped with a log line.
- [ ] Intro wait starts only after the game process launch and never delays provider/helper startup.

---

### Sub-feature C: Overlay playback (window, audio, skip, finish)

#### 1. User-visible behavior
- Frameless black fullscreen window, always-on-top level `screen-saver`, no taskbar entry, on the display containing the launcher window (else primary) (`WR/src/core/video-overlay.js:33-36`, `:44`). Title "2K Startup Video" (`video-overlay.html:2`).
- Video `object-fit: contain`, pointer cursor (`video-overlay.css:1`). Bottom hint bar: "2K LAUNCHER" · **"Click · Enter · Esc · Controller A/B to skip"** · "NBA 2K27 loads independently during Play" (`video-overlay.html:3`).
- Error: "Unable to play this MP4. Click or press Escape to return." shown, auto-closes after **2500 ms** (`video-overlay.html:3`; `video-overlay-page.js:10`, `:25`).
- Skip inputs (all ignored for the first **700 ms**): mouse `pointerup`; `Escape`/`Enter`/`Space` on keyup (keydown swallowed); Gamepad API buttons 0, 1, 9 (A, B, Start) requiring a neutral state first, then press, then **200 ms** after release; plus the native XInput helper with the same rules (`video-overlay-page.js:2`, `:11-23`; `VH/ControllerSkip.cs:14-23`, `:44`). Input is observed, never intercepted or injected — it can also reach the game (`video-overlay-page.js:20`; README `nba2k27-mod-launcher/README.md:46-48`).
- End of video -> close. After close (non-preview, not game-end/launch-failure/launcher-exit) the **game** is focused (`blacktopTools().focusGame`), else the launcher; preview focuses the launcher (`video-overlay.js:59`; `WR/src/main.js:1951`).

#### 2. Timing and gating
1. `waitForGameWindow` spawns `GameWindow.exe --wait <launcherPid> <gamePath>`; JS timeout **121000 ms**; helper loop 50 ms, own limit **120000 ms**; ready when the exact-path NBA2K27 process has a visible, non-minimized, unowned top-level window with client area >= **640x360** that stays the same HWND for **>= 250 ms** -> prints `{"event":"window-ready","pid":N}`; also `game-exited`, `cancelled`, `timeout` (`WR/src/core/game-window.js:3-15`; `VH/GameWindow.cs:17-40`). A newer `showWhenGameReady` aborts the older wait (`video-overlay.js:14`). Log events `waiting-for-game-window`, `game-window-ready`, `video-wait-cancelled`.
2. `show()`: validate path is a regular `.mp4` ("The startup MP4 is missing or invalid."); if a window already exists just focus it (`video-overlay.js:26-29`).
3. Initial volume `{volume:0.25, muted:false, source:'fallback'}` (`:31`); page also sets `video.volume=0.25` **before** assigning `src` (`video-overlay-page.js:3-4`).
4. Watchdog **615000 ms** (10 min 15 s) -> `finish('timeout')` (`video-overlay.js:40`).
5. Start the volume monitor **before** loading the page (awaits up to 750 ms for a first sample) (`:42`; `game-volume.js:50`).
6. Load page, `show()`, `setFullScreen(true)`, `focus()`, log `shown` (`video-overlay.js:44`). Then start `ControllerSkip.exe --watch <launcherPid>` (`:45`).
7. Page fetches `startup-video:source` (file URL) and volume, applies volume, sets `src`, `play()`; failure -> error panel -> finish('error') after 2500 ms (`video-overlay-page.js:25`).
8. `finish(reason)` (`video-overlay.js:48-64`), idempotent, order: bump generation and abort a pending wait -> clear watchdog -> log `closed` -> **mute the window's audio first** (`setAudioMuted(true)`) -> stop volume monitor (restores game audio) -> stop controller helper -> focus (race with **1500 ms**) -> destroy window. Reasons seen: `ended`, `skip`, `error`, `controller-skip`, `timeout`, `window-close`, `renderer-failed`, `load-failed`, `audio-helper-failed`, `launch-failed`, `game-ended`, `launcher-exit`.
- If the audio helper dies while it owns the game mute -> `finish('audio-helper-failed')` (`video-overlay.js:42`; `game-volume.js:23`).
- Logs: JSON lines in `Logs/video-overlay.jsonl` (`WR/src/main.js:1949`, `:1952`).

#### 3. Mechanism
- **Window/IPC**: Electron-style `BrowserWindow` with sandboxed preload exposing `startupVideo.source/finish/volume/onVolume` (`video-overlay-preload.js:2-6`). IPC handlers authorize the exact overlay webContents ("Unknown video window") (`video-overlay.js:8-12`). Navigation and popups denied (`:37`). CSP `media-src file: https://launcher-resources.invalid` (`video-overlay.html:2`).
  - In the WPF host, `setAlwaysOnTop` -> WPF `Topmost`; `setFullScreen` -> `WindowStyle.None` + `Maximized`; `setAudioMuted` -> command `audio-muted` (`HOST/worker/host.cjs:33`, `:47`, `:54`; `HOST/NativeBridge.cs:145`, `:154`; `HOST/NativeWindow.cs:326-332`). `file://` URLs are rewritten to `https://launcher-resources.invalid/__file/<token>` served with HTTP Range support (206 partial) for seeking (`HOST/NativeWindow.cs:270-287`).
- **GameVolume.exe** (`VH/GameVolume.cs`), started by `WR/src/core/game-volume.js:20`:
  - Args: `--duck <launcherPid> <gamePath>` for real launches (detached so it survives the launcher's job), `--watch` for preview (`game-volume.js:20`).
  - Every **200 ms** (duck) / **500 ms** (watch) enumerates all active render endpoints' audio sessions whose process is an `NBA2K27.exe` from the exact path; reports `{"event":"volume","found":true,"volume":min level (active sessions preferred),"muted":any}` when changed, else `found:false` (`VH/GameVolume.cs:102-153`, `:190-193`).
  - `--duck`: per game session (by instance id, max 64) takes a `MuteLease`: if not originally muted, `SetMute(true)` with a fixed event-context GUID; reports the **original** mute state back (so the video is not muted by its own duck); if the user unmutes, ownership is dropped and never re-muted (`:92-100`, `:130-141`). Never calls the volume setter (`:81`). Emits `game-audio-held`.
  - Restore on stdin EOF, parent exit or 615000 ms: each lease restored (up to 5 attempts, 100 ms apart) -> `game-audio-restored` / `game-audio-restore-failed` (`:154-162`, `:186-194`).
  - JS side: cached last live volume to `Data/startup-video-volume.json` `{schema:1,gamePath,volume,muted}` (atomic); used (source `cached`) before the game creates its session, else fallback 0.25 (`game-volume.js:12-15`, `:35-41`; `WR/src/main.js:1949`). Stop: close stdin, wait **2000 ms** (duck) / **800 ms** (watch); a ducking helper is **never killed** (restoration pending is tracked and blocks the next start: "Previous intro audio restoration is still pending.") (`game-volume.js:10`, `:44-49`).
  - Page applies live volume/mute updates (`video-overlay-page.js:5-7`).
- **ControllerSkip.exe** (`VH/ControllerSkip.cs`): `--watch <launcherPid>`; polls XInput (`xinput1_4`, fallback `xinput9_1_0`) slots 0-3 every **16 ms**; buttons mask `0x3010` (A 0x1000, B 0x2000, Start 0x0010); edge rule: neutral first, press at >= 700 ms since start, fires 200 ms after release; prints `{"event":"skip","slot":N}` and exits; also `controller` connect events; self-limit 615000 ms (`:14-23`, `:37-46`). JS stop: close stdin, wait 600 ms, then kill (`WR/src/core/controller-skip.js:17-22`). First `skip` row triggers `finish('controller-skip')` once (`:14`).

#### 4. Build profiles
- Build-independent. Supported on every game build WPF ran (no game memory).

#### 5. Rust must match
- [ ] No intro window until the exact game process has a visible unowned >= 640x360 window stable for 250 ms; give up after ~120 s silently.
- [ ] Window is frameless, black, topmost, fullscreen on the launcher's display, with the hint bar text exact.
- [ ] Initial volume 25 % (cached last game level if known), then follows the game's Windows session level/mute live; never 100 % at start.
- [ ] During a real launch the game's audio sessions are temporarily muted (only if they were not already muted) and restored on every finish path, including launcher crash/exit (helper outlives the launcher); a user unmute is not fought; master/other apps untouched; volume levels never written.
- [ ] Preview does not mute the game and returns focus to the launcher.
- [ ] Skip: click, Enter, Esc, Space, Xbox A/B/Start; ignored for the first 700 ms; controller needs neutral -> press -> 200 ms after release; input not intercepted.
- [ ] Decode/play error shows "Unable to play this MP4. Click or press Escape to return." and closes after 2.5 s.
- [ ] Finish order: silence video -> restore game audio -> stop controller watcher -> focus game (<=1.5 s) -> destroy window. Game end / launch failure / launcher exit never focus the game.
- [ ] Hard watchdog closes the intro after 10 min 15 s.

---

### Sub-feature D: Retired native startup movie route (history; NOT wired in this build)

Do not port this unless the user asks again. Evidence it is unwired: `main.js` requires `Mp4Library`/`VideoOverlay`/`SharedPresentationSession` only (`WR/src/main.js:74-81`); nothing requires `presentation-manager`, `presentation-session`, `movie-import`, `startup-transition-builder` (grep of `WR/src`); the shipped runtime has only `video/ControllerSkip.exe`, `GameVolume.exe`, `GameWindow.exe` (`WR/runtime-resources/manifest.json`), no `NBA2K27StartupPresentation.exe` / `NBA2K27StartupBlankAccelerator.exe`; the shipped `WR/runtime-resources/session-loader/version.dll` contains no `startup-movie-test` / `LOCAL_NATIVE_CLIP` strings (byte grep, 2026-10-17). Retirement: `nba2k27-mod-launcher/README.md:50-55`.

What it was (for reference only):
- **Native movie file** `presentation/startup.mov` (2K native format, header versions 2/3/4, block chain from 0x1000 or 0x4000, 16 KB..2 GB) served to the game via the provider as logical path `movies/era_intro_current.mov` (`WR/src/core/presentation-manager.js:6-73`; `presentation-session.js:6`).
- **MP4 -> native conversion** in a hidden renderer: 960x544 @ 30 fps all-intra DCT codec, quality ladder 75..15 to fit 107168 B/frame, 48 kHz stereo PCM 1600 samples/frame, blocks of <= 10 frames / 256 KiB, max 18000 frames, <= 512 MB input, 1 h deadline (`WR/src/core/movie-import.js:9-27`; `movie-import-page.js:6-23`; `native-movie-codec.js:33-40`).
- **Trigger helper** `NBA2K27StartupPresentation.exe --wait-and-trigger <receipt> [phase.txt]`: waits <=120 s for the game, <=30 s for the module, requires build `6AA2013D:36558000`, original 7 bytes `C6 86 A0 0E 00 00 01` at RVA `0x3051647` and 3 anchors, and that no menu exists yet (menu table `0xAC77660`, 31×64 B); patches operand byte RVA `0x305164D` 1->0 (VirtualProtectEx + flush), watches up to 30 s (startup state at `0xA201970`), restores 0->1; receipt schema `startup-presentation-v1` `{status: armed|restored|process-exited|rejected|timeout|restore-failed, pid, armed, restored, message}` (`VP/startup_presentation_sep11.cpp:11-131`). JS side `PresentationSession` spawned it detached, monitored provider reads of the movie for up to 90 s at 250 ms, wrote per-attempt `launch.json`/`result.json` evidence, never killed a helper while the game ran (`WR/src/core/presentation-session.js:9-113`).
- **Blank-page accelerator** `NBA2K27StartupBlankAccelerator.exe --wait-and-accelerate <receipt>`: same build guard + 16-byte anchor at RVA `0x74E00`; reads state `0xA201970` / float timer `0xA201974`; per state thresholds 1:5.0, 2:0.5, 3:4.0, 4:0.5, 5:4.0 s; writes the timer to the threshold once per state (max 5), stops at state 6, 60 s limit; receipt `startup-blank-accelerator-v1` (`VP/startup_blank_accelerator_sep11.cpp:12-254`). Normal path 3->4->5 = 8.5 s of blank dwell removed; watched run: 3 writes, mask 56 (`nba2k27-mod-launcher/STARTUP_VIDEO_OFFLINE_REPORT.md:15-21`, `:76-78`).
- **Visual neutralization** (staged archives `bootup.iff`, `gooey_scenes/start_up_bg.iff`, `gooey_scenes/fe_loading_screen_bg.iff`, hashes listed) and the conclusion that one stock 2K27 splash always remains because it is cached before the provider registers (`STARTUP_VIDEO_OFFLINE_REPORT.md:8-15`, `:41-56`, `:76`).
- **Startup transition builder**: rebuilt `gooey_scenes/startup_boot_transition.iff` with a custom 2048x1024 BC7 background and zeroed overlay model counts, stock member SHA-256s pinned (`WR/src/core/startup-transition-builder.js:9-123`). Not referenced anywhere.
- **version.dll test lane** (`VP/startup_movie_test.hpp`, compiled only with `NBA2K27_STARTUP_MOVIE_TEST`, `VP/version_proxy.cpp:17-18`, `:2040-2042`): INT3 breakpoint on the constructor store at RVA `0x304F1A7` of the **older** build `6A974D13:368C3000`, writes `obj+0xEA0` = 0 to enable playback, configured by `startup-movie-test.cfg` beside the exe. Test-only.
- **Shared alias recovery (still live)**: `SharedPresentationSession.restore()` deletes a leftover `presentation/launcher_shared_startup_<uuid>.mov` only if its SHA-256 matches the journal `%userData%/presentation/shared-movie.json`, and only when the game is not running; runs at startup, at Play, on launch failure, at session cleanup and at shutdown (`WR/src/core/shared-presentation.js:42-57`; `WR/src/main.js:1714`, `:1839`, `:2049-2051`, `:2078`, `:2270`). `prepare()` is never called in this build.
- **Rust historical artifact**: `nba2k27-launcher-tauri2/artifacts/startup-intro-integration-20261016.md` describes the Rust **MP4 overlay** design (not the native route): same timing rules (window >= 640x360 stable 250 ms, 700 ms opening guard, 200 ms release, quiet 25 % fallback, mute-only game sessions, restore before destroy) plus stricter extras (wait for HTML `canplay` before muting, Alt-Tab cancels, authenticated audio guardian exe `startup-audio-guardian.exe`).

#### 5. Rust must match
- [ ] Do not stage/patch native movies, archives, timers or code for the startup intro; the WPF release behavior is the external MP4 overlay only.
- [ ] If legacy `launcher_shared_startup_*.mov` aliases exist from very old WPF releases, removal must be hash-verified and only with the game closed (or simply never touch them).

---

### Sub-feature E: Launcher startup splash (not the game intro)

#### 1. User-visible behavior
- Small 420x240 non-resizable window, background `#0c1922`, title "Starting 2k Launcher", shown on `ready-to-show` only if the main window is not visible yet (`WR/src/main.js:860-871`).
- Content: app icon 48 px, heading "2k Launcher", text "Preparing your workspace…", spinner (0.9 s rotation; 2.5 s with reduced motion) (`WR/src/renderer/startup-splash.html:7-17`; `startup-splash.css:1-10`).

#### 2-4.
- Pure launcher UI; no game interaction. Build-independent.

#### 5. Rust must match
- [ ] Optional: a small "Starting 2k Launcher" splash only until the main window is visible.

---

## Known divergences in current Rust port (brief, not re-verified)
- Rust has `src/startup_mp4.rs`, `startup_video_*.rs`, `video_library*.rs`, `startup_audio_guardian.rs`; status "Startup MP4 import/preview/select/play: UNV, decode checks only" and "rename/remove/length: UNV (0.48)" (`nba2k27-launcher-tauri2/artifacts/feature-gap-inventory-20261017.md:95-96`).
- The Rust design deliberately adds gates WPF does not have (HTML `canplay` before mute, foreground/Alt-Tab cancel, authenticated helper, no-intro-without-exact-PID binding) (`nba2k27-launcher-tauri2/artifacts/startup-intro-integration-20261016.md:35-42`). WPF shows the window and starts audio ducking before playback starts and does not cancel on Alt-Tab.
- WPF stores the library at `Library/Videos/mp4-library.json`; Rust reads the same IDs via `legacy_saved_media` (`startup-intro-integration-20261016.md:21`).

## Open questions / UNVERIFIED
- Whether the WPF window ever lost topmost to an exclusive-fullscreen game: WPF README only recommends borderless and warns "exclusive fullscreen can cover another window" (`nba2k27-mod-launcher/README.md:45-46`). **UNVERIFIED** live.
- `video-overlay-page.js` reads gamepads via the Gamepad API inside WebView2; whether this works in the WPF WebView2 host or only the native `ControllerSkip.exe` fires — **UNVERIFIED** (both are wired).
