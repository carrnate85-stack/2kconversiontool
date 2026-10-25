# Mod Library 3D preview

The 2K Explorer renderer is a normal launcher feature intended for a later
release. It is currently staged only in the isolated dev build while model
coverage and memory use are tested.

## Current test-build behavior

- Mod Library offers **3D Preview** for `.iff` and `.iff.bin` entries.
- The selected loose mod is decoded read-only. The installed NBA 2K27 catalog
  supplies referenced dependencies and Oodle support.
- Character, ball, apparel, and arena payloads use the Exporter's canonical
  shared renderer. Explicit rebuilds follow the current canonical revision;
  normal startup uses the installed snapshot.
  Arenas display geometry first, then **Load more textures** adds one
  bounded batch and **Load all textures** continues through the remaining
  batches. Reaching the test-build memory budget leaves the partially textured
  arena visible.
- Loose player mods load sibling config, item, and geometry IFFs from the same
  mod folder before falling back to the installed-game catalog, matching the
  Explorer dependency behavior. The optional **Configs** control loads every
  configuration declared by the player and preserves the current camera.
- The selected path is canonicalized and must remain inside the active mods
  root. Requests are serialized through an isolated worker process.
- Closing the preview cancels outstanding work. Neither game nor mod files are
  written.

## Build boundary

`Start Dev.vbs` calls `Start-Dev.ps1` to open the installed dev executable and
start its local Vite UI server if needed. It never builds, syncs, or reads the
Exporter project. `Start-Dev.ps1 -CheckOnly` validates this path without opening
the app. Ongoing Exporter work cannot prevent normal startup.

`Rebuild Dev.vbs` runs `Dev.ps1 -NoRun` to explicitly update the dev build.
`Dev.ps1` enables the temporary `dev-model-preview` Cargo feature. Before Vite
or Rust compilation, the Exporter's `shared/model-preview/Sync-ModelPreview.ps1`
generates and hash-verifies the Launcher mirrors. A newer canonical revision is
adopted automatically; a revision changing during sync stops the build with a
retry message. The verified revision manifest,
native worker, and notices are copied under `Dev/Portable/App/tools`, so the
running Launcher never depends on the Exporter installation. After a successful
build, `Stage-DevModelPreview.ps1` installs a verified renderer snapshot and
switches `Dev/Portable/App/model-preview-current.json` to it. Vite serves that
snapshot, never the mutable build mirrors, only in serve mode. `Build.ps1` is unchanged, normal Vite
builds do not copy the renderer, and release folders do not receive the worker.

The Launcher sync wrapper also copies and hash-verifies the sibling canonical
`shared/nba2k-assets` crate (Cargo.toml and src/) into the standalone worker.
Renderer updates and their codec dependencies therefore advance together.

## Verification snapshot

- Character `png2488.iff`: 6/6 companions, 17 textures, and zero skipped items;
  selecting `BASE` adds its configuration mesh and preserves all dependencies.
- Ball `ball_gameplay_official_wnba_wnba_official.iff`: 1 mesh, 3 textures.
- Arena `arena_015_int.iff`: 236 meshes, 5,672 instances, zero duplicate
  transforms, two correct basket/glass/rim placements, and 719/719 linked
  textures loaded through optional staged batches.
- Actual dev launcher: textured NBA ball rendered in the Mod Library modal and
  reported `Preview ready`.

## Before release promotion

Remove the temporary build gate only after wider mod-family testing, decide a
release memory policy for very large arenas, bundle the renderer and worker in
the normal package with their notices, and run the full release QA/checkpoint
workflow. Do not copy the current dev-only packaging step into an existing
release folder.
