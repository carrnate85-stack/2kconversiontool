# Rust / Tauri 2 prototype 0.3.0 — QA milestone, 2026-10-11

Status: offline milestone; 1:1 parity is not achieved. Do not replace Main.

Executable: `Builds/0.3.0/Portable/App/2k Launcher Rust Prototype.exe`.
The final hash, byte size and packaged UI-check count are recorded in `Portable/BUILD-INFO.json`.

## Added

- Native exact-variant outfit composition, preserving the observed ordering of tops, layers, shoes, socks, bottoms and accessories. The embedded variant metadata contains no live addresses or game-group hashes.
- Observed-fit and compatible-layer selectors; `.2koutfit` import into a draft and export to Library/Outfits. Exports reconstruct portable identity rather than leaking local preset IDs or filesystem paths.
- Native Player DNA schema: 455 field definitions, choices, limits, compatible historical schema IDs, birthday/years-pro validation and season-total/award validation.
- `.2kplayer` import/export, category staging, local editable tabs including play types, field filtering, native local save and instant local reset. This is not a live roster editor. Attributes currently display raw mapped values.
- All 4,274 bundled NBA 2K23 players are validated against the native target schema.

## Confirmed corrections

- Fresh camera defaults used two names absent from the curated WPF list. They now use Side and Nosebleeds. Existing saved names are left unchanged.
- Shirtless was also exposed as a second raw `NO SHIRT` entry and as an unsupported women's placeholder. Only the actual men's variant is offered; automatic randomization excludes it.
- Changing/randomizing a top could retain a previous top's new variant identity. The backend now rebinds only a compatible observed variant or clears that identity, preventing misleading export.
- Export readiness now belongs to the exact current outfit draft, rather than an asynchronous validation result for a previous draft. The GUI sharing check waits for that draft to be validated and records status text, alerts and a screenshot on failure.
- Saved outfit JSON could bypass typed validation on reload. Settings, library and outfit loads now validate and have bounded reads. Invalid files are preserved, not silently replaced.
- Raw Windows atomic-renaming calls used ordinary paths, unlike Rust's long-path-aware filesystem reads/writes. A fresh-profile repeat exposed a 272-character export temporary path and error 0x80070003. Rename paths now use canonical extended Windows paths without lossy filename conversion, and JSON temporaries use a short independent basename. A regression checks create/replace and media publication above 260 characters, including duplicate-import rejection.
- The new DNA card initially missed the established content-padding class; screenshot review caught and corrected it.
- Unchanged generated assets were being rewritten on every preparation. Asset/fixture/parity generators now retain identical files, reducing unnecessary rebuild work. This is not a claim that the earlier Rust compiler fault has been diagnosed.

## Regression coverage

- 10,517 native outfit-composition cases compared with the untouched WPF JavaScript composer.
- 2,444 native Player DNA validation cases compared with the untouched WPF validator.
- Boundary checks for all 455 mapped fields, malformed dates, invalid seasons/awards, unsupported schemas, unsafe portable metadata and incompatible outfit variants.
- Catalog-wide native DNA validation, local persistence/export round trips, saved-state corruption/size limits and rejection without overwriting source data.
- Actual packaged Tauri/WebView2 UI checks cover the existing profile/media/catalog/overlay workflows plus outfit round trips/export, DNA birthday/years-pro save, reset, play-type controls and export.

Final source passed strict TypeScript/Vite compilation, 34 Rust tests, 10 frontend tests and 31 packaged GUI checks (75 distinct checks across those suites). Oracle cases are assertions inside the Rust suite, not additional independent end-to-end gameplay tests. The GUI schema-readiness check was corrected to wait for the New local player button to become enabled before clicking it; a prior run had clicked the correctly disabled button.

## Final packaged verification

- EXE: 21,069,312 bytes (20.09 MiB).
- SHA-256: `C896985173044A862B33989D1A11C3DDCDDB39641D636F07D605C631D138CBD1`.
- Full pipeline: `artifacts/smoke-01866f4b099342b9b99161272f9b5e79/Logs/ui-smoke.json`, 31 checks passed. This compiler/build process was pinned to one CPU core as a diagnostic; no persistent system setting changed.
- Same final EXE: five additional independent fresh-profile runs, unpinned, each passed all 31 checks. Five `.2koutfit` and five `.2kplayer` exports also passed the unchanged WPF validators. The test EXE had no remaining processes after exit.
- Unpinned native bootstrap readings: 240.77, 252.30, 248.18, 254.70 and 249.75 ms. These are internal bootstrap measurements, not click-to-first-window timings or a performance comparison with WPF.
- Reports/screenshots: `artifacts/final-v03-longpath-1-23ff72b79c32497cbc149dbc0a73f26a`, `artifacts/final-v03-longpath-2-bc79e743c5b447898064b31c4480e0c7`, `artifacts/final-v03-longpath-3-f9ad9283309d4c778da9a1ded3672974`, `artifacts/final-v03-longpath-4-e6893af58e224082a2c39356d443f94d`, `artifacts/final-v03-longpath-5-5cef130baf4247059287608b2fc97c92`. Each has a Logs/ui-smoke.json and native WebView2 screenshots.

## Safety and rollback

No game process was launched, injected into or modified. Main App, Data and Library were not installed over or migrated. The WPF app was not open during the reference-window check, so no reference session was started; source and existing screenshots were used for styling.

Rollback: `Backups/v0.2.0-before-outfit-parity-20261011-092232`, containing affected pre-change sources, metadata/configuration and the original 0.2.0 executable. Builds/0.2.0 remains available.

File chooser interaction itself still needs manual QA. Legacy shared outfits beyond the three-accessory UI limit or with unpreservable implicit pieces are rejected, not modified. Converted rating displays, editable season/award grids, live roster connection and verified game saves remain unported. Unsaved local Player DNA edits are not persisted when navigating away; use Save local draft first.

Major remaining parity: native launch/helper ownership and session restoration/rearming; real F8/window integration; live Blacktop customization and game rules; startup-video/Xbox/Mixer behavior; Jukebox game bundles; Team DNA/editor/coaches/rotations/playbooks; mannequin and animation rendering; settings/library migration, diagnostics and update transactions. These require separate offline tests and applicable live gameplay acceptance before Main promotion.

The prior v0.2.0 and this v0.3.0 build both encountered Rust compiler STATUS_ACCESS_VIOLATION failures. Reducing application optimization to level 1 also failed, with STATUS_ILLEGAL_INSTRUCTION; that experimental profile override was removed. Windows events place the faults in rustc_driver, and recent events include another project's compiler too. This is evidence of a shared compiler/environment problem, not proof of a hardware or toolchain diagnosis. No release compiler fault is hidden by Build.ps1. Three subsequent standard optimized compilations passed with the build process pinned to one CPU core; the final app also passed unpinned repeats. This changes no system-wide settings and is not proof of a root-cause fix or reproducible clean builds.

An earlier packaged candidate passed the UI suite but failed a longer fresh-profile repeat at outfit export. The selected preset was valid. Added failure diagnostics reproduced Windows path error 0x80070003 with an over-limit temporary path, exposing the raw rename bug above. The production freshness guard and deterministic sharing fixture are retained as separate protections. Do not treat an earlier candidate's successful run as verification of the final source.
