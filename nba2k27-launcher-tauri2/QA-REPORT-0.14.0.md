# Rust / Tauri 2 QA 0.14.0 — original WPF browser editors

Isolated parity work, not a replacement for Main. No Main App installation, storage migration or game access was performed. Main's EXE remains SHA-256 `5D35A27176E91547FDCC6BCC09017A2AD0E0030C12C5A9FEA7F207BBDF0D227E`.

## This milestone

- Reused the original WPF roster/team HTML, CSS and JavaScript rather than redesigning them in React. The default-browser workflow is restored through explicit buttons, with separate clearly labeled player/team sample-layout previews. The 123 embedded source assets are unchanged and hash-checked; only the original browser bridge and a small inert QA layout-bootstrap script are inserted into the served HTML. Seven old hidden QA pages are excluded. Source hashes are in `assets/wpf-browser-manifest.json`.
- Added a Rust-owned loopback browser transport: a temporary port on 127.0.0.1, BCrypt-generated 256-bit session token, exact Host/Origin checks, constant-time token comparison, no CORS grants, narrow static asset routes and RPC methods, no arbitrary paths or native-command forwarding. Headers/body sizes, framing, request duration and concurrent workers are bounded. Responses disable caching and framing. The launcher stops its listener and pending reads on exit; it does not stop the user's browser.
- Connected the original browser's bundled NBA 2K23 search and player/category/season review to Rust, without live roster access. Search now matches WPF's accent normalization, token matching, category context and team labels. Compared 57 search responses and five full player/season reviews with the unchanged WPF implementation. JSON numbers are compared as JavaScript Numbers, so 220 and 220.0 are equivalent; their numeric values are preserved.
- Browser live roster/team loading, imports/staging, game saves, artwork/audio/animations and presets remain explicitly unported. The original pages fail those actions without any game read/write or file-picker dispatch. Layout samples are not disguised as live players or teams. The existing React DNA tools remain separate local-draft tools, not original-editor styling parity.
- Added a compact two-step catalog/category import popup to the separate local Player DNA tools: native paging/search/category filters; clear/all supported category controls; exact selected-source review; guarded staging that preserves unselected fields. Cancel/back preserves search. No automatic local Save or game write occurs. This is useful offline functionality, but not a substitute for the original browser editor.
- Guarded lazy player/wardrobe initialization so simultaneous cold requests do not decode the same embedded document multiple times. Added a concurrent cache regression. Fixed the native main-window title retaining an older version.

## Verification

Current source passes 106 Rust tests and 22 frontend tests. Includes browser asset/hash/routes, authenticated/unauthenticated and foreign-origin/host RPC, blocked save methods, ambiguous HTTP framing, cryptographic token, pending-read/listener shutdown, concurrent cold caches and exact WPF catalog comparisons. Production browser opening is refused in smoke mode; browser inspection uses a separate private fixture with no game access.

Resumed October 15, 2026 (America/New_York), without changing the saved executable. Final packaged build passed all 90 GUI checks and WPF sharing validators on three normal-affinity repeats (process affinity 4294967295): UI-ready times 316.3801, 282.0922 and 260.0570 ms. These are local repeated smoke measurements, not clean-machine benchmarks. EXE: 24,379,904 bytes, SHA-256 `D9EF01387F5A48FA3262F3A64993CFB8AA4C048D74E1BC870D4E7842163E8D18`, built 2026-10-11T20:19:16Z. The earlier shutdown-interrupted repeat is not counted as a pass.

The final binary passed the compiled HTTP fixture after the catalog-fingerprint optimization: 122 unchanged asset hashes checked (123 source assets including the bridge-modified index), source search returned two Curry records, review returned 13 seasons, and game-save requests (400), wrong tokens (403) and foreign origins (403) were rejected. One cached-search check took 10.4415 ms, not a general latency guarantee. The fixture exited normally after its own stop marker; no game was accessed. Evidence: `artifacts/browser-wpf-v014-resumed-56f060bf41ab4d42a80c314eb3d375a0/Logs/browser-http-check.json`. Repeat evidence is in `artifacts/resumed-v014-repeat-{1,2,3}-*/Logs/ui-smoke.json`.

Chrome previously blocked the loopback visual preview with ERR_BLOCKED_BY_CLIENT; browser policies were not changed or bypassed. Default-browser visual acceptance remains pending. Source-byte identity and HTTP tests do not establish rendered visual acceptance. No game or installed App was changed. A six-file allowlisted share packager now verifies each ZIP entry against its source hash, excludes Data/Library/cache/logs/session tokens, and refuses to overwrite an existing ZIP.

Automated Steam, observer, rule writes, F8 production binding and real-network requests are refused in smoke mode. Existing private owned-window/memory/helper tests do not establish live-game acceptance.

Prior milestone details remain archived in `QA-REPORT-0.13.0.md`: WPF season arithmetic, 455 presentation fields, rating/salary/unit/color conversions, Player/Team DNA and sharing, guarded rules/F8 ownership, portable storage, previews and restoration/rearming fixtures. No old verified release was deleted.

Compiler/environment instability remains undiagnosed. Build-shell one-core affinity is retained; compiler optimization settings are unchanged. Normal-affinity GUI repeats are separate. No clean-machine startup or reproducible-clean-build claim.

## Rollback

Coherent tested fallback: `Backups/v0.13.0-before-player-import-popup-20261011-154300`, including source/assets and the 0.13 EXE (`C16A76897749540AFA55B17A9FE0B287C8E9DFD3E4BA661E9D13F5D5081DD2F9`). Additional affected-file snapshot: `Backups/v0.14.0-before-original-browser-workspace-20261011-160000`. A coherent current source/assets/EXE checkpoint is saved at `Backups/v0.14.0-tested-browser-checkpoint-20261015`. `Builds/0.13.0` and its tested share ZIP remain untouched. Main remains your WPF build; no rollback of user Data/Library is necessary.

## Remaining for 1:1 parity

1. Live roster/team/staff/rotation/uniform loaders; browser import/staging, before/after review and verified rollback-safe game saves; artwork, audio, animations and presets. Keep the original browser UI unchanged while porting these services.
2. Full mod/profile/provider application pipeline, conflict review and session-aware restoration.
3. Native Blacktop player/outfit/arena/ball/scoreboard/crowd/unlock/scaler executors, verified carry-over, restoration and Quick Play/Main Menu rearming. New native F8 and Game Rules writes still need explicit live acceptance.
4. In-game Jukebox codecs/bundles/playback/restoration; camera names/tracking; fullscreen startup-video timing, Xbox skip, Windows Mixer and game-audio restoration.
5. 3D mannequin/mesh/texture rendering and animation cache/preview port.
6. Main settings/library compatibility and migration, secondary Steam libraries, update/rollback transactions, feature-health diagnostics, redacted support bundles and hang/crash capture. Basketball-Reference real-page fetching needs manual acceptance; parser/network-policy tests are offline.

The versioned 143-route inventory remains `complete: false`. Visible controls and saved preferences do not establish game functionality. Full 1:1 parity is not achieved.
