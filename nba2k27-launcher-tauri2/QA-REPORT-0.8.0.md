# Rust / Tauri 2 prototype 0.8.0 — native Blacktop observer, 2026-10-11

Isolated QA milestone, not installed over Main. The user reported 0.7.0 Steam launch working. This milestone connects the first real native Blacktop memory reader, not the writing helpers or full game workflow.

## Changes and guards

- Opt-in Start/Stop read-only observer on Play, enabled only for this prototype's running launch session. Current configured folder, journal launch folder, exact process path/PID/birth time, unique process presence and no other recognized launcher are required. Memory reading is disabled in GUI smoke/overlay command contexts and does not resume automatically after restart. It stops after game exit, on request, or launcher shutdown.
- Rust/Win32 query + VM_READ only, using a retained exact game handle. No process writer, code hook, remote allocation/thread, game-file transaction or input injection was added. No Node/.NET/script/runtime/helper executable is needed for this reader. Existing inert-child Job Object tests remain separate.
- Supported profile: x64 PE timestamp 0x6AA2013D / image size 0x36558000. The refresh signature and mode-owner RIP-relative anchor must match before reading mapped state. Protected-process checks are repeated around observation; module inspection checks anti-cheat at binding. Unknown identity/access, unsupported code/build, partial reads or torn mode/lineup state discard the sample and clear displayed readiness.
- Detector ports WPF's exact root/vtable/stage and loaded-model lineup checks. Match detection additionally requires Blacktop GameData mode 6 because gameplay hashes are shared with Freestyle. Inactive stage-1 Main Menu and Quick Play are distinct verified boundaries; arbitrary outside modes do not count as safe exits. This reader does not call the restoration/rearming writers.
- Changed-state, time/session-tagged diagnostic snapshots save in the prototype's own Logs/blacktop-observer.json. Log failure stops reading. No diagnostic dump or private library migration. Main WPF, App, Data and Library were not modified.
- Confirmed safety gap fixed: start_protected_game.exe was not included in native launch/preflight process guards. It and all EasyAntiCheat-prefixed process names now block eligibility/observation. This does not prove offline/anti-cheat-free status by absence alone.

## Offline regression coverage

Sparse-memory fixtures run the same header/signature/context/lineup parser as production. They cover complete/incomplete selection, Main Menu/Quick Play, match vs shared Freestyle hashes, mismatched mode/root, torn reads, invalid counts/addresses, wrong code/header, unknown/foreign/reused game identities and protected/other launcher conflicts. A real Win32 unit test reads its own test-process buffer through the same limited access mask, verifies attempted writes are denied and verifies that buffer remains unchanged. No game process is touched by tests.

Launch-monitor tests cover refusing an unlaunched/waiting observer, unknown process state, enabling only the bound running fixture, observer default-off on restart and automatic ended/reset state. Post-intent Steam guard regression now also includes start_protected_game.exe. GUI checks retain the launch gate, inert helper shutdown and all previous catalog/DNA tests, add the sparse-memory observer report and refuse live observing in smoke/unlaunched UI.

## Final acceptance

- Strict TypeScript/Vite release build succeeded. **68 Rust tests, 10 frontend tests and 58 packaged GUI checks passed (136 checks)**. Packaged profile: artifacts/smoke-1e56f64e86ea4344be1bc0e4686d6620. Its monitor shutdown measured 0.2091 ms. Readiness and minimum-window screenshots were inspected; controls fit without horizontal page overflow.
- Three further fresh-profile, normal-all-core runs passed all 58 GUI checks each. Profiles: artifacts/final-v08-accept-1-5c4711da96a643cab6f5d3b54019455c, artifacts/final-v08-accept-2-9d67961a73254386b5ab26b38e198c2a, and artifacts/final-v08-accept-3-1787181a3e3f4638bcff1a3d49f35c88. Affinity was 4294967295 in each.
- Internal backend/bootstrap startup measured 262.6066 / 255.2565 / 265.259 ms; monitor shutdown measured 0.1086 / 0.1968 / 0.098 ms. These are internal smoke measurements, not click-to-visible-window, active game-reader shutdown, or complete app-exit benchmarks. The dedicated 500 ms idle-monitor threshold passed.
- Each repeat confirmed zero remaining owned inert helpers; the final process query found zero processes using this QA EXE. Untouched WPF validators accepted the three repeats' 3 outfit, 6 Player DNA and 3 Team DNA exports, plus the pipeline's 1 outfit, 2 Player DNA and 1 Team DNA exports.
- Existing baseline oracles remain: 10,517 outfit, 2,444 Player DNA, 2,059 Team DNA and 244 season merge assertions; all 4,274 bundled players validate. These assertions and repeat checks are not additional live gameplay tests.
- Final EXE: Builds/0.8.0/Portable/App/2k Launcher Rust Prototype.exe; **21,451,264 bytes** (20.46 MiB). SHA-256: **15274E6CE18ECF9524251D7E65470ED3FE81FA158E9982DCC44B29B99FC42930**. The earlier 0.7.0 EXE still matches its recorded hash. The share ZIP contains only this tested EXE and the five release metadata/documents/notices files, not Data/Library/Cache/Logs or private content.

## Build limitations

Restricted tool execution denied native build-resource generation and the cached frontend tool's path canonicalization. These failed invocations are not passing tests. Approved local execution is used for the isolated build/QA. Prior Rust 1.99.0 compiler/environment faults remain undiagnosed; release optimization is unchanged, build-shell-only one-core affinity is retained, and final GUI repeats run unpinned. No reproducible clean-build claim.

## Rollback / live checks

Rollback: Backups/v0.7.0-before-blacktop-observer-20261011-124157; affected sources/assets/config/docs and verified 0.7.0 EXE. Its SHA-256 is F5A514C27D4F536F64449FC9C5AD4F1061C3A0D19848DBFAB7AE0EEAA620A7E0. Builds/0.7.0 is untouched; previous report is QA-REPORT-0.7.0.md.

Live acceptance required: launch offline without-EAC from this prototype, wait for game-running, start read-only observer, verify selection/lineup → match → Quick Play/Main Menu → game exit, then verify restart defaults off and exit releases the handle promptly. If held, report its message; do not infer an applied feature. No agent-driven live game reads/changes or launch were performed.

Still unported: actual native F8/live player binding, outfit randomization/carry-over, arena/ball/scoreboard/rule/crowd writers, feature restoration/rearming executors, cameras, startup video/Xbox/Mixer, game Jukebox, live roster/team writes, renderer/animations, support/migration/update transactions and secondary Steam library resolution. Read-only mode observation is progress toward those features, not full 1:1 parity.
