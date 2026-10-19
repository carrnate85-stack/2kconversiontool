# Rust / Tauri 2 prototype 0.7.0 — connected Steam-only QA, 2026-10-11

Isolated QA version, not installed over Main. This connects basic Steam dispatch and native observation, not the full modded Play/Blacktop pipeline.

## Changes

- Play → Check launch readiness remains read-only. A separate Steam-only QA button opens a confirmation explaining exactly which launcher features will NOT apply. Acknowledgement and the expected current game folder are validated natively; overlay windows and automated smoke mode cannot dispatch Steam.
- Native launch validates the primary-library Steam manifest and exact without-EAC option, blocks unknown process/file state, existing game/EAC/recognized launcher/backend, unowned proxy and pending recovery. The checks repeat after durable intent is saved and before explicit executable/argument dispatch. No shell/script is used.
- A per-portable-root gate refuses overlapping active launch sessions. Failed journal save prevents dispatch. Failed dispatch closes the empty-feature record, or holds it for review if closure fails. Other recognized launcher instances/backends block launch; this is not a universal cross-app/global game-launch lock.
- Native monitoring survives page navigation. It records an observed game only when folder/path, exact process creation time and a post-request start match. This observation does not grant game-memory ownership or prove without-EAC status. Identity changes, multiple games and unknown process state hold the record. Known game absence closes an observed Steam-only session.
- Pending Steam-only records reload on startup. Corrupt journals or records containing another engine's helper/file leases remain untouched. A game never observed after two minutes remains pending, not silently successful. Clear finished launch requires fresh known game absence and closes only the supported empty-feature record; it does not stop Steam/the game.
- Explicit exit handling wakes and joins the native monitor rather than waiting for its polling interval. Closing the launcher while the game runs leaves the durable record pending for recovery; it never terminates the game.
- Private helper supervision has one whitelist entry: this executable's inert QA probe. It pins the binary against write/delete sharing during validation/spawn, checks exact identity/hash/session, uses retained process handles and a kill-on-close Windows Job Object. Steam and NBA 2K27 are never placed in these jobs. No arbitrary executable/PID command is exposed, and no game-memory helper is bundled/enabled yet.

## Regression coverage

- New native tests cover durable waiting/running/closed transitions; restart recovery; unknown process state; foreign/reused game identities; save/dispatch failures; a new EAC/launcher conflict after intent; unsupported journal preservation; repeated monitor shutdown; and recognized launcher/backend filtering.
- Packaged GUI checks cover launch confirmation/acknowledgement/cancel, refusal of real Steam dispatch in smoke mode, fixture session transitions, dedicated monitor shutdown latency, actual inert helper handshake/graceful exit/Job Object forced exit, and refusal of a foreign-session stop. Only inert launcher-created probe processes run; fake Steam/game fixture files are never executed.
- Previous outfit, Player DNA, Team DNA and season oracles remain: 10,517 / 2,444 / 2,059 / 244 assertions from unchanged WPF functions. All 4,274 catalog players still validate. These are not live gameplay tests.

## Final acceptance

- Strict TypeScript/Vite release build succeeded. **58 Rust tests, 10 frontend tests and 56 packaged GUI checks passed (124 checks)**. Packaged profile: `artifacts/smoke-e9b80e59f57941ad930105e0e61b1c61`. Its native monitor shutdown measured 0.0932 ms. Final readiness and confirmation screenshots were inspected.
- Three additional fresh-profile runs of the exact final EXE passed all 56 GUI checks each, with normal all-core affinity (4294967295). Profiles: `artifacts/final-v07-accept-1-b4851ec8b4c44e74a76175615f4cad3c`, `artifacts/final-v07-accept-2-3d7a15c58b954a67a11224050e34e1ff`, and `artifacts/final-v07-accept-3-817bcc5b97e14127bf9cd545a5fd33d5`.
- Those runs measured backend/bootstrap startup at 274.3982 / 260.2086 / 269.8861 ms, and monitor shutdown at 0.162 / 0.1424 / 0.0919 ms. These are internal QA measurements, not click-to-visible-window or complete app-exit benchmarks. Each run met the dedicated 500 ms monitor threshold.
- Each fresh run reported zero remaining owned helpers. The final Windows process query found zero processes using this packaged EXE. Untouched WPF validators accepted the three runs' 3 outfit, 6 Player DNA and 3 Team DNA exports, plus the build pipeline's 1 outfit, 2 Player DNA and 1 Team DNA exports.
- A real **read-only** CLI preflight against the installed game passed, resolving app 4356430 and `steam://launch/4356430/option1`. Profile: `artifacts/native-v07-preflight-636b3180c446469cb33de03aaa3d8a0c`. It did not dispatch Steam. Installed Main EXE and Steam manifest hashes matched before/after: `5D35A27176E91547FDCC6BCC09017A2AD0E0030C12C5A9FEA7F207BBDF0D227E` and `363CE4592AC4496D5AD2EDF43CE02AA0362A1237D21A213B729B59EF6E2AA0C9`.
- Final EXE: `Builds/0.7.0/Portable/App/2k Launcher Rust Prototype.exe`, **21,315,584 bytes** (20.33 MiB). SHA-256: `F5A514C27D4F536F64449FC9C5AD4F1061C3A0D19848DBFAB7AE0EEAA620A7E0`.
- One initial fresh-profile attempt under restricted execution exited before producing a GUI report; its process query was also denied. It is not counted as a pass. The three completed repeats above ran with approved local execution permissions.

## Test-development issue and build limitation

The initial 500 ms wall-clock shutdown assertion failed under the complete unit suite pinned to one core. Three isolated runs of that same test passed in 10–30 ms. Unit tests now assert shutdown completion/idempotence; the latency requirement is measured in dedicated packaged QA, not inside the CPU-contended unit suite. A second intermediate run still used the earlier compiled shared-fixture assertion; it was removed from the source before final acceptance. No passing claim is made for either failed intermediate pipeline.

The prior Rust 1.99.0 compiler/environment fault remains undiagnosed. The optimized build retains normal release optimization and uses one-core affinity only on the isolated build shell. Fresh app repeats must run unpinned. This is not proof of a reproducible clean-build fix.

## Safety / rollback

Automated work did not launch Steam/NBA 2K27, inject, alter game memory/files, stop existing processes, install over Main or migrate Data/Library. Native fixtures are disposable; packaged profiles remain under artifacts. Inert probes are stopped only through their retained owner handles/jobs.

Rollback: `Backups/v0.6.0-before-native-launch-20261011-120334`, affected source/config/assets and verified 0.6.0 EXE. SHA-256 `B1FC2032F5223AD496AC48330F03AD8B68D28B520EE88BF332657CC64AF4EC48`. Builds/0.6.0 remains untouched. Previous report: QA-REPORT-0.6.0.md.

## Live acceptance and remaining parity

Manual next check: close WPF/other launcher instances and NBA 2K27, open the isolated prototype, check readiness, acknowledge Steam-only limitations and confirm Launch via Steam. Verify waiting → running, normal game exit → closed, then prompt launcher exit. Actual Steam/game behavior remains unverified until this is done.

No real memory helper, asset provider, game-file transaction executor or live Main Menu/Quick Play/selection detector is connected. The private restoration/rearming policies alone do not establish Blacktop behavior. Native F8, outfits/arena/ball/scoreboard/rules/crowd, cameras, video/Xbox/Mixer, in-game Jukebox, live roster/team writes, strategy/playbooks, mannequin/animations, diagnostics, migrations and update transactions remain unported. Secondary Steam libraries are still unsupported. Existing installed game mods are left untouched, not reset to stock.
