# Feature, Look And Quality Improvement Audit

Objective updated October 16, 2026. This replaces strict 1:1 parity as the
acceptance standard. PARITY-AUDIT.md remains historical regression evidence.

## Acceptance Criteria

- Retain the working Full Swap, Preview, Configs, Body, Hair, Headbands, CLI,
  Health, settings, templates and favorites workflows.
- Preserve approved styling and rendering quality, staged-first dependencies,
  conversion correctness and Blender integration.
- Reconsider inherited implementations and parity-era replacements when another
  approach is measurably faster, simpler, more reliable or less wasteful.
- Exact internal architecture, control IDs, payloads and byte equality with an
  older build are not requirements in themselves. Existing comparisons remain
  useful protection for geometry, texture metadata and file preservation.
- Make scoped, measured changes; preserve user data and Blender extensions;
  retain a few rollback releases and provide the sharing EXE and launcher.
- Distinguish automated/local evidence from game, native-dialog and clean-machine
  checks. Missing external checks do not block independent improvement work.

## Review Inventory

The implementation review is frozen at .42 for release sign-off. The reconciled
inventory below summarizes the measured improvements and deliberate preservation
decisions; historical batch sections retain their original observations. External
QA remains explicitly unverified. No new discretionary optimization is queued.

| Area | Current Finding / Next Evidence |
| --- | --- |
| Catalog listing | 0.2.20 adds coherent bounded caching; 0.2.21 replaces initial accessory listing with the shared Rust manifest index and removes unused eager descriptors. All 6,451 installed asset rows match the original reader |
| Frontend catalog duplication | 0.2.20 removes the stale second list cache; tab/category/settings refresh and selected-row preservation pass |
| Cache ownership | 0.2.20 limits preview invalidation to player-file edits, reset, game settings and repair. Templates and CLI settings no longer discard geometry |
| Native asset loading | Shared Rust Archive Studio reader; staged-first resources, per-player invalidation, scoped cancellation, source freshness and bounded ownership verified across batches 12-24 |
| Preview decoding/transport | Owned transferable worker buffers and binary native catalog geometry replace repeated JSON/base64 copies; exact typed arrays, donor-frame reuse, packet bounds and source preservation verified |
| UI/state | Compact scrolling/drawer preserved; latest-request ownership, reset barriers, finite Body Fit drafts, cross-view locks and real save/readback verified through .42 |
| Conversion supervision | Isolated Python workers, acknowledged scoped cancellation, owned job lifetimes, atomic publication and highest-detail distance references verified without replacing proven conversion math |
| Python compatibility helpers | Intentionally retained for tested format/conversion behavior. Native DDS, archive/catalog reads and transport replace costly paths; 38 inherited regressions and representative generation/output comparisons protect retained helpers |
| Blender integration | Immutable selected-config snapshots, independent handoff lifetimes, stale-return guards, atomic return publication and failed-return preservation verified. All 197 original tools remain byte-identical |
| Runtime/startup | Development packages excluded; cached launches lease the ready engine without waiting for verified background retention. .42 adds fourteen-file bounded readiness checks and verified repair |
| Storage | Inventory/lease/settings-aware cleanup preserves modified, legacy, active and user-referenced folders. Exact-path release retention keeps .41/.40 rollbacks and the .5 benchmark alongside .42 |
| Packaging/sharing | One sharing EXE, no Node backend or .NET window host; payload/codecs/197 Blender files verified. CLR/VC helper requirements are disclosed; clean-PC sign-off remains external |
| Renderer quality | Approved strand lighting/coverage retained; five real assets pass and key tangent screenshots match .41 exactly. Complete RGB boundary response, private texture transactions and staged-priority tangent bindings verified |
| Tests/maintenance | Focused ownership, failure injection, actual GPU/readback, file preservation and representative UI/Blender workflows supplement historical comparisons; exact old implementation identity is not a requirement |

## First Improvement Batch

Baseline 0.2.19, isolated data and original installed manifest:

- Hair: 5,361 rows, 4,184,859-byte full response, 1,871 ms repeated read and
  another Python worker.
- Facial hair: 1,090 rows, 938,369-byte response, 1,623 ms repeated read and
  another Python worker.
- Template save discarded all 4,417,338 bytes of prepared player archive data.

Implemented behavior for 0.2.20:

- Cache the two accessory list kinds independently in Rust, at most 8 MiB of
  serialized response per kind. Do not cache invalid/oversized/unstable results.
- Retain every list field and preview descriptor. Recheck resolved/preferred
  manifest, settings and extractor presence before reuse.
- Remove duplicate per-kind UI list storage and recheck on Hair tab entry.
  Settings changes release obsolete bridge descriptors; an unchanged list
  recheck preserves its selected row.
- Keep manifest-only lists across New Swap; clear lists for settings/repair.
- Preserve preview payloads for templates, CLI writes/language and the lifetime
  of an open Blender handoff. Actual player edits/Blender returns still
  invalidate; New Swap still releases preview payloads.
- No shader, conversion math, Python runtime or Blender extension edits.

Tests: accessory-catalog-cache.cjs, preview-cache-ownership.cjs and Rust unit
guards, followed by packaged UI, staged texture, rendering and recovery checks.
Old-build failure reports are retained alongside the new reports.

Verified so far:

- All release gates and 59 Rust tests (39 app, 20 shared) pass.
- Repeated backend hair/beard list reads: 81/17 ms, down from 1,871/1,623 ms.
  First loads remain 1,782/1,701 ms; this does not claim faster initial parsing.
- All list fields retained, no worker on cache hits; same-size manifest edits,
  external settings, preferred-root restoration and invalid-kind guards pass.
- Actual packaged UI verifies category/tab refresh, selected-row retention and
  settings changes within the same category, with zero JavaScript errors.
- Template and CLI language changes retain 4,417,338 archive bytes. A real
  private-player body-fit edit invalidates them and changes the fingerprint;
  New Swap clears payloads, and the original player hash is unchanged.
- Staged texture checks pass again: 19 DDS payloads, ten uploaded channels,
  region-specific fallback and edit invalidation. Cached preview decode is
  21 ms; all seven GPU meshes are reused.
- Package integrity confirms unchanged runtime hash and 197 original Blender
  files. Final full UI/automatic staging, all five approved strand rendering
  controls, favorites/theme and failure/cancellation recovery checks pass.
- UI screenshot readiness now waits for the startup overlay to finish rather
  than treating an early engine-status message as a fully ready screen.
- Desktop launcher points to verified 0.2.20. Removed only the unused 0.2.17
  sharing EXE (58,993,438 bytes); 0.2.19/0.2.18 remain as rollbacks and 0.2.5
  remains the performance-test baseline. Temporary fixtures from this batch
  are removed; production apps, exports and Blender tools are not deleted.

Follow-up findings from the first cache update:

- Initial list generation started Python and produced eager preview descriptors
  that the TypeScript list contract did not use. Closed by the second batch.
- The worker transport wrapper cloned typed-array results without transfer
  lists. Closed by the third batch, with retained donor-cache ownership checks.
- The latest swap validator retains two missing low-shape reference warnings.
  Determine whether those are external-companion false positives or a real
  distance-visibility risk; do not preserve them solely to match older output.
- The build generated the worker entry through pinned source replacements.
  Closed by the third batch's owned entry module. Guarded format-parser hooks
  remain, retaining the proven parser rather than duplicating its implementation.

## Second Improvement Batch

Implemented in 0.2.21:

- Native accessory listing reuses the same Rust manifest index as preview
  extraction. It does not decode geometry/textures or create disk previews.
- The list response contains only list data; the selected asset still gets its
  geometry descriptor on demand through the existing preview endpoint.
- The original reader remains an automatic fallback and an explicit comparison
  path. Native/compatibility results have separate cache keys.
- Native listing has a registered cancellable job. Global/command cancellation
  cannot fall through into a new compatibility worker.
- Existing source-kind filters are retained because the installer uses those
  supported manifest asset kinds; this is a conversion-safety boundary, not an
  arbitrary requirement to copy older implementation details.

Evidence:

- All 5,361 hair and 1,090 facial-hair rows match every original asset field,
  name, flag and order. No missing UI/catalog feature was substituted for speed.
- First hair listing, including cold manifest indexing: 813 ms versus the
  original reader's 1,886 ms. Facial-hair listing then reuses that index:
  30 ms versus 1,633 ms. These are development-PC samples, not universal bounds.
- Cached responses take 21/5 ms in the comparison run. Full list IPC payloads
  fall from 4,184,859/938,369 bytes to 1,020,663/228,333 bytes, about 76% smaller.
- A fresh native cache test samples 791/26 ms first reads and 18/3 ms cached
  reads; same-size edits, external settings, reset and preferred-root restoration
  pass using private native manifest-only fixtures. Marker EXE/DLL files are
  never executed or loaded. Separate UI fixtures exercise automatic fallback.
- Native cancellation/recovery, explicit compatibility-worker termination and
  diagnostics, all eight UI tabs, full-swap automatic staging, body-fit controls,
  both catalogs, desktop/compact rendering, five approved strand controls,
  favorites/theme and failure/cancellation recovery pass.
- Staged-texture checks again retain all 19 DDS payloads, ten uploaded channels,
  range/bindings and region-only fallback. Cached preview decode is 21.6 ms with
  seven unchanged GPU meshes reused.
- All release gates and 62 Rust tests (42 app, 20 shared) pass. Runtime payload
  and all 197 Blender files are unchanged; no shared-library or renderer changes.
- Public EXE integrity is reverified and the desktop launcher points to 0.2.21.
  Retention is 0.2.21 plus 0.2.20/0.2.19 rollbacks and the used 0.2.5 performance
  baseline. Only the unused 0.2.18 EXE was removed (58,993,950 bytes).
  All owned app test processes exited; production apps and exports are untouched.

Output-quality follow-up:

The read-only accessory-reference-inspection.json report confirms that the
latest test swap's hair and beard meshlo names are absent from both their
regular and tangent-space SCNEs. A duplicate-key-preserving inspection also
finds no hidden definitions for those names. The manifest lists no separate
low-shape companion for these slots. The subsequent stock comparison below
changes the interpretation; local absence alone does not prove invalid native
metadata or establish the cause of an in-game symptom.

## Third Improvement Batch

Implemented in 0.2.22:

- Preview results transfer their owned ArrayBuffers to the UI instead of asking
  structured clone to copy them. Shared views are deduplicated; shared-memory,
  empty buffers and cycles are handled safely.
- Native heads arrive as fresh hydrated buffers; fallback heads return cloned
  arrays when included in a result. Borrowed cached arrays remain worker-owned.
  Unit and packaged tests verify that transfer never detaches retained heads
  or invalidates the reusable tangent frame.
- A small owned worker/index.js replaces slicing/replacing the original worker
  source during builds, removes an unused head-cache instance and a dead source
  replacement, and retains serial processing and failure recovery.
- The original format parser, projection rules, renderer, conversion helpers
  and Blender extensions are retained; no domain-logic rewrite.

Verification:

- Exact prior-build hashes match across nine real worker cases: native/cached
  tangent beard; first/cached/borrowed/revisited fallback heads; tangent afro;
  first/repeated staged player. Geometry arrays, groups and DDS/range bindings
  match. Native and fallback head caches remain valid after transfers.
- The staged result transfers 43 buffers totaling 3,284,958 bytes; afro transfers
  1,124,812 bytes; the native beard/head payload transfers 453,368 bytes.
- Controlled structured-clone samples on the same actual staged payload have
  medians of 27.6 ms copying versus 18.7 ms transferring (five samples).
  End-to-end decode timings varied and do not establish a uniform whole-preview
  speedup. Native IPC/base64 and texture strings remain separate costs.
- All release gates and 62 Rust tests pass, with new worker ownership/entry
  release gates. Final full UI/automatic staging passes with unchanged desktop
  and compact visible-pixel counts. A background Blender 5.1 Edit Mode round
  trip passes with 16 textures, jersey, two references, four meshes, 29 materials
  and eyesRecalculated true.
- Final staged-texture checks pass: all 19 DDS payloads, ten uploaded channels,
  range/bindings, edit invalidation and regional fallback are retained. Seven
  GPU meshes are reused with a 22.9 ms cached preview sample.
- All five approved strand visible-pixel counts remain identical. Favorites/
  theme, cancellation/New Swap and failed-swap button recovery pass.
- Runtime payload and all 197 Blender files remain unchanged. Public EXE
  integrity is reverified; all owned app/WebView test processes exited.
- Launcher points to 0.2.22. Retention is 0.2.22 plus 0.2.21/0.2.20 rollbacks
  and the required 0.2.5 baseline. Only unused 0.2.19 was removed
  (58,934,558 bytes); production apps, settings, exports and Blender tools remain.

Stock-reference investigation:

- accessory-distance-stock-report.json compares a freshly extracted stock
  png06827 and the test conversion, checking every inspected source-file hash
  afterward. Both hair and beard use the same locally undefined meshlo names
  in stock and converted packages.
- Stock geometry retains its authored reduced-distance vertex counts; converted
  geometry correctly uses full-detail counts at every level.
- Therefore the warning is not evidence of a newly introduced conversion bug.
  Native engine interpretation still needs gameplay evidence; no speculative
  output rewrite or warning suppression is applied.

## Fourth Improvement Batch

Implemented in 0.2.23:

- A scoped packaging policy omits unreferenced build tooling: pip, PyInstaller,
  setuptools and their build dependencies, plus the unused legacy capstone
  package. No files are deleted from the production app or source runtime.
- All 2,262 retained Python-runtime files and all 197 Blender files are compared
  byte-for-byte with the production sources during release publication.
  Image/compression/RAR support, Tcl/Tk, asset templates and format helpers stay.
- Workers start with isolated Python configuration and bytecode disabled.
  Bootstrap and template persistence now succeed with an invalid ambient
  PYTHONHOME and a deliberately conflicting PYTHONPATH, where 0.2.22 failed.
- Prepared runtimes record file sizes and hashes during extraction. Best-effort
  retention keeps the current and most recent prior inventoried runtime.
  Active leases, modified files, added files/directories, configured user paths,
  links and legacy caches without inventories prevent deletion.
- Retired caches are renamed before verified files are removed. Interrupted
  deletion retains its inventory for retry. The release helper no longer runs
  broader recursive runtime cleanup that would bypass these safeguards.
- Managed Blender tool paths follow the current runtime in custom test profiles
  as well as default storage. External custom tool paths and settings stay intact.

Verified:

- Sharing EXE: 59,033,374 -> 52,479,538 bytes. Expanded payload:
  109,698,108 -> 86,851,442 bytes. Files: 4,937 -> 2,535.
- All release gates and 69 Rust tests pass (49 app, 20 shared), including active
  leases, interrupted deletion, links, modified/user files and configured paths.
