# Version 2 cameras, scaler and Freestyle QA

Source version 0.33.0 connects three optional features to the Rust provider for the October 16 NBA 2K27 Version 2 executable. Main and its Data and Library are preserved. No game process was opened and no installed game files were changed by this offline pass. This QA is not installed over Main.

## Connected features

- Cameras: stable camera IDs, saved order, WPF-compatible tracking/framing, whole height/zoom steps, native setting synchronization and an optional lock that defaults off. Exact-owned temporary English labels are published before Steam dispatch and the same bytes are frozen for the provider, with the required stock override. Existing localization mods are preserved and block this optional route. Camera-memory publication requires an inactive Main or Quick Play menu with no loaded players, rechecked after allocation and before each write. Private data stays allocated until game exit.
- Scaler: the original 95–100% shared-input policy with exact float rounding and ownership checks. One verified four-byte input is changed; player skeleton/cache fields are not repeatedly rewritten. Old unsupported prototype values remain saved but block the feature plan until reset. Coaches and arena NPCs are unchanged.
- WNBA Freestyle: four category-navigation DWORDs after two matching deep team-selection/roster snapshots. Full table ownership, including untouched words, is checked after intent flushing and before each write. Navigation persists for the game session, matching WPF. Roster data is not modified.

Cameras and WNBA Freestyle default off. Default 100% scale makes no write. Choices are frozen for the next native modded launch; saving a preference does not alter a running game. Each feature reports waiting, active or held independently of mod-loader success. The general legacy write gate remains disabled on Version 2.

## Session safety

The worker retains the exact launch-owned game identity and independently verifies each feature's signatures. Its write capability accepts only fixed non-executable data spans and journal-authorized bytes. Read-only pages are temporarily writable only for the bounded update, then their actual original protection is restored and verified, including error paths.

A pinned intent log precedes allocation/publication. Its bounded append and recovery limits agree. Unknown ownership, partial writes, context changes and failed verification stop further feature writes for that session. Closing the UI prevents new feature writes while the provider retains assets and private allocations through game exit. Recovery never follows saved memory addresses. Loader, generated selector and profile cleanup precedes memory-only receipt validation, so a damaged feature receipt cannot strand a verified owned loader.

Camera-label files have a separate fixed-path receipt binding directory/file identities, hashes and session. They are never replaced while the game runs. Restoration removes only the exact recorded file, before releasing the selected mod-profile lease. Unrecorded or partial stages are held for review, not adopted by filename. The original 20-second loader settling gate is unchanged; no locale reload or earlier activation is guessed.

This pass fixed the missing camera-label stock override, an absent-receipt recovery check that incorrectly required an already-removed mods folder, and manual recovery ordering. Damaged memory-only rules/player/team/uniform/arena/ball/outfit/feature receipts now hold final review without preventing independently verified loader, camera and profile restoration. The corrupt receipt remains unchanged. Camera labels are also reverified immediately before Steam dispatch, after selector preparation.

The 0.32.1 process-ownership refresh fix is retained. A fresh snapshot may clear an exited enumeration row; a still-present unreadable owner remains blocking. No user processes are stopped by these checks.

## Offline verification

The isolated camera suite passed 17 tests and its explicit installed-executable disk audit. Scaler passed 13 tests, 524 comparisons against original C# policy and its disk audit. Freestyle passed 12 tests, 52 original-policy comparisons and its disk audit. Camera transforms retain 315 original WPF byte comparisons. These are disk/fixture checks, not gameplay acceptance.

Final acceptance passed 475 native tests and all 40 frontend tests, plus 100 packaged UI checks in the build and two fresh 100-check repeats. Ten explicit/manual native audits are excluded from the ordinary suite; the three connected Version 2 feature disk audits passed again separately against the installed executable. Browser authentication/no-game write refusal, WPF shared exports and owned provider shutdown also passed. The provider fixture exited in 83 ms. No real game was opened. Evidence is in Builds/0.33.0/QA-VERIFICATION.json and artifacts/build-v033-acceptance-20261016.log.

Integrated tests cover exact intent tokens, failed preconditions, partial transactions, context changes during journal flushing, exhausted receipt budgets, protection races/write/readback/restore errors, initial Home stage zero, closed-owner behavior, unsupported settings, full Freestyle table progress and preserving existing localization mods.

Additional camera filesystem tests use the actual production Windows publication, directory and unlink helpers. Orchestration fixtures cover early/late byte consistency, foreign profiles/aliases, cancellation, interrupted recovery before profile restoration, and all eight damaged-memory-receipt classes plus a successful cleanup. These fixtures use disposable files and fake game identities, never a real game process.

An initial restricted-sandbox full run could not exercise Windows process/file-sharing fixtures and failed with access-denied errors. Its log is retained. The normal-permission offline suite is the acceptance run; test permissions are not loosened in the app.

## Rollback and remaining checks

Backups/v0.32.1-before-v2-camera-scaler-freestyle-port-20261016 contains 446 hash-verified source/release files without user Data, Library, caches or logs. Backups/v033-shared-runtime-pre-edit-20261016 also preserves the integration files before edits. QA0.32.1 remains frozen. Main is not replaced.

The new verified source/release checkpoint is Backups/v0.33.0-verified-v2-feature-milestone-20261016. Unwired next-milestone sources in that checkpoint are not claims of enabled features. The installed Main hash remains 5D35A27176E91547FDCC6BCC09017A2AD0E0030C12C5A9FEA7F207BBDF0D227E.

Later live verification must cover Version 2 mod appearance/cleanup, camera visibility/tracking/framing/names/HZ controls, scale appearance/court reloads, Freestyle navigation after returning through Quick Play, UI closure while the game runs and normal game exit. Do not run multiple launchers against the game.

Other Version 2 tabs are progressing separately. Rules/presentation, arena/ball and the roster/team reader have independently audited profiles and new backend adapter work; Jukebox and startup-video adapters are in progress. They are not enabled by this build. Blacktop WNBA, arena/ball/outfit/rule writes, NPC scaling, full startup-video/audio coordination and the remaining editor services still need their own backend integration and acceptance. Some older generic QA control descriptions refer to the supported September build, not Version 2 capabilities; use the per-build gates and this report. This is not full WPF feature coverage.
