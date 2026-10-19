# 0.20.0 native mod-profile launch QA

Isolated QA verified on 2026-10-16 at 08:40 Eastern. This is not full feature parity or live-game acceptance.

Scope: explicit native mod-profile/asset-provider launch; a same-EXE hidden worker retained through exact game exit; the unchanged production native DLL; no-copy profile junctions; bounded original-compatible asset reads; no-overwrite loader publication; hash/identity-guarded recovery. The native pipe prefix now matches the production DLL. First-start defaults persist the portable settings marker required by that DLL's diagnostics contract.

No Blacktop outfit, arena, ball, scoreboard, crowd, scaler, camera, Jukebox or startup-video executors are enabled by this QA launch. The existing original WPF browser editors and native roster transactions are preserved. No Main install, WPF Data/Library migration, or real-game automated test is authorized or performed.

Tests: 219 Rust tests passed, zero failures (sequential, 52.44 seconds); strict TypeScript/Vite build and all 23 frontend tests passed. The actual packaged EXE passed 92 UI checks, then three normal-affinity repeats of all 92 checks. Repeat UI-ready times were 279, 259 and 329 ms; these are warm isolated-fixture measurements, not game-start or universal cold-start promises. Exported files passed the unchanged WPF validators. The original-browser HTTP fixture verified 122 unchanged assets, category/season search, origin/token rejection and absent game-save capability. The packaged same-EXE provider test passed exact-child ownership, reused-identity rejection, explicit-start gating and clean exit; measured helper stop was 83 ms. All automated fixtures had no real-game access or writes.

Executable: `Builds/0.20.0/Portable/App/2k Launcher Rust Prototype.exe`, 28,068,352 bytes (26.77 MiB). SHA-256: `AC193CCC27F205933258AD768483C201781E863A935FD0E0D5D52B943D4ADDAD`. Detailed evidence: `Builds/0.20.0/QA-VERIFICATION.json`; the pipeline fixture is `artifacts/smoke-bf9b059a91db4733bcc19106f5c04dcb`. Production native DLL hash: `ECEF8C6C856A60AF3387BE50D76BD30B53A80F80F404476F5C31D5A5453E91DF` (unchanged).

Original-source compatibility fixtures also passed: 650 protocol requests, 18 responses, seven error cases and 71 asset-service steps. Current-user pipe ACL, exact retained client/game identity, bounded I/O, cancellation/drain, worker exit, ten loader-file tests and seven profile-junction tests passed. These comparisons do not establish live game acceptance.

Main remains unchanged: SHA-256 `5D35A27176E91547FDCC6BCC09017A2AD0E0030C12C5A9FEA7F207BBDF0D227E`. No Main install or user Data/Library migration occurred. The build shell was pinned to one CPU because of the previously observed development-machine compiler instability; GUI repeats used normal affinity. This is not a clean-machine reproducible-build claim.

Rollback: `Backups/v0.19.0-native-uniform-save-checkpoint-20261016` is the complete previous milestone. Pre-change files are also in `Backups/v020-before-provider-pipe-20261016`, `Backups/v020-before-provider-worker-20261016` and `Backups/v020-before-modded-launch-integration-20261016`. Builds/0.19.0 remains unchanged.

Live checks still needed: selected-profile stock replacement and missing-asset routing; native loader reaches Active; selection/match loading; verified worker exit and exact original-profile restoration after game exit; launcher-close-during-game behavior; crash/restart recovery. If Steam never creates a game after dispatch, the worker deliberately retains pending resources rather than assuming the asynchronous launch was cancelled; do not retry or manually delete files. Report that held state for inspection.