- Packaged standalone imports, PNG/zstd codecs, Tcl and health checks pass with
  only Windows on PATH. Removed packages are absent.
- Packaged retention fixtures preserve current/prior, modified, untracked,
  legacy and configured-path caches, and remove only eligible obsolete caches.
- First/cached startup samples: 2,958/668 ms, versus 3,303/575 ms in the prior
  baseline. These samples do not establish a cached-startup improvement.
  Runtime corruption repairs and two-instance lease protection pass.
- A Windows-only-PATH 2K23 facial-hair Full Swap and both preview-resource
  pathways pass with zero validation errors and 13 companions each.
- Available 2K18, 2K19, saved 2K20 and custom 2K26 conversions pass with zero
  validation errors and unchanged source hashes. Fresh 2K25 remains unavailable.
- Full UI, automatic staging, 113 body fields, both catalogs, rotation and
  desktop/compact rendering pass with identical visible-pixel counts.
- Textured Blender 5.1 Edit Mode round trip passes: 16 textures, two references,
  jersey, four meshes, 29 materials and eyesRecalculated true.
- Staged-first checks retain all 19 DDS payloads and ten uploaded channels,
  TXTR ranges/bindings, regional fallback and edit invalidation. Seven unchanged
  GPU meshes are reused; the cached preview sample is 25.5 ms.
- All five approved strand controls retain their visible-pixel counts.
  Favorites/theme, cancellation/New Swap, failed-swap recovery, settings,
  templates, CLI input/language guards and export-preserving cleanup pass.
- ZIP, stored RAR and solid RAR inspect/stage successfully with exact file
  hashes. Unsafe/ambiguous/link sources are rejected without staging leaks;
  archive-source Full Swap and reset preserve the export.
- All 6,451 accessory rows retain their fields/order. Native first/cached
  hair reads are 1,314/35 ms; beard reuses the shared index in 52/8 ms.
  Original-reader comparison is 2,006/1,682 ms. These are local samples.
- Published EXE integrity is reverified and the desktop launcher points to
  0.2.23. All owned app/WebView test processes have exited. Retention is 0.2.23
  plus 0.2.22/0.2.21 rollbacks and the used 0.2.5 performance baseline.
  Only the unused 0.2.20 EXE was removed (58,939,166 bytes). The release helper
  did not delete any prepared runtime; production apps and exports are intact.

This payload changes, so the first launch prepares the slimmer runtime once.
Subsequent launches reuse it. Retention is intentionally not a promise of exactly
two folders: active, legacy, changed or user-referenced caches are preserved.

## Fifth Improvement Batch

Implemented in 0.2.24:

- Replaced inherited compact-preview minimum heights with available-height
  sizing. A scoped owned stylesheet preserves the blue theme and existing
  controls without modifying the original app's styles or renderer.
- Render Summary is collapsible. It defaults to a drawer on compact windows
  and an adjacent inspector on wide windows. Selection details, warnings and
  Blender status remain available. Adjusted orbit and zoom survive toggling.
- Preview uses an icon navigation rail at narrow widths. Refresh is an icon
  button with its existing handler, accessible label and disabled state.
- Body-fit editing places its controls beside an unobstructed player canvas.
  Camera controls and the jersey toggle remain available in that editing mode;
  closing the panel restores the complete normal toolbar.
- Original config selection, Blender options, lighting, materials, texture
  decoding, conversion math and all 197 Blender files remain in place.

Verification:

- The 0.2.23 baseline reproduces 727/757 px of internal overflow at 900x650
  and 900x620. At 900x620, scrolling can clip all 14 preview controls.
- The updated real app has zero internal or horizontal overflow and no clipped
  toolbar controls at 1364x861, 1024x768, 900x650, 900x620 and 720x620.
  All configuration selectors remain at least 157 px wide.
- At 1364x861, the normal preview canvas grows from 736x431 to 804x497.
  At 900x620, the entire 820x298 canvas and status remain on screen.
- Six light/dark fit-editor cases at desktop, minimum and high-DPI-equivalent
  sizes retain nonblank, unobstructed player canvases. Calculate Fit and Close
  align; scrollable fit controls and Apply remain within the panel.
- Adjusted camera preservation, Refresh, Summary open/close, Fullscreen API
  rendering and Escape recovery pass. Screenshots were visually inspected;
  visual QA caught a collapsed config selector that the initial geometry-only
  assertions missed, and the test now guards that failure.
- All release gates and 69 Rust tests pass. Full UI/automatic Full Swap staging,
  113 body fields, both catalogs, rotation and compact rendering pass.
- Textured Blender 5.1 Edit Mode round trip passes again: 16 textures, two
  references, jersey, four meshes, 29 materials and eyesRecalculated true.
- All 19 staged DDS payloads, ten uploaded channels, TXTR ranges/bindings,
  regional fallback and edit invalidation pass. Seven GPU meshes are reused;
  the cached preview sample is 26.6 ms.
- All five approved strand pixel counts remain unchanged. Favorites/theme,
  cancellation/New Swap and failed-swap recovery pass.
- Sharing EXE is 52,481,074 bytes. Runtime payload hash is unchanged from
  0.2.23, so its prepared runtime is reused. All bundled Blender files and
  retained Python-runtime files pass the release integrity check.
- Published integrity is reverified and the launcher points to 0.2.24.
  All owned app/WebView tests exited. Retention is 0.2.24 plus 0.2.23/0.2.22
  rollbacks and the used 0.2.5 baseline. Only the unused 0.2.21 EXE was removed
  (59,033,374 bytes); no prepared runtime, production app or export was deleted.

Next concrete investigations:

- Profile remaining native geometry and texture-decoding transport costs after
  the binary archive transport improvement below.
- Continue reviewing UI request/state ownership and inherited build/typing
  assumptions; retained control-ID comparisons are historical safety evidence,
  not a requirement to keep redundant controls or brittle source generation.
- Profile remaining startup and compatibility-worker overhead. Keep conversion
  dependencies and Blender tools intact; replace domain logic only with evidence.

## Sixth Improvement Batch

Implemented in 0.2.25:

- Prepared archive cache stores bounded raw binary packets instead of base64
  JSON. Registered preview archive/file requests use Tauri binary responses,
  transferred from the UI to the decoder worker. Member views share the packet
  buffer. Local-file and compatibility JSON APIs remain available.
- Preserved resource registration, scoped cancellation, mutation locking,
  content/geometry hashes and cache eviction. The binary endpoint rejects
  unregistered/local paths and unsupported operations. Packet parsers enforce
  size, kind, member count, unique names and contiguous range bounds.
- This is not end-to-end zero-copy: outgoing Rust response ownership still
  needs a packet copy, and renderer texture payloads remain base64.

Verification:

- All release gates and 72 Rust tests pass. All 197 Blender files and 2,262
  retained runtime files match the established payload; runtime hash is
  unchanged from 0.2.23/0.2.24, so prepared runtime reuse is retained.
- Fourteen preview archives preserve every compatibility field, member size,
  content key and geometry key against the retained 0.2.24 baseline. Worker
  hashes independently verify binary contents; transferred UI buffers detach.
- The sample uses 50,244,477 binary wire/cache bytes versus 66,974,157 JSON
  bytes (25.0% less). Warm transport samples are 440-461 ms versus 689 ms for
  JSON in the same build. These are archive-transfer measurements, not claims
  about total preview latency or process memory.
- The real packaged player API uses 17 binary packets and no JSON replies;
  catalog preview also exercises binary transport. Full Swap completes and
  automatically stages its output. All eight tabs, 113 body fields, catalogs,
  rotation, jersey and compact preview pass.
- Textured Blender Edit Mode return passes with 16 textures, two references,
  jersey, four meshes, 29 materials and eyesRecalculated true.
- Staged DDS/binding/range checks, cache reuse and all five approved strand
  pixel controls pass. Nine worker transport cases preserve prior geometry
  and textures. Cancellation isolation, favorites/theme and failed-swap
  recovery pass; responsive layout and all six fit-editor cases pass again.
- Sharing EXE is 52,490,802 bytes. Published payload integrity is verified;
  the desktop launcher points to 0.2.25. No runtime folders were removed.

## Seventh Improvement Batch

Implemented in 0.2.26:

- Bound Hair-tab target-slot reads to their request ID, staged player, selected
  configuration and asset category. Older results/errors cannot overwrite newer
  slots or repopulate a reset session. Staging and config edits invalidate slots
  before asynchronous refresh work can publish another selection.
- Install uses the same readiness predicate as its button: current context,
  completed slot read, a selected asset and a slot from the returned list.
  Late install success/failure still logs its result but cannot change another
  player's status or preview state. Category controls disable during installs.
- Concurrent same-category catalog callers share the pending read instead of
  returning before data exists. New categories own their results and loading
  state; obsolete catalog failures cannot clear the current list.
- Conversion/fitting code, rendering math, dependencies, settings, favorites and
  all Blender tools remain in place.

Verification:

- Before correction, delayed-response tests reproduced eight target-slot
  failures and four catalog-request failures. Sixteen focused ownership cases
  now pass, including late install results and retry/failure recovery. Both new
  test files are release gates; TypeScript and all 72 Rust tests also pass.
- An isolated packaged app verifies shared pending catalog reads, latest-category
  wins, ignored obsolete errors, disabled pending target controls, latest-config
  labels, rejected stale installs and reset isolation. Existing manifest refresh,
  settings refresh and selected-row preservation checks still pass.
- Real native hair, beard and 2K27 headband installs preserve the original config.
  All 113 body fields remain available; negative/over-one and dimension values
  persist, and unchanged fields and original source archives remain unchanged.
- Full UI Full Swap completes and auto-stages. Both catalogs, rotation, jersey
  and compact preview pass. Textured Blender Edit Mode return retains 16 textures,
  two references, jersey, four meshes, 29 materials and eyesRecalculated true.
- All 19 staged DDS payloads, ten uploaded channels and TXTR range/binding checks
  pass. Seven unchanged GPU meshes are reused. All five approved strand pixel
  controls are unchanged. Favorites/theme, cancellation and failed-swap recovery
  pass. Five responsive sizes and six fit-editor cases remain nonblank and free
  of clipped controls or overflow; a compact screenshot was visually checked.
- Sharing EXE is 52,491,314 bytes. Published integrity and the unchanged runtime
  payload hash are verified; all 197 Blender files and 2,262 retained runtime
  files match. The desktop launcher points to 0.2.26.
- All owned test processes exited. Retained builds are 0.2.26 plus 0.2.25/0.2.24
  rollbacks and the used 0.2.5 baseline. Removed only the superseded 0.2.22/0.2.23
  EXEs (111,512,912 bytes); no runtime, production app, export or Blender tool
  was removed.

Next investigations:

- Coalesce obsolete queued decoder selections while preserving donor-head caches
  and independent Blender preparation.
- Review catalog/preview cache identity across changed game roots and manifests,
  staging-selection ownership, and mutation/read UI state.
- Continue startup and compatibility-worker profiling. This request-ownership
  batch is a reliability improvement, not a measured startup speed claim.

## Eighth Improvement Batch

Implemented in 0.2.27:

- Replaced the promise-chain decoder backlog with a serial queue that coalesces
  explicitly tagged catalog selections. An active decode completes and retains
  useful caches; only obsolete queued selections are removed. Every removed
  request receives a cancellation reply. Untagged catalog operations, staged
  previews and independent Blender reference work are not coalesced.
- Preserved operation validation, serial native-reader ownership, transfer
  semantics, cache limits, projection math and failure recovery.
- Fixed a copied UI shortcut: selecting A, then B, then the still-displayed A
  previously returned early and allowed B's late result/error to win. A now
  restores its cached preview and supersedes B's UI request. Repeated identical
  active selections and unchanged idle selections still avoid duplicate work.

Verification:

- A deterministic baseline and the packaged 0.2.26 benchmark both decode all
  twelve selections in a burst. The new worker test decodes active/last only,
  settles ten superseded requests and retains independent FIFO work. Invalid
  operation handling, active failure recovery and queue restart pass.
- Three real 12-selection bursts each decode two previews, skip ten before
  decoding and make two native geometry calls rather than twelve. Samples are
  445.4, 446.4 and 465.9 ms; the saved 0.2.26 comparison is 2,092.4, 2,402 and
  2,455 ms. These are local rapid-selection measurements, not a uniform speed
  claim for initial loads, staging or conversions.
- Latest positions, UVs, indices, original vertex IDs, groups and textures match
  the corresponding single-selection result. Donor head/frame caches survive;
  explicit cancellation drains all burst requests and a fresh selection resumes
  with matching output and the native head cache retained.
- Packaged baseline A-B-A renders the wrong B asset. The updated real renderer
  retains A's displayed key and payload with the spinner hidden. Both screenshots
  were inspected. Focused UI tests also cover obsolete failure and active/idle
  deduplication. Queue/UI tests are release gates.
- Thirteen native/fallback hair/beard cases preserve existing geometry, UV,
  triangle, source-ID and texture comparisons. Catalog rendering changes actual
  canvas pixels, rotates correctly and resets bounded caches. Earlier slot,
  category, settings/manifest refresh and selected-row checks still pass.
- All release gates and 72 Rust tests pass. Full Swap completes and auto-stages;
  eight tabs, 113 body fields, both catalogs, jersey and compact rendering pass.
  Textured Blender Edit Mode return retains 16 textures, two references, jersey,
  four meshes, 29 materials and eyesRecalculated true.
- All 19 staged DDS payloads, ten uploaded channels and TXTR range/binding checks
  pass. Seven unchanged GPU meshes are reused. All five approved strand pixel
  controls remain unchanged. Nine worker transport cases preserve prior geometry
  and textures. Favorites/theme, failed-swap recovery and unrelated-preview
  cancellation isolation pass.
- Sharing EXE is 52,491,314 bytes. Published payload integrity and all 197 Blender
  files/2,262 retained runtime files are reverified. The unchanged runtime hash
  permits prepared-runtime reuse, and the desktop launcher points to 0.2.27.
- All owned test processes exited. Retention is 0.2.27 plus 0.2.26/0.2.25 rollbacks
  and the used 0.2.5 baseline. Removed only the unused 0.2.24 EXE (52,481,074
  bytes); no runtime folders, production apps, exports or Blender tools were
  removed.

Next investigations:

- Review cached preview cancellation barriers and cache identity when game roots
  or manifests change, plus staging-selection and mutation/read UI ownership.
- Profile native catalog geometry transport/hydration against the already-warm
  original worker path. Native is not uniformly faster in the 13-case comparison;
  do not force architecture choices without isolating the remaining costs.
- Continue startup/compatibility-worker and inherited build/typing review. The
  broad improvement objective remains active; external release gates below are
  unchanged.

