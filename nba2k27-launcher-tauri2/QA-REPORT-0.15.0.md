# Rust / Tauri 2 QA 0.15.0 — native roster/team reading

This is an isolated parity milestone, not a replacement for WPF. The full 1:1 parity goal remains active. Main App, Data and Library are not modified. The coherent 0.14 checkpoint and share ZIP remain untouched.

## Changes

- Added a native Rust roster snapshot engine behind the original WPF browser editor's existing Bootstrap, Refresh and auto-attach requests. Original HTML, CSS and JavaScript remain unchanged; the React DNA tools remain separate local-draft tools.
- Reads player identity, vitals, positions, numbers, appearance item candidates, all 455 serializer fields, birthday/years-pro and the validated archetype-cache key. Reads named teams, roster types/history, arena links with separate arena/floor filenames, 32 uniform slots, all 85 base uniform fields and optional custom uniform metadata.
- The original C# reader is used only as a build-time oracle against its own synthetic allocation. It is not bundled or executed at runtime. Three full snapshots cover 299 named players, all rating values, team/arena/uniform associations, invalid UTF-16 names, NaN/infinite candidate values, high-bit optional-pointer sentinels and a custom uniform. Rust output is compared with the unchanged reader, not hand-authored expected values.
- Player serializer metadata is generated from the original C# layout and cross-checked field-by-field against the original browser schema. This adds a small embedded data document, not a .NET/Node/Electron runtime.
- Native access uses retained query/read-only process handles: no VM_WRITE, VM_OPERATION, injection, hooks or helper process. Checks configured executable path, exact process identity, competing launchers, protected processes/modules, PE/build anchors, the serializer SHA-256 and active database ownership. Tables are bounded inside the discovered allocation, aligned, non-overlapping and re-read before publication. Invalid/absent optional team/uniform directories remain empty as in WPF; no invalid pointer is followed.
- Uses the fingerprinted active owner instead of a broad process-memory scan. The live allocation/header relationship still needs gameplay acceptance; unsupported owners/layouts fail without writes rather than guessing.
- A scan has a 12-second read/decode budget, bounded chunks, one-operation ownership and cancellation checked between memory chunks and CPU-only record decoding. Closing cancels the roster operation before the browser server joins its workers. Automatic browser tabs share a pending read and use WPF's 15-second retry interval; a manual refresh is not throttled or interrupted by automatic attachment. Refresh never silently reuses an edited draft. Root/owner pointer sentinels are rejected before offset arithmetic.
- Sample-layout previews now use a separate loopback listener/port/token with no native roster capability. Their original automatic attachment cannot accidentally read the running game. Authenticated live-read RPC accepts no arguments or frontend pointers. Game-write and unported methods remain blocked.

## Verification

Targeted native snapshot, malformed-range, concurrent-operation/cancellation and no-game/foreign/protected-owner tests pass. The original reader oracle produced 100, 99 and 100 players. The 99-player case tests pure decoding only; production still requires the original 100-decoded-player threshold.

The frozen-source build passed all 116 Rust tests, 22 frontend tests and 90 packaged GUI checks. Three separate normal-affinity repeats (4294967295) each passed all 90 checks plus the unchanged WPF outfit/player/team export validators. UI-ready times were 252.6984, 239.7506 and 257.0189 ms on this PC; these are repeated local smoke timings, not clean-machine benchmarks. The real Windows own-process permission test covers both observer and roster query/read handles: reads succeed, writes are rejected without changing the target, and roster VirtualQueryEx succeeds with its query-only permission.

Final EXE: 25,808,896 bytes, built 2026-10-16T01:13:23Z, SHA-256 C14F220BB794D2284090174E735296DBF149E4916F298248D8633947C3D6465B. Build-shell affinity remains pinned to one core because of the previously undiagnosed toolchain instability; runtime repeat tests use normal affinity. Compiler optimization settings were not changed. No reproducible-clean-build claim.

The compiled no-game HTTP fixture passed: all 122 unchanged static asset hashes matched (123 source assets including the bridge-modified index), source searches/reviews worked, and saves (400), wrong tokens (403) and foreign origins (403) were blocked. A cached-search check took 9.3708 ms. Evidence: artifacts/browser-wpf-v015-final-b2e4b30de44b46e0a78f56174eb13820/Logs/browser-http-check.json. A real-socket unit check separately verifies the normal read callback, its no-argument restriction, blocked write methods, separate preview port/token and lack of preview-native capability. Release scanning confirms the C# fixture/test-only marker strings are absent. No C# helper or .NET/Node runtime is shipped.

No test in this milestone opens NBA2K27 or writes game memory. Own-process synthetic reads and socket callbacks are not a live-game acceptance claim. Browser visual acceptance remains pending after Chrome previously blocked the loopback preview with ERR_BLOCKED_BY_CLIENT; no browser policy was changed or bypassed. Main's EXE hash is unchanged: 5D35A27176E91547FDCC6BCC09017A2AD0E0030C12C5A9FEA7F207BBDF0D227E. The parity inventory's baselineMainSha256 is the original main.js source hash, not this installed EXE hash.

## Important incomplete services

The native browser snapshot is read-only QA. Save & verify, player/team DNA game application, NBA 2K23/Basketball-Reference staging into a live player, seasons/awards, staff/rotations, artwork/audio/animations, reference-name/asset catalogs and retained NBA/WNBA scan history are not ported by this milestone. Existing browser controls for those operations must fail explicitly; they must not imply a successful save.

The local draft editors and their existing offline import/export behavior are unaffected.

## Rollback and evidence

- Coherent source/assets/EXE checkpoint: Backups/v0.14.0-tested-browser-checkpoint-20261015.
- Prior tested build: Builds/0.14.0/Portable/App/2k Launcher Rust Prototype.exe; SHA-256 D9EF01387F5A48FA3262F3A64993CFB8AA4C048D74E1BC870D4E7842163E8D18.
- Original-reader synthetic fixture source: test/RosterReadFixture.cs.
- Native record/layout evidence: assets/roster-layout.json and assets/roster-read-regressions.json (the latter is test-only and not shipped).

## Remaining for full 1:1 parity

1. Finish live roster/team/staff/rotation/uniform services, verified saves and rollback, import/export/staging, seasons, artwork/audio/animations/presets and retained/source asset catalogs, keeping the original browser UI.
2. Full mod/profile/provider application, conflict review and session restoration.
3. Blacktop outfit/arena/ball/scoreboard/crowd/unlock/scaler executors, carry-over, restore/rearm, and live native F8/rule acceptance.
4. In-game Jukebox, live camera names/tracking, fullscreen startup-video timing, Xbox skip, Windows Mixer and game-audio restoration.
5. Mannequin and animation rendering/cache port.
6. WPF storage compatibility/migration, secondary Steam libraries, updates/rollback, diagnostics/support/hang capture, and real Basketball-Reference/manual browser acceptance.

Full parity is not achieved. Passing pure snapshots, transport or UI tests does not verify live loading or saving.
