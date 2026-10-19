# Rust / Tauri 2 QA 0.19.0 — native uniform saves

Isolated parity milestone, not a WPF replacement. Full 1:1 parity remains in progress. Main, Data and Library are untouched. The coherent tested 0.18 source/assets/executable checkpoint remains the fallback.

## Changes

- Original browser uniform Save & verify is connected to Rust for all 52 existing controls: 21 colors, 28 packed choices and three headband settings. Original WPF HTML/CSS/JavaScript styling and controls are unchanged.
- Fixed a confirmed legacy validation gap in the native port: changed dropdown values were compared to the old values, rejecting valid edits. The backend instead checks the immutable tab baseline and expected original packed words before planning the desired values.
- Team/uniform IDs, filenames, pointers and unmapped bits are preserved. A uniform shared by valid slots remains shared, matching WPF. Native-only snapshot metadata owns addresses; browser requests cannot choose write addresses.
- The short-lived native writer accepts only 25 mapped four-byte words in private non-executable memory. Exact process birth/path, supported build/serializer, database and table bounds, selected team/slot, uniform identity and all referencing team slots are checked before writes. Every changed word is read back against the full expected uniform record.
- Durable intent precedes writing. Failed writes, read-back, cancellation and completion-record failure reverse only exact owned prefixes. Partial/foreign values or changed ownership hold recovery; memory writes are never replayed. Pending/malformed uniform journals block player/team/uniform/rule writes and new launches. Valid pending records clear only after known game exit; malformed records stay untouched.
- Successful saves reload actual roster/team/uniform data, including shared slots. Other tabs keep their old baselines. Failed post-commit refresh invalidates only the saving tab and explicitly reports that the verified save occurred, preventing silent replay.
- Uniform saves are unavailable in isolated layout previews. No Node, .NET, Python or Electron runtime was added.

## Verification

Final source passed all 168 Rust tests, all 23 frontend tests and TypeScript compilation. The unchanged WPF JavaScript validator generated 911 request cases; the original C# writer generated 115 own-process cases (107 valid, eight rejected) covering every control at both boundaries, shared slots, immutable identities and invalid requests. These development-only fixtures do not access NBA 2K27. Final uniform oracle: artifacts/uniform-write-oracle-cd8559417a3f482bb8a93acc53ca6e33.

The packaged build passed 90 GUI checks. Three sequential normal-affinity repeats each passed all 90 checks and the unchanged WPF outfit/player/team sharing validators. Internal UI-ready measurements were 254.402, 243.2929 and 247.99 ms on this PC; these are not pre-process startup or clean-machine benchmarks. Runtime affinity was normal (4294967295). The build shell remains single-core for the previously undiagnosed compiler instability; compiler optimization settings were not changed.

One earlier single-core parallel run passed 167/168 tests but exceeded the existing browser-shutdown test's one-second wall-clock assertion. That test passed in isolation. The final full pipeline ran Rust tests sequentially and passed 168/168, including the same shutdown assertion; no production timeout was relaxed. A new test-only helper-reference compilation error was corrected before that final run.

The compiled browser fixture passed 122 unchanged asset hashes (123 source assets including the bridge-modified index), source search/review and category/season selection (200), absent player/team/uniform save capability in previews (400 with valid tab IDs), foreign-origin rejection (403) and wrong-token rejection (403). Search took 9.4556 ms. Evidence: artifacts/browser-wpf-v0190-final-e933191fbc3344ca8914d7aa407ba9ea/Logs/browser-http-check.json. All four repeat/browser fixture processes exited normally.

Final EXE: 26,460,160 bytes, built 2026-10-16T11:27:58Z, SHA-256 40670FE7DE592B83B003D2DEFF135BD282FE4F8BD80798D2DE0CCCAE615821AC. Test-only fixture markers are absent. Main remains SHA-256 5D35A27176E91547FDCC6BCC09017A2AD0E0030C12C5A9FEA7F207BBDF0D227E. Pipeline smoke: artifacts/smoke-9ac0cbc41d2d4d6fb569c498ca26dbbf. Machine-readable evidence is Builds/0.19.0/QA-VERIFICATION.json. The source checkpoint and six-file share package use explicit allowlists and per-file hashes, excluding Data, Library, caches, logs and tokens.

No automated test launches or modifies NBA 2K27. Live uniform/player/team/rule writes, separate floor persistence and manual browser/dialog behavior still need acceptance. Original-browser visual acceptance remains pending after the prior Chrome ERR_BLOCKED_BY_CLIENT; no security setting was changed or bypassed.

## Rollback

- Coherent fallback: Backups/v0.18.0-native-team-save-checkpoint-20261015, with CHECKPOINT-MANIFEST.json.
- Prior QA EXE: Builds/0.18.0/Portable/App/2k Launcher Rust Prototype.exe; SHA-256 074597CB4EF59F25F7B9D149CB161D260E8D999186792E7008CE23DAFC6B9C58.
- Pre-edit backup: Backups/v019-before-uniform-backend-20261015-230200.
- Current coherent source/assets/EXE checkpoint: Backups/v0.19.0-native-uniform-save-checkpoint-20261016, with CHECKPOINT-MANIFEST.json.
- Development-only oracles: scripts/uniform-write-regressions.mjs, test/UniformWriteFixture.cs and generated assets/uniform-*.json. They are not bundled runtimes.

## Remaining parity

Live staff/rotations/strategy/playbooks and Team DNA browser transfer; live seasons/awards/allocation transactions; custom source catalogs, From/audio, artwork/animation, 2KDB/Auction; complete mod/profile/provider application/restoration; Blacktop outfit/arena/ball/scoreboard/crowd/unlock/scaler executors/rearming; Jukebox, live camera tracking/names, fullscreen MP4 timing/Xbox/Mixer; mannequin/animation rendering; WPF storage migration, secondary Steam libraries, updates/rollback, diagnostics/support/hang capture; real Basketball-Reference and live F8 acceptance.

Full parity is not achieved and the goal remains active. WPF remains Main. Do not run both launchers against the game.

## Next delivery priority

Per the user's latest direction, prioritize actual modded launch/provider routing and restoration, then Blacktop outfit/arena/ball/scoreboard application, selection-to-match carry-over and Main Menu/Quick Play rearming. Do not continue expanding optional editor fields ahead of those core workflows. The current package still does not apply these game features. The existing WPF flow and native loader protocol have been traced; its old provider is a self-contained .NET 8 worker, so a Rust provider port is needed to retain the prototype's no-bundled-runtime design.
