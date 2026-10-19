# NBA 2K27 launcher — QA085 review remediation

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
