# Rust / Tauri 2 QA 0.18.0 — native basic team saves

Isolated parity milestone, not a WPF replacement. Full 1:1 parity remains in progress. Main, Data and Library are untouched. The coherent tested 0.17 source/assets/executable checkpoint remains the fallback.

## Changes

- Original browser Team Editor Save & verify now calls a typed Rust backend, without altering the original WPF HTML/CSS/JS or introducing a new editor design. Names, city/abbreviations, logo, roster type, historic year and arena/floor are supported. Destination Team ID stays unchanged; staff, rotation, playbook/strategy and uniform writes remain unported.
- Separate arena/floor preserves WPF copy-on-write behavior: clone a populated arena into the first unique, unused empty reserved slot, substitute only its floor and preserve the reserved ID, then link the selected team. Shared donors are never modified. Ambiguous/missing IDs, missing floors, occupied slots and stale values fail before writes.
- Private immutable snapshot metadata supplies all addresses and ownership. Native writes require exact process birth/path, supported build/serializer, active database/table bounds, nonoverlapping player/team/arena/uniform directories, team identity, donor digest and other-team references. The short-lived writer permits only two fixed private non-executable data spans, not arbitrary browser addresses or code pages.
- Durable intent precedes every transaction; every patch has read-back verification. Reported failures, cancellation and completion-record failure reverse only exact owned prefixes. Foreign/partial writes or changed owners hold recovery. The team link is restored before clone data. Recovery is never replayed after restart.
- Pending or malformed team journals block further native player/team/rule writes and new launches. Valid pending records clear only after known game exit. Malformed records are preserved.
- Successful saves reload actual team/arena/player links. A failed post-save read-only refresh invalidates only the saving tab and explicitly reports the verified save, avoiding silent replay or misleading rollback claims. Other tabs retain their original baseline.
- Expanded exact WPF request validation covers JS number/string/array coercion, UTF-16 lengths and exponential-number formatting. Control-character text is refused before native planning. Full clone recovery has an explicit bounded 512-KB journal limit checked before publication.
- Closing/deadline is rechecked immediately after ownership inspection and before each new write. The original success response counts eight verified team fields plus the optional clone, while physical chunk writes are a separate diagnostic; the existing toast no longer miscounts chunks as fields.
- Normal browser team-save capability is removed from isolated layout previews. No .NET, Node or Electron runtime was added.

## Verification

The final source passed 156 Rust tests, all 23 frontend tests and the TypeScript build. Original-source comparison covers 263 JS validation cases and 12 original C# writer cases, executed only against the fixture's own synthetic allocation. Six valid cases match final team/arena bytes; six invalid cases leave bytes unchanged. Failure tests exercise every planned write before/after failure, every read-back, every cancellation prefix, closing during pre-write inspection, durable-record failures, partial/foreign values, owner changes and forged patch spans. Native permission tests write only to the test process's own allocation.

The packaged build passed 90 GUI checks. Three further sequential normal-affinity runs each passed all 90 checks and the unchanged WPF outfit/player/team file validators. Internal UI-ready times were 244.0889, 269.4414 and 284.3452 ms on this PC, not clean-machine or pre-process startup benchmarks. Runtime affinity was normal (4294967295); the build shell alone remains pinned to one core for the previously undiagnosed compiler instability. Compiler optimization settings were not changed.

The compiled loopback browser fixture passed source search/review, category and portable season selection (200), 122 unchanged asset hashes (123 source assets including the bridge-modified index), absent player/team save capabilities (400 with valid tab IDs), foreign-origin rejection (403) and wrong-token rejection (403). Search took 18.9474 ms. Evidence: artifacts/browser-wpf-v0180-final-042d54f4aa3c40a783f9c9b6e7ec854e/Logs/browser-http-check.json. Its exact owned process exited normally. No owned QA process remained after verification.

Final EXE: 26,298,368 bytes, built 2026-10-16T02:58:35Z, SHA-256 074597CB4EF59F25F7B9D149CB161D260E8D999186792E7008CE23DAFC6B9C58. Test-only fixture markers are absent. Main remains SHA-256 5D35A27176E91547FDCC6BCC09017A2AD0E0030C12C5A9FEA7F207BBDF0D227E. Pipeline smoke: artifacts/smoke-f6329d754acc42efa522c082031b2d5e. The source/executable checkpoint and six-file share ZIP use explicit allowlists and per-file hashes, excluding Data, Library, caches, logs and tokens. Machine-readable evidence is Builds/0.18.0/QA-VERIFICATION.json.

No automated test launches or modifies NBA 2K27. Live team/player/rule writes, separate floor persistence and manual browser/dialog behavior still need acceptance. Original-browser visual acceptance remains pending after the prior Chrome ERR_BLOCKED_BY_CLIENT; no security setting was changed or bypassed.

## Rollback

- Coherent fallback: Backups/v0.17.0-browser-transfers-checkpoint-20261015.
- Prior QA EXE: Builds/0.17.0/Portable/App/2k Launcher Rust Prototype.exe; SHA-256 4CBF32B1202C4D02FDFAA72B78F1E2C0B29BB88F7C262C3771219CA15A828FD1.
- Pre-edit backup: Backups/v018-before-team-backend-20261015-223400.
- Current source/assets/EXE checkpoint: Backups/v0.18.0-native-team-save-checkpoint-20261015, with CHECKPOINT-MANIFEST.json. The prior 0.17 checkpoint is preserved separately.
- Development-only oracles: scripts/team-write-regressions.mjs, test/TeamWriteFixture.cs and generated assets/team-write-*.json. They are not bundled runtimes.

## Remaining parity

Live staff/rotations/strategy/playbooks/uniforms and Team DNA browser transfer; live seasons/awards/allocation transactions; custom source catalogs, From/audio, artwork/animation, 2KDB/Auction; complete mod/profile/provider application/restoration; Blacktop outfit/arena/ball/scoreboard/crowd/unlock/scaler executors/rearming; Jukebox, live camera tracking/names, fullscreen MP4 timing/Xbox/Mixer; mannequin/animation rendering; WPF storage migration, secondary Steam libraries, updates/rollback, diagnostics/support/hang capture; real Basketball-Reference and live F8 acceptance.

Full parity is not achieved and the goal remains active. WPF remains Main. Do not run both launchers against the game.