## Ninth Improvement Batch

Implemented in 0.2.28:

- Split the global preview stop queue into staged/catalog barriers. A cached
  decode now waits for its own barrier, including legacy fit-base requests.
  Decoder generations reject work cancelled while waiting. Independent Blender
  preparation bypasses both unrelated barriers.
- Terminated worker messages cannot launch JSON/binary reads; old worker errors
  cannot cancel a replacement worker. Existing pending-call draining and scope
  tags remain in place.
- Recheck request epochs after fallback/preparation responses and before cache
  publication. Cancelled work cannot restart decoding or restore expired
  descriptors/staged results after Reset.
- New Swap requests both scoped stops concurrently. Cleanup still waits for
  both drains and private Blender preparation, and retains exported files.

Verification:

- The initial delayed-response baseline reproduces eleven failures. The expanded
  sixteen-case regression gate passes: cached and fit-base barriers, symmetric
  scope independence, independent Blender preparation, parallel Reset ordering,
  stale worker messages/errors, cancelled fallbacks and post-reset cache ownership.
- The first packaged baseline test paused because its attempt to overwrite a
  read-only Tauri export did not install the delay hook. Property inspection
  identified this harness error; only its owned test window was closed. The
  corrected harness replaces/restores the writable core namespace, checks hook
  installation and bounds its observation wait. This was not an app-hang finding.
- Corrected 0.2.27 and 0.2.28 packaged comparisons verify the actual change:
  cached catalog work waits for its held stop only in 0.2.28; staged preparation
  dispatches before that catalog stop drains only in 0.2.28. Both then return
  valid output: retained native head cache, seven player meshes and 16 textures.
- The packaged Reset check dispatches both stops before release in 0.2.28 rather
  than sequentially in 0.2.27. Cleanup waits, finishes, releases native caches
  and preserves the exported source fixture.
- Rapid-selection tests still decode two of twelve requests and reject ten before
  decoding, preserve final geometry/UV/texture hashes and donor head/frame caches,
  and pass explicit cancellation/resume and real-renderer A-B-A. A screenshot was
  inspected. This lifecycle batch is not a new general load-time speed claim.
- Thirteen native/fallback accessory comparisons, catalog rendering/rotation,
  cache bounds/reset, selected-row preservation and settings/manifest refresh
  checks pass. Existing slot/category ownership checks pass.
- All release gates and 72 Rust tests pass. Full Swap completes and auto-stages;
  all eight tabs, 113 body fields, both catalogs, jersey and compact preview pass.
  Textured Blender Edit Mode return retains 16 textures, two references, jersey,
  four meshes, 29 materials and eyesRecalculated true.
- All 19 staged DDS payloads, ten uploaded channels and TXTR range/binding checks
  pass, with seven unchanged GPU meshes reused. All five approved strand pixel
  controls remain unchanged. Nine worker transport cases preserve prior geometry
  and textures. Favorites/theme, failed-swap recovery and unrelated-preview
  cancellation isolation pass.
- Published EXE integrity and all 197 Blender/2,262 retained runtime files are
  reverified. Sharing EXE is 52,491,314 bytes; the runtime hash remains unchanged
  and the desktop launcher points to 0.2.28.
- All owned test processes exited. Retention is 0.2.28 plus 0.2.27/0.2.26 rollbacks
  and the used 0.2.5 baseline. Removed only unused 0.2.25 (52,490,802 bytes);
  no runtime, production app, export or Blender tool was removed.

Next investigations:

- Carry source identity across backend catalog validation, bridge descriptors,
  worker donor heads and UI asset/head caches when game roots or manifests
  change. The current UI cache keys contain asset ID/category or donor PNG only;
  successful settings saves clear bridge caches but not those UI caches.
- Evaluate source-handle lifetime against the bounded descriptor/archive caches
  during long catalog sessions; the native registry has a 512-resource limit.
- Audit staging-selection and mutation/read ownership, build-input consistency
  if sources change during a build, native geometry transport/hydration costs,
  startup work and the inherited UI typing/build assumptions.
- Keep conversion correctness and approved rendering as invariants. The broad
  objective remains active; these remaining areas have not been declared fixed.

## Tenth Improvement Batch

Implemented in candidate 0.2.29:

- Add an authoritative accessory source revision from a structured serialization
  of watched paths, optional file stamps, lengths and modification times. Both
  accessory categories share a revision for the same source. Unstable/noncacheable
  reads receive a unique revision instead of falsely advertising reusable data.
- Carry game root/revision through the typed bridge. A changed revision clears
  obsolete descriptor/worker state; unchanged category/list reads retain it.
  An older differing-revision response cannot move the accepted source backwards.
- Successful settings/repair commits invalidate pending preview generations and
  staged/catalog caches. Failed settings writes do not discard usable previews.
  Independent Blender handoff decoders are not cancelled by this invalidation.
- Bind UI pending lists to the configured source, detach pre-commit list requests
  on a settings save, and invalidate asset/head/rendered caches on source changes.
  Old-source previews cannot refill the new caches after their replies arrive.
  Preserve favorites/camera preferences and a selected ID still present in the list.
- Require a validated current catalog and source-aware target slots for installs.
  Existing category, active-selection, cancellation and stale-install guards remain.
- Add twelve focused bridge/UI cases and packaged same-ID/different-root,
  unchanged-source, manifest-edit and settings-during-list checks. The initial
  pre-fix ten-case probe reproduced nine failures; all twelve final cases pass.

Verified on the candidate:

- Full release gates: 53 application plus 20 shared Rust tests, TypeScript checks,
  preview/lifecycle/queue tests, Python service tests, nine geometry guards and
  all 38 inherited 2K25/config/facial-hair regression cases pass.
- Packaged source-switch/list ownership checks pass with zero JavaScript errors.
  All 5,361 hair and 1,090 beard rows/fields match the original reader. Unchanged
  cached list reads measured 48/10 ms locally; no claim of a new cold-load gain.
- Thirteen native/fallback asset comparisons preserve positions, UVs, original
  vertex IDs, indices and texture metadata. Rapid 12-selection bursts still decode
  twice, preserve final output hashes and reuse donor heads/tangent frames.
- Packaged independent cancellation, scoped drain/reset barriers, A-B-A restoration,
  Full Swap with automatic staging (zero errors, two tracked low-detail-reference
  warnings), 113 body fields, and textured Blender 5.1 Edit Mode return pass.
- Package integrity verifies all 197 original Blender files and 2,262 retained
  runtime files. Runtime payload remains
  56aab207c04dbcb54e7ebaf9938b3f5a94b58b38e9fda5fcae45e09bb6cca214;
  candidate EXE is 52,447,794 bytes, with no runtime re-extraction implied.

Verification interruption, not signed off:

- Native/compatibility staged geometry and texture comparisons passed, then the
  staged native/cache gate failed as the game installation lost its manifest and
  mod.exe. A fresh native resource call subsequently failed with missing-file
  error 2. The source player and its companions still exist unchanged.
- The user confirmed NBA 2K27 version 2 was updating at 99%. Steam's install
  record still had old build 25221291 and target 25761995 with staged depots;
  Steam process 9644 was confirmed live. No old game files were restored.
- Final native staged-priority, strand-render, recovery and worker-transport
  checks must be rerun after the game update. The launcher remains on 0.2.28;
  0.2.29 is a built candidate, not an unconditionally verified release.

Remaining review:

- Source revisions cover watched catalog inputs, not a continuous watcher or
  arbitrary archive edits that preserve those inputs. Review that separate
  native source identity boundary without adding costly full-archive hashing.
- Audit native resource lifetime: the registry is limited to 512 unique URIs,
  while bridge/UI/native descriptor/head/archive caches have different lifetimes.
  Do not evict an active Blender/worker resource merely to avoid the limit.
- Continue staging/mutation ownership, native geometry hydration/transport costs,
  startup work and inherited UI typing/build reviews.
- Investigate the two retained low-detail hair-reference validator warnings.
- External game, fresh-source, native-dialog and clean-machine QA remain open.

Build consistency follow-up while Steam was committing the game patch:

- Added content-based checkpoints before/after frontend generation, before Rust
  compilation, and after packaging/integrity gates before publication. Rustfmt
  runs before capture. Generated DOM declarations are excluded as derived output;
  their HTML source and generator are included. Generated frontend output receives
  its own digest so later edits cannot silently differ from embedded assets.
- Capture local Rust/UI/worker/engine/build/test files, build dependencies, the
  retained original engine payload and decoder/roster helpers, Reference/src, and
  shared native asset code. Exclude runtime-policy omissions, generated bytecode,
  shared target output and top-level documentation. Game archives and external
  compiler/toolchain internals are not part of this developer-change guard.
- Fixture tests detect same-size timestamp-preserving content edits, additions,
  removals, retained/restored inputs and output exclusion, and preserve the prior
  release when an assertion fails. This is checkpoint validation, not a hermetic
  filesystem snapshot or proof against edits deliberately changed back between
  checkpoints.
- A complete 0.2.29 rebuild passes every existing release gate and all four input
  checkpoints over 4,895 files. Measured stream-hash snapshot cost was 2,567 ms
  locally. It adds no application startup work and no runtime payload changes.
- BUILD.json now records input digest
  a71963a2738dc76e7e8adb2dc6f65a7ab7c7def38a76c8174cc8597471211b90
  and frontend digest
  b1e6c31826ddd044f1dc27e0f29a4d486361e98f3e90131764ba406642bc1b89.
  Rebuilt candidate EXE SHA-256 is
  03d61ced4bcd1235b15e9fa51e96c6de1cb47646ede1f94b09e30a3c1fbf2a4d;
  size and original runtime payload remain unchanged. Whole-EXE bytes are not
  claimed identical to the earlier build (PE/build metadata can differ).
- Steam's content log confirms a commit from D:/SteamLibrary to the C: game
  folder; its updater process remains live. All owned Character Mod Tool test
  windows/workers exited, and no old game files were copied back into place.
  The candidate still needs the game-dependent final checks before launcher update.

## Eleventh Improvement Batch

Implemented in candidate 0.2.30 (includes unreleased 0.2.29):

- Confirmed that native Sources accumulated across preparations until 512 unique
  registrations, independently of the bounded archive/head/bridge/UI caches.
  Raising that limit would only postpone the failure, so the existing resource
  cap remains and resources instead get explicit reference-counted owners.
- Each native player/accessory preparation owns a deduplicated URI set. Failed
  preparations release their partial registrations. Shared URIs remain available
  until their last owner releases. Owner count is also bounded at 128.
- Staged decode/cache-hit/fallback/cancellation and independent Blender reference
  preparation release native resources when their payloads are complete. Catalog
  descriptors use a 48-entry LRU rather than periodic whole-cache clearing.
  Evicted active descriptors remain pinned through their borrowers' completion.
- Retired/cancelled preparation results and replaced native fallback descriptors
  release their owners. Reset/source invalidation retires cached descriptors.
  Owner IDs remain monotonic across clear/reset; a delayed old release cannot
  retire a newer preparation with the same URIs.
- Cached archive/blob bytes do not authorize reads after the resource is released.
  Raw permitted local-IFF reads no longer create permanent registry entries.
  Archive/head byte budgets, native packet formats and geometry/texture math remain.
- Registry-only release bypasses the player-file mutation lock and remains safe
  during shutdown. A one-second bounded test reproduced the new cleanup/Reset
  lock contention before this fix, then passes after moving cleanup outside that
  unrelated lock. Cleanup still serializes through the native asset mutex.
- Stats expose registered-resource/owner counts and both caps for diagnosis.

Verification completed:

- 61 application plus 20 shared Rust tests pass. Added tests cover 2,000 sequential
  preparations, overlapping readers, duplicate URI registration, idempotent release,
  partial rollback, old reset IDs, cached-read authorization and both hard limits.
- Ten bridge lifecycle cases pass, including 250 distinct descriptor selections,
  LRU touch/eviction, active borrower retention, staged cache/fallback/cancellation,
  delayed preparation cleanup and Blender preparation across catalog eviction.
  The pre-change generated 0.2.29 bridge reproduced all ten ownership failures.
- Packaged pre-publication RPC gate passes 600 distinct hair/beard registrations
  using a private 3,001-row manifest. Maximum registered resources is three, final
  registered resources/owners are zero. Shared-reader survival, owner cap/recovery,
  failed preparation rollback and stale reset releases all pass. This fixture
  tests metadata ownership only; no geometry or DLL is decoded/loaded.
- Earlier attempts correctly aborted before publication because the private
  registration fixture lacked required game EXE/Oodle presence markers. Completed
  the never-executed/never-loaded placeholders; production validation was not relaxed.
- Full release build passes all existing Python/format/config/queue/texture/service
  gates, original 197 Blender-file comparisons, retained-runtime checks and the
  four build-input consistency checkpoints. Private packaged catalog/source/picker
  ownership checks pass with zero JavaScript errors.
- Candidate size: 52,469,298 bytes; SHA-256
  0a14d4d9ec264141b9b923fb54e4bdf8c5d8f6201f1fa2ffbd8bfdd02de4372a.
  Runtime hash remains 56aab207c04dbcb54e7ebaf9938b3f5a94b58b38e9fda5fcae45e09bb6cca214.
  Build inputs: 4,897 files, digest
  117bbb3b9cb553788324406deb0890ee88ad48416ffe62c09d1b0b98b8a47d46;
  frontend digest 886aa36ab119480ea9ead8841632b162b13cd8b232dc1de17feba9b9774eb195.

Pending before release:

- Game version 2 was still committing in Steam (same confirmed live updater 9644,
  old build 25221291, target 25761995); manifest/mod.exe remained absent. Do not
  restore old files or relax validation to manufacture passing real-asset tests.
- The new packaged catalog ownership test awaits real assets: 64 distinct bridge
  descriptor identities for one real hair asset, stable geometry/UV/texture hashes,
  48 retained owners/three shared resources, cache survival on Cancel and full
  release on Reset. Its repeated source is deliberate; the native fixture covers
  600 distinct sources separately without expensive repeated geometry decoding.
- Rerun native/fallback catalog comparisons, staged DDS/TXTR priority/cache/GPU
  reuse, strand rendering, full swap/auto-staging, textured Blender, worker
  transport and recovery against 0.2.30 after the game update.
- The desktop launcher remains on fully checked 0.2.28; neither intermediate
  candidate is presented as an unconditionally verified release. Rollback pruning
  waits until final candidate acceptance.

Remaining review findings:

- Catalog source revisions currently watch manifest/settings/extractor inputs.
  Native Catalog::load also requires NBA2K27.exe and the Oodle DLL. Review presence
  or version changes in those prerequisites when deciding cached native versus
  fallback list reuse, and review archive edits outside current watched metadata.
