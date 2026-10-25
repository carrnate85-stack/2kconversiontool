# Exporter arena renderer handoff

## Destination

- Canonical exporter: `D:\Projects\2k Exporter\nba2k27-exporter-tauri2`
- Confirmed against Exporter `0.1.27` on 2026-10-25.
- Test asset: `C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27\mods\arena_015_int.iff`

## Goal

Fix two confirmed arena-preview defects in the exporter, then make the exporter and launcher consume one shared preview implementation so lighting, materials, performance, picking, placement, and texture staging cannot drift again.

## Issue 1: basket components are instantiated repeatedly

The bug is in `src/scene.rs`, in `collect` / `placements`. Each object placement is indexed under:

1. direct `Target` and object-name variants; and
2. every primitive `Mesh` and `Material` alias in the target model.

Those two classes are merged into the same map. A direct model key can therefore be contaminated by placements derived from another model's primitive aliases.

Measured on `arena_015_int.iff` before the fix:

- 236 decoded meshes
- 5,684 reported instances
- the `basket0:basket`, `basket0:glass`, and `basket0:rimr` meshes each received all six basket/glass/rim placements
- each should receive only its own two authored placements, one at each court end
- six exact duplicate-transform groups were also present

Representative wrong result for each of the three geometry models:

- basket placements at Y `+1310.64001` and `-1310.64001`
- glass placements at the same two transforms
- rim placements at Y `+1311.331324` and `-1311.331324`, Z `304.764029`

### Required algorithm

Maintain separate maps while walking the SCNE:

- `direct`: variants of object `Target` and object name
- `fallback`: variants derived from primitive `Mesh` and `Material`

After traversal, insert a fallback key only when no direct key exists:

```rust
for (key, places) in fallback {
    direct.entry(key).or_insert(places);
}
```

Do not merge fallback placements into a key that has direct authored placements.

That separation is necessary but not sufficient because `variants()` also creates broad direct aliases such as `MAIN@basket0`, shared by basket, glass, and rim objects. At model assignment time in `src/preview.rs`, filter a candidate placement list as follows:

1. Prefer placements whose normalized `target` exactly matches the geometry model path.
2. Also accept a matching final path/colon leaf for namespaced targets.
3. If no direct target matches, retain the alias-derived list as the compatibility fallback.
4. Collapse identical 16-value placement matrices within the selected list.

The tested launcher implementations are in:

`C:\Users\carrn\CodexProjects\2kconversiontool\nba2k27-launcher-tauri2\vendor\model-preview-worker\src\scene.rs`

`C:\Users\carrn\CodexProjects\2kconversiontool\nba2k27-launcher-tauri2\vendor\model-preview-worker\src\preview.rs` (`direct_placement_target` and `selected_placements`)

Regression tests should prove:

- a direct `basket` target wins over another model's primitive alias named `basket`
- a primitive alias still resolves when no direct target/name key exists
- a broad `MAIN@basket0` candidate is filtered to the geometry model's own target
- identical matrices from different aliases collapse to one instance
- the real arena produces two placements each for basket, glass, and rim geometry

Verified result from the rebuilt launcher worker on the real `arena_015_int.iff`:

- 236 meshes
- 5,672 instances (down from 5,684)
- zero exact duplicate-transform groups (down from six)
- basket geometry: exactly two basket placements
- glass geometry: exactly two glass placements
- rim geometry: exactly two rim placements

## Issue 2: Load all stops at roughly 30 / 719 textures

The exporter has the same stopping behavior in `frontend/App.tsx` in `arenaBatch`.

This unconditional check runs for both staged loading and Load all:

```ts
if ((shouldAppend ? arenaBytes.current : 0) + reply.bytes > 128 * 1024 * 1024)
  throw new Error('Arena texture preview reached its 128 MiB in-memory budget...');
```

On `arena_015_int.iff`, it stops around 30 / 719 linked textures. The launcher fix keeps the safety limit for incremental `Load more`, but explicitly lets `Load all` continue through every remaining batch:

```ts
const limitReached = !allBatches
  && (shouldAppend ? arenaBytes.current : 0) + reply.bytes > 128 * 1024 * 1024;
```

Also ensure Load all remains enabled after incremental loading reaches its staged limit. It must resume at the current cursor rather than restart at zero. Keep sequential batches, progress, cancellation ownership, completed uploads, and geometry identity intact.

Update the current smoke assertion at `frontend/App.tsx` around the arena texture checks: Load all should now reach `cursor === total`, not treat the 128 MiB exception as success.

## One shared renderer

The launcher and exporter should not keep hand-maintained copies.

This is build-time source sharing, not a runtime dependency between installed apps. Each app must remain fully standalone and package its own built renderer assets and native worker; installing one app must never require the other app to be present.

Recommended shape:

1. Move the browser renderer (`frontend/public/renderer`) into one canonical shared preview package owned by the exporter workspace.
2. Move the Rust SCNE/preview logic (`scene.rs`, `preview.rs`, material/texture decode dependencies) into one shared crate or canonical worker package.
3. Keep only thin host adapters:
   - exporter adapter: catalog resource IDs, dependency actions, export actions
   - launcher adapter: loose mod path, read-only Mod Library modal
4. Generate/copy packaged renderer assets during each app's build; do not hand-edit the generated launcher copy.
5. Add a manifest/hash parity test so CI fails when either host ships renderer files from a different shared revision.

The shared browser renderer already supports raycast selection and emits `model-preview:pick`; the launcher only needed to forward that event through its iframe bridge and render a read-only inspector.

## Arena speed parity

The exporter still reaches an interactive arena faster than the launcher. Do not reduce model quality or eager-load textures to hide this. Profile the same loose `arena_015_int.iff` through both hosts after sharing the core.

The launcher currently adds an external worker process and a large JSON payload hop before WebView rendering, while the exporter owns its native backend directly. The shared design should preserve the fast native decode path and avoid reserializing large geometry arrays between native processes. Prefer shared/binary geometry buffers or an in-process shared crate API; keep staged textures separate from geometry readiness.

Previously measured warm launcher phases for this arena were approximately 2.3 s catalog readiness, 0.1 s loose-file read, 7.9 s geometry readiness, and 0.8 s for the first 20-texture batch. Re-measure in the exporter chat rather than treating those older phase figures as current acceptance results.

## Acceptance checks

- `arena_015_int.iff` shows one complete basket at each end of the court, with no stacked/duplicated basket, glass, or rim objects.
- Incremental Load more still works.
- Load all progresses beyond 30 / 719 and reaches 719 / 719 unless the user cancels or a real decode error occurs.
- Object click selection still highlights the selected instance and reports SCNE object, target, primitive, material, shader, and texture dependencies.
- Exporter and launcher renderer distribution hashes identify the same shared renderer revision.
- The launcher reaches interactive geometry within the same practical range as the exporter for the same arena and warm catalog; record phase timings for both.
- No source IFF, game asset, mod, or export output is mutated by preview operations.
