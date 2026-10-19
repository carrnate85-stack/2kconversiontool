# 0.21.0 native launch-path diagnostic QA

Isolated QA verification passed on 2026-10-16. This build does not claim the 0.20 live-launch failure is fixed.

The user's 0.20 attempt ended before Steam dispatch with Windows error 0x80070003. Its provider/session records closed and no Rust loader/profile recovery markers remained. A read-only check found no version.dll or temporary launcher staging files left in the game folder. No automatic retry or game mutation was performed.

Changes: retain contextual operation/path details around provider configuration, profile preparation, loader ownership/publication and staging-file creation. Add regressions for spaces/parentheses, ordinary/verbatim paths and combined preparation in a deep portable tree. No game feature is newly enabled; Blacktop feature writes remain disabled.

Tests: all 221 Rust tests passed (40.12 seconds, sequential); all 23 frontend tests and strict TypeScript/Vite passed. The actual packaged EXE passed all 92 UI checks plus one normal-affinity repeat. Original-browser HTTP/assets and packaged same-EXE worker lifecycle checks passed. Shared exports passed unchanged WPF validators. Automated tests did not access/launch/write NBA 2K27. Evidence: `Builds/0.21.0/QA-VERIFICATION.json`; pipeline fixture `artifacts/smoke-12c76660551648a4ae59726878dc2f8e`. The normal-affinity repeat reached UI-ready in 272 ms; this warm fixture is not a universal cold-start promise.

Executable: `Builds/0.21.0/Portable/App/2k Launcher Rust Prototype.exe`, 28,076,544 bytes. SHA-256: `F42E859B80A4C48A7E8A874D3439ADA47C68AF0D6EDD909C0B1A8DB2554E1B80`. Main SHA-256 remains `5D35A27176E91547FDCC6BCC09017A2AD0E0030C12C5A9FEA7F207BBDF0D227E`. No .NET/Node/Electron runtime was added. Builds remain pinned to one CPU due prior development-machine compiler instability; no clean-machine reproducibility claim is made.

Rollback: `Backups/v0.20.0-native-mod-launch-checkpoint-20261016` (365 files, including the verified EXE and evidence); `Builds/0.20.0` remains unchanged. Main and its Data/Library were not installed over or migrated.

Required next live check: close 0.20, open isolated 0.21, use Launch with mod profile (QA) once, then report the full provider error or loader-active status. Do not repeatedly retry a pending/held launch. A Windows-preparation failure is distinct from a Blacktop match failure.