- Native preparation fallback currently does not retain its original error in
  the returned warnings, unlike a decode-stage fallback. Improve that diagnostic
  without changing compatibility behavior or misreporting missing dependencies.
- Continue staging/mutation read ownership, native geometry hydration/transport,
  startup and UI typing/build assumptions, plus the tracked low-detail warnings.
- Broader game, native-dialog, clean-machine and fresh 2K25 acceptance remain open.

## Twelfth Improvement Batch

Additional fixes in the unreleased 0.2.30 candidate:

- Cached accessory lists now watch NBA2K27.exe and data/oodle/oo2core_9_win64.dll
  for both resolved/preferred roots, alongside the existing manifest/settings/
  extractor inputs. Restoring native prerequisites cannot leave an old fallback
  list indefinitely cached. Revisions still use metadata, not whole-game hashing.
- Native Assets source identity also includes manifest/game EXE/Oodle metadata.
  Changes retire archives, heads and resource owners together; any configuration
  failure (including a missing/moved root) clears the old native state. The
  manifest/local-IFF size guard remains; large EXEs only receive metadata reads.
- A preparation-stage native failure is retained as a bounded compatibility
  warning without mutating original warnings or geometry/texture payloads. Cached
  geometry gets the current reason, not the previous failure. If both native and
  compatibility preparation fail, the error reports both reasons and keeps the
  compatibility cause. Cancellation still rejects and cannot warm the cache.
- Added two native prerequisite unit cases, six bridge diagnostic cases, a test
  watchdog helper and child-process watchdog verification. Six mocked lifecycle
  suites now fail instead of silently exiting zero with unfinished promises.

Evidence:

- The first diagnostic probe initially exited zero without reaching its final
  report: it awaited cancellation before releasing the deliberately held fallback
  request, while cancellation correctly waited for that request. Fixed the test
  ordering, added the watchdog, and reran it. The pre-change generated bridge then
  reproduced four diagnostic failures; all six final cases complete and pass.
  Do not count the earlier silent exit as passing evidence.
- Watchdog verification launches private Node children: an unfinished promise
  fails with exit 1; a completed case clears its timer and exits 0. The watchdog
  is limited to mocked tests and does not terminate app/game/Blender processes.
- 63 application plus 20 shared Rust tests pass, including prerequisite creation,
  version metadata changes, removal/restoration, moved game roots, native cache
  retirement, existing ownership/authorization and original geometry invariants.
- Full build, all retained regression gates, six diagnostics, watchdog checks,
  600 packaged metadata registrations and input consistency checkpoints pass.
  Original runtime payload, 197 Blender files and 2,262 retained runtime files
  remain unchanged.
- Packaged isolated UI verifies fallback-to-native recovery after prerequisite
  creation, fallback after EXE removal, native recovery after restoration, and
  decoder revision refresh. Existing root/category/manifest/selection/target-slot
  ownership cases still pass with zero JavaScript errors. Presence markers are
  never executed or loaded; this is not actual texture/geometry decoding.
- Current candidate: 52,500,018 bytes; EXE SHA-256
  3815901f36fc0ad1bc9fd8b2640a36761e1d7471c402e9f1ccc45b5d242a05e2.
  Inputs: 4,900 files, digest
  9f8f415cbe4e1d567567b01e1d7479b87d043da9688e37c63510485af6810066;
  frontend digest cdf94bdbcd39b31dc58e3344da56cc647498012c609b151b64a9f91f61ac4120.
  Runtime hash remains 56aab207c04dbcb54e7ebaf9938b3f5a94b58b38e9fda5fcae45e09bb6cca214.

Still pending:

- The actual version-2 game update still had staged depots (old build 25221291,
  target 25761995), with the same live Steam updater 9644 and no restored
  manifest/mod.exe. No game files were restored or modified by this work.
- Real-asset catalog output/ownership, native/fallback/staged rendering and DDS
  priority, full swap/auto-staging, Blender return, recovery and worker transport
  must run against this final candidate once the installation is stable.
- Source identity does not yet monitor arbitrary game archive edits outside the
  watched metadata. Continue staging/mutation ownership, native hydration/transport,
  startup/UI typing/build review, low-detail warnings and external release QA.
- Launcher remains on fully checked 0.2.28; the goal remains active, not complete.

## Thirteenth Improvement Batch (Source Checks)

- Staging now owns a generation token from inspection through config loading and
  dependent views. New Swap invalidates pending staging before reset finishes.
  Player, source, body metadata and baseline commit together; old previews and
  headband reads are invalidated immediately. Late results/errors cannot revive
  a reset player or combine metadata from two players.
- Config reads share pending work for the same target. Changing players clears
  old rows and closes old dialogs immediately. Loading disables writes; a read
  requested during a save waits for it. Save results, errors, busy cleanup and
  navigation remain bound to the target and request that started them, including
  reset/restage of the same path. Late dialog submissions cannot alter a newer
  dialog.
- Nine staging cases and ten config cases pass against the actual transpiled UI
  code, with watchdogs. The initial probes reproduced eight staging and ten
  config failures. TypeScript checks pass. Both suites are release gates now.
- These are source/isolated checks, not packaged real-game acceptance. The
  existing 0.2.30 candidate predates these source changes and must be rebuilt.
  Body-fit/headband mutation ownership and Blender UI session ownership still
  need review; this batch does not claim those are solved.
- Steam has restored manifest/mod.exe, but its record still lists build 25221291
  with staged depots for 25761995. Do not decode from the installation until the
  commit completes. No Steam or game files were changed by this work. The
  launcher remains on verified 0.2.28; no new release has been delivered.

## Fourteenth Improvement Batch: Player Edit Ownership

- Body-fit saves and body chest fixes own the target object, body request and
  staged generation. Current errors remain visible; old successes/errors/finally
  blocks cannot update a new player's metadata, status or busy controls. Body
  chest correction now marks the body editor busy and disables overlapping
  writes/read refreshes on that editor until completion.
- Preview body-fit/chest writes own an edit token, stage and render request.
  Reloads preserve newer drafts, stale writes mark the current preview dirty,
  and only the owning operation releases its busy state. Current body-fit saves
  still minimize the panel. Switching stage immediately clears the old preview
  edit flags/drafts and headband write state.
- Headband completion and config-refresh continuations are target/request-bound;
  a new headband read owns its final status. Reads during an active headband
  write cannot invalidate the write's completion. No headband fitting/export math
  changed.
- Selection attempts and committed stage generations are separate: a failed
  inspection does not strand an existing valid save, while New Swap retires it
  before backend cleanup finishes.

Verification and publication:

- The initial source probe reproduced 16 failures across 18 cases. After fixes,
  21 player-edit cases pass, including failed-selection and early-reset checks.
  Nine staging and ten config cases remain green. All three suites are enforced
  build gates with completion watchdogs. TypeScript and all 83 Rust tests pass.
- The rebuilt packaged WebView passes eight actual DOM/control edit workflows
  with delayed mock APIs and zero JS errors. These are UI ownership checks, not
  claims that mocked edits exercised geometry or wrote game files.
- Steam completed at 13:51:07 local time: build 25761995, StateFlags 4, installed
  target depots and no StagedDepots. The later tests read this completed install;
  no Steam/game files were restored or modified by this work.
- All 6,492 real hair/beard catalog rows match the original reader (5,399 hair,
  1,093 facial hair). Thirteen native/fallback assets pass. Burst tests decode
  only two of twelve latest selections, preserve exact output, exercise 64 real
  descriptor identities with at most 48 owners/three shared resources, retain
  cached owners through Cancel and release all through Reset.
- Full UI passes with eight tabs, 113 body fields, seven meshes/16 textures,
  rotation and compact rendering. A real 2K23 beard Full Swap auto-stages and
  validates with zero errors and the two tracked low-detail-reference warnings.
  Textured Blender Edit Mode round-trip returns with eyesRecalculated true,
  two references, 16 textures, 29 materials and four meshes.
- Staged DDS/TXTR priority, geometry/GPU reuse, five strand render controls,
  favorite/theme persistence, cancellation, failure/New Swap recovery and nine
  exact worker-output cases pass. Inspected main/afro/straight screenshots.
  Native first-load timing is mixed across assets; no universal speedup claim.
- Final 0.2.30 EXE: 52,501,042 bytes, SHA-256
  fc5c42d9783a77ea51c69772eb76d245bf401b82c0946bc4fcc80e0149c625e2.
  Inputs: 4,904 files, digest
  6d86a13302d4cedb85c49d286fa291c3dbbef22f30a2d9c2665b58f12e0dd193;
  frontend 6cd488fb374a78deec56e3ab1d389ea9790f91f46ab2b6ec0994c128ec2f6753.
  Runtime payload remains
  56aab207c04dbcb54e7ebaf9938b3f5a94b58b38e9fda5fcae45e09bb6cca214;
  all 197 Blender files and 2,262 retained runtime files are identical.

Remaining review:

- Blender UI session ownership across player switches, and cross-tab mutation
  coordination, still need targeted review. Backend Blender operations themselves
  were not changed. Current source fingerprint does not watch arbitrary game
  archive edits outside its tracked metadata.
- Audit failed Reset recovery while a write is pending: retiring the stage
  generation must not strand busy flags if backend cleanup fails and the old
  player remains selected. Successful Reset and ordinary failed selection are
  covered; failed Reset during a mutation is not yet covered by this batch.
- Native geometry hydration/transport, startup/typing/build review, the two
  low-detail warnings and external QA remain. Do not mark the broad goal complete
  from this release's passing regressions alone.

## Fifteenth Improvement Batch: Blender UI and Failed Reset

- The Blender UI uses normalized target-owned live records, not a global busy
  flag. Target switches restore the correct active session and status; inactive
  terminal events release their own records. Open-result continuations cannot
  regress returning/complete states or clear another handoff. Old handoff/session
  IDs are ignored. New Swap does not clear live backend-owned Blender records.
- Reset drains six ordinary player mutation command types even when preview
  cancellation fails, retains the original cleanup error and remains retryable.
  Live Blender supervision is intentionally excluded from that mutation drain.
- Failed Reset retires obsolete UI writes, clears stranded edit/read busy flags,
  rereads retained player/config data and preserves unapplied body-fit drafts.
  Values already saved to disk are reconciled away; invalid input stays available
  for correction. Recovery failure still releases controls. A resetting guard
  prevents new staging/writes and config-preview selection during cleanup.
- The body Refresh button now explicitly calls its options-bearing function;
  pointer events are not mistaken for recovery options. Ordinary Refresh keeps
  its prior discard behavior. No Blender extension/conversion algorithm changed.

Evidence:

- The initial Blender UI probe reproduced eight failures in ten cases. All ten
  now pass, including inactive-player events, opening/terminal ordering, a newer
  same-path handoff, case/slash normalization and view reset. These use the actual
  transpiled UI functions, not a separate session implementation.
- Twelve staging/reset/recovery/retry and 26 player-edit cases pass. The bridge
  drain test covers all six edit types, rejected writes, original cleanup errors
  and retry. Ten config cases and adjacent lifecycle/scope/freshness tests pass.
- Build gates, type checking, 63 application/20 shared Rust tests and 38 inherited
  engine tests pass. The packaged resource fixture still exercises 600 distinct
  registrations with three live resources and zero retained at the end.
- Ten packaged WebView DOM/control workflows pass with controlled delayed/mock
  operations and zero JS errors. New cases verify failed Reset draft preservation,
  retry, independent A/B Blender opens, late results, restore, returning/closed
  events and New Swap retaining a live handoff. This is not live Blender UI proof.
- Separate real-asset full UI/2K23 Full Swap and auto-stage pass with zero errors
  and the same two tracked low-detail warnings. Textured Blender Edit Mode return
  passes: eyesRecalculated true, two references, 16 textures, 29 materials/four
  meshes. Staged DDS priority, geometry/GPU reuse, catalog burst/resource scopes,
  five strand controls, theme/favorites/recovery and nine exact worker outputs
  pass. No change to renderer math is claimed.
- 0.2.31 EXE: 52,501,554 bytes; SHA-256
  1791c0fda966af53611b4468cd8ad77d5cb47af0ca2269bacd8d19d06d1bd4f5.
  Build inputs: 4,906 files; SHA-256
  5770efc49861114ce04960244a5365bf04809cb950d9ab6d6a4ea5f0a698ffa6;
  frontend a776140bb35b489d7d9f3f959d948caca7fcb1cffefafea699c8c07ef86363bc.
  Runtime payload remains
  56aab207c04dbcb54e7ebaf9938b3f5a94b58b38e9fda5fcae45e09bb6cca214;
  all 197 Blender files/2,262 retained runtime files are identical.

Remaining review:

- Cross-tab mutation coordination and cache invalidation after player edits need
  further review. Arbitrary game archive edits outside watched source metadata,
  native hydration/transport, startup/typing/build and low-detail warnings remain.
- Inspect actual main-preview accessory visibility as well as mesh counts, and
  validation of invalid body drafts before renderer hydration. Existing numeric
  parity tests alone do not prove every material is visible or all draft inputs
  are safe for rendering.
- External QA below remains. The broad improvement objective is still active.

## Sixteenth Improvement Batch: Preview Draft Boundary and Real Controls

- Preview reloads accept drafts only for current schema fields with numeric,
  finite values representable as 32-bit game floats. They preserve finite
  negatives/over-one values and do not coerce raw input to zero. Body-table
  overflow input is marked invalid rather than passed to preview/write actions.
- Incomplete preview dimension text is stored separately from numeric geometry
  drafts. Invalid fields are marked and block Apply/Calculate; correction and
  Revert recover. Same-player reload retains incomplete controls safely, while
  new staging/Reset removes them and removed schema keys are retired.
- A real-input probe caught a second issue missed by earlier mesh/field counts:
  controls constructed during loading remained disabled after loading finished.
  The shared control-state update now refreshes input editability directly from
  current load, chest and Reset busy flags. It does not rebuild all rows merely
  to enable/disable them. Renderer deformation/shader math was not changed.

Evidence:

- Initial draft probe reproduced seven failures in nine cases. Fifteen final
  cases pass, covering current schema, raw/unsupported types, nonfinite/overflow
  values, finite negatives and values over one, source/previous drafts, Apply,
  correction, Revert, stale keys and field lifecycle. All are build gates.
- The ad hoc real-input test initially timed out on a genuinely disabled head-X
  input. The focused lifecycle test then reproduced that failure. Its first
  plain-object fixture omitted initialized busy flags; correcting those defaults
  made the fixture match real DOM behavior. The build gate had rejected the
  failing fixture build; do not count it as passing publication evidence.
- Permanent full UI test now actually types into a real player's dimension field,
  checks it is enabled after loading, compares finite geometry bounds through
  blank input, verifies correction/Revert, inspects invalid aria state, captures
  the panel, and verifies the source IFF hash did not change. All pass on the final
  candidate. Inspected artifacts/body-fit-invalid-input.png visually.
