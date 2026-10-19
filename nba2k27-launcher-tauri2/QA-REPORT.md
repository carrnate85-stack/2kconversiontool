# QA 0.53.1: selector launch-stop fix

0.53 live: Play stopped with "A different signatureconfigs_all.iff mod is installed; it was left untouched." The installed complete selector mod (mods\char, 2026-10-10) is not in WPF's ownership state (that lives in WPF's own portable folder), so the new WPF multi-config rule stopped every launch. 0.53.1: a complete foreign selector bundle is served unchanged (0.52 behaviour) with a note in the selector source; a partial conflicting bundle still stops (WPF). Test: a_complete_foreign_selector_bundle_is_served_unchanged_and_play_continues.

# QA 0.53: outfit carry session fix, auto-random + flicker, ball prep speed, safety review, UI port, previews, launch speed

0.52 live results (2026-10-18): Random all faster (LIVE). Failed: outfit carry (provider session ended at match load: its own status-file write returned Access denied and was treated as fatal, cancelling the outfit lease), automatic random never fired, PLAYER list flickered every second, ball prepare slow (~7-8 s), WNBA Freestyle (game loads roster via VCRESOURCE archive path; 0 hooked-device requests) - WNBA parked.

In 0.53:
- Launch session: a status-file write failure at match load no longer ends the launch (carry-hold-fix).
- Overlay: automatic random fires at selection (WPF autoRandomSelection); background lineup reads no longer re-render or disable the PLAYER list (WPF 0.37.73 renderer fix); no-op overlay publishes skipped.
- Ball: V2 prepare matches WPF 4-samples/100 ms flow (~7-8 s -> ~0.4 s est.).
- Safety review: 10 fixes (rules/launch deadlock, Freestyle half-publication, camera journal budget, launch-blocking Jukebox receipt, torn outfit report, ...).
- Mod install: new-file rename can no longer replace a racing file; plan lock held through commit.
- UI port: every page within 0.1-5.9% pixel mismatch of WPF (dark/light).
- Previews: Prepare all previews 25,529/25,529 offline, mannequin shoes, animation preview.
- Launch speed: prepare steps in parallel (~6-11 s saved); V2 readiness probe present but off by default.
- Game-update resilience: per-feature Mapped / Needs mapping in Settings > Advanced.

# QA 0.52: outfit carry/Random speed, arena timing, WNBA override fix, Jukebox parity

0.51 live results (2026-10-17): LIVE — player dropdown finds players, outfits + Random apply, court ball model-confirmed (Moneyball), scoreboard, arena after waiting at selection, hang capture hotkey (keyboard-poll fallback) + button, camera status card. Failed: outfit carry (V2 worker never armed: bridge refused silently, likely a launcher-named process during game start), Random too slow, arena missed when starting quickly, arena picker slow/odd, WNBA Freestyle (override armed but game never asked the hooked device for roster/learn2k_roster.iff: 0 provider queries).

In 0.52 (worker DLL 645984B76691111224089F73A11B0BD8A9F8C7BECC22BB8E826A6EC4BB2CFF7C):
- Outfit carry: bridge retries "other launcher present" every 2 s for 10 min; any other refusal writes STATE=FAILED with the exact check (permit-timeout / lease-mismatch / protected-present: names); Rust treats an unarmed worker as a visible retryable wait, not a hold.
- Random all / auto-random: one native batch (compose all, write back to back, one combined refresh wait, per-slot readback); status shows write/refresh ms.
- Arena: prepares on the first selection check; F8 badge shows Preparing until arena/ball/scoreboard are ready; F8 arena picker uses the ball/scoreboard panel (user-requested divergence from WPF), opens instantly.
- WNBA Freestyle (experimental, off by default): case-insensitive forced-override matching + roster/learn2k path trace in the proxy.
- Jukebox parity: in-game cover art (file-only route), automatic crash restore, WPF batch-add/filename/.aac/new-playlist behaviour.

# QA 0.51: Blacktop fixes from the 0.50 live test + experimental WNBA Freestyle

0.50 live results (2026-10-17): LIVE — overall WPF look approved; arena and scoreboard picks; all Blacktop removals incl. reloads/second match; Freestyle Jukebox playlist (song recognized; FREESTYLE_REDIRECT_OPEN/READ in trace); Alt-Tab return; camera status card; hang capture via Settings button. Failed: player dropdown/auto-random (outfit capture required game mode 6; V2 reported 0/4 at selection), overlay flicker, Hide overlay button, court ball unchanged (wrong record), Ctrl+Shift+F12 hotkey. A stray dev launcher process held outfits for one game (side sessions now barred from running launcher EXEs).

In 0.51:
- Outfits: no game-mode requirement at selection (WPF BlacktopTool.cs parity; match still requires 6); retained lineup for the dropdown; scan-on-open; real empty/error reasons; live capture ~1 ms with layout hint.
- Overlay: hides at match start and when leaving Blacktop (two readings), returns at the next selection; Hide sticks until F8/next selection; F8 presses survive failed reads; foreground classification/debounce fix.
- Ball: swaps the record the court ball actually loads, following the game's ball setting (WPF method).
- Hang capture: WPF-style key-watch fallback for Ctrl+Shift+F12; state shown in Settings and Logs\game-capture-hotkey.log.
- WNBA in Freestyle (EXPERIMENTAL, off by default, Unlocks page): merged NBA+WNBA learn2k roster built locally from the installed game (key stays local), served via the 4th override slot, WPF category write. The withdrawn 0.41 loader swap stays disabled.
- Plus parity-session work landed since 0.50 (editors read-only pieces/audio preview, previews, settings).

# QA 0.50: 0.49 live fixes + WPF-reference parity work (snapshot build)

