# Rust / Tauri 2 QA 0.16.0 — explicit native player saves

Isolated parity milestone, not a WPF replacement. The full 1:1 parity goal remains active. Main App, Data and Library are untouched. The tested 0.15 checkpoint remains the fallback.

## Changes

- Connected the original browser editor's existing Save & verify action to a native Rust player-data writer. Original WPF HTML/CSS/JavaScript styling remains unchanged. Opening or refreshing the editor only reads; sample-layout previews cannot read or write game data.
- Supports names, cyberface ID, positions, jersey number, weight, height, wingspan, potential, 441 editable serializer fields, birthday and years pro. Fourteen read-only fields remain blocked. From/audio and non-player services remain unported.
- Each browser tab retains an immutable snapshot. Saving rechecks the exact configured game identity, serializer/build signatures, active database/allocation/count, record index, immutable player ID, appearance pointer and original values. An older tab cannot silently advance to another tab's saved baseline. Sixteen bounded tab leases share snapshots rather than copying a complete roster per tab.
- The short-lived Windows writer accepts only relative offsets in the owned player record/measurement spans and private non-executable read/write pages. No code patches, injection, remote allocation, thread control or runtime helper is used. Ordinary scans retain query/read-only permissions.
- Names are whole UTF-16 fields. Disjoint packed edits preserve neighboring bits, including the original helper's false Peak Age/jersey overlap case. No-op saves retain original float precision rather than quantizing the displayed weight. Shared measurement data is rejected using every table record, including unnamed records. Measurement pointers overlapping the player table are rejected. Status polling cannot silently rearm an expired tab.
- A flushed atomic intent precedes writes. Every patch checks ownership and original/neighbor values, then reads back. A reported write failure is considered possibly written. Rollback restores only exact owned states; partial/foreign data is held without overwriting. The returned editor row comes from verified read-back, not request values.
- Pending/damaged player journals block saves, Game Rules writes and new launches. A shared transaction gate serializes independent native writers/recovery. Clear finished launch resolves memory-only recovery only after a known game exit; nothing is replayed on restart. Successful edits still require saving the roster in-game.

## Verification

The final source passed 135 Rust tests, 22 frontend tests and 90 packaged GUI checks. Three additional normal-affinity repeats (4294967295) each passed all 90 checks and the original WPF outfit/player/team export validators. UI-ready times were 227.8740, 255.5501 and 235.9146 ms on this PC, not clean-machine benchmarks. The build shell remains pinned to one core because of the previously undiagnosed toolchain instability; compiler optimizations were not changed.

Coverage includes failures before/after every write, failure at every read-back point, close cancellation with guarded rollback, durable-intent/commit failures, partial/foreign values, changed owners, recycled player IDs/measurement pointers, shared/aliased measurement data, stale/expired/preview tabs, malformed recovery preservation and authenticated narrow socket routing. The Windows writer test changes only its own disposable allocation and rejects out-of-range, read-only and executable destinations. Read-only scan handles cannot write.

Final EXE: 26,045,952 bytes, built 2026-10-16T02:04:27Z, SHA-256 71DA1DA10B5E8E82C2559D941C058ABDF650CD29E29CE0917DFC06E9F8497483. Release scanning found no test-only fixture/failure markers. Main remains SHA-256 5D35A27176E91547FDCC6BCC09017A2AD0E0030C12C5A9FEA7F207BBDF0D227E. Automated tests never launch or write NBA 2K27.

The compiled no-game HTTP fixture passed: 122 unchanged static hashes (123 source assets including the bridge-modified index), source search/review, absent-preview-save rejection (400), foreign-origin rejection (403) and wrong-token rejection (403). Cached search took 11.0534 ms. Evidence: artifacts/browser-wpf-v016-final-8f04ee4b1c8147179df634350b061252/Logs/browser-http-check.json; the owned fixture exited normally. A separate real-socket unit test checks a synthetic normal save callback, invalid-tab rejection and the preview's absent native capability. The first HTTP wrapper falsely interpreted a stale shell exit code despite passing JSON; the wrapper was corrected and rerun successfully.

Build-time original-source oracles cover 345 WPF request-validation cases, four actual unchanged C# helper basic writes to its own disposable allocation, 910 pure field-patch cases and 16 birthday/years-pro plans. The 0.15 original-reader oracle remains in use for full player/team/arena/uniform decoding. .NET is used only by development-time oracles; no C# helper, .NET, Node or Electron runtime is shipped.

Live roster reading and Save & verify acceptance are pending. Browser visual acceptance also remains pending after Chrome previously returned ERR_BLOCKED_BY_CLIENT; no browser policy was changed or bypassed. HTTP fixtures and synthetic buffers do not establish live-game compatibility.

## Rollback

- Coherent source/assets/EXE fallback: Backups/v0.15.0-native-roster-checkpoint-20261015.
- Previous QA: Builds/0.15.0/Portable/App/2k Launcher Rust Prototype.exe, SHA-256 C14F220BB794D2284090174E735296DBF149E4916F298248D8633947C3D6465B.
- Pre-tab/save-integration source backup: Backups/v016-before-tab-ownership-20261015-214942.
- Original-source fixture drivers: test/RosterReadFixture.cs and test/PlayerWriteFixture.cs. Regression payloads under assets are test-only, not embedded in the release.

## Still incomplete

Team/staff/rotation/uniform saves; browser DNA/import staging, seasons/awards, From/audio, artwork/animation/presets and retained/source catalogs; full mod/profile/provider application/restoration; Blacktop outfit/arena/ball/scoreboard/crowd/unlock/scaler executors and carry-over/rearming; live F8/rule acceptance; in-game Jukebox, camera names/tracking, fullscreen MP4 timing/Xbox/Mixer; mannequin/animation rendering; WPF storage migration, secondary Steam libraries, update/rollback, diagnostics/support/hang capture and real Basketball-Reference acceptance.

Full parity is not achieved. WPF remains Main. Do not run both launchers against the game.