- Eleven isolated packaged edit workflows pass with zero JS errors. One earlier
  run completed its UI assertions but failed private-folder cleanup with EPERM;
  it was not counted as a passing run. No owned processes remained, and the exact
  verified link-free test folder was removed. Bounded standard fs.rm retries now
  handle transient Windows cleanup locks without swallowing persistent failures.
  The final run exits zero and removes its own folder. Production cleanup policy
  and user/runtime/tool folders were not changed by this test hygiene fix.
- New main-accessory visibility test measures stock, prior-converted and freshly
  converted png06827 at front/side/back views, with hair/beard hidden separately.
  All three have visible contributions: side hair 7,424/7,911/7,911 pixels and
  front beard 981/1,248/1,248 pixels. Material diagnostics retain color, lightmap,
  tangent/range and cutout-depth bindings. Inspected stock/converted side images.
  This did not reproduce a general renderer visibility bug, so no strand shader
  or fitting change was made. It is representative evidence, not every asset.
- Final build: all 83 Rust tests, 38 inherited engine tests and retained gates
  pass. Real full UI/2K23 Full Swap/auto-stage, staged DDS priority/cache and GPU
  reuse, textured Blender Edit Mode return (eyesRecalculated true), five strand
  controls, recovery/favorites/theme and nine exact worker outputs pass. The same
  two low-detail reference warnings remain; no universal performance gain claimed.
- Final 0.2.32 EXE: 52,502,066 bytes; SHA-256
  f222547dcae9e454e4b5fe72d0ad31a458d0d359469e230b6ab1d4cab19c8e8c.
  Inputs: 4,908 files; SHA-256
  4ca38c699e26dd8a292af4455b4066d78873539c8e6b8958bc5845239c9ba8dc;
  frontend 622d7213c26f272553d17b6ae88aaaf4528e0e5e3090664343dd3c995c242cb3.
  Runtime payload remains
  56aab207c04dbcb54e7ebaf9938b3f5a94b58b38e9fda5fcae45e09bb6cca214;
  all 197 Blender files and 2,262 retained runtime files are identical.

Remaining review:

- Cross-tab mutation coordination/cache invalidation, native geometry hydration/
  transport, watched-source coverage, startup/typing/build review and the two
  low-detail warnings remain. Main payloads include both regular/tangent pairs;
  profile whether any are redundant before considering filtering. Names/counts
  alone are not enough to justify removing geometry or changing fitted output.
- Broader visual/in-game source coverage and external QA below remain. Do not
  mark the broad goal complete from this release's representative checks.

## Seventeenth Improvement Batch: Binary Catalog Geometry

- Native catalog geometry uses the existing bounded binary IPC envelope, with
  explicit host capability negotiation. JSON remains available for older hosts
  and diagnostic comparisons. Existing archive/file packets are unchanged.
- Cached donor heads retain raw typed-array bytes and small descriptors instead
  of base64. Geometry packet arrays are aligned, range/type/padding checked and
  independently owned by asset and donor. Returned asset mutation cannot corrupt
  a cached head or later response. Original source IDs and float bits survive.
- Raw geometry calls use the existing resource authorization, source revision,
  mutation lock, cancellation scope and closing guards. Local paths cannot bypass
  registered manifest sources. Late replies cannot reach a retired worker.
- Tangent reconstruction, renderer shaders, texture bindings and conversion/
  Blender algorithms were not changed. No disk cache or dependency was added.

Evidence:

- All 85 Rust tests, strict/incremental TypeScript checks, inherited 38 engine
  tests and existing release gates pass. New packet/bridge gates cover malformed
  descriptors, alignment/size limits, negative zero, exact indices, capabilities,
  JSON override, transferred buffers, scoped drains and late/error responses.
- Four packaged cases hash every mesh array, group and texture against 0.2.32;
  all match exactly, including independent buffers and cache mutation isolation.
  Head-inclusive native results transfer two buffers rather than 10/13; borrowed
  beard results transfer one rather than eight, with identical array bytes.
- Three controlled native failures preserve diagnostics and exact original-
  decoder geometry/texture output. The first test tried replacing Tauri's read-
  only invoke wrapper and did not inject a failure; that run failed and is not
  acceptance evidence. The corrected test injects at the owned worker boundary.
- Thirteen native/original-reader asset comparisons pass within the retained
  0.0001 position tolerance, with unchanged texture metadata/payloads and exact
  indices/source IDs. Catalog rotation, burst coalescing, 48-owner limits, Reset
  release, independent cancellation and nine legacy worker comparisons pass.
- Final exact sharing EXE passed real Full Swap/automatic staging, preview input
  validation, staged DDS/TXTR priority and GPU reuse, textured Blender Edit Mode
  round trip with eyesRecalculated true, main hair/beard visibility, five strand
  controls, eleven isolated edit workflows and favorites/theme/error recovery.
  The same two low-detail mesh-reference warnings remain; not fixed by transport.
- Retained cache measurement for the same two donor heads: 2,186,490 bytes in
  0.2.32 versus 1,912,846 in 0.2.33 (12.5% less); both retain the 32 MiB budget.
- Warm latency is mixed. Final back-to-back nine-sample medians (old -> new ms):
  dreads with head 126.4 -> 109.2; beard with head 77.5 -> 83.3; afro with head
  140.4 -> 128.9; borrowed-head beard 57.7 -> 72.6. Same-build JSON controls were
  173.3/115.1/198.1/72.9 ms. Initial/candidate runs differed substantially, so no
  universal speedup is claimed. Initial reports are preserved separately from
  artifacts/geometry-transport-baseline.json and geometry-transport-report.json.
- Final EXE: 52,539,442 bytes; SHA-256
  e9c8c2bb6e33f6c2323192e840a4fe4c7e40437e532cbce198c80bde35164690.
  Inputs: 4,910 files; SHA-256
  44a4d53bc4df83542e94eb21783094bae03ef3e1b7c62e7f6d6982a1dcf20d7b;
  frontend 1df958ca18acb6c0cc0e438dc266323d8b1e4ffd4f7120e74b1baf0b0167ad87.
  Runtime payload remains
  56aab207c04dbcb54e7ebaf9938b3f5a94b58b38e9fda5fcae45e09bb6cca214;
  all 197 Blender files and 2,262 retained runtime files are identical.

Remaining review:

- Cross-tab mutation/cache coordination, archive/source watch coverage, startup/
  typing/build/disk review, duplicate ordinary/tangent payload profiling and the
  two low-detail warnings remain. Catalogs still read archives for materials and
  textures; binary geometry does not eliminate those reads or all worker copies.
- All external QA below remains. This incremental release does not complete the
  broad feature/appearance/output-quality and architecture-improvement objective.

## Eighteenth Improvement Batch: Local Edits and Manifest Cache Ownership

- Successful player edits now retire all local preview handles/packets without
  clearing immutable manifest/shared resources or donor heads. Cached packets
  remember whether they came from a local file, so anonymous/local entries are
  retired even after their resource registration was released. Byte accounting
  is recomputed from retained packets/blobs within the existing budgets.
- Mixed leases lose their retired local members but keep manifest ownership.
  A delayed old release cannot decrement a freshly registered local resource,
  including when its URI is reused. Reset and source/settings changes still clear
  the appropriate full caches; authorization is checked before cached reads.
- Bridge staged entries carry their target identity. Successful edits invalidate
  that target's cached/pending data, including during asynchronous resource
  release. New reads wait for matching pending writes; unrelated target entries
  and rejected-write caches are retained. Matching Blender completion and Full
  Swap outputs use the same invalidation. Renderer/engine math is unchanged.

Evidence:

- Real 0.2.33 baseline, with a disposable copy of the player: body-fit save erased
  18 registrations, all five archive packets and the donor head; both warmed
  catalog assets fell back. The original IFF remained byte-identical.
- Final 0.2.34 private-copy test passes: registrations 18 -> 17, packets 5 -> 4,
  retained archive bytes 10,310,212 -> 6,989,985, donor bytes unchanged at 956,423.
  The old local handle expires; the mixed lease's body map remains registered.
  Both hair and beard stay native with headCacheHit true and exact pre-edit
  geometry/texture hashes. Saved hips.tx reloads as 0.25. A nonexistent-target
  save is rejected without discarding native catalog data. Reset ends at zero
  registered resources, owners and payload bytes; original fixture is unchanged.
- Initial bridge probe reproduced eight failures: six same-target cache cases,
  an in-flight decode and a read started during a write. Sixteen final cases pass,
  including unrelated/failed cache retention, matching/obsolete Blender events,
  overwritten swap outputs, release-time edits and target/fingerprint collisions.
- Native unit test covers mixed manifest/shared/local owners, anonymous cached
  local packets, idempotent retirement, retained heads/blobs, late release after
  same-URI registration and full Reset cleanup. All 86 Rust tests and retained
  release gates pass, as do 38 inherited engine regressions. One initial strict
  TypeScript check rejected the empty-string union from the old cache expression;
  explicit optional cache lookup fixes the type boundary. That failing check was
  not acceptance evidence. Final source and packaged checks pass.
- Exact final EXE passes 13 catalog comparisons, four exact transport cases and
  three failure fallbacks, catalog burst/owner limits, cancellation isolation,
  eleven packaged edit workflows, real Full Swap/auto-staging, textured Blender
  return with eyes recalculated, staged DDS/TXTR priority/GPU reuse, main hair/
  beard visibility, five strand controls and favorites/theme/failure recovery.
  The same two low-detail reference warnings remain; no general speedup claimed.
- Final EXE: 52,532,786 bytes; SHA-256
  4269e4cf48bc7abd5d1dd32f9c9d8116cfc1ceeb61a7e3f7b1fe126c4e4ae307.
  Inputs: 4,912 files; SHA-256
  6eabdde09f95c72f37b8eb6d44c0dbce5932c5b5a97160c4a70a59ad93d03f85;
  frontend b17a6132c4920a9f8c4902100eb616e91579144833115bb3a14b294b57575b90.
  Payload remains
  56aab207c04dbcb54e7ebaf9938b3f5a94b58b38e9fda5fcae45e09bb6cca214;
  all 197 Blender files and 2,262 retained runtime files are identical.

Remaining review:

- These checks cover data requests/cache ownership, not every asynchronous GPU
  renderer commit or independently prepared Blender reference bundle. Review
  those UI mutation windows, cross-player local dependency aliasing, partial
  engine-write failures, watched-source/archive coverage, startup/typing/build/
  disk behavior and the two low-detail warnings. All local native packets remain
  conservatively retired after edits; this is not per-package native eviction.
- External QA below remains. The broader improvement objective is still active.

## Nineteenth Improvement Batch: Renderer Commit and Visible Recovery

- A shared UI invalidation boundary now covers accepted body/chest, accessory,
  config and Blender edits. It cancels obsolete main-renderer work, retires its
  UI request, removes partial geometry and leaves the staged player dirty rather
  than permitting an old renderer completion to mark it current. Other players
  and idle committed scenes are not cleared by this boundary.
- Body-fit baselines/drafts have an explicit target owner independent of the
  current rendered path. Same-player cancelled loads preserve valid/incomplete
  input; new staging/Reset clears the owner. Current schema/finite-float checks
  remain. Cancelled renderer results cannot mark a view current.
- Interrupted visible views queue a fresh load, guarded by target, stage, request,
  active view, loading/write and Reset state. Newer intent suppresses the queued
  recovery. Config selection no longer increments a second unrelated request
  counter after shared invalidation. Refresh is disabled during preview writes.
- Renderer payload identity commits only after successful preparation. A pending
  same-key request cannot enter the completed reuse path. Failed/current-cancelled
  loads clear partial state; obsolete completions leave the latest load alone.
  Successful material/lighting/deformation calculations are unchanged.

Evidence:

- Real 0.2.34 renderer-yield baseline reproduced an old body-fit value 0 being
  marked current (dirty false, loadedPath set) after a mocked save of 0.25. The
  first harness run had not initialized the lazy renderer; it failed before the
  probe and is not baseline evidence. The corrected run yielded twice and
  reproduced the race. Source IFF hash was unchanged.
- Initial 0.2.35 candidate passed cancellation/manual recovery. Final candidate
  also passes automatic visible recovery: after cancellation geometryIds is [],
  saved/preview baseline is 0.25 and dirty is true; manual retry renders seven
  meshes with 0.25. A visible second edit recovers automatically to 0.5, seven
  meshes, loading/dirty false, zero JS errors. Mutations are explicitly mocked;
  real geometry/material preparation and source-file immutability are checked.
  Inspected artifacts/preview-gpu-edit.png: recovered render is nonblank/framed.
- Renderer unit coverage includes committed identity, completed reuse, same-key
  pending loads, stale completions, texture failure/full retry, outer and direct
  inner cancellation, existing lighting response, target-scoped clearing and
  preserved numeric/invalid drafts. Visible-recovery guards cover stage/request/
  view changes, a newer load and Reset. Eighteen draft cases pass, including
  cancelled-render ownership, cross-player owner rejection and cancelled results.
  Existing 26 edit, ten config, ten Blender UI and hair-target tests pass.
- An isolated fixture initially lacked transparentBackground, and the hair-target
  fixture had not included the new real shared helper. Those failures were not
  accepted. Fixtures now exercise the helper; final gate suite passes, including
  all 86 Rust tests, TypeScript and 38 inherited engine regressions.
- Exact final EXE also passes the real private body-fit save/cache test, four
  exact transport cases and three failure fallbacks, 13 native/original catalog
  comparisons, burst/owner limits, scoped cancellation, eleven packaged edit
  workflows, real Full Swap/auto-stage, textured Blender return with eyes
  recalculated, staged DDS/TXTR priority/GPU reuse, main hair/beard visibility,
  five strand controls and favorites/theme/failure recovery. The same two
  low-detail reference warnings remain. No general speedup is claimed.
- Final EXE: 52,532,786 bytes; SHA-256
  f76f908766e5ad5484831e69dface10453be86284edd2332dc04a7f20068adda.
  Inputs: 4,914 files; SHA-256
  dd3a4c80d9399bcdae8a523856e11300009a23b69bae52bb22a9b071f6df0e10;
  frontend e4a0696ef273fbdaecbd8ab06f1ae3c2f1f6534b85c2e1fd769273d96c9b5c9b.
  Payload remains
  56aab207c04dbcb54e7ebaf9938b3f5a94b58b38e9fda5fcae45e09bb6cca214;
  all 197 Blender files and 2,262 retained runtime files are identical.

Remaining review:

- Independent Blender reference preparation, unsaved draft input during GPU
  preparation, cross-player local dependency aliasing, partial engine-write
  failures, watched-source/archive coverage, startup/typing/build/disk behavior
  and the two low-detail references remain. Inspect clear-time editor retention
  before claiming all CPU memory is released with a renderer clear.
- Cached Preview-tab entry still writes the renderer-loading status even when
  the module/view is already current (visible in the probe screenshot). Review
  that initializer/status path separately; it does not mean the recovered mesh
  is still loading. External QA below remains; the broad objective is active.

## Twentieth Improvement Batch: Blender Return Preservation

Changes:

- Blender reference preparation drains existing same-player writes and rejects
  six ordinary same-player edit commands until the opened event confirms input
  ownership. Other players remain editable; settings changes abort obsolete
  reference preparation, including fallback paths. This is UI command ordering,
  not an atomic snapshot of all externally edited dependency files.
- Failed/conflicting Blender returns preserve exact IFF bytes in Blender Recovery.
  The native path uses bounded streaming hashes, read sharing that excludes
  concurrent writers, create-new copies, content verification for reuse and
  deduplicated target/content identities. Existing changed files are not replaced.
  Sidecars and session markers track confirmed copies; recovery errors keep the
  original export session. Health distinguishes recovered user work from cache.
- Python supervision waits for Blender to close on export errors. Cleanup now
  requires a matching applied digest or a matching, scoped recovery copy. Native
  shutdown, Drop and dead-session retirement recover orphan root-level return
  IFFs before removal; backup/input/static-jersey references are not recovered.
  This does not claim autosave of arbitrary .blend files or unsent edits.
- Preview view requests own their initializer continuations; stale success/error
  callbacks cannot update a different view. Cached entry retains rendered status.
- No Blender extension, conversion, tangent, deformation, material or lighting
  algorithms changed. Native hashes stream 64 KiB blocks instead of allocating
  an entire IFF simply to hash it. Recovery copies are intentional user storage.

Evidence so far:

- New preparation probe initially reproduced eight failures; all eleven ordering,
  mutation, target, opened/failure and settings cases pass. Preview initializer
  probe reproduced four failures in five cases; all five now pass.
- The installed 0.2.35 Python service fails four of twelve supervision cases:
  unapplied return disappears, changed recovery confirmation still permits
  cleanup, export errors do not wait, and applied marker is absent. Current
  service passes all twelve. The latter marker case tests a new mechanism; the
  other three expose behavior risks in the released code.
- Ten native recovery/retirement tests cover exact bytes, deduplication, changed
  existing copies, foreign paths, byte budgets, health bounds, successful return,
  abandoned-session recovery, unavailable storage and Windows junction rejection.
  All 96 Rust tests (76 app + 20 shared), 38 inherited conversion regressions and
  TypeScript/build gates pass.
- First packaged GPU run failed because warm geometry preparation finished below
  the normal eight-millisecond yield threshold, so the second edit was never
  injected. Instrumented keys confirmed the intended new payload reached the
  real renderer. The test now uses a deterministic clock to exercise time slicing,
  asserts that the second path actually yielded, and restores the clock afterward.
  This is a test correction, not an app rendering change. Manual 0.25 and automatic
  0.5 recovery both pass with seven meshes and the source IFF unchanged.
- Steam state confirms build 25761995 installed with downloaded/staged byte totals
  complete; manifest and mod.exe are present. Game data remains read-only.
- The cancellation acceptance test's old hard-coded 5,361 hair count failed
  against the updated manifest's 5,399 rows. It now compares every recovered row
  against the complete independent compatibility reader. Hair (5,399) and facial
  hair (1,093) row/name/order/flag comparisons pass; cancelled native listing
  does not fall through to compatibility work and recovers normally.
- All 15 packaged checks passed on final EXE SHA256
  60802461c4bb2709892c6965317846ca8e1e41f716166adb3568c9fcc24f139d:
  real GPU edit recovery, native edit/cache ownership, real Full Swap/auto-stage,
  textured Blender return/eye recalculation, staged DDS/TXTR precedence and GPU
  reuse, catalog rendering/performance/bursts/cancellation/full-list comparison,
  eleven control ownership workflows, main accessory visibility, five strand
  controls, favorites/theme/failure recovery and exact binary-worker geometry.
- Packaged recovery acceptance uses a private temporary data root and a copied
  representative IFF. Startup recovers identical bytes, excludes backup/jersey
  references, leaves a live owner alone, reports recovery in Health, preserves it
  during cache cleanup, deduplicates on restart and keeps the original export
  when recovery storage is occupied. Source hash is unchanged; the entire owned
  fixture, including its temporary prepared runtime, is removed afterward.
- Sharing EXE is 52,585,982 bytes. Its 4,918-file source fingerprint is
  2fc9a2f22eb9faa89af03feec5dcd769ed6aa9726bdcdf8f4d5d34951a51bd82;
  frontend fingerprint is
  a6cd2ccdee858fd48c765b9695ba6e2daa6851924bbe824663167719879eea83.
  The supervision-service edit changes payload SHA256 to
  d21155c3a610a8fe4443a0eee77d12a5da229b1ee40623b3419d0ba27f965360,
  so first use prepares this runtime once. All 197 Blender extension files and
  2,262 retained Python-runtime files are identical to the preservation reference.

Remaining review:

- External/direct writes and Full Swap output replacement during Blender bundle
  preparation still need native dependency snapshot/hash review. Ordinary UI edit
  coordination does not prove cross-process filesystem atomicity.
- Unsaved draft input during GPU preparation, cross-player local aliases, partial
  write failures, clear-time editor retention, startup/typing/disk behavior, two
  tracked low-detail references and external QA remain. The broad goal is active.

## Twenty-First Improvement Batch: Body-Fit Draft Commit

Changes:

- Staged body-fit metadata remains the renderer's baseline; UI drafts no longer
  mutate the returned preview payload or become dimension baselines. After a
  successful current render, the UI applies the latest finite draft through the
  existing editor API. No extra deformation is requested when values match.
- Body-to-Preview draft synchronization uses the explicit draft owner even when
  loadedPath is empty during first-load preparation. Inputs update draft state,
  not an editor whose geometry is being replaced. Completed geometry receives
  those latest values. Existing cancellation/target guards reject obsolete work.
- Incomplete Body inputs propagate their raw invalid state to Preview while
  retaining the last finite visual value. Apply stays blocked. Revert synchronizes
  invalid-only fields too, not just fields with a different numeric draft.
- No renderer/deformation algorithm, shader, Blender extension, engine service,
  file format, IPC contract, cache or dependency was changed. This revises the
  order in which existing UI state and editor APIs are used.

Evidence:

- The initial ten-case deferred-render probe reproduced five failures: pending
  dimensions contaminated renderer baselines, first-load input was ignored,
  rebuilding input reached the old editor, a baseline revert retained an older
  draft, and incomplete input lost its latest finite visual value. All eleven
  final cases pass, including invalid-only Revert, other targets, obsolete loads,
  failed rendering, metadata immutability and the no-extra-pass path.
- The released 0.2.36 EXE independently reproduces the visible error for spine2.tx
  on the representative converted player: before-load, during-load and incomplete
  input cases each differ from the independent reference by 10,351 pixels (RMS
  5.10448). Revert also contaminates metadata, and incomplete input permits one
  mocked save attempt. No fixture IFF was written during these checks.
- Final 0.2.37 GPU tests use real Body input handlers, the production renderer and
  a deterministic clock to enter time-sliced preparation. All four cases match
  independent references exactly: zero differing pixels and RMS zero. The three
  edited references each differ from the neutral pose by 10,351 pixels, proving
  this is not a blank/no-effect comparison. Invalid input blocks Apply with zero
  save attempts; metadata remains staged; cached refresh retains geometry IDs;
  the source IFF hash is unchanged. Screenshot reviewed for intact UI/layout.
- All 96 Rust tests, 38 inherited conversion regressions, 18 validation cases,
  eleven commit cases and TypeScript/build gates pass. All 16 packaged checks
  pass on EXE SHA256
  f75da601ab7e1a5113830aa7569d01265c084d11e0ed6a42a3ba75495615e59a:
  new draft GPU comparison, saved-edit GPU recovery, native edit/cache ownership,
  real Full Swap/auto-stage, textured Blender return/eye recalculation, staged
  DDS/TXTR priority/GPU reuse, catalog performance/bursts/cancellation/full-list
  comparison, edit controls, accessory visibility, five strand controls,
  favorites/theme/recovery, exact binary transport and Blender-return recovery.
- One native-preview launch was not created because automatic permission review
  timed out. A single retry passed; no running job was restarted on a timeout.
- Sharing EXE is 52,586,494 bytes. The 4,920-file source fingerprint is
  53a2f944c8996f2df955ba2dca3ec98be6302fd6e4f60480d97dad697b4a85fc;
  frontend fingerprint is
  48787ac8acfe5b437a2b0584ccd7f2e3fef852cb1d9cd258a44e369e61fd0bdf.
  Runtime payload remains
  d21155c3a610a8fe4443a0eee77d12a5da229b1ee40623b3419d0ba27f965360,
  so the prepared 0.2.36 runtime is reused. All 197 Blender extension files and
  2,262 retained Python-runtime files remain identical to the reference.

Remaining review:

- Cross-tab typing during pending file saves still needs its own audit; the GPU
  commit tests do not prove write-completion draft retention. External/dependency
  snapshots during Blender preparation, local aliasing, partial write failures,
  clear-time editor CPU references, startup/typing/disk behavior and the two
  tracked low-detail references remain. In-game animation/config/roster behavior,
  current native picker interaction and clean-machine release QA remain external.
  The full goal remains active, not redefined around these passing tests.

## Twenty-Second Improvement Batch: Full Swap Distance References

The user reaffirmed the target: working features, appearance and output quality,
not exact implementation parity. This batch addresses a concrete exported-player
defect rather than carrying over the old warning-only behavior.

Evidence and implementation:

- Read-only inspection of the prior real 2K23 png4100 -> 2K27 png06827 export
  confirmed that `hair_dreads_loShape` and `facialHair_beard_loShape` were absent.
  Regular and tangent companions retained only their primary meshes, with all six
  distance levels using 14,084 hair vertices and 888 beard vertices respectively.
- The Tauri-only Full Swap validator now corrects existing `meshlo` fields after
  companion conversion/highest-detail normalization/custom naming, before normal
  archive validation and the existing journaled publication. It never rewrites a
  published player, the input target, another game's output or original app code.
- Every required regular/tangent variant must have the exact primary mesh and
  pass existing highest-detail buffer/index validation. Missing/unverified variants
  remain untouched; duplicate geometry, invalid indices, non-full-detail streams,
  oversized/duplicate archive members and sources outside the private stage are
  rejected. Custom names and regular-only/tangent-only modes are supported.
- Only the appearance JSON distance fields change. Unrelated member bytes,
  configuration paths/RDAT, ZIP metadata/comment, geometry, UVs, textures and
  physics are preserved. Replacement is private/atomic; failure leaves the staged
  file intact and removes its temporary copy. Later validation failure leaves the
  prior published output intact. A second pass makes no write.
- Sixteen focused tests pass, including real export copies and failure/preservation
  checks. An initial test found the ZIP archive comment was not retained; this was
  fixed before the build. The lower-detail rejection assertion was corrected to
  match the existing validator's actual error message.
- All 96 Rust tests, 38 inherited conversion regressions and release build gates
  pass. Seven targeted packaged workflow checks pass on final EXE SHA256
  431e25606f88e2f4c35f5b353ee1373309f4695e0424191fc460eea4afed43af:
  real Full Swap/auto-stage, textured Blender return/eye recalculation, stock/prior/
  fresh accessory visibility, staged DDS/TXTR priority and GPU reuse, four body-fit
  GPU draft comparisons, favorites/theme/cancel/failure recovery and Blender return
  recovery. The real export now reports eight archives, zero errors, zero warnings,
  and both explicit corrected references. Final read-only inspection confirms that
  both distance selectors resolve in regular/tangent companions and all six levels
  still retain full geometry. The fresh preview screenshot was reviewed.
- Sharing EXE is 52,587,818 bytes. The 4,921-file source fingerprint is
  01f5ae0cf41379633b6adb4ac9bd1f99431b88ab43a013cced6a9ee65308d044.
  Frontend fingerprint remains
  48787ac8acfe5b437a2b0584ccd7f2e3fef852cb1d9cd258a44e369e61fd0bdf.
  Runtime payload is now
  2b7fa9f63443756de8780f1428ab89439e1fde1bea9a079ab9f3151a6dc85a4b;
  one new managed runtime is prepared for the engine change, then reused. All 197
  Blender extension files and 2,262 retained Python-runtime files are unchanged.

Remaining functional review:

- Native/config-specific accessory installation is not changed by this Full Swap
  correction. Its own distance-reference behavior still needs representative
  checking. Cross-tab typing during pending saves, Blender preparation dependency
  snapshots, local aliases/external changes, partial write failures, clear-time
  editor retention and practical startup/disk behavior remain scoped reviews.
- In-game distance/animation/physics/configs, live roster access, current native
  picker interaction, fresh 2K25 source coverage and clean-PC distribution remain
  external release checks. Correct references and passing previews are not proof
  of gameplay behavior. Existing working differences are not parity blockers.

## Twenty-Third Improvement Batch: Cross-View Body-Fit Editing

The current read/write paths confirmed that each editor disabled only itself.
The other view could admit edits or a competing save, then have those drafts
cleared by the first save completion. Ordinary Body tab entry also reread disk
with explicit-discard semantics. Preview invalid text did not reach Body's Apply
guard, unlike invalid Body text flowing to Preview.

Functional corrections:

- A small target-scoped busy predicate is shared by Body/Preview action guards,
  input availability, queued input handlers, Revert/Calculate and chest operations.
  It covers Body reads as well as writes; a new ordinary read may supersede an old
  read, but reads cannot interrupt pending writes. Both views unlock after failure.
  Existing player/generation ownership guards still retire stale completions.
- This deliberately extends the existing per-view save lock to both views instead
  of adding a concurrent-edit rebase system. Live editing remains available while
  idle, but another view cannot change a value during its brief persistence window.
  It is a simpler workflow improvement, not preservation of old parity behavior.
- Body tab entry uses preserve-draft semantics; the explicit Refresh command keeps
  its intentional discard behavior. Known finite preview drafts are retained across
  an automatic reread, including the last finite value behind incomplete text.
- Invalid Preview text now synchronizes to Body's draft/validation state. A valid
  correction or Revert clears it in both views. Explicit saved-value synchronization
  clears obsolete invalid state. Successful Preview Apply still minimizes the panel.
- No backend, conversion, deformation, shader, Blender, settings or favorite code
  changed. No new cache or runtime payload was introduced.

Verification:

- The new real-window test uses mocked mutations and an isolated test-data folder;
  no player files are written. It reproduced six failures among nine cases on the
  actual 0.2.38 EXE. All nine cases pass on 0.2.39: cross-view locks/competing Apply,
  failure unlock/draft retention, numeric and incomplete tab-entry preservation,
  explicit Refresh and both chest-operation directions.
- The first baseline harness itself awaited an incorrectly admitted competing save
  before settling it. Only its identified child test window was stopped; the test
  was corrected to settle competing requests and rerun after terminal exit. No
  user window was closed and no live test was restarted on an observation timeout.
- All 96 Rust tests, 38 inherited conversion regressions, 16 distance-reference,
  29 edit-ownership, 18 draft-validation and eleven draft-commit cases pass.
- Seven packaged checks pass on EXE SHA256
  54d1b237583fa41de3b16341df98e4566d0c0320f2809e4af36db1a908273976:
  new cross-tab controls, existing player-edit UI ownership, real GPU draft
  comparisons, saved-edit GPU recovery, real Full Swap/auto-stage, textured Blender
  round trip with eyes recalculated and favorites/theme/reset/failure recovery.
  The fresh Full Swap retains zero errors and warnings. Four real GPU draft
  comparisons remain zero differing pixels/RMS zero against independent references.
  The updated preview screenshot was reviewed.
- Sharing EXE is 52,587,818 bytes. The 4,922-file source fingerprint is
  3d7eb89e5ffff5ed2fab6bcfd2f585bc5e7a0417b5e137645c167de3e53b5a5b;
  frontend fingerprint is
  a71508a01bd691b0e201c90425b579d41cef2f32558d5e23d93d244faa7a5d23.
  Runtime payload remains
  2b7fa9f63443756de8780f1428ab89439e1fde1bea9a079ab9f3151a6dc85a4b,
  reusing the prepared 0.2.38 engine. All 197 Blender extension files and 2,262
  retained Python-runtime files remain identical to the reference.

The user explicitly authorized agents. Eight focused agents reviewed or implemented
disjoint accessory, publication, runtime, Body/Preview, snapshot and acceptance-test
work. Their changes were reviewed and integrated; dispatch alone is not evidence
of completion. The full goal remains active, with the external QA checks below.

## Twenty-Fourth Improvement Batch: Transaction Safety And Selected Inputs

- Native/config Hair and Facial Hair installs now validate exact no/yes/both
  geometry variants, primary/distance selectors and material bindings before
  journaled appearance-last publication. Thirty-five isolated cases cover
  staged precedence, paired and tangent-only slots, partial texture failures,
  external edits, interruption and rollback. Historical real candidates that
  cannot resolve fitting/material inputs fail before publication; fixture tests
  are not presented as proof that every manifest hair can be installed.
- Interrupted accessory transactions recover before Python reads and native
  reads/capture. A native read checks under the mutation read gate, releases it
  before exclusive recovery, then retries under read protection. Old immutable
  snapshot URIs do not reread live outputs. Recovery attempts retire local preview
  resources even when rollback fails after partially restoring files. Failed
  player mutations also retire local resources.
- Journal signatures are cached per canonical player with bounded oldest-entry
  eviction, not one entry per retained transaction. Independent review found and
  corrected file-alias bypass and multi-player eviction-loop edge cases. Focused
  Rust regressions cover aliases and two players with 600 retained journals.
- Recovery never recursively deletes retained transaction directories. Unknown
  `.blend` files, changed data and rollback evidence remain intact. Runtime
  cleanup/reset/shutdown retain sessions with accessory evidence or uncertain
  trees. Seven focused session-retention cases pass, including nested evidence,
  renamed journals, links, malformed journals and scan bounds.
- Blender capture follows only the selected config's resolved dependency graph,
  staged overrides, gender and skin color. Inactive configs and unrelated colors
  no longer exhaust capture budgets. Before/after generation checks still catch
  newly appearing selected overrides. Immutable packets/copies survive decode
  without holding the mutation gate; changed settings/source inputs reject stale
  launches. Twenty-five Python snapshot and ten bridge scenarios pass.
- Player return publication validates and flushes a unique sibling copy before
  atomic replacement. Twenty-two focused cases include real process termination,
  Windows locks, source/target conflicts and preserved backups. This is not a
  filesystem compare-and-swap or a power-loss guarantee.
- Body Fit reloads inherit incomplete raw inputs and the last finite values from
  the correct owner, including first Preview load. Refresh cannot race Apply;
  eleven new focused regressions pass. Existing draft/shared-lock tests remain.
- Runtime preparation journals a fingerprint-bound inventory before extraction.
  Verified interrupted scratch cleanup retries under its preparation lock with
  pinned Windows handles. Unknown content, modified Blender tools, alternate
  streams and unprovable/truncated files are preserved and reported, not adopted
  or removed. Fifteen focused scratch cases pass, including abrupt process exit.
- Real current-source Python services completed a Blender 5.1.2 snapshot round
  trip: nine captured inputs, two reference meshes, fourteen textures, Edit Mode
  auto-send, eye recalculation, changed vertex data and one atomic publication.
  Originals/companions were preserved and the fixture cleaned. This specifically
  tests Python service wiring, not the Rust UI-host lease/native transport.
- Real .39 config baseline independently reproduced two absent distance shapes.
  Existing native-headband membership and shared-stock recolor rejection passed,
  with all original configs and companions preserved. A Fury bone-index difference
  remains an unverified observation, not a diagnosed failure or an automatic fix.

The first real .40 config acceptance exposed a validation regression absent from
the fixtures: stock png06827 hair and beard appearances name optional occlusion
inputs, while their authored texture IFFs contain only color, tangent and lightmap
TXTRs. A fresh private manifest extraction confirmed this inventory. Those absent
optional inputs are now preserved exactly, without fabricated pixels or aliases;
required bindings and any present TXTR pixel data remain validated. A hair/beard
regression covers this layout. The acceptance harness also now canonicalizes
Windows extended-length paths before checking its private-runtime ownership.
Earlier failed reports remain preserved rather than overwritten.

The first .40 build was deliberately withheld by source-input verification after
the independent recovery review landed two final fixes. It was not published as
a checked release. The stable-input rebuild and packaged acceptance are recorded
with the final .40 release evidence.

Final 0.2.40 acceptance:
- All full build gates pass: 110 application Rust plus 20 shared decoder tests,
  38 inherited regressions, 35 accessory/recovery, 25 snapshot, 22 atomic
  publication, 16 distance-selector and eleven new reload cases, alongside the
  existing TypeScript/ownership/worker/render/packaging gates.
- Real config-only hair and beard installs now have valid primary/distance
  references in required regular/tangent variants. Native headband membership,
  shared-slot rejection, textured config switching/rotation, original configs
  and all sixteen original companion hashes pass. No UI mutation was mocked.
- Actual Body/Preview writes read back both saved values, preserving the other
  112 body fields and non-appearance members. Nine cross-tab UI scenarios pass.
  Four live GPU draft comparisons remain zero differing pixels/RMS zero; the
  saved-edit recovery rendering check also passes. Those GPU mutation responses
  are mocked; the separate persistence test performs real writes.
- Fresh 2K23-to-2K27 Full Swap completes with eight archives, zero errors and
  warnings, 25 passed checks and automatic staging. Both textured Blender tests
  pass with eyes recalculated. The current-service snapshot round trip takes
  5.17 seconds and exercises one real atomic publication; Rust host lease/native
  transport is covered separately by focused tests, not that Python test.
- Packaged native first-read recovery restores an interrupted transaction,
  preserves unknown Blender work and blocks a later partial-rollback conflict.
  Stale local resource reads are rejected. Favorites/theme/reset/failure recovery
  pass. The final config and GPU screenshots were reviewed; no owned UI/Blender
  acceptance process remains.
- One fixture copy returned ENOENT before editing began; source versus destination
  was not established. Direct sequential extraction/inspection retained the first
  player, and the instrumented UI rerun passed without a production lifetime
  change. Its failed report remains. The first normal-data GPU check also hit its
  30-second startup timeout during one-time preparation/retirement. A subsequent
  bootstrap completed retirement of one verified unused runtime in 21.003 seconds;
  the next launch took 751 ms with no maintenance/extraction events. Warm rendering
  checks then passed. This is not a claim that cold preparation takes 751 ms.
- Sharing EXE is 52,766,241 bytes, SHA256
  9a890cb3234b2835c527d09dd930a12e952ec5168fa295e769aa801916491237.
  The 4,943-file source fingerprint is
  de9a388bff70005ffa9f84f4ddac9bd833a20246f544927de31515c72214bcfa;
  frontend fingerprint is
  6cf4dbbb16f9d16c2e20659a318b58f78570b3c05ef33c7bebacabd584224c35.
  Payload is 0bba3a56a19943d07d29e814e90a47fb69d02890aa2dafbfc845315345c53c21.
  All five prototype modules, 197 Blender files and 2,262 retained Python-runtime
  files are verified byte-identical to their respective sources.

## Twenty-Fifth Improvement Batch: Startup And Redistribution Evidence

- The .40 maintenance-bearing bootstrap took 21.003 seconds and emitted a
  retirement event, followed by a 751 ms repeat. These observations implicated
  synchronous retirement, not extraction on the already usable engine path.
  A separate owned-cache baseline takes 1,415 ms and confirms old-cache deletion
  finishes before .40 bootstrap returns; its 16,841 ms byte-verified fixture
  seeding time is not included in the startup measurement.
- Usable runtimes now acquire a shared lease, recheck readiness and return without
  waiting for the global preparation lock. One background thread owns an additional
  lease while it takes the preparation lock and performs verified maintenance.
  Cold extraction retains its inventory preflight, all file flushes and durable
  readiness ordering. Duplicate cold decompression is not claimed fixed.
- Settings writes use an OS sidecar lock after engine readiness. Cleanup additionally
  pins the actual settings file against writes/replacement, including older writers
  that do not honor the sidecar. Missing or invalid settings defer maintenance.
  Slow-path scratch cleanup uses the same protection. Twelve focused tests cover
  concurrent cached readers, repair exclusion, independent maintenance lifetime,
  saved-path changes, both cleanup paths and bounded diagnostics.
- Retired-runtime deletion now re-verifies and pins all remaining files/directories,
  including original inventory bytes and ready-marker content, before first deletion.
  Seventeen focused Windows tests cover locks, replacement, ADS, links, changed
  metadata, foreign Blender work, interruptions and retry leases. Pre-rename
  verification and current/prior/active/configured-path protection remain intact.
- Residual filesystem races are explicit: a concurrent change between pre-rename
  verification and rename can move into `.retired-*`, where fresh verification
  prevents its deletion. Windows base-stream pins do not atomically exclude new
  alternate streams; rechecks detect injected ADS but cannot prove exclusion of
  an adversarial last-instant stream write. A very late root entry may leave an
  unverified retained folder after retry metadata has been deleted. None of these
  cases authorizes blanket removal or an absolute concurrency guarantee.
- Independent review caught the slow-path settings gap. A separate proposed swap
  writer finding was withdrawn after inspecting the actual .31 packaged backend:
  the Electron reference had a save absent from this package's route. No unnecessary
  long-running swap settings guard was added based on that incorrect reference.
- The redistribution audit checks 63 PE images/482 DLL references, isolated Python
  imports/codecs, native wheel RECORD hashes and system-only-PATH helper probes.
  It proves retained decoders require external CLR 4 framework assemblies, and a
  BC7 texconv probe loads four VC++ v14 DLLs from this PC's System32. The Rust host
  uses static CRT and is not a managed host. Python's required 33 imports and ten
  native wheel identities pass. Optional CFFI/peutils failures are recorded, not
  mislabeled required-feature failures. A clean Windows VM remains untested.
- Health now reports background maintenance and prerequisite file presence without
  equating presence with activation, an exact Framework product minimum, or clean-PC
  readiness. Sharing documentation links Microsoft's Framework/VC++ guidance.
  No installers, system PATH changes or registry modifications are performed.

- Final .41 acceptance passed all twelve commands: runtime maintenance/retention,
  isolated Python/codecs, runtime corruption recovery, real Full Swap and staging,
  GPU draft preservation, actual Body Fit persistence, config/accessory editing,
  both Blender round trips, UI recovery and redistribution dependency audit.
  All 162 Rust tests and build gates passed. Health badge regression coverage
  adds ten cases; actual Health UI shows Info/Info/Unverified prerequisite badges,
  Info maintenance status, no page errors and no text overflow.
- The final EXE is 52,855,841 bytes with SHA-256
  `19a86a56f85dba5a6c13e97d1d2583dfa5631a12ff5a24d31659c50a478e8911`.
  Source fingerprint (4,951 files):
  `62ced88f6e5c63b7630f2ed63450f91f685d2a25727ec3614038e9a5156f3fcd`;
  frontend: `df707031fde86f5fe6c99527d5e90c6a71fbd580553af31c7ac2678954da25fb`.
  Payload `0bba3a56a19943d07d29e814e90a47fb69d02890aa2dafbfc845315345c53c21`
  is unchanged from .40. The earlier .41 candidate's three small diagnostic JSON
  files are explicitly retained separately from final-candidate evidence.
- Final seeded bootstrap was 1,106 ms; maintenance finished 3,158 ms after return
  (4,180 ms internal duration). Its 30,516 ms fixture seeding is not startup.
  Recovery measured fresh 16,342 ms, cached 861 ms, bridge repair 22,786 ms,
  Python repair 24,282 ms, ready-marker repair 23,919 ms, cached-after-repair
  935 ms. Two active instances still prevent repair. No claim is made about
  every corruption mode or clean-PC startup.
- The launcher and all 197 unchanged original Blender tools were reverified.
  The exact unchanged .38 EXE was removed after absolute path, reparse, size,
  SHA-256, active-script, Desktop-shortcut and process checks: 52,587,818 bytes
  reclaimed. No runtime, Blender tool, export or other app was removed.
- Final publication preserved a concurrent test-only Archive Studio path update
  to `D:/Projects/2k Exporter/nba2k27-exporter-tauri2`. A timing-sensitive lifecycle
  assertion was corrected to wait for the restarted native invocation before
  testing registration cancellation; it passed ten consecutive isolated runs.
  Application cancellation behavior was not changed. All build gates were rerun;
  the rebuilt source and frontend fingerprints match. Earlier .41 candidate
  acceptance measurements remain labeled by their executable hashes.
- Published-candidate acceptance repeated all twelve runs successfully. Final
  Health UI was visually inspected with no page errors or text overflow. Eighteen
  compact JSON reports (668,069 bytes) are preserved in
  `artifacts/release-evidence/0.2.41`, without runtime/DDS/screenshot duplication.
  Final EXE/source/frontend hashes and the Desktop shortcut target were verified;
  no owned test or Blender process remained. Two recent rollbacks (.40/.39) and
  the explicitly retained .5 benchmark remain.