0.49 live results (2026-10-17): LIVE — Jukebox import + in-menu playback; scaler groups retry (coaches 4/4, cheerleaders 20/20, arena 159/159, status active); arena and scoreboard picks at Blacktop selection; Blacktop removals (first match); Random all eventually applied. Editor probe: staff/seasons layouts and V2 serializer fields proven live. Failed: outfit coordinator entry race at selection (no players, no auto-random); Random all very slow; overlay flicker; ball refused ("active gameplay balls exist"); Freestyle playlist locked (fix landed after the 0.49 build). WNBA: Freestyle loads its own Learn2K roster (334 teams, 0 WNBA); stock TKU_WNBA path exists; VCBLOWFISH key material captured read-only for the merge research.

In 0.50 (built from a frozen snapshot while parity sessions continue):
- Outfits: List retries after an error (entry race self-heals); real error text; Random all fast path (one checkpoint per read, layout hint; 2,198 -> 408 reads per lineup in fixture), strict WPF Random-all semantics; auto-random WPF cadence.
- Overlay: focus judged by process like WPF; overlay/popups count as game; 350 ms hide debounce; timestamped transitions.
- Ball: WPF method (swap the court ball record's filename at selection).
- Freestyle playlist on V2 (version.dll V2 profile; DLL re-pinned).
- Plus whatever the WPF-reference parity sessions (UI port, editors, mods, Play/video, Jukebox, settings, previews, arena) had landed in compiling form at snapshot time; see their reports.

# QA 0.49: outfit carry, overlay WPF match, Blacktop reliability

0.48 live results (2026-10-17):
- Passed: scaler status now verifies players in a full NBA game; V2 roster/team reads; Blacktop drone/DJ/MC/crowd removal on the first match; Random clothing visible for 3 of 4 players; scaler groups initially applied.
- Failed: scaler character groups held after one refused write; arena/ball/scoreboard did not apply at Blacktop selection (flicker + held arena); crowd reappeared on the second match; clothes never carried into the match (worker treated the MANAGER_INSTANCE +0x78 interface pointer as the full object); Random all failed with a full lineup; player dropdown and overlay look not like WPF; overlay did not reappear after Alt-Tab.
- Editor live probe: player->team link and season layout proven; staff layout confirmed (6 shared coaches); rotations empty on that screen.

Fixes in 0.49:
- Scaler groups: a character whose data moves under its write is skipped and retried (at most 3 times per character), never holding the whole feature.
- Arena, ball, scoreboard: tolerate the Blacktop selection mode-field flicker; pre-publication changes are retryable.
- Presentation re-applies after the game's own end-of-match reset, on every Blacktop visit.
- Outfit carry follows WPF's version.dll logic (manager = interface - 0x78). Worker DLL rebuilt from source (toolchain reproduced the old DLL byte for byte first); final assets/native/version-outfits-v2.dll SHA256 272E0355DB6D49AF99F56870087270DE7A7BCF6BDE2202561652CBBFD1E8DEE2, pinned. Also WPF-parity items from artifacts/wpf-version-dll-audit-20261017.md: in-DLL two-piece check (D3), 3-sample match stability (D4), visible "carry disabled" reason (D2), legacy V2 log noted in support bundles (D6), producer immediate disk check (D9).
- Random all randomizes every readable slot and names a reason for each refusal; Random player chooser; compact live player dropdown.
- F8 overlay copies WPF's measured CSS and markup, including a WPF-style Edit outfit editor; Alt-Tab back restores it; no Enable checkboxes remain (arena/ball/scoreboard/outfits).
- Courtside celebrities follow "Arena personnel and crowd"; scaler status works in full NBA games.

# QA 0.48: overlay parity, V2 outfits, mod install, roster saves and more

0.47 live results (2026-10-17): before-Play steps 1-7 passed (branding, Mods restyle, scaler toggles, cameras, Blacktop switches, storage folder, one-click Play). In game: cameras active; scaler confirmed by read-only inventory (players 24/24, referees 4/4, coaches 4/4 x0.9506, cheerleaders 20/20, arena personnel 159/159); overlay auto-opens at player selection; V2 roster/team reads load. Failures: Blacktop drone/DJ/MC/crowd held (game mode-field flicker); ball/scoreboard F8 picks reverted (per-item Enable checkboxes); scaler status read "unverified" in a full NBA game.

In this build (each from its own session; see their reports and Backups\qa048-*):
- F8 overlay matched to WPF; per-item Enable checkboxes removed (picks at player selection apply directly; backend checks unchanged).
- Blacktop presentation waits through sub-millisecond mode-field blips and retries an interrupted drone/DJ/MC write.
- V2 Blacktop outfits enabled via the existing admission proof (version2OutfitCarryMapped), plus Random player / Random all players.
- Mod install from files, folder, ZIP and RAR (Windows tar.exe), with per-file Replace/Keep conflict review; refused while the game runs.
- Camera status card with Retry / Stop & restore; startup video rename/remove/length; Jukebox repair missing tracks / refresh bundle.
- Saved arena and scoreboard defaults apply automatically at launch; arena favorites in F8.
- V2 roster saves: player, team text and 52-control uniform Save & verify, limited to fields proven from the exact V2 exe.
- Scaler: courtside celebrities (PLAYER-class characters outside the rostered-player table) follow "Arena personnel and crowd"; the status can now confirm players during a full NBA game (observation accepts any mode; writes still only at Main/Freestyle/Blacktop).

Live checks required: see the 0.48 test list given to the user (mod install incl. RAR, camera controls, video/jukebox tools, saved defaults, overlay look and ball/scoreboard picks, Blacktop removals and crowd restore, outfits + Random, roster save plan, celebrity scaling, hang capture, second-launch speed).

# Scaler character groups + parallel gap work QA 0.47

**Scaler character groups (WPF parity).** Coaches, cheerleaders and arena personnel now follow the saved scale. The Scaler page shows three working toggles (default on, as in WPF), frozen into the launch plan.

How it works, mapped live and read-only on Version 2 and pinned to disk RTTI:
- Each MANAGER_INSTANCE is its own list node under the image sentinel 0xA409E90; the actor is at +0x110 and the state byte at +0x130.
- Classes: COACH 0x686C518, CHEERLEADER 0x6866488 and GENERIC 0x687C888 (descriptions at actor +0xF30, +0xED0 and +0xF30 respectively).
- All three classes' interface slot +0x190 is the same evaluator players use (0x6E4630). It tests description +0x7D bit 2 and then reads the shared input.

Writes:
- A selected character not already opted in gets that bit set (1 byte).
- Already-loaded cheerleaders and arena characters get one cache seed of original scale x shared input (8 bytes). This is exactly the native result: a natively opted-in referee caches 0.93937 = stock 0.98819 x 0.9506.
- Referees and players already carry the bit and are never touched.

Safety:
- Writes go to private read/write heap memory only, never the image.
- Each write has an exact journal intent.
- The full object chain is re-verified immediately before every write; a character that unloads stops the batch without a write.
- Ownership conflicts or write failures hold only the character groups; player scaling continues.

Note: at the 97% setting these groups reach 95.06% of their stock size (raw input 0.98 x 0.97), the same as WPF.

Live snapshot tool: `scripts/Read-CharacterScaleInventory.ps1` (QUERY_INFORMATION|VM_READ only).

Also in this build, from parallel sessions: Mods page WPF styling (bundled rows, sortable headers, icons, tag pop-up, Open/Folder); honest Unlocks controls and removal of the Prototype branding. See each session's report for details.

Live check required: a Quick Play NBA game with Scaler 97% and all groups on. Coaches, cheerleaders and courtside staff should look slightly smaller, and the Scaler status should list group counts. Then run the inventory script and confirm `sharedOptIn` counts for COACH, CHEERLEADER and GENERIC.

# Camera copy-on-write fix QA 0.46

LIVE RESULT (2026-10-17): PASSED. Cameras reached Active during startup (before loader-active, no wait at Main). User confirmed in Freestyle: camera names, order, tracking and height/zoom correct, and kept after re-entering from Quick Play.

0.45 live result (2026-10-17): held after 10 restored attempts with "Feature span is not committed readable non-executable data". The journal shows every attempt failed on the very first public write (registry slot 63) before any game byte changed; private initialization had succeeded. Because 0.45 now publishes at about 16 s (loader-armed), the game's camera .data pages were still copy-on-write (PAGE_WRITECOPY), which the writer's region check rejected. A later read-only VirtualQueryEx (QUERY_INFORMATION only) on the same game showed those pages as PAGE_READWRITE once the game had touched them; 0.44, which published later, had written 13 entries successfully.

0.46 accepts PAGE_WRITECOPY for game-image data pages only. WriteProcessMemory completes the copy, as the WPF helper's plain write did; readback verification is unchanged. Private memory still requires PAGE_READWRITE, and executable or guard protections are never accepted (new regression test). Everything else from 0.45 is unchanged. Pre-edit backup: `Backups/qa046-before-writecopy-20261017`.

# Camera activation QA 0.45

Fixes the failed 0.44 live camera test ("Camera menu changed after publication started; no further writes") by matching the retained WPF 0.1.0-wpf.24 camera behavior, verified from its packaged NBA2K27CameraController.exe source: no menu/gameMode gate, install once per game process as soon as the static camera tables validate, all-or-nothing transaction with RestoreAll on failure, and automatic retry.

- Activation starts at loader-armed instead of after loader-active plus the 20-second mod-loader settle gate (about 21 seconds earlier in the 0.44 session timing). Feature ticks run every 500 ms instead of 1000 ms, and the two-sample stable-menu wait is gone. You no longer have to stay at Main.
- Publication runs as one short transaction: one process-ownership scan and camera profile proof cover all its bounded writes (proof expires after 2 s). Each write still checks process liveness, retained identity, region kind and exact before-bytes, with durable intent and protection receipts.
- Any failure during publication restores every public edit made so far using the plan's tolerant ownership rules, then retries with the same retained allocation. A failed restore, or 10 consecutive restored attempts, still holds the feature.
- The game-owned native settings block (+0x20..0x30) and runtime-availability flags no longer count as foreign or transitional. In the 0.44 capture the game had rewritten exactly these after the partial write.
- A full stock reset republishes from a fresh capture re-planned into the same retained allocation, which must match the prepared private bytes exactly. The stale gameMode=4 at Main no longer matters to cameras; scaler gating is unchanged.
- The access-open retry window is 40 attempts (20 s at 500 ms), matching WPF's 20-second transient window.

Camera positions, tracking, height/zoom policy, allowlists and WNBA withdrawal are unchanged. Pre-edit files: `Backups/qa045-before-camera-activation-20261017`.

Live check required: launch QA 0.45 with cameras enabled. Status should reach Active during startup or by Main without waiting there. Then check names/order/tracking/height/zoom in Freestyle, return to Quick Play and re-enter.

# Camera preparation retry QA 0.44

Latest live result (2026-10-17): FAILED. Camera publication began, then held with "Camera menu changed after publication started; no further writes" before any mode-list descriptor update. At the returned Main menu, a read-only snapshot showed an empty wrapper table with stale gameMode=4, which current rearm checks reject. No follow-up runtime fix or 0.45 build was made. See CLAUDE-HANDOFF.txt and artifacts/qa044-camera-live-failure.json. The offline results below remain valid but do not establish gameplay success.

This isolated QA fixes the confirmed 0.43 camera failure: private camera initialization completed, but the final inactive-menu check returned NotReady and permanently held the feature before any public camera-list write. Fully initialized camera plans are now retained before that check. Returning to a stable Main or Quick Play menu reuses the same allocation and repeats private-byte, source-capture, build and public-ownership checks before publication.

Only a typed readiness loss with zero completed public writes can defer. The same guard is used for first publication and stock-reset rearming, including the check after durable write intent. A transition after any public write, a foreign value, a changed source/private allocation, a failed write, or a profile/I/O error still holds for the session. There is no rollback or retry of a partially published generation. No safety/profile/address gate was broadened. WNBA remains disabled.

Pre-edit files, including version metadata and documentation, are hash-checked in `Backups/qa044-before-camera-retry-20261017`; the complete prior QA checkpoint is `Backups/v0.43.0-workflow-layouts-20261017`. Main, the running game and user Data/Library are not modified by this update.

Validation passed 922 native tests (22 explicit/manual audits excluded) and 139 frontend tests. The isolated camera/scaler harness passed 39 fixture checks, and both explicit installed Version 2 disk audits passed with no process access or game-file writes. The six new regressions cover pre-publication menu transitions, transitions during intent recording, successful retry from another safe menu, post-write transitions, ownership/write errors, typed versus text-only errors, and reusing an initialized unpublished plan without another allocation. The packaged build passed all 119 UI checks, followed by three fresh-process repeats of all 119 checks, shared WPF file validators, browser HTTP isolation, and provider-worker cleanup. Compilation used the workstation's existing affinity workaround; packaged UI verification used normal affinity.

Exact candidate: `Builds/0.44.0/Portable/App/2k Launcher Rust Prototype.exe`, 35,346,944 bytes, SHA-256 `FD780885BEEEEB1DE38A4FC44A0E9DF38FF7A04360FAF78AB9329B6335F15DC7`. Evidence: `Builds/0.44.0/QA-VERIFICATION.json`, `artifacts/qa044-build.log`, `artifacts/qa044-camera-scaler-disk.log`, and `artifacts/qa044-verification.log`. Main remained SHA-256 `B86C1AAFA291DA5850DA93A90796F15A84EA57B225393F83591761D56316F958`. Fresh-process UI readiness was 271, 266 and 275 ms; these are not game-launch timings.

Live check still required: close the game and previous launcher normally, open QA 0.44, enable the desired cameras, launch and wait at Main until camera status is Active. Check the camera list, tracking and height/zoom in Freestyle; return to Quick Play and re-enter. If preparation is interrupted before publication, return to Main/Quick Play and verify it recovers instead of staying Held. Do not use this to recover a 0.43 provider already running in another session.

## Previous Play, Jukebox, camera/scaler workflow QA 0.43

QA 0.43 continues offline integration without installing over Main. Every tab restores the shared courtside banner, green icon, title, subtitle and version badge from the supplied WPF references. Play restores the compact ready card and three-column profile/Jukebox/startup-video controls. Its real playlist dropdown and enable switch use revision-checked saved-state updates without initializing music decoding or playback. Profiles restores the two-column saved-list/help layout, validates duplicate names and keeps active settings consistent during rename/removal. Removing a profile never deletes mod files. Mod library uses compact controls and a dense table while preserving expansion/scroll state; unavailable Open/Install backend actions are not represented by dead buttons.

Jukebox retains its playlist-first layout, distinguishes transient poll failures from action errors and reports artwork operations accurately. Its unverified Version 2 Freestyle mapping remains visibly unavailable. Camera configuration shows the selection count, rejects an enabled empty list, and separates stock-name reset from height/zoom reset. Scaler changes are local until Save size; unsupported legacy values can be repaired, including a clamped 100% value. Idle session monitoring uses five-second polling, while active launch/recovery and Advanced retain one-second updates. Advanced displays existing bounded launch-stage timings.

Settings can create a support ZIP from small launcher text/JSON diagnostics with bounded file/byte/depth limits. It redacts portable/game/mod roots, Windows usernames and recognized credential fields, refuses linked source files, and excludes media, Data/Library contents, caches, binaries, dumps and prior support exports. It is local and user-directed; review before sharing. Regression checks cover redaction, linked/oversize exclusions and no heavy Jukebox initialization.

Scaler and Unlocks use the supplied two-column/card references without exposing unsupported native capabilities. Settings groups installation, portable storage and support, with runtime/readiness details collapsed. The old prepared-playlist caching switch was a real WPF optimization that Rust does not yet implement. Permanent `Library/Jukebox/Audio` content is not disposable cache. The separate cache design and invalidation requirements are recorded in `artifacts/prepared-playlist-cache-gap-20261017.md`; no nonfunctional cache toggle is shown.

The 361-file isolated mod-link fixture measured 5,398 ms activation and 2,519 ms restoration, compared with 11,867 ms and 5,164 ms for the prior implementation. Fresh guards remain before every mutation. These are fixture timings, not a measured gameplay launch. Camera/scaler publication guards and the withdrawn WNBA route from 0.42 remain in force. Feature readiness now correctly describes that withdrawal.

Final verification passed 916 native tests (22 explicit/manual checks ignored), 139 frontend tests, and all 119 packaged no-game UI checks. Three additional fresh-process runs each passed those 119 checks and the original WPF share-file validators, followed by the isolated browser HTTP and provider-worker cleanup checks. UI readiness in the three repeats was 269, 299 and 268 ms; these are isolated launcher UI timings, not Steam/game launch timings. Screenshots of Play, Profiles, Mod library, Jukebox, Presentation, Unlocks, Scaler and Settings were reviewed. Narrow-window checks cover the restyled workflows. Earlier packaging failures exposed stale test selectors and a fixture profile that retained its default folder; both were corrected without weakening checks. A compiler access violation required the machine's compilation-only affinity workaround; packaged verification restored normal affinity.

Exact candidate: `Builds/0.43.0/Portable/App/2k Launcher Rust Prototype.exe`, 35,314,176 bytes, SHA-256 `933D9A074F85095A34905CBF9708B4D95A10A73ABA5A124E77C15B2DBB84C9AE`. Evidence: `Builds/0.43.0/QA-VERIFICATION.json`. Main remained `B86C1AAFA291DA5850DA93A90796F15A84EA57B225393F83591761D56316F958`; no Main install, user Data/Library migration or game modification occurred. Prior complete rollback is `Backups/v0.42.0-offline-hardening-20261017`; the new verified source/executable checkpoint is `Backups/v0.43.0-workflow-layouts-20261017`.

Remaining live verification: camera menu/order/tracking/height/zoom and rearming, visible player scale and rearming, Jukebox playback/order/restoration, startup video sound/focus and a normal second launch. WNBA Freestyle is unavailable until a compatible mixed roster and game-owned loading boundary are proven; no enabled WNBA route is claimed. The preserved historical snapshot proves all 30 NBA and 15 WNBA teams in one 403-row generation, but does not establish Version 2 compatibility. See `artifacts/learn2k-general-mixed-roster-evidence-20261017.md` for the current extraction/decoder evidence and remaining proof requirements.

## Previous crash-safety and offline hardening QA 0.42

QA 0.42 withdraws the experimental Version 2 WNBA Freestyle loader after QA 0.41 crashed during the first transition from team selection to a Freestyle court. The 0.41 implementation replaced the NBA roster-loader delegate with a sibling WNBA-only thunk; offline validation did not prove that thunk's invocation contract or resulting roster lifetime. The saved preference remains intact, but the control is unavailable and the UI, runtime and final writer independently refuse the route before process access, journal creation, memory protection changes or writes. No WNBA memory change is attempted in this QA.

Camera and scaler publication now validates the retained private camera allocation before initial publication and every reset rearm, distinguishes active, stock-reset, transitional and conflict states, and permits only exact policy-generated geometry. Initial process access gets two bounded no-write retries before the feature holds. Startup-video import progress is tied to a caller-generated operation identity so a stale terminal event cannot claim a later import; cancellation and shutdown are covered. Ball, arena, scoreboard and outfit catalogs reuse only exact immutable request tuples, eliminating repeat IPC work without crossing filters or pages.

Jukebox preparation keeps full process/ownership checks before and after its large stock hash but uses a cheap sealed-source authorization check for each hash callback. Player-package restoration removes one redundant process scan per file while retaining a fresh closed-game proof before every unlink and marker removal. The 361-file cleanup fixture now performs exactly 363 restore scans and completed in 2.061 seconds on this workstation; live session cleanup remains hardware-dependent. Provider bootstrap and indexing timing are now included in launch evidence.

Offline validation passed 911 native tests with 22 explicit/manual checks ignored, plus 119 frontend tests. The optimized package then passed 114 no-game UI checks in the build and three more fresh-process repeats, including the shared WPF export validators, HTTP fixture isolation and provider-worker cleanup. The executable is `Builds/0.42.0/Portable/App/2k Launcher Rust Prototype.exe`, SHA-256 `C87960F298A39334E20BB9BEE5AB428875C61854581A3EF9F6E924654ABD8735`, 33,966,080 bytes. Installed Main remained SHA-256 `B86C1AAFA291DA5850DA93A90796F15A84EA57B225393F83591761D56316F958`; NBA 2K27 and user Data/Library were untouched. Evidence: `Builds/0.42.0/QA-VERIFICATION.json`. Live checks still required: stock Freestyle entry with the old WNBA preference retained must remain stock and not crash; camera tracking and rearming; visible scaler behavior; Jukebox playback/order; startup-video import/cancel; normal cleanup and a second launch.

## Withdrawn Version 2 WNBA Freestyle loader QA 0.41

**Do not use QA 0.41 for WNBA Freestyle.** Its experimental sibling-loader route caused a live game crash during the first Freestyle court entry and was withdrawn in 0.42. The following section is retained only as historical implementation evidence.

The WNBA Freestyle option now has a Version 2 roster-loading route instead of depending on WNBA teams already being present in the default NBA roster. The route uses NBA 2K27's own sibling Freestyle/2KU loader thunks: it temporarily selects the stock WNBA roster loader from a stable Main or Quick Play boundary, restores the normal stock loader after team-selection initialization, validates the resulting roster, and only then applies the existing four category-navigation changes. It does not inject roster rows, execute a remote function, patch executable code, or borrow the separate Blacktop callback.

The implementation is pinned to the installed Version 2 PE timestamp and image size. It verifies the exact general-loader, WNBA-loader and common-loader bytes; both delegate objects and vtables; the one permitted slot; memory type/protection; before/after values; durable intent; protection restoration; and readback. Only the two known stock endpoints are permitted. A 60-second entry timeout restores the normal loader. Returning to Main or Quick Play rearms the stock WNBA loader for another Freestyle entry while leaving the category bridge applied for the game session, matching the previous WPF lifecycle.

Offline verification passed 896 native tests and 110 frontend tests, with 22 native checks intentionally manual or opt-in. The optimized package passed 114 no-game UI checks and the shared WPF export validators. It did not open NBA 2K27 or install over Main. The built executable is `Builds/0.41.0/Portable/App/2k Launcher Rust Prototype.exe`, SHA-256 `44F498912083648C38DF9492FC300C33D5777B246769C6A225990EE55B040551`, 33,927,168 bytes. Main remained SHA-256 `B86C1AAFA291DA5850DA93A90796F15A84EA57B225393F83591761D56316F958`. Pre-edit rollback files are in `Backups/qa041-before-wnba-loader-20261016`; the complete previous release checkpoint remains `Backups/v0.40.0-browser-overlay-workflows-20261016`.

Live verification is still required. Enable WNBA teams in Freestyle before Play, launch through the QA mod-profile route, enter Freestyle directly from Main/Quick Play, confirm the WNBA teams and one playable court, return to Quick Play, then enter Freestyle a second time. Finally exit normally and confirm provider cleanup. A successful Blacktop visit does not count as this test. Blacktop WNBA remains disabled; camera/scaler/Jukebox and total launch timing retain their separate pending gameplay checks.

## Previous browser and overlay workflows QA 0.40

Roster Editor and Team Editor now lead with the familiar browser editor. Local DNA drafts, catalogs and file sharing remain available in a collapsed, clearly labeled area; they are not presented as changes to the game roster. Collapsing that area preserves its draft state. The existing browser pages, import/export formats and native write guards are unchanged.

The primary Preview overlay button opens the shared, top-aligned offline preview window. Repeated clicks reuse that window. The inline preview remains available under Advanced preview tools, and catalog shortcuts remain available. Sample previews do not gain live-game capabilities.

Regression coverage checks the browser-first entry points, retained local draft workflows, and the primary preview button's window position, width and reuse. The smoke-only inspection route is restricted to the main smoke window and reports only the offline preview window's geometry. Packaged testing keeps that window hidden, including repeated opens.

The final pipeline passed 886 native tests, 110 frontend tests and 114 packaged no-game UI checks; 22 native checks remain explicitly manual or opt-in. A fresh packaged repeat passed all 114 checks, the shared WPF export validators, browser-server isolation and provider-worker cleanup. Main retained SHA-256 `B86C1AAFA291DA5850DA93A90796F15A84EA57B225393F83591761D56316F958`; user Data and Library and the running game were untouched. Evidence: `Builds/0.40.0/QA-VERIFICATION.json`. Rollback source and executable: `Backups/v0.39.0-camera-scaler-preparation-20261016`; additional pre-edit files: `Backups/qa040-before-ui-workflows-20261016`.

The broader rebuild is not complete. WNBA Freestyle still lacks a verified Version 2 mixed-roster loader. Cameras, scaler, Jukebox playback and launch timing still require the gameplay checks described below; UI tests do not establish those results.

## Previous camera and preparation QA 0.39

This separate QA version addresses the camera rejection observed in 0.38, restores editable camera names, improves loaded-player scaler diagnostics, and reduces repeated read-only preparation checks. It does not replace Main or copy a running launcher's Data or Library.

The camera validator now accepts either the preferred image base or the exact retained loaded base in the live PE header. The recorded live header used the latter. Disk identity, section layout, code anchors and camera ownership checks remain required; unrelated bases remain rejected.

Scaler verification recognizes the independently mapped Version 2 manager interface and private player descriptor. The status follows loaded player caches instead of continuing to show only the pre-load result. Native cached sizes include player-specific factors and are not reported as percentages. No per-player size writes were added; visual scale still needs a new gameplay check.

Preparation continues hashing every game-executable byte. Cancellation and ownership callbacks now run at bounded 16 MiB intervals during that read, with a final callback, instead of after every 1 MiB buffer. Camera and scoreboard manifest scans use 16,384-row read-only batches and a final check. Fresh publication and mutation guards remain unchanged. The 0.38 session took 40.9 seconds to dispatch Steam, including 21.3 seconds for Jukebox preparation; the new total launch time has not yet been measured in game.

The Presentation page restores WPF-style camera-name editing with fixed native identities, length and character validation, and locked stock names. Camera order remains keyed by native identity. Held, unavailable and unverified native features are visible on Play without opening Advanced.

WNBA Freestyle remains incomplete. The stable default-roster snapshot contained 334 teams, no expected WNBA clubs and one of the two expected national teams. This version reports that missing roster directly instead of waiting indefinitely without explanation. It does not inject team pointers or enable the unverified Version 2 mixed-roster callback. The older WPF category switch also required the teams to be present; its separate Blacktop roster-loading hook is not equivalent to the Freestyle category edits.

The final build pipeline passed 886 native tests, 107 frontend tests and 111 packaged no-game UI checks. The native suite excludes 22 opt-in/manual audits. Separately, all 36 camera/scaler harness checks, including their installed Version 2 disk profiles, passed; the actual Jukebox music/native-container disk audit passed too. Presentation was visually reviewed from the packaged capture. Build-shell command resolution and an outdated camera confirmation assertion were corrected before this final pass.

Fresh-process verification is recorded separately in `Builds/0.39.0/QA-VERIFICATION.json`. Rollback source and executable: `Backups/v0.38.0-launch-progress-recovery-20261016`. Additional pre-edit snapshot: `Backups/qa039-before-offline-fixes-20261016`. Live checks still required: camera list/tracking, requested player scale, Jukebox playback, total launch timing, normal cleanup and a second launch. The broader rebuild goal remains active; `artifacts/qa039-followup-plan.json` records the next UI work and the unresolved WNBA dependency.

## Previous launch progress and recovery QA 0.38

This isolated QA update addresses slow mod preparation and misleading launch/recovery feedback. Main and QA 0.37 remain available.

Player-folder preparation now reduces repeated process scans during read-only discovery and validation. Fresh checks before hard-link creation, publication, and owned cleanup remain in place. A synthetic 361-file profile with the production process-scan callback improved from 33.5 to 10.6 seconds in one before/after pair, with scans reduced from 1,809 to 738. This measures link preparation, not Steam or total game startup. The 60 isolated filesystem tests and benchmark passed; evidence is in `artifacts/player-startup-performance/handoff-20261016.json`.

The Play page continues polling during preparation and shows the actual phase and elapsed time. Native status no longer waits on the mod-indexing controller lock. Camera names, scoreboards and loader preparation have distinct stages, and each session records bounded stage timings for diagnosing remaining delays.

Reopening reconciles a completed provider's outer journal before exposing Play. Unknown/live processes and remaining owned files continue to block a new launch. A historical error no longer marks a fully restored, closed session as blocked. Finish cleanup is available beside a recovery warning rather than only inside Advanced.

Closing the original launcher during preparation or an active provider session offers normal taskbar minimization. Explicitly choosing No still exits. Reopened windows do not inherit the old helper's authority. If the original launcher exits or crashes, close the game and let cleanup finish before reopening it; automatic authenticated reconnection remains future work.

The reported 0.37 session did finish restoring its owned files after the user closed the game and launcher. Its feature activation had stopped because a new launcher window appeared while the old helper was waiting for the game. No manual receipt deletion or game-file removal was used.

Validation passed: 877 native tests, 103 frontend tests, and 109 packaged no-game UI checks; 22 explicit/manual native audits remain excluded. The new build retains 0.37 feature gates: custom in-game Jukebox covers and Version 2 outfit carry remain disabled. The next live check is Jukebox, cameras and scaler, including normal game exit and a second launch. Offline results do not establish gameplay acceptance.

Final fresh-process verification also passed all 109 UI checks, browser workspace checks and provider-worker cleanup checks. The main executable hash is unchanged. Evidence: `Builds/0.38.0/QA-VERIFICATION.json`. The isolated fresh UI became ready in 333 ms; this is not a measurement of game launch time.

Rollback source: `Backups/qa038-session-progress-before-20261016`, `Backups/player-folders-before-startup-perf-20261016`, `Backups/provider-phase-timing-20261017`, and `Backups/qa037-owned-close-prompt-20261016`. Complete previous milestone: `Backups/v0.37.0-jukebox-simple-play-qa-20261016`.

## Previous Jukebox workspace QA 0.37

This isolated QA version restores the WPF playlist-first workflow: a compact song list with artwork, Play/Edit/Remove, playlist search, New/Rename/Reorder/Delete, file import, Folder and Save to share. Main 0.36 is not replaced.

The main page has one prominent Play button, concise session status and the existing profile, music and video choices. Play performs fresh read-only checks before a short acknowledgement dialog. Diagnostic status, Steam-only testing, recovery, observer and manual in-game tools remain in a collapsed Advanced section and mount only when opened; no launch guard was removed.

## Jukebox editing and artwork

Song editing preserves native identities, audio and playlist order. Find online searches MusicBrainz and the Cover Art Archive only after an explicit click. Choose file prepares a centered 256-pixel preview and native BC1 cover without Node, .NET or an external image converter. PNG, JPEG and BMP use Windows imaging; WebP depends on the installed Windows codec. Searches have bounded responses, trusted HTTPS destinations, rate limits and cancellation checks.

Find missing artwork keeps existing covers, reports partial failures and allows stopping between requests. Private preview tokens are limited by size and lifetime; choosing an image does not save it until Save song. Playlist sharing retains edited metadata and artwork, including stock references without duplicating stock audio. Older playlist package formats remain readable; packages containing stock overrides require this version or another reader supporting format 4.

Unchecked song choices remain local until Save selection, Save or Use for next Play. Conflicting membership changes are disabled until those choices are saved or restored. Polling preserves stable song rows, scroll and focus. Imports and file dialogs remain explicit; no network lookup runs at startup or merely by browsing Jukebox.

## Scope and remaining checks

Artwork previews and sharing are connected. In-game custom artwork publication is still disabled: the provider currently uses stock cover resources. The new bounded artwork-session planner is isolated for later native-loader integration. Freestyle playlist event mapping also remains unavailable on Version 2. These are not implied by a successful library edit.

Prepared Version 2 outfit modules and loader/cancellation safety fixes are included in source and offline regression coverage; the new outfit capability remains disabled pending its separate integrated admission review. Other game features retain the 0.36 gates. No new gameplay test or Main installation is part of this update.

All 866 native tests and 101 frontend tests passed, with 22 explicit/manual native audits excluded. The final packaged executable passed 109 no-game UI checks, including stable row focus/scroll, checkbox-conflict handling, real native artwork conversion and edited-artwork sharing. Extra fresh-process/browser/provider repeats remain pending for this handoff. Real online-provider availability and gameplay acceptance remain user tests. Detailed evidence is in `Builds/0.37.0/QA-VERIFICATION.json`.

New recovery tests also found and fixed a canonical-path spelling mismatch in the unreleased outfit loader. The fix retains strict file identities and does not enable that feature. The previous installed Main remains the rollback; the complete frozen 0.36 source/release checkpoint is `Backups/v0.36.0-main-media-promotion-guardfix-20261016`.

## Previous Main milestone

The following records the separate 0.36 saved-media milestone, not new 0.37 gameplay acceptance.

### Saved media Main QA 0.36

This update prepares the Rust/Tauri launcher for the approved Main promotion, with the existing saved startup videos and Jukebox playlists. It does not establish full WPF feature coverage or live gameplay acceptance.

## Saved content and storage

- Original WPF `Data/settings.json`, MP4 catalog/files, compressed playlist bundles and Runtime are preserved.
- Rust settings use `Data/Rust/settings.json`. Safe game/mod folders and automatic-overlay preference import once. Unsupported native options remain in the original settings, not silently enabled.
- Six visible playlists retain exact song order and metadata. All 157 native-Opus songs import without decoding/re-encoding. Old custom-bank collisions remap against the current Version 2 stock catalog.
- Default soundtrack and disabled startup-video preference remain unchanged. A remembered playlist UI selection is not treated as an active playlist.
- All four original MP4s are reused in place. The new strict UUID catalog is `Data/Rust/startup-videos.json`; new imports stay in portable `Library/Videos/mp4`.
- The hidden recovery playlist remains untouched in the old library. Its missing references do not affect the six visible playlists.
- Reviewed media publication adds only five absent datasets, with exact identities, no-overwrite renames, a durable receipt and explicit rollback. A partial/foreign publication holds instead of merging or replacing user files.

## Startup video

An enabled selection is resolved by ID, verified and retained before Steam dispatch. The external fullscreen player waits for the exact launched process and a stable visible game window. It uses no game MOV replacement, game-memory patch or injected input.

Click, Enter/Esc/Space or Xbox A/B/Start skips. Alt-Tab cancels without stealing focus. A small embedded Rust audio-only guardian follows the game's Windows mixer scalar, temporarily mutes only owned matching game sessions and conditionally restores them. Video silence is acknowledged before game audio restoration. Unknown ownership/restoration holds; it is not blindly unmuted.

Cleanup retries failed native silence/window destruction. Authenticated failures before any audio mutation finish without claiming an owned mute. Later/unknown failures remain held. The helper has no Tauri/provider/Steam/game-memory entrypoint and needs no launcher-ownership exemption.

MP4 import uses validated containers and stable IDs, not filename overwrite. The import UI currently shows its normal busy state; detailed byte progress/cancellation has not yet been connected to this service's callback.

## Stability fixes

- Namespaced settings use strict JSON, retained full file identity, durable compare-and-swap saves and read-only launch admission. Corrupt/stale/uncertain state blocks a launch rather than using cached options.
- A narrow shared gate prevents a decoder child from appearing between idle validation and Steam dispatch. Fresh authoritative controller/process checks also run at actual decoder creation. Native Opus/PCM passthrough, previews and playlist edits remain available.
- Closing cancels video hashing and both new Play paths. A nonblocking native stop flag rechecks after long provider preparation and immediately before dispatch. An unstarted provider stops gracefully; uncertain cleanup retains its exact ownership.
- The new Main executable name remains in launcher ownership classification; no process-name exception is introduced.
- Main and startup-intro views have separate permissions. The intro can acknowledge/skip only its own generation, not invoke general launcher/native commands.
- Existing guarded Version 2 mod loading, arena/gameplay-ball, scoreboard, presentation, rules, cameras, scaler and Freestyle routes are retained. Version 2 outfit carry-over is still disabled.

## Offline verification

The final integrated native suite passed 819 tests with no failures; 20 explicit/manual audits were excluded. All 87 frontend tests and strict TypeScript/Vite compilation passed. The packaged GUI passed all 105 checks in the build pipeline and in three additional fresh processes. Browser HTTP/shared-file checks and the provider lifecycle fixture passed; test-only markers were absent from the release.

All four actual saved MP4s decoded at the start, midpoint and near the end: 12 actual frame positions, with audio muted, no game/guardian access and no Main changes. These are decoding checks, not live fullscreen/audio acceptance. Exact executable SHA-256: `B86C1AAFA291DA5850DA93A90796F15A84EA57B225393F83591761D56316F958` (32,399,360 bytes). Versioned `QA-VERIFICATION.json` and `SAVED-MP4-VERIFICATION.json` retain the detailed evidence.

Separate acceptance already passed:

- 0.35 foundation: 726 native tests, 76 frontend tests, 105 GUI checks in three fresh processes, private browser/provider checks, and 668 hash-verified rollback files.
- Settings compatibility: seven fixture groups, including four interrupted-save phases and unchanged original WPF bytes.
- Saved-media/video service: 63 tests passed; two explicit disk audits excluded.
- Jukebox admission/service harness: 90 passed; one explicit disk audit excluded.
- Startup intro/audio helper: 76 passed; one opt-in file audit excluded. Frontend intro/decode regressions also pass.
- Publication: six fixture groups cover partial copies, all five target collisions, races, same-byte foreign replacements, parent identity changes and rollback restart.
- Actual saved library: all six playlist plans, 157 Opus resources/previews, 216 artwork blobs, native masks and music-file round-trips passed read-only. The 382-file staged inventory and original Main metadata hashes were unchanged.
- Fixed Main prepublication verification passed with no writes: 378 new dataset files / 871,169,873 bytes, four existing videos reused.
- App promotion: 80 synthetic assertions passed twice independently, including late-process refusal, exact rollback, foreign-App preservation and original Data/Library/Runtime preservation.

These checks do not launch or modify NBA 2K27. Main promotion requires the separate approved operation after the build/frame checks.

## Rollback and remaining live checks

The promotion retains the complete old App at `Recovery/App-before-Rust-0.36.0-20261016` before the new App is made active. Original settings/media and Runtime stay in place. The media publisher can roll back only its own new datasets; edited/foreign content is held.

Source/release foundation rollback: `Backups/v0.35.0-verified-v2-gameplay-media-foundation-20261016`. The versioned 0.36 checkpoint adds the integrated source and release before Main installation.

Still requires agreed live gameplay: fullscreen/game-window timing, Xbox skip, mixer matching and restoration after skip/end/error/launcher exit, Jukebox overlap/order/normal cleanup, and the new actual court-ball route. Scoreboard/presentation/camera/scaler/Freestyle/rules lifecycle acceptance remains feature-specific. Basic mods, manual F8/Alt-Tab hiding and AAU Gym have only the earlier QA0.34 user confirmation.

Advanced WPF editor services, Blacktop WNBA, automatic outfits, 3D mannequin/animation previews and broader legacy-data migration remain unfinished. The rebuild goal remains active.
