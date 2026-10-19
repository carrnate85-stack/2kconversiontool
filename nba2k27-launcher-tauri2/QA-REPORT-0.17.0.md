# Rust / Tauri 2 QA 0.17.0 — original-browser transfers and presets

Isolated parity milestone, not a WPF replacement. The full 1:1 parity goal remains active. Main, Data and Library are untouched. The tested 0.16 source/assets/executable checkpoint remains the fallback.

## Changes

- Original browser NBA 2K23 category selection now returns the supported selected sections to the unchanged WPF review/staging workflow. No alternate game writer was added: explicit Save & verify remains required. Portable season selection and optional awards are supported as source data, not live season application.
- Copy from another loaded player uses this browser tab's immutable snapshot and exact source identity key. Reconstructs portable Player DNA without process IDs, addresses or team links. Copying is local data only; it neither scans nor writes game memory. Live season-history copying remains explicitly unavailable.
- Original Player DNA import/export buttons now use native launcher-owned Windows dialogs. Browser requests cannot supply a path. Imports are stable-read and 256-KB bounded; exports validate/subset the schema and atomically publish only to the user-selected destination. Cancel leaves files untouched. Dialogs retain their COM object on the main STA, verify their own HWND/thread/process, use native file-type filters and do not add exports to Recent Documents.
- Pending dialog waits are bounded/cancellable. Main CloseRequested cancels file/preset requests before destruction; shutdown closes only its exact owned dialog on the owning apartment before joining HTTP workers. Queued requests cannot reopen a closing launcher. This lifecycle still needs manual dialog acceptance; synthetic chooser tests do not prove Windows-shell behavior.
- Signature/equipment/coaching preset validation and portable Library/editor-presets storage are connected to the original browser controls. Random IDs, atomic no-clobber publication, category/range validation, reparse-point rejection, stable bounded reads and cancellable bounded listing preserve existing/damaged files. Coaching identity/contract fields are excluded. The live staff backend remains unported.
- Tiny separate browser adapter performs the original localeCompare sort. Original WPF renderer HTML/CSS/JS assets remain unchanged; only the served index adds the native adapter. No .NET, Node or Electron runtime was added.
- Fixed JavaScript-number source IDs such as 512.0 being rejected despite matching integer IDs. Portable source names no longer gain an extra space when a name part is absent; From-name whitespace uses the original JS character set.

## Verification

The final source passed 143 Rust tests, the TypeScript build, all 23 frontend tests and 90 packaged GUI checks. Three additional normal-affinity repeats (4294967295) each passed all 90 checks and the original WPF outfit/player/team file validators. UI-ready times were 254.5232, 242.7246 and 280.1237 ms on this PC, not clean-machine benchmarks. The build shell stays pinned to one core because of the previously undiagnosed toolchain instability; runtime repeats use normal affinity and compiler optimizations were not changed.

Original-source fixtures cover 76 loaded-player copies, 46 source category/season selections and 1,592 preset validation cases. Existing 0.16 transaction/ownership/failure tests remain included. The original C#/JS oracles are development-time tools only, not bundled runtimes. The JavaScript adapter test runs as a JavaScript test; no Node type/runtime dependency was added to the app.

Final executable: 26,157,568 bytes, built 2026-10-16T02:26:47Z, SHA-256 4CBF32B1202C4D02FDFAA72B78F1E2C0B29BB88F7C262C3771219CA15A828FD1. Test-only transfer/fixture markers are absent. Main remains SHA-256 5D35A27176E91547FDCC6BCC09017A2AD0E0030C12C5A9FEA7F207BBDF0D227E.

The compiled no-game browser fixture passed category selection (200), portable season selection (200), source search/review, 122 unchanged static asset hashes (123 source assets including the bridge-modified index), absent-preview-save rejection (400), foreign-origin rejection (403) and wrong-token rejection (403). Search took 9.6582 ms. Evidence: artifacts/browser-wpf-v017-final-6273a1a3d7814c978809e12c083d6ecd/Logs/browser-http-check.json. The owned fixture exited normally.

Tests use synthetic snapshots and injected file selections only. No native file dialog is displayed automatically. The file contract tests cover portable round-trip, cancellation before publication, malformed-input preservation, ignored RPC paths, invalid/busy requests and no hidden write path. Preset tests verify all three categories, category mismatch, no-clobber behavior, damaged-file preservation and closing-state rejection. Real-socket tests check normal copy/preset callbacks and absent preview capabilities. The UI-only sorting adapter is tested against localeCompare.

No automated test launches or modifies NBA 2K27. Native live read/save and browser/dialog focus, appearance and cancellation still need manual acceptance. Original-browser visual acceptance remains pending after the prior Chrome ERR_BLOCKED_BY_CLIENT; no security setting was changed or bypassed.

## Rollback

- Coherent fallback: Backups/v0.16.0-native-player-save-checkpoint-20261015.
- Prior QA EXE: Builds/0.16.0/Portable/App/2k Launcher Rust Prototype.exe; SHA-256 71DA1DA10B5E8E82C2559D941C058ABDF650CD29E29CE0917DFC06E9F8497483.
- Pre-edit backup: Backups/v017-before-browser-transfers-20261015-220902.
- Original-source oracle: scripts/browser-transfer-regressions.mjs and assets/browser-transfer-regressions.json. Oracle data is test-only, not embedded in the release.

## Remaining full-parity work

Team/staff/rotation/uniform live saves; live seasons/awards and destination ownership/allocation transactions; custom 2K23 catalogs, From/audio, artwork/animation, 2KDB/Auction and retained/source catalogs; full mod/profile/provider application/restoration; Blacktop outfit/arena/ball/scoreboard/crowd/unlock/scaler executors and rearming; live F8/rule acceptance; in-game Jukebox, camera names/tracking, fullscreen MP4 timing/Xbox/Mixer; mannequin/animation rendering; WPF storage migration, secondary Steam libraries, updates/rollback and diagnostics/support/hang capture; real Basketball-Reference acceptance.

Full parity is not achieved. WPF remains Main. Do not run both launchers against the game.
