# Version 2 arenas balls and Game Rules QA

Source version 0.34.0 connects Blacktop arena and ball choices and all 39 Game Rules controls to independent NBA 2K27 Version 2 backends. The existing overlay and catalog styling is retained. This is an isolated QA build, not a Main installation. Main, Data and Library remain untouched.

## Connected features

Arena choices use the launch-owned active provider, exact catalog IDs and stable Blacktop player selection. Native arenas change only the two arena/floor mapping IDs. Template arenas require both installed assets and publish the bounded template filenames before the mapping. Returning through Main Menu or Quick Play retains and revalidates the owned mapping; choosing Default restores the original values at selection.

Ball choices use the same owned provider session with a separate capability. Four unchanged unloaded snapshots, spaced at least 100 ms apart, are required before changing the native normal-ball filename. Loaded resources, busy transitions and non-normal native models are held rather than forced. Menu restoration and next-match rearming use the actual transaction engine. Every new game session begins at Currently installed. No resource counters, engine hooks or executable memory are changed.

Game Rules uses the existing Refresh and explicit Apply workflow on Play and F8. All 39 fields have independently verified Version 2 getters. A one-use snapshot binds the exact game, mode, owner and all neighboring values. A pinned durable intent precedes the delayed write handle. Only the selected one/four-byte field may change; read-back must confirm every other rule is unchanged. Conditional rollback retains an unresolved receipt rather than overwriting foreign values. Rules are not forced on automatically or replayed after restart.

The 0.33 camera ordering/tracking/names, player scaler and Freestyle WNBA navigation remain connected through their own opt-in profiles. Default settings make no new automatic feature writes. The legacy global write gate remains disabled on Version 2; outfit and editor writes are not enabled by these adapters.

## Repeated launch and ownership fixes

Actual production file operations reproduced fresh-session failures in generated selectors, camera labels and player-package links. NTFS can preserve a destination's prior creation timestamp during a legitimate rename while the persistent file ID remains unchanged. New receipts compare the full kernel file ID and volume, retaining directory, path, session and content checks. Old receipts remain readable with their strict original identity comparison. Foreign files with identical bytes still fail ownership checks.

Regression fixtures repeat 12 fresh selector sessions and 12 fresh player packages on the same target paths, repeat camera publication, recover old-format receipts and reject same-byte replacements. Existing profile restoration and user-content sentinels are preserved.

The integrated Rules adapter also uses retained waitable-handle liveness instead of the undefined live-process exit timestamp returned by GetProcessTimes. Unknown wait results, exited handles and changed birth identities are rejected. This change grants no additional memory-write authority.

## Offline verification

The full native suite passed 539 tests with no failures; 14 explicit/manual audits are excluded from the ordinary run. All 41 frontend tests and the strict TypeScript/Vite build passed. The three new installed-executable disk audits for arena, ball and Rules passed separately, including the full executable hash and independent layout signatures. Ordinary automated tests use disposable files, fake game memory and launcher-owned processes, not a game session.

The packaged WebView2 pipeline passed all 100 checks, followed by two fresh 100-check repeats. Browser authentication, no-game write refusal, original shared-file validators and owned provider shutdown passed. Final results and executable hashes are recorded in Builds/0.34.0/QA-VERIFICATION.json and artifacts/qa034-final-package-verification-20261016.log. The generated arena overlay and numeric Rules screenshots were visually reviewed; live browser/native-dialog acceptance remains pending.

New integration tests exercise the actual arena/ball Engine and Runner with real receipts: selection-to-match, loaded-resource refusal, Main/Quick Play restoration, rearming, UI-owner closure, pre-publication deferral and uncertain-write holds. Rules tests cover all supported choices, unchanged neighbors, journal faults, stale owners and conditional rollback. Independent installed-executable disk audits verify the new arena, ball and Rules profiles without launching or writing to the game.

Evidence is retained in artifacts/build-v034-acceptance-20261016.log, artifacts/qa034-rules-integrated-final-20261016.log and artifacts/qa034-v2-arena-ball-rules-disk-audits-20261016.log. Initial regression and integration failure logs are retained separately from final acceptance.

## Rollback and remaining work

Backups/v0.33.0-verified-v2-feature-milestone-20261016 contains 530 hash-verified source/release files and no user Data or Library. QA0.33 remains frozen. Additional selector/player pre-edit backups preserve the confirmed NTFS regressions. No Main installation is authorized by this QA pass.

Live acceptance is still required for Version 2 selected-folder mod appearance, arena/template loading, ball appearance across two matches, Main/Quick Play transitions, stock-menu Rules comparison and restore, cameras, scaling, Freestyle navigation, closing the UI during a game and normal exit cleanup. Do not run multiple launchers against one game.

A separately reported launch refusal involved unreadable node.exe PID 25596. Read-only checks confirmed that PID had exited by inspection time; its origin cannot be established after exit. This package retains the conservative fresh-scan ownership policy. A narrow retained-handle process-teardown improvement is next-milestone work, not a claimed fix in 0.34.

Jukebox, scoreboards and court-presentation execution are being rebuilt in parallel. Their source modules and offline tests are not claims of enabled features in this package. Automatic outfits, Blacktop WNBA, remaining editor services, startup-video/audio coordination and 3D previews still require integration and acceptance. Feature readiness remains incomplete.
