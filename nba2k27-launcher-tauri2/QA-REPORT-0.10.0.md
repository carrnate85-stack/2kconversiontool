# Rust / Tauri 2 prototype 0.10.0 — native F8 QA

Isolated parity milestone, not a replacement for Main WPF. Data and Library in Main remain untouched. Native F8 and rule writes need live acceptance; prior Steam-only launch and Blacktop boundary observation were reported working by the user.

## Changes

- Explicit opt-in native F8 uses this prototype's launched-process identity (PID, executable path, birth time) and a unique visible game-client window. The overlay has that exact HWND as its native owner; arbitrary PIDs/HWNDs cannot be passed by the frontend. Unsupported/missing/minimized/ambiguous game windows hold without guessing.
- F8 is registered only during foreground verified Blacktop selection/match. Foreground changes are checked at 40 ms; mode/window/process checks at 250 ms. Unknown/loading/Main Menu/Quick Play states hide the bar and release the hotkey. Auto-show at selection and auto-hide at match use saved preferences. No fallback hook or keyboard injection.
- The window is initially hidden and is not shown before the React UI reports its measured bar height. Physical client coordinates support negative monitor positions and avoid title-bar/fullscreen guesses. Catalogs expand height only; they preserve width, top position and bar geometry, and collapse on the same button. The owned window stays warm between catalogs and hides rather than leaving another process.
- Offline preview and native F8 share the same React overlay and 39-row native-rule panel. Offline rule examples are local, not game access. Live F8 uses the already-guarded one-rule writer; all prior snapshot expiry/ownership/read-back/rollback/journal safeguards remain. Other catalogs are local previews, and live player binding is explicitly unavailable, not a fake Stephen Curry.
- Fixed stale monitor/UI layout races with revision/epoch commits. Fixed Hide being undone on the next poll. Avoid repeated unchanged position/size/show/focus operations. Focus returns only from this owned overlay to the same verified game window, never from another app. Destroyed owned windows disable the binding; re-enabling creates a new window for the new game session.
- Native F8 monitor and hotkey shut down before the existing session monitor. Game/Steam are not stopped and no game memory or files are modified by the window/hotkey code.
- A missing UI-ready message or lost two-second UI heartbeat expires after 15 seconds, hiding F8 and releasing the hotkey without game actions. Rule access also refuses an expired UI lease. No F8 registration occurs before UI readiness.
- Corrected two catalog gaps exposed by the owned-window fixture: 9,738 additional WPF accessories were not included in the selectable Rust wardrobe, and “shared” renderer labels were incorrectly excluding 457 shoe choices from gender-filtered pickers. The native wardrobe now has 38,108 rows, all 20,939 WPF accessories in 19 families and all 503 shoes with their exact observed/additional WPF fit pools. New choice coverage does not imply new embedded renders or game carry-over compatibility. The UI still enforces three accessories and randomization preserves shoes.

## Verification

Final TypeScript/Vite, 13 frontend, 88 Rust and 74 packaged GUI checks passed. Tests include hidden own-window geometry, wrong-PID rejection, stale layout/hide commits, a 15-second UI watchdog, exact WPF accessory/shoe coverage and an inert F24 hotkey queue/release test. Three additional normal-affinity GUI runs passed all 74 checks (startup to React bootstrap: 299.3 / 257.6 / 260.0 ms). All shared exports passed the WPF validators. These are isolated warm-machine measurements and offline tests, not game acceptance or clean-machine benchmarks. Automated production Steam, observer, rules and F8 enable calls remain refused in smoke mode.

Final EXE: 21,696,000 bytes; SHA-256 79A895E031759341C716A35CA0DFF993D2217BB38ADAE013DB15067EA5C79DDB. Packaged evidence: artifacts/smoke-e7f66902774343cbbc18c7822d4cd16c; unpinned repeats: artifacts/final-v010-watchdog-{1,2,3}-*. Final package not installed over Main.

The packaged native overlay fixture uses the exact production window builder, owned by the hidden QA main window. It never attaches to NBA 2K27 or reserves F8. It checks owner/visibility/physical dimensions, catalogs' same-button collapse, shared rules, accessories and shutdown. All screenshots are from this fixture or the offline preview.

Prior compiler/environment faults remain undiagnosed; compiler optimizations are unchanged and build-shell one-core affinity is retained. Final GUI repeats must be unpinned. No clean-build reproducibility claim.

## Rollback and remaining work

Backup: Backups/v0.9.0-before-native-overlay-20261011-141720 (affected sources/config/docs and verified 0.9 executable). Builds/0.9.0 remains untouched. Archived report: QA-REPORT-0.9.0.md. Previous EXE SHA-256: EAAAC50E7E3933CBE818FBE0A423C5E8BD3B596DD24C0BA80BE3EC3F03884729.

Live checks: enable native F8 after a prototype Steam-only launch; verify top placement, F8 toggle, same-button catalogs, Game Rules current reads, changed/restored numeric values, foreground loss, selection/match/menu rearm and game/launcher exit. Do not run WPF simultaneously. No agent-driven live access, game launch or Main installation was performed.

Still unported: live player binding and outfit/arena/ball/scoreboard/crowd executors with restoration/rearming; full mod/profile launch pipeline; game Jukebox; live camera tracking/names; startup-video launch timing/Xbox/Mixer; live roster/team writes; mannequin/animation rendering; support/hang capture; WPF storage migration/update transactions and secondary Steam libraries. Full 1:1 parity is not achieved.