### Archive Studio Renderer Review

Read-only review of the moved active `frontend/public/renderer` found useful
next-increment ports: hue-preserving whole-vector boundary response, explicit
strand UV semantic propagation, cancellation-safe texture batch ownership,
material-assigned tangent lookup and dependency refresh. Armpit material/color
and preview-only small-mip handling require a separate bounded native change.
Do not replace the existing approved lightmap, fiber specular, tangent-range or
multisample coverage handling. Staged-first textures, normal packing,
nearest-boundary matching, camera and Body Fit ownership must be preserved.
Three.js/DDSLoader vendor hashes already match. No donor files were modified
or copied for this .41 release; the renderer ports are not claimed implemented.

## Twenty-Sixth Improvement Batch: Frozen Renderer And Startup Candidate

- All 168 Rust tests and mandatory build gates pass. Fourteen nonempty regular
  bootstrap files and the exact bounded ready marker are checked before cache
  reuse. Missing/empty/link/directory and verified-replacement fixtures pass.
  No whole-cache hashing was added to normal startup.
- Complete RGB boundary response is ported with whole-vector bounds, not guessed
  skin colors. Existing lighting, normal packing and boundary matching are kept.
  Direct GPU readback observes `[1.5,2,4.5]`, rejecting an invalid response to
  `[1,1,1]`; 224,678 colored pixels and 112,338 changed pixels prove a nonblank,
  effective fixture. This is synthetic preview evidence, not an exported player.
- Texture-only updates are private until their final progress callback and
  ownership check. Actual renderer-function tests cover cancellation, exceptions,
  concurrent/latest selection, rollback, append/deduplication and cache lifetime.
  Material-assigned tangent maps cannot outrank a staged map from an earlier
  source priority. Color replacement updates dependency metadata without bytes.
- Five actual hair/beard GPU assets pass. Native semantic metadata confirms UV0
  for the afro and UV1 for the tangent beard; selection is deliberately unchanged.
  The afro and tangent-beard screenshots are SHA-256 identical to .41. The donor
  global UV0 override and separate armpit feature were not copied speculatively.
- Eight real feature-candidate workflows pass: strand rendering, Full Swap and
  staging, main accessory visibility, Body Fit persistence, config/accessory
  editing, direct and managed Blender handoffs, and UI recovery. The final
  published executable repeats strands, Full Swap/staging, Blender and recovery.
- The initial runtime-recovery run passed twelve missing and four empty damage
  cases, preserving settings, exports, user tools and four changed backups. It
  then exited 1 because the assertion accepted only a lease-refusal message and
  Windows returned preparation-lock refusal instead. The actual repair remained
  blocked. Both safe refusals are now accepted, with no-folder-change assertions.
  A separate `--lease-only` run exits 0, proves two active instances block repair,
  and completes final Health checks. The final published EXE subsequently passes
  the uninterrupted corrected combined command with exit 0: twelve missing and
  four empty cases, active-instance protection, four preserved changed backups
  and final Health checks. Its test-owned fixture was cleaned after completion.
- Damage-case timings: fresh 15,976 ms, cached 1,087 ms, missing repairs
  24,060-25,439 ms, empty repairs 18,458-67,004 ms, cached afterward 1,068 ms.
  The slow empty cases retain changed backups safely. Separate guard timings:
  fresh 17,406 ms and cached 956/1,049 ms. These are local observations.
  Final combined timings: fresh 14,373 ms, cached 979 ms, missing repairs
  17,015-26,422 ms, empty repairs 23,727-61,936 ms, cached afterward 997 ms.
- Final EXE SHA-256 is
  `8578796a35646e0f37bff5d9ed1bc2bbc89977571f3d2f64af3585d4e584b9a5`,
  52,859,937 bytes. Source (4,955 files):
  `ee44c587f603234975c7ee97d9a896d7cb7bd5a680677997bac83de9a3ee168a`;
  frontend `a90fc390a5f9ad596bbc4a4c68e3ea153c946cf6d2dd435d9e96601462498ec8`.
  Reconstructing the prior test-script hash proves every other build input is
  identical. Executable code/data/unwind/resources/relocations match the earlier
  candidate; read-only data differs and is explicitly not called byte-identical.
  The runtime payload remains `0bba3a56a19943d07d29e814e90a47fb69d02890aa2dafbfc845315345c53c21`.
- Original 197 Blender files remain exact. Only unchanged .39 was retired after
  absolute path/all-ancestor reparse, size/hash, process, root/dist script,
  Desktop/Start-menu/pinned-taskbar shortcut checks: 52,587,818 bytes reclaimed.
  No runtime folder, user export, tool or other app was removed by release cleanup.
  Scope is frozen; automated verification is complete for this candidate.
  External QA remains separate from further discretionary implementation work.
- Final sharing/source/frontend/launcher verification passes; no owned test or
  Blender process and no recovery fixture remains. Compact JSON reports,
  including the final combined recovery result, are preserved in
  `artifacts/release-evidence/0.2.42`.
  Runtime, image and texture payloads were not duplicated there. Practical release
  readiness is estimated at 95%, not a claim of complete external sign-off.

## Release Assessment

### 0.2.43 Portable Packaging Update

The user authorized an optional portable Blender release, then requested both
Standard and With-Blender downloads. The same sharing EXE is used in both.
Blender 5.1.2 Windows x64 and its matching source archive were downloaded from
the official Blender server; the binary matches its published SHA-256, and the
source matches the published MD5 and a pinned local SHA-256. Original notices
and every one of the 6,368 portable files are preserved, not copied from the
developer's installed Blender or mixed with personal preferences/addons.

The resolver prefers the adjacent Blender for new/automatic setups, preserves
valid explicit settings and saves bundled selection as automatic instead of an
absolute path. Relocation and switching between Standard/bundled paths are
covered. The new mandatory resolver module joins bootstrap readiness checks.
No conversion math, renderer behavior or original Blender extension was changed.

Eight new Python path cases, the existing runtime-path test, 168 Rust tests and
mandatory release gates pass. Exact archive CRC/EXE/official-file comparisons,
fresh detection, save/relocation, custom override/reset, a real saved 2K23 png4109
to freshly extracted 2K27 png06827 swap and a portable 5.1.2 Edit Mode export
round trip pass. The Standard package keeps installed discovery. Test-owned
fixtures are removed, and outputs/settings/tools outside them are preserved.

Two initial harness issues were corrected: deeply nested test extraction needed
Windows extended paths, and explicit Blender selection needed a fixture named
blender.exe rather than the app EXE. These were test issues, not conversion
failures. The release README recommends a short extraction location for ordinary
Windows archive tools. The local Build Tools SDK discovery failure was addressed
by selecting an already-installed complete SDK in the build process only; no
system registry, SDK installation or user runtime requirement was changed.

0.2.43 supersedes the frozen .42 candidate only for this requested setup work.
The previously deferred external QA remains unverified, and the release stays
beta with an estimated practical readiness of 95%. No further discretionary
feature audit is part of this update. Earlier release records below are historical.

### Frozen .42 Candidate Scope

The user requested moving toward wrap-up. No new discretionary feature work is
planned before release sign-off. The .42 candidate contains the verified complete
skin-response vector port, transactional GPU texture refresh, staged-priority
explicit tangent binding, byte-free color dependency refresh, native UV-semantic
diagnostics and fourteen-file startup readiness/repair checks. It deliberately
preserves proven hair/beard UV selection, lighting, normal packing and coverage.
The uncertain donor-wide UV0 override and separate armpit feature were not ported.
Both worker agents are closed; parent owns the frozen sources and verification.

0.2.42 is the frozen release candidate and includes the earlier improvements.
The interrupted real-asset checks passed after Steam completed its update.
0.2.41 and 0.2.40 remain verified rollback builds; 0.2.5 is retained as an explicit
benchmark baseline. Superseded 0.2.26 and 0.2.29 EXEs were removed after exact
path/size/reference/process checks, reclaiming 104,939,108 bytes. No other app,
runtime, Blender tool, export or report was removed. The Desktop launcher target
and final sharing EXE hash were verified. This is not a fully signed-off stable
WPF replacement. The superseded 0.2.27 EXE was removed after exact path, size,
reference, link and process checks, reclaiming another 52,491,314 bytes. No
runtime/tool/export was removed. The 0.2.31 sharing hash and launcher were verified.
The superseded 0.2.28 EXE was removed after exact path, size, reference, link and
process checks, reclaiming another 52,491,314 bytes. The 0.2.32 sharing hash and
launcher were verified. No runtime/tool/export was removed.
For 0.2.33, the exact superseded 0.2.30 EXE was removed after path, link, size,
reference and process checks, reclaiming 52,501,042 bytes. The sharing hash,
Desktop launcher target and 4,910-file source fingerprint were verified after
acceptance. No owned test process remained. No runtime/tool/export or other app
was removed; 0.2.32/0.2.31 remain rollbacks and 0.2.5 remains the benchmark.
For 0.2.34, 15 small build/baseline/acceptance JSON files were copied into
artifacts/release-evidence/0.2.34 (245,250 bytes) so subsequent tests do not replace
this release's evidence. No DDS payloads, runtime copies or screenshots were
duplicated. The exact superseded 0.2.31 EXE was removed after path, link, size,
reference and process checks, reclaiming 52,501,554 bytes. The sharing hash,
launcher target and 4,912-file source fingerprint were verified. No owned test
process remained; no runtime/tool/export or other app was removed.
For 0.2.35, 18 compact build/baseline/acceptance JSON files were preserved in
artifacts/release-evidence/0.2.35 (271,291 bytes), without duplicating runtime,
DDS payloads or screenshots. The exact superseded 0.2.32 EXE was removed after
path/link/size/hash/reference/Desktop-shortcut/process checks, reclaiming
52,502,066 bytes. Sharing hash, launcher and 4,914-file source fingerprint were
verified; no owned test process remained. No runtime/tool/export or other app
was removed. 0.2.34/0.2.33 remain rollbacks; 0.2.5 remains the benchmark.
For 0.2.36, 22 compact evidence/build/acceptance JSON files were preserved under
artifacts/release-evidence/0.2.36 (250,385 bytes). Only the unchanged superseded
0.2.33 EXE was removed after exact path/link/size/hash/reference/Desktop-shortcut/
process checks, reclaiming 52,539,442 bytes. Sharing EXE hash, Desktop launcher,
source/frontend fingerprints and absence of owned test processes were verified.
No runtime folder, Blender tool, export or other app was removed by release
cleanup; the isolated recovery test removed only its own temporary fixture.
For 0.2.37, 24 compact build/baseline/acceptance JSON files were preserved under
artifacts/release-evidence/0.2.37 (254,841 bytes), without copying image, texture
or runtime payloads. The exact unchanged 0.2.34 EXE was removed after path/link/
size/hash/reference/Desktop-shortcut/process checks, reclaiming 52,532,786 bytes.
The 0.2.37 sharing hash, Desktop launcher, source/frontend fingerprints and absence
of owned acceptance processes were verified. No runtime folder, Blender tool,
user export or other app was removed by release cleanup. 0.2.36/0.2.35 remain
rollbacks and 0.2.5 remains the explicit benchmark.
For 0.2.38, 17 compact build/test/verification JSON files were preserved under
artifacts/release-evidence/0.2.38 (158,177 bytes), without duplicating runtime,
image or texture payloads. The exact unchanged superseded 0.2.35 EXE was removed
after absolute path/reparse/size/hash/reference/Desktop-shortcut/process checks,
reclaiming 52,532,786 bytes. 0.2.37/0.2.36 remain verified rollbacks; 0.2.5 remains
the benchmark. Sharing EXE, launcher, source/frontend fingerprints and absence of
owned acceptance processes were verified. No runtime folder, Blender tool, user
export or other app was removed. An initial ad-hoc read-only fingerprint command
omitted the runtime-policy helper and failed; loading both required helpers gave
the expected fingerprint. The packaged release tests were unaffected.
For 0.2.39, 18 compact build/baseline/test/verification JSON files were preserved
under artifacts/release-evidence/0.2.39 (32,335 bytes), without duplicating runtime,
image or texture payloads. Only the exact unchanged superseded 0.2.36 EXE was
removed after absolute path/reparse/size/hash/reference/Desktop-shortcut/process
checks, reclaiming 52,585,982 bytes. 0.2.38/0.2.37 remain verified rollbacks and
0.2.5 remains the benchmark. Sharing EXE, launcher and source/frontend fingerprints
were verified; no owned UI/Blender acceptance process remained. No runtime folder,
Blender tool, user export or other app was removed.
The supplemental real Body/Preview persistence check used actual file writes on
a disposable player copy: both Apply paths read back correctly, all 112 untouched
body fields and all non-appearance archive members were preserved, Preview Apply
minimized, and a subsequent textured refresh retained the saved value. Original
source files and copied accessory packages were unchanged.
Feature coverage, output comparisons, appearance and Blender integration are
well exercised. Clean-PC redistribution, latest native picker interaction,
fresh 2K25 source coverage and live game/roster acceptance remain external
release gates. Keep WPF available as a fallback during broader beta use.

For 0.2.40, 27 compact build/test/diagnostic/verification JSON files are preserved
under artifacts/release-evidence/0.2.40 (226,798 bytes), without duplicating any
runtime, image or texture payload. The exact unchanged 0.2.37 executable was
removed after absolute path, ancestor/reparse, size, hash, process, active-script
and Desktop-shortcut checks, reclaiming 52,586,494 bytes. 0.2.39/0.2.38 remain
rollbacks and 0.2.5 remains the benchmark. The release helper removed no runtime,
Blender tool, user export or other app. The app's own verified retention had
separately removed one unused runtime during the measured maintenance launch;
unknown/modified legacy caches remain protected. Sharing hash, launcher and
source/frontend fingerprints were verified after acceptance. Eleven required
acceptance workflows passed, with mocked versus real mutation scopes recorded.
This release is estimated at 94% toward practical release readiness, not exact
parity or a claim that the remaining external gates have passed.

## External QA

Fresh 2K25 source coverage, in-game animation/LOD/physics/config checks, live
roster access, outstanding native picker checks and clean-machine redistribution
remain quality checks with missing external prerequisites. These are bounded
release sign-off items, not reasons to preserve the old architecture or demand
exact parity. Per the user's wrap-up direction, discretionary optimization is
deferred once the current renderer/recovery candidate is verified.
The user explicitly deferred the manual game/roster/native-picker checks for
later. The final combined automated recovery check is now complete; no further
feature work or release rebuild is needed for this verification-only update.
The earlier one-off catalog worker exit remains unproven; bounded failure
diagnostics are retained, not replaced with a blind conversion retry.
