# 0.32.1 ownership refresh QA

This isolated update addresses the second pre-Steam ownership failure. Main, Data and Library remain untouched. It retains the Version 2 mod-provider profile without enabling unported native memory features.

## Launch ownership

QA0.31 stopped before Steam dispatch because Node PID 8312 had no readable identity. That PID was absent when checked afterward. A subsequent native metadata-only audit against the open QA launcher passed, with only the exact expected launcher parent. These observations establish a transient blocker, not the Windows query error at the time of failure.

An unreadable identity now triggers one fresh, complete process snapshot and metadata pass. This replaces stale enumeration rows rather than ignoring unknown Node processes or exempting a PID. A still-present unreadable Node, another launcher, a running game or a protected process remains blocking. New or reused PIDs are inspected from the fresh snapshot. If the refresh fails, launching remains blocked. Normal successful scans do not repeat, and there is no sleep or indefinite retry.

Persistent pre-dispatch failures include the metadata query stage and Windows error. The opt-in owner audit also records whether a refresh occurred and its initial and final identity errors. No command lines or authentication tokens are collected. Process checks never stop applications or access game memory.

The UI version now comes from the compiled Cargo package version rather than a separately maintained string. A regression test requires it to agree with both package.json and the Tauri configuration.

## Test scope

All 390 native tests passed with zero failures and six opt-in audits excluded from the ordinary suite. The final native run took 98.81 seconds. All 37 frontend tests, strict TypeScript and production bundling passed. The packaged EXE passed 100 UI checks plus two fresh-profile repeats, 100 each. Browser authentication, no-game save rejection, WPF file-sharing validators and owned-provider shutdown passed. The provider exited in 83.126 ms; normal-affinity repeat UI-ready times were 798 and 444 ms, not cold-start guarantees. The corrected version label was inspected in the packaged screenshot.

Regression cases cover a disappearing unreadable Node, persistent unreadable ownership, reused PIDs, newly appearing game/protected/launcher owners, transient identity recovery, failed refresh enumeration, confirmed exits and filtering unrelated processes. Existing retained-handle liveness and exact-parent protections are preserved. The final metadata-only idle owner audit passed with no unknown identities or competing owners. An audit using the previous QA parent's ended PID correctly refused that identity; its failed audit log is retained rather than counted as a passing live-parent check.

Evidence is Builds/0.32.1/QA-VERIFICATION.json, artifacts/build-v0321-owner-refresh-final-20261016.log, artifacts/qa0321-final-release-verification.log and artifacts/qa0321-idle-owner-metadata-audit.log. The isolated EXE is 29,546,496 bytes, SHA-256 B9D4248A97FFCE35D9BC15D2A27218A38F6AC6150D895AD2FD1FEF47FC4953B5. Main was not installed or changed; its SHA-256 remains 5D35A27176E91547FDCC6BCC09017A2AD0E0030C12C5A9FEA7F207BBDF0D227E.

The unfinished camera-policy work is preserved separately and excluded from this hotfix. Cameras, scaler and WNBA Freestyle remain pending. The Version 2 profile remains observation/file-provider only; arena, ball, outfit, roster and Game Rules writes are not enabled for this game update.

## Rollback and live test

Backups/v0.31.0-before-owner-refresh-fix-20261016 contains 445 hash-verified source and release files, including the earlier unfinished camera work, without user Data, Library, caches or logs. Frozen QA0.31 and Main are unchanged.

Close the earlier QA launcher normally, then open Builds/0.32.1/Portable/App/2k Launcher Rust Prototype.exe. Select the installed game and complete Mods folder, then use Launch with mod profile (QA). Verify known mod appearance, exit NBA 2K27 normally and let provider cleanup complete. This live launch and restoration check remains required. Do not run both QA versions together.
