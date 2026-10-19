# Rust / Tauri 2 prototype 0.2.0 — QA milestone, 2026-10-11

Status: **offline milestone; 1:1 parity not achieved; do not replace Main**.

Executable: `Builds/0.2.0/Portable/App/2k Launcher Rust Prototype.exe`

SHA-256: `D279E7C3158875C220E63DDF00C77C3DFD1A988AAF4938DB51122E86358281E5`

EXE: 19,913,728 bytes (18.99 MiB). No .NET, Node, Python or Electron runtime is shipped. Shared WebView2 is required. This incomplete prototype is not a feature-equivalent size comparison with WPF.

## Added in this milestone

- Native saved profiles, searchable folder tree and tags, local playlists and ordered songs.
- Saved camera enabled/order/height/zoom values; lock defaults Off; expanded startup-video library.
- Saved outfit library, all accessory slots, fit-specific choices and clothes/accessory randomization that preserves shoes.
- Full baseline choice metadata: 28,370 garment/shoe/accessory records plus manual Shirtless, 426 ball choices including Currently installed, 154 arena choices and 19 scoreboards. Existing exclusions are preserved.
- Native garment category search/pagination and persistent optional 256 px preview ZIP import with progress and deduplication.
- All 39 game-rule controls displayed in the shared offline overlay; canonical rule mappings retained as metadata, not enabled game writes.
- WPF palette/brand/court styling, compact 70 px wide-screen overlay bar and responsive two-row bar, consistent catalog header/return buttons.
- A versioned inventory of 143 baseline entry routes and ten workflow groups, explicitly marked incomplete.

## Confirmed defects fixed

- Coarse Windows timestamps could collide between parallel test roots and atomic-write temporary names. IDs now include an atomic counter.
- Damaged optional preview packs could abort startup. They now report a pack error and use bundled fallback pictures.
- Reimporting an identical preview ZIP duplicated content. SHA-256 identity now reuses the existing verified file.
- Shirtless was an extra tile outside pagination. It is now the first normal, searchable top choice.
- One window could overwrite unrelated settings from the other. Native settings patches now merge under the save lock.
- The overlay could add two different accessories for one slot. Both pickers now use the same native slot-replacement logic and three-accessory validation.
- First-page outfit spacing still required scrolling at 1440 × 900. Four rows now fit; long labels are limited to two lines.
- Unfiltered clothing catalog opens had an unnecessary 100 ms delay. Only typed searches retain that debounce.

## Verification

- Strict TypeScript and production Vite build: passed.
- Rust: 20 tests passed.
- Vitest: 10 tests passed.
- Final binary: 25 actual Tauri/WebView2 integration checks passed on three consecutive isolated runs. Total: 55 checks/tests in these suites, not a claim of complete WPF coverage.
- Checks cover native persistence, minimum 1060 × 700 window overflow, mod scroll retention, playlist and camera saves, four-row picker fit, category filtering/pagination, exact-ID imported 256 px image rendering, randomization constraints, saved outfits, fixed overlay geometry, same-button close and cross-window synchronization.
- Final reports/screenshots: `artifacts/final-candidate-67de65d9d68b4fe4bfd520fdadb81415/Logs`, `artifacts/final-repeat-v02-1/Logs`, `artifacts/final-repeat-v02-2/Logs`.
- Internal bootstrap timings on those runs: about 247, 261 and 250 ms. These exclude some process startup work and are not cold-launch comparisons.
- Final prototype processes exited; none remained in the post-test process check.
- Both existing Main preview ZIPs were validated read-only: 38,576 / 28,838 thumbnails, all 256 px, about 9.4 / 7.1 seconds. Reports: `artifacts/real-preview-check-34e5ab92/Logs/preview-check.json` and `artifacts/real-preview-check-59ea671e/Logs/preview-check.json`. Neither ZIP was changed or copied into Main.

## Build-environment limitation

Several Rust release compiler invocations terminated with STATUS_ACCESS_VIOLATION. A retry compiled the final candidate successfully, and that binary passed the three packaged checks above. The compiler fault remains undiagnosed; reproducible clean builds are not yet proven. Build.ps1 now limits release Cargo workers to one, but this is not claimed as a confirmed fix. No corresponding application crash occurred in the recorded final runtime tests.

## Safety, rollback and remaining parity

No Main App, Data or Library was changed; no NBA 2K27 process was launched, modified or injected into. Native scans avoid junction traversal. Media imports preserve originals and reject destination collisions. Invalid settings/library files are retained rather than silently reset. Preview validation rejects unsafe paths, malformed PNGs, duplicates and oversized packs. Remote navigation and browser permission requests are denied.

Portable prototype storage is separate and is not yet WPF-compatible. Normal meaningful application data and its WebView2 profile stay in its own Data, Library, Cache and Logs. Ordinary OS-managed housekeeping can occur elsewhere.

Rollback source and old EXE: `Backups/v0.1.0-before-parity-20261011`. The old `Portable/App` prototype and old share ZIP are unchanged. Main WPF remains installed.

Still unported: game launch, all helper ownership/journals/restoration/rearming, live Blacktop writes and F8 behavior, video/controller/Mixer integration, in-game Jukebox/cameras, complete roster/team editors and DNA formats, coaches/rotations/playbooks, mannequin/animations, diagnostics and update/rollback. Manual picker/media-codec usability also needs checking with actual user files. Full game testing will be required after those pipelines are ported. The parity inventory and README enumerate these gaps; no catalog-only screen is counted as a working live editor.
