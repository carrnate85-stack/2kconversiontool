# Rust / Tauri 2 prototype 0.4.0 — QA milestone, 2026-10-11

Status: offline milestone; 1:1 parity is not achieved. Do not replace Main.

Executable: `Builds/0.4.0/Portable/App/2k Launcher Rust Prototype.exe`.
Final executable size/hash and packaged GUI result are recorded in `Portable/BUILD-INFO.json`.

## Added

- Native Team DNA validation, five-category import/staging, `.2kteam` export to Library/Teams, local persistence and reset. No source/destination team ID, staff assignment ID, live address or process metadata is transferred.
- Local team info and separate arena/floor stems; 32 staff roles with 175 mapped staff/coaching controls, including identity, tendencies, systems, proficiency and badges.
- 48-minute, five-player rotation grids and 52 uniform controls, matched by type/side/designation rather than source uniform ID. Colors currently use unsigned raw ARGB numbers; attributes and badges use raw mapped values.
- Unsaved Player/Team DNA drafts now survive page navigation within their current window. New draft asks before discarding edits. Save is explicit; closing/reopening restores only saved data. Reset does not refresh the live game.
- Per-editor operation gates and disabled editing during native save/import prevent rapid duplicate operations or edits racing an in-flight save.

## Regression coverage

- 2,059 Team DNA validation comparisons against the unchanged WPF validator, including all 175 staff controls, all 52 uniform controls, staff slots, Unicode text limits, rotations, arena/floor stems, invalid IDs and metadata stripping.
- Existing 10,517 outfit-composition comparisons and 2,444 Player DNA comparisons are retained. All 4,274 embedded catalog players are validated against the native Player DNA schema.
- Native Team DNA save/read/export round trips; invalid, oversized or malformed files rejected without replacing saved/source data. Existing long-path atomic save/media publication tests remain.
- Strict TypeScript/Vite builds, 37 Rust tests, 10 frontend tests and 40 packaged GUI checks passed (87 distinct suite checks). Oracle cases are assertions within the native suite, not additional end-to-end gameplay tests.
- New packaged GUI checks exercise arena/floor separation, staff fields/system 15, 48 rotation rows, unsigned ARGB, selective file-import staging, export, unsaved navigation retention, discard confirmation and reset.

## Final packaged verification

- EXE: 21,154,816 bytes. SHA-256: `6219407E54998C6F3D05DD970F2AC8B1C9E1C477BDD9C146535A4569DBB0655F`.
- Full pipeline profile: `artifacts/smoke-c95375c27b084b9088af28040185c236`, 40 checks passed. Its outfit, player and team exports passed the untouched WPF validators. The build/test shell was pinned to one core; it changes no persistent setting.
- Same executable: three additional unpinned fresh long-path profiles each passed all 40 checks; three additional files of each sharing type passed the WPF validators. No process from this test executable remained after exit.
- Repeat reports/screenshots: `artifacts/final-v04-longpath-1-2dc62c6595214ffebac351b2a17042d5`, `artifacts/final-v04-longpath-2-2e1b8055979f4545990b34931cf128d9`, `artifacts/final-v04-longpath-3-9fdc9ca48e57478a8f56132e1dc8c5e1`.
- Unpinned internal bootstrap readings: 270.52, 242.20 and 250.95 ms. These are not click-to-first-window timings or a comparative WPF benchmark.
- Team info/staff screenshots were inspected for visible controls, spacing and overflow. The source WPF application was not launched.

## Safety and rollback

Only the isolated prototype was built/tested. No game was launched, injected into or modified; Main App, Data and Library were not replaced or migrated.

Rollback: `Backups/v0.3.0-before-team-parity-20261011-100446`, containing pre-change affected source/configuration/assets and the verified 0.3.0 executable. Its SHA-256 is `C896985173044A862B33989D1A11C3DDCDDB39641D636F07D605C631D138CBD1`; Builds/0.3.0 remains untouched. The prior report is preserved as QA-REPORT-0.3.0.md.

The packaged Team import test creates an owned QA `.2kteam` fixture and uses the actual bounded native file reader and category-review popup. Native file-picker interaction still needs manual QA; no external file chooser is automatically clicked.

Live team/staff binding, destination-player rotation matching, strategy/playbooks and rollback-safe game saves remain unported. Other major remaining parity: launch/Steam/EAC checks; helper ownership, journals, restoration/rearming; live F8/Blacktop changes; camera tracking; fullscreen startup-video/Xbox/Mixer integration; in-game Jukebox; editable Player seasons/awards and rating conversions; mannequin/animation rendering; diagnostics, migration and update transactions. Native ownership tests and applicable live acceptance are required before Main promotion.

## Build environment limitation

Previous optimized builds faulted in Rust 1.99.0's rustc_driver (STATUS_ACCESS_VIOLATION / STATUS_ILLEGAL_INSTRUCTION), including Windows events from another project. Root cause remains undiagnosed. This build uses a one-core affinity on its isolated build shell only, with the original optimized release profile and no persistent system changes or hidden compiler retry. Application acceptance runs are unpinned. Passing results do not prove a toolchain/environment fix or reproducible clean builds; see QA-REPORT-0.3.0.md for prior evidence.
