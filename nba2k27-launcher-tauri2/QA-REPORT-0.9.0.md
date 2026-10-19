# Rust / Tauri 2 prototype 0.9.0 — guarded native Game Rules

Isolated QA milestone. Main WPF and its App/Data/Library are not replaced. The user live-verified the prior Steam launch and 0.8 Blacktop detector across selection, match, Quick Play, Main Menu and game exit. The new rule writer still requires live acceptance.

## Changes

- All 39 WPF GameRuleData mappings/ranges are native and checked against unchanged authoritative source by a build oracle. Numeric choices match previous live mapping checks. No rule is forced On.
- Play → Native Game Rules offers an explicit current-value read and per-rule Apply. Acknowledgement defaults off. The backend owns a one-use, 30-second snapshot of exact process identity/session, module base, mode/owner context and all 39 values. Requests cannot supply addresses or a game PID. Unsupported raw values are read-only.
- Explicit access uses a short-lived Win32 handle, exact process/path/birth checks, fresh protected/other-launcher guards, executable signatures and anti-cheat module checks at binding. Rules require verified Blacktop selection/match plus a private committed PAGE_READWRITE allocation covering the owner window. The observer remains read-only and independent.
- Durable intent saves in Data/game-rules-journal.json before a one/four-byte write. Complete context/values are checked immediately before writing and after read-back. Failure rolls back only an exact owned expected value group. Changed owner/mode/neighbors or foreign values refuse rollback and retain pending recovery. Pending/malformed journals block new writes/launches. Clearing pending memory recovery requires a fresh known snapshot with every NBA process absent; damaged journals are preserved.
- Successful choices remain session-memory changes, matching WPF. No automatic replay, game-file edits, hooks, remote allocation/thread, menu-navigation restore or shutdown writes. Restore a choice through a fresh Apply of its original value. Native F8/player binding and other game features remain unported; the overlay preview stays a dummy workflow.
- Corrected the new allocation-check access-mask gap: VirtualQueryEx needs PROCESS_QUERY_INFORMATION, not only QUERY_LIMITED_INFORMATION. Regression exercises exact rights on an owned test allocation, checks one/four-byte write/read-back and rejects a read-only page. It never accesses NBA 2K27.

## Verification

- Final strict TypeScript/Vite build succeeded. **80 Rust tests, 12 frontend tests and 62 packaged GUI checks passed (154 distinct checks).** Final pipeline profile: artifacts/smoke-476b1b9b6a804e0aaf0b9086e45d37aa. The rule mapping oracle verifies all 39 IDs, labels, offsets, widths and ranges against WPF. Valid transaction fixtures cover all 130 different before/after choice pairs, including restoring Off/Enabled/original values.
- Three additional fresh-profile, normal-all-core packaged repeats passed all 62 GUI checks each. Profiles: artifacts/final-v09-accept-1-51ffa884ad3b42fbb9d542e19681ae8f, artifacts/final-v09-accept-2-0d33970b9ee04570997336f69564e9cc, artifacts/final-v09-accept-3-a803de128ca14622b036bb2f1f3a02f2. Each process affinity was 4294967295. Internal backend/bootstrap startup: 251.0983 / 245.4137 / 276.1722 ms; idle monitor shutdown: 0.1042 / 0.1368 / 0.1004 ms. These are not click-to-visible-window, active game-reader shutdown or whole-app exit benchmarks.
- Each repeat reported zero remaining owned inert helpers; final exact-executable process query found zero QA processes. The unchanged WPF validators accepted the repeats' 3 outfit, 6 Player DNA and 3 Team DNA exports, plus the final pipeline's 1/2/1 exports. Existing oracles passed: 10,517 outfit, 2,444 Player DNA, 2,059 Team DNA and 244 season merge assertions; all 4,274 catalog players validate. These are offline compatibility tests, not live game tests.
- Real packaged screenshots of the upper grid, numeric choices and minimum-width layout were inspected. All 39 rows render, Apply stays disabled without acknowledgement/a changed value, and a refused command consumes the UI snapshot. Minimum-window grid has no horizontal overflow. All live rule reads/applies and Steam dispatch are refused in smoke mode; fixtures never access the game.
- Final EXE: Builds/0.9.0/Portable/App/2k Launcher Rust Prototype.exe, **21,555,712 bytes (20.56 MiB)**. SHA-256: **EAAAC50E7E3933CBE818FBE0A423C5E8BD3B596DD24C0BA80BE3EC3F03884729**. The share ZIP includes only the tested EXE and five metadata/document/notices files, with no Data/Library/Cache/Logs or private content. Main WPF still matches SHA-256 5D35A27176E91547FDCC6BCC09017A2AD0E0030C12C5A9FEA7F207BBDF0D227E; the previous 0.8 EXE and ZIP match their recorded hashes.

The new mapping oracle caught a Double Dribble label typo; it was corrected. A torn-read fixture initially reused its previous read counter and failed; resetting it made the intended mid-read mutation deterministic. Those intermediate failed runs are not counted as passing release tests; the full suite was rerun on the final artifact.

Fixtures cover mapped ranges, unsupported/no-op/stale changes, expiry/session/PID reuse, failed intent persistence, write failures after mutation, exact rollback, foreign values/neighbors, failed commit/rollback persistence, pending recovery and malformed journal preservation. Sparse-memory parsing rejects unsupported modes, torn state and owner changes. GUI smoke blocks live rules read/apply and Steam dispatch.

Prior compiler/environment faults remain undiagnosed. Optimizations stay unchanged; build-shell-only one-core affinity is retained. Final packaged GUI repeats must run unpinned. No reproducible clean-build claim.

## Rollback / remaining checks

Backup: Backups/v0.8.0-before-native-rules-20261011-135456, including affected source/config/assets/docs and verified 0.8 EXE. Builds/0.8.0 and its ZIP remain untouched. Previous EXE SHA-256: 15274E6CE18ECF9524251D7E65470ED3FE81FA158E9982DCC44B29B99FC42930. Archived report: QA-REPORT-0.8.0.md.

Live verification: launch offline without-EAC through this prototype; compare current reads with the stock menu; change and restore 3Pt Shots, each three-second rule and Foul Out separately. Confirm no extra rule changes and no writes outside Blacktop. Do not run WPF simultaneously. No agent-driven live game access, game launch or installed App changes were performed.

Still unported: native F8/player binding, outfit/arena/ball/scoreboard/crowd executors and restoration/rearming, full mod/profile pipeline, cameras, startup-video timing/Xbox/Mixer, game Jukebox, live roster/team writes, mannequin/animation rendering, support capture, migration/update transactions and secondary Steam libraries. Full 1:1 parity is not achieved.
