# NBA 2K27 launcher — development source checkpoint, October 25, 2026

Current package version: 0.53.46. This checkpoint includes the latest local launcher source, roster-editor updates, standalone shared-renderer integration, and the development-only Custom Blacktop catalog with right-click editing and levels-folder browsing. It does not publish an application release or promote the dev features to production.

**Known issue:** custom Blacktop file routing remains under investigation. Drew League works when manually installed as `arena_700_int.iff` / `arena_700_int_floor.iff`, but the catalog test produced a black arena. The live roster retained the generated custom filenames; the provider handle inspection found no open arena IFFs. A Ready mapping status alone does not establish successful asset loading. Automated fixture passes do not establish in-game acceptance.

Latest focused validation: 106 arena tests, 19 provider tests, 18 frontend tests, TypeScript, the isolated headless import/edit/cancel/save flow at 1100px and 600px, and the dev executable build passed. Two opt-in tests in those native suites were ignored. Installed runtime data, personal settings, session receipts, local diagnostics, game assets and compiler output are excluded from this source checkpoint.

The shared renderer is generated from the Exporter's canonical `shared/model-preview` package at build time. Set `NBA2K_MODEL_PREVIEW_ROOT` when syncing on another workstation. The checked-in generated source snapshot records its revision in `dev-assets/model-preview/PREVIEW-REVISION.json`; edit the canonical package rather than these generated mirrors. The packaged viewer is standalone at runtime. `Start Dev.vbs` opens an already installed local dev build and does not synchronize the Exporter or compile on startup.

For the current viewer-enabled dev backend, use `cargo build --locked --no-default-features --features dev-model-preview --bin nba2k27-launcher-tauri2` with Vite on localhost:5173. Native payloads and renderer worker packaging still require their documented build inputs; a fresh-machine build has not been validated by this checkpoint.

## Historical QA085 checkpoint

Complete Rust/Tauri 2 launcher source and required embedded assets, including QA085 review fixes. The retained native outfit payload is unchanged from QA084. This is an unpromoted development checkpoint; new editor paths still need live-game acceptance. Package version remains 0.53.1. The older README is historical; use NEXT-CHAT-HANDOFF-QA085.txt and docs/reviews/CODEX-RESPONSE-2026-10-19.md for current status.

## Build on Windows

Requires the MSVC Rust toolchain supporting edition 2024, Visual Studio C++ Build Tools, Node.js, pnpm and the installed Microsoft WebView2 runtime. From this directory:

```powershell
pnpm install --frozen-lockfile
pnpm build
cargo build --locked --release --bin nba2k27-launcher-tauri2
```

Development backend: `cargo build --locked --no-default-features --bin nba2k27-launcher-tauri2`, with Vite on localhost:5173. Building does not install or launch the game. Historical Build.ps1/Dev.ps1 use the original workstation toolchain layout; direct commands avoid that private layout.

Checked-in assets include native loader payloads and compressed generated data required by Rust include macros. Native sources/build scripts are included. Regenerate the browser editor with `node scripts/prepare-browser-workspace.mjs`; sibling reference sources are included in their original relative locations. No installed game EXE or game dump is included. Actual gameplay requires a separately installed supported game.

## Validation

The original exact QA084 image passed 1,497 Rust tests (54 ignored), 224 frontend tests, 14 browser regression scripts and 10 native fixtures. Owned transactions also passed 32 helper-death cycles and 13 fault scenarios. This does not establish a clean build on another machine or live-game acceptance. Historical QA073 Rust-test and QA078 compiler crashes remain unresolved.

```powershell
pnpm test
node scripts/test-season-import-staging.mjs
cargo test --locked --no-default-features --bin nba2k27-launcher-tauri2 roster_v2_ -- --test-threads=1
cargo test --locked --no-default-features --bin nba2k27-launcher-tauri2 player_catalog -- --test-threads=1
cargo test --locked --no-default-features --bin nba2k27-launcher-tauri2 -- --skip roster_v2_ --skip player_catalog --test-threads=1
```

Use one compiler job on the original workstation. Its documented affinity workaround is not established as a universal requirement.

## Scope

Includes backend/frontend, native source and required payloads, bundled assets, tests, scripts, schemas, documentation and source dependencies. Excludes personal Data/Library/Cache/Logs, dependency installs, compiler output, staged/installed binaries, backups, crash dumps and optional external thumbnail caches; local originals remain untouched. Historical QA starters reference excluded local staged builds. Checkpoint receipts reference local evidence archives that are intentionally excluded.

Automatic custom-arena restoration after a roster reload within the same launch remains held until a committed native roster epoch is proven. Existing ownership, durability and no-replay guards remain.

## QA085 review work

Confirmed review fixes cover transaction recovery, shared native write admission, media bounds, filesystem cleanup, overlay command authority and frontend state. External unsigned updates are disabled until a publisher verification key is provisioned; explicit PreviousApp rollback remains guarded. WIC process containment, safe malformed-name row handling and other deferred items are listed in the review response. No gameplay acceptance, installation or promotion is implied.

QA085 final validation: 1,529 Rust tests passed (54 explicitly ignored), 232 frontend tests, 15 browser/source scripts, TypeScript/Vite/dev builds, and two read-only installed-disk audits. Isolated native transaction fixtures also passed; see the review response and QA085 handoff for exact-image qualifications.
