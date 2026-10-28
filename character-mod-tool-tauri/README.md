# Character Mod Tool: Rust + Tauri 2 + TypeScript

This is a separate migration preview, not a replacement for the production app.
Production files and the earlier Rust prototype are left intact.

The current objective is equivalent features, appearance and output quality,
not strict 1:1 implementation parity. Previously copied behavior is eligible for
improvement when evidence shows a faster, simpler or more reliable approach.
See IMPROVEMENT-AUDIT.md for review coverage and measured changes;
PARITY-AUDIT.md is retained as historical regression evidence.

## 0.2.56 Beta: NBA 2K18 to NBA 2K23 Player Route

- Adds NBA 2K23 as a target for NBA 2K18 Full Swap sources while leaving the existing 2K26/2K27 routes unchanged.
- Writes the transferred player directly into 2K23 packed model buffers without changing target topology, indices, weights, morph payloads or materials.
- Propagates the transferred shape through all five lower body/head LODs so the target player does not return at gameplay distance.
- Rebuilds the native 2K23 `face####.iff` texture companion from the 2K18 face color/normal while retaining target-only pore and wrinkle maps.
- Preserves the 2K23 target hair, facial hair, headbands and other companions in this first route; source accessory conversion remains deliberately disabled pending separate validation.

## 0.2.55 Beta: Native Jersey Body-Fit Morphs

- Decodes the 2K27 static jersey and shorts `THIN`/`THICK` morph buffers and applies their authored per-vertex displacement in the preview.
- Preserves independent absolute weights for `THIN` and `THICK`, including negative values and values above `1`, instead of collapsing them into a hand-tuned `THICK - THIN` estimate.
- Keeps surface binding for body-dimension changes and retains the regional response only as a fallback for garments without native fit morphs.

## 0.2.54 Beta: In-Game-Oriented Jersey Fit

- Separates player dimensions from clothing fit in the live preview: skeleton translation fields deform the body, while regional `THIN`/`THICK` values alter jersey clearance around it.
- Keeps the static jersey surface-bound to body dimension changes before applying its regional fit offset, so body proportions and clothing fit no longer collapse into one deformation.
- Adds regression coverage proving `THICK` changes the jersey without changing body geometry.

## 0.2.53 Beta: Body-Fitted Jersey Preview

The static jersey now binds to the staged player's underlying body surface and follows
the full thick/thin and XYZ body-dimension displacement. This replaces the previous
fixed 45% jersey response, preserves the garment's original local clearance and leaves
conversion-specific protrusions visible for clipping inspection. Renderer diagnostics
report binding coverage and clearance drift. Close Seams is now a text-only control
using the same sizing as the neighboring preview-row buttons.

## 0.2.52 Beta: Safer Seam Closing

Close Seams now preserves the target arms/chest border around the traps and moves
only the converted face/neck edge to it. The automatic matcher uses a narrower
distance limit and rejects triangle inversion, excessive area distortion and edge
stretch before publishing. A native-scale UV-split residual is reported without
pushing the target body into the jersey. The preview toolbar uses an inward-fold icon
instead of the visually confusing combine symbol. Every applied repair still
creates a separate seam backup before replacing the editable player.

## 0.2.51 Beta: Direct Player Editing

One Player tab contains Select Player and Convert Player choices centered in the workspace header,
separated by an OR divider. Returning to Player remembers the chosen workflow.
Stage Player is also available from the main toolbar
and the empty Preview screen.
Choose a local NBA 2K27 player IFF/ZIP/RAR or a player from the NBA 2K27 manifest.
The app creates a separate editable package under the configured output folder's
Edited Players directory, then opens it in Preview. Body, Configs, Hair, Headbands
and the Blender round trip use that copy without a Full Swap source/target pair.
Original files are not edited. Older-generation players still require Full Swap.
Unsaved body/config edits and active Blender sessions block replacing the stage.
New sessions open in Select Player; the conversion workflow remains in Full Swap.

## Architecture

- Tauri 2 owns the window, packaged frontend, IPC and event delivery.
- Rust owns dialogs, file reads, runtime preparation, worker lifetimes and cancellation.
- The application screens and native bridge compile from TypeScript source in web/.
- Strict TypeScript checks cover IPC responses, events, preview data and the bridge.
- The inherited UI uses incremental type checking and generated HTML element types;
  its compiler currently permits implicit parameter types during the migration.
- The original Python engine and bundled Blender extensions perform conversions.
- The production UI, Explorer renderer and preview decoder are reused.
- The existing Explorer renderer and decoder remain JavaScript compatibility modules;
  Three.js and the bundled icon library are not rewritten or removed.
- The 3D preview draws with the 2K Exporter's shared renderer, vendored in
  web/renderer/shared with shared-renderer.lock.json. build-frontend.cjs refreshes it
  from 2k Exporter/shared/model-preview when that folder exists (shared-renderer.mjs;
  NBA2K_SHARED_RENDERER overrides the path) and otherwise checks the copy against its
  lock. Edit the shared renderer there, not here: web/renderer/model-preview.js
  (body fit, editor display, portrait mode) wraps it through createModelPreview.
- Preview decoding runs in browser WebWorkers, not a Node process.
- The worker entry and result transport are owned modules; completed geometry
  buffers transfer to the UI while retained donor heads remain isolated.
- Native Rust preview extraction shares the Archive Studio asset library in
  ../shared/nba2k-assets. It retains the manifest index and reads geometry,
  TXTR metadata, shared buffers and DDS textures directly in memory.
- The native preview caches decoded archives within a 192 MiB payload budget;
  edits invalidate the cache. Unchanged previews reuse geometry and GPU textures.
- Unsupported native formats fall back to the original reader, including the
  Blender reference handoff. Conversion/export math is not replaced.
- Existing managed format-decoder helpers remain bundled. There is no .NET window host.

## Restored Workflows

Full Swap, Preview, Configs, Body, Hair, Headbands, CLI and Health use the production
interface. Manifest selection, templates, source hair/beard conversion, staging,
body-fit editing, config-specific accessories and native asset installation call
the original engine. Preview texture resolution prefers staged files before
manifest fallback, including body regions. Static uniform references, eye/lash
dependencies, the body-fit preview editor and the existing Blender handoff are
connected through the replacement bridge.

The Blender scripts themselves are unchanged. Automated checks pass for a real
2K23-to-2K27 facial-hair full swap, native textured rendering and rotation, both
catalogs, body-fit/config reads, static jersey chest correction, and a textured
Blender 5.1 round trip from Edit Mode with eyes recalculated. Cached UI startup
measured 993 ms on the development PC. Fresh 2K18, 2K19, 2K23 and browsed 2K26
conversions pass with zero validation errors. A complete saved 2K20 source package
also passes. 2K25 passed earlier audits but its manifest is currently unavailable
for a fresh check. Live roster writes and in-game acceptance still need testing.

## 0.2.43 Beta: Optional Portable Blender Download

Two ZIP downloads contain the same app EXE: Standard (about 46 MB) and
With-Blender-5.1.2 (about 566 MB). The latter includes the unmodified official
Windows x64 portable distribution, its original notices and matching source
archive. Extract the full ZIP into a reasonably short location before running.
The standalone sharing EXE remains available for existing installations.

New setups detect `Blender/blender.exe` beside the app. Valid explicitly saved
Blender settings remain preferred. Selecting the bundled copy stores automatic
selection rather than its absolute path, so moving the release folder continues
to work. Clearing the Blender setting also restores automatic selection.
The Standard download keeps installed-Blender discovery. No Blender installation
is changed and the portable distribution is not copied into runtime caches.

Eight new path regressions and all 168 Rust tests pass, alongside the mandatory
release gates. Exact ZIP checks verify the EXE is identical in both packages and
all 6,368 Blender files match the checksum-verified official distribution.
The portable integration test covers first use, settings save, relocation,
explicit override/reset, a real saved 2K23 png4109 to manifest 2K27 png06827 Full
Swap with zero validation errors, and an Edit Mode Blender export round trip.
The return IFF passes archive CRC checks. All 197 original Blender tool files
remain unchanged. Test inputs and user settings are kept separate.

This is a packaging/setup update, not a change to conversion or renderer math.
Previously deferred game/roster/picker, fresh 2K25 and clean-PC checks remain
unverified. Practical release readiness remains estimated at 95%.

## 0.2.42 Release Candidate: Final Renderer And Recovery Pass

Scope is frozen for release verification, not exact implementation parity.
This pass ports the verified Archive Studio skin-boundary response without
changing the existing lights, normal packing or nearest-boundary sampling.
RGB gains are accepted as a complete bounded vector or rejected; channels are
not independently clipped into a different hue. No skin color is hard-coded.

Texture-only refreshes now prepare GPU textures privately, then commit only
while the same model and request still own the renderer. Cancellation, upload
failure, final-progress callbacks and newer selections cannot publish a partial
batch. Staged tangent maps outrank manifest fallbacks; explicit material bindings
win within the same source priority. Color replacements refresh byte-free
dependency metadata. Approved hair lightmap, fiber specular and coverage remain.

Native geometry reports its selected and alternate UV semantics for diagnostics.
It does not force UV0 over currently working hair/beard UVs. The broader donor UV
override and armpit enhancement are not part of this release candidate.

Cached startup checks fourteen mandatory bootstrap files for missing/empty,
directory or reparse-link problems and checks the exact 64-byte readiness marker.
Failures use the existing verified repair path; normal startup does not hash the
entire cache. Nonempty altered files still require explicit integrity checking.
The conversion/Blender engine payload is unchanged from .41.

Remaining release sign-off is bounded: native file/folder picker interaction,
fresh 2K25 source coverage, in-game blink/LOD/physics/config and live roster tests,
and a clean Windows machine with documented prerequisites. Further discretionary
feature work is deferred while these checks are completed.

Verification: all 168 Rust tests and mandatory build gates pass. The .42 feature
candidate passed real Full Swap/staging, five strand GPU assets, main Preview,
Body Fit persistence, config/accessory editing, both Blender handoffs and UI
recovery. The published test-only rebuild repeats strand, Full Swap, Blender and
UI recovery checks. All 4,954 other build inputs and the compiled frontend are
unchanged. Key tangent screenshots match the .41 baseline exactly. A direct GPU
fixture verifies unclipped RGB response `[1.5, 2, 4.5]` and whole-vector rejection.

All sixteen missing/empty-file repair cases passed with user data preserved.
The initial combined recovery run then stopped at an over-specific lock-message
assertion: preparation-lock refusal was also safe. After correcting that test,
the final published EXE passes the uninterrupted combined recovery run: all
twelve missing-file and four empty-file repairs, active-instance protection and
final Health checks. Cached startup measured 979/997 ms in this isolated run.
Only the test-owned fixture was removed. Manual game, roster and native-picker
checks remain deferred at the user's request; they are not reported as passed.

Sharing build: `dist/Character-Mod-Tool-Tauri-Preview-0.2.42.exe` (52,859,937 bytes),
SHA-256 `8578796a35646e0f37bff5d9ed1bc2bbc89977571f3d2f64af3585d4e584b9a5`.
Desktop launcher: `Character Mod Tool Tauri Preview.lnk`.
All 197 Blender tools are unchanged. Rollbacks .41/.40 and the .5 benchmark remain;
only the exact unused .39 EXE was retired (52,587,818 bytes).

## 0.2.41 Beta: Nonblocking Runtime Maintenance

A usable prepared engine is leased and reused without waiting for old-cache
retirement. Verified retention runs in a background thread with its own current
runtime lease and the preparation lock. Concurrent cached launches remain usable;
repair still requires exclusive access. Initial extraction keeps its durable
journal, full verification and file flushes.

Settings saves coordinate with maintenance, and the settings file stays protected
against replacement while configured paths are checked. Post-retirement deletion
pins and verifies all remaining entries before removing any handle. Unknown
files, alternate streams and modified Blender tools are preserved on detection.
Health exposes maintenance state and explicit sharing-prerequisite diagnostics.
Presence checks are not represented as loader or clean-PC verification.

Final .41 verification: all 162 Rust tests, build gates, twelve acceptance runs
and the real Health-screen layout check passed. Cached bootstrap measured
861-935 ms. In the seeded maintenance test, bootstrap returned in 1,106 ms while
cleanup finished 3,158 ms later; fixture seeding is not startup time. A fresh
runtime took 16.342 seconds and corruption repairs took 22.786-24.282 seconds.
These are local measurements, not clean-PC guarantees. All 197 original Blender
tool files remain byte-identical. The payload is unchanged from .40, so that
prepared engine is reused.

Sharing build: `dist/Character-Mod-Tool-Tauri-Preview-0.2.41.exe` (52,855,841 bytes),
SHA-256 `19a86a56f85dba5a6c13e97d1d2583dfa5631a12ff5a24d31659c50a478e8911`.
The Desktop `Character Mod Tool Tauri Preview.lnk` points to this build.
Rollbacks .40 and .39 remain alongside the .5 benchmark. The unchanged, unused
.38 EXE alone was removed (52,587,818 bytes); no runtime, export or tool was
removed by release retention. Compact evidence is in
`artifacts/release-evidence/0.2.41`.

## 0.2.40 Beta: Safer Editing And Handoffs

Native/config hair and facial-hair installs prepare and validate the complete
package before publishing. Regular, tangent-only and paired slots retain the
correct primary mesh, highest-detail distance reference and material bindings.
Publication is journaled with the appearance file last. Interrupted work is
recovered before staged-player reads, including the native renderer. Conflicts
block the read instead of overwriting newer work. Recovery evidence and unknown
Blender files are preserved through reset, shutdown and dead-session cleanup.

Blender opens an immutable capture of the selected config and its resolved
dependencies. Decode and Blender supervision do not hold the player-edit gate.
Changed source inputs/settings reject a stale handoff. Inactive configs and
unrelated skin-color files do not consume capture limits. Returned player files
use validated, flushed sibling copies and atomic replacement.

Body Fit now preserves invalid inputs and the last finite draft across the first
Preview load and reloads. Refresh cannot race a pending Apply. Runtime startup
can safely retry verified interrupted extraction cleanup; changed tools and
unknown contents remain untouched. The original Blender extensions and fitting
math are not replaced.

Verification includes 130 Rust tests, 35 accessory/recovery cases, real config-only
hair/beard installation, headband membership, actual Body/Preview save/readback,
pixel-identical draft render comparisons, fresh Full Swap with auto-staging, and
textured Blender 5.1 round trips with eye recalculation. All 197 Blender tool
files remain identical to the reference. A repeat prepared-runtime bootstrap
measured 751 ms on this PC; first-time engine preparation/retirement is separate.
The launcher targets 0.2.40. Versions 0.2.39/0.2.38 remain rollbacks and 0.2.5 is
retained as the benchmark. In-game and clean-PC acceptance are still outstanding.

## 0.2.39 Beta: Body-Fit Editing Safety

Body and Preview now share a target-scoped busy state during body-fit reads,
saves and chest operations. The other view cannot admit a competing Apply or
queued input while those values are being persisted. Failed operations unlock
both views and retain the draft; unrelated-player work is not locked.

Entering Body preserves unsaved Preview values and incomplete numeric inputs,
including the last finite visual value. Incomplete Preview inputs also block
Body Apply. Explicit Refresh still discards drafts intentionally. Preview Apply
continues to minimize the panel after a successful save.

The real-window test reproduced six failures in nine cases on 0.2.38; all nine
pass on 0.2.39. All 96 Rust tests, 38 inherited conversion regressions, 29 edit
ownership cases, 18 draft validation and eleven draft commit cases pass. Seven
packaged checks cover cross-tab editing, player-edit UI ownership, live GPU
drafts, saved-edit recovery, Full Swap/auto-stage, textured Blender return and
UI recovery. Four GPU draft comparisons remain pixel-identical to independent
references. Conversion/deformation/shader code and Blender tools are unchanged.
The prepared 0.2.38 runtime is reused; no new runtime extraction is needed for
this UI update. The launcher targets 0.2.39; 0.2.38/0.2.37 remain rollbacks.

## 0.2.38 Beta: Accessory Distance References

Full Swap now aligns hair and facial-hair distance selectors with their verified
retained highest-detail meshes. The representative 2K23-to-2K27 export previously
referenced two absent low-detail shapes despite repeating its full geometry at
every distance. Its fresh export now has zero validation errors and warnings,
and automatically stages after completion.

This runs only inside the unpublished NBA 2K27 swap transaction, after companion
conversion/renaming and before final validation/publication. Required regular and
tangent-space variants must contain the primary mesh and pass buffer/index/LOD
validation. Unverified or missing variants do not trigger a guessed correction.
Only existing `meshlo` fields change; geometry, textures, physics, configurations,
appearance RDAT and the original Blender tools are preserved.

All 96 Rust tests, 38 inherited conversion regressions, 16 focused distance tests
and seven packaged workflow checks pass. Those checks cover a fresh Full Swap,
textured Blender return with eye recalculation, accessory visibility, staged DDS/
TXTR priority, body-fit GPU drafts, favorites/theme/reset recovery and Blender
return preservation. The unchanged frontend retains its approved rendering.
This engine update prepares one new managed runtime, then reuses it. The launcher
targets 0.2.38; 0.2.37/0.2.36 remain rollbacks. In-game distance/animation and the
other external release checks remain outstanding; this is still a beta.

## 0.2.37 Beta: Body-Fit Draft Rendering

Unsaved body dimensions no longer become the renderer's baseline when refreshing
the preview. Geometry initializes from staged values, then receives the latest
valid same-player draft. Edits made while geometry is preparing survive the
rebuild, including edits made from Body before the first preview has loaded.

Incomplete inputs now stay synchronized between Body and Preview, block Apply
consistently and keep the last finite visual value. Revert also clears invalid-only
edits in both views. Unedited loads do not add another deformation pass.

The real-renderer test reproduces the old visible mismatch and checks four cases
against independently applied references; 0.2.37 has zero differing pixels in all
four, preserves staged metadata and makes no player writes. All 96 Rust tests,
18 validation cases, eleven commit cases and 16 packaged checks pass. Conversion,
deformation and shader math, Blender tools and runtime payload are unchanged.
The existing 0.2.36 runtime is reused. The launcher targets 0.2.37; 0.2.36/0.2.35
remain rollbacks. This is still a beta with external release checks outstanding.

## 0.2.36 Beta: Blender Handoff Safety

Blender preparation waits for existing edits to the same staged player, then
temporarily rejects new edits to that player until Blender owns its input copy.
Other-player edits remain available. Changing game settings cancels obsolete
preparation instead of launching a bundle made from the previous settings.

If a Blender return cannot replace the staged player, its IFF is preserved under
the app data folder's `Blender Recovery` directory. Health offers Open Folder.
These files are user work, not disposable cache; modified recovery files are not
overwritten or automatically removed. If recovery storage is unavailable, the
original session export stays in place. This protects returned IFFs, not arbitrary
unsaved Blender scenes. Existing target-digest conflict checks remain in force.

Preview tab initialization now respects the latest view selection and no longer
leaves a cached render displaying the renderer-loading message.

This build updates the small Python supervision service, so its runtime payload
needs one preparation on first use. All 197 Blender extension files and 2,262
retained Python-runtime files remain unchanged. Conversion and shader math are
unchanged. All release gates and 15 packaged acceptance checks pass, including
real Full Swap/auto-stage, textured Blender return, preserved hair/beard rendering,
cache ownership and recovery. The updated manifest's complete 5,399 hair and
1,093 facial-hair rows match the compatibility reader. The Desktop launcher
targets 0.2.36; 0.2.35/0.2.34 remain rollbacks. In-game and clean-PC acceptance
remain outstanding, so this is still a beta. Details are in IMPROVEMENT-AUDIT.md.

## 0.2.35 Beta: Renderer Commit Ownership

Successful edits now cancel obsolete in-progress renderer work as well as data
requests. Drafts have an explicit player owner, so cancelling a draw does not
discard valid/incomplete body-fit input or carry it into a different player.
An interrupted visible preview reloads automatically unless a newer stage,
request, tab change, active write or Reset has superseded it. Refresh is disabled
during preview writes. Normal Apply still minimizes the body-fit panel.

Renderer reuse now requires a completed payload. Pending loads cannot advertise
themselves as reusable geometry; current failures/cancellation clear partial
state, while older completions cannot overwrite the latest load. Existing
material, lighting, camera and deformation behavior is retained on success.

A real renderer-yield probe reproduced 0.2.34 marking an old 0-valued baseline
current after a mocked save of 0.25. The final build cancels that geometry, retains
0.25, retries successfully, and automatically recovers a visible second edit to
0.5. These writes are mocked; geometry/material preparation is real and the IFF
is hash-checked unchanged. The real private-file edit/cache test also still passes.

All release gates, 18 draft cases, real Full Swap/auto-stage, Blender return,
staged texture/GPU reuse, catalogs, strand visuals and recovery checks pass.
Runtime/Blender tools and conversion/shader math are unchanged. The launcher
targets 0.2.35; 0.2.34/0.2.33 remain rollbacks. This remains a beta; outstanding
UI transaction windows and external QA are tracked in IMPROVEMENT-AUDIT.md.

## 0.2.34 Beta: Edit-Aware Preview Caches

Successful local player edits no longer erase unrelated native catalog handles,
manifest archive packets, shared texture blobs or cached donor heads. Local
handles and packets are still retired, including anonymous local reads. Old mixed
leases cannot revoke a newly registered local handle when they release later.
Reset/settings/game-source changes retain their full invalidation behavior.

The bridge invalidates cached and pending staged data for the edited player,
including matching Blender returns and overwritten Full Swap outputs. Reads
requested during that player's write wait for it to settle. Other players' cache
entries and rejected-write caches remain usable. Sixteen focused cases cover
these boundaries, cleanup-time edits and identical cross-player fingerprints.

The released 0.2.33 regression was reproduced with a private body-fit save:
18 registrations and the donor head were lost, forcing hair/beard to fallback.
On 0.2.34 only the local registration expires (18 -> 17); both catalog selections
stay native, reuse the donor and match exact geometry/texture hashes. The edited
value reloads correctly, Reset releases all owners, and the original is unchanged.

All 86 Rust tests, inherited gates, real Full Swap/auto-stage, Blender return,
staged texture precedence, catalog/rendering and UI recovery checks pass. Runtime,
Blender tools, conversion math and shaders are unchanged. No disk cache was added.
The launcher targets 0.2.34; 0.2.33/0.2.32 remain rollbacks. Wider UI/GPU mutation
coordination and the external QA gates below remain; this is still a beta.

## 0.2.33 Beta: Binary Catalog Geometry

Hair/beard catalog geometry now uses bounded binary IPC instead of base64 JSON.
Typed arrays share one owned asset buffer and a separate donor-head buffer;
keeping a head cannot retain an unrelated hair packet. Cached native heads store
raw array bytes rather than base64. Older hosts retain the JSON path, and native
failures keep their diagnostics while using the original decoder.

Four exact-output cases match 0.2.32 bit-for-bit for every mesh array, UV, source
ID, group and texture hash. Thirteen broader native/original-reader comparisons,
three injected fallback cases, cache/burst/cancellation checks and visible strand
rendering pass. Real Full Swap/auto-stage, editable preview controls, staged DDS
priority/GPU reuse, textured Blender return with eye recalculation and recovery
also pass on the final sharing EXE. All 85 Rust tests and inherited gates pass.

The two sampled donor heads retain 1,912,846 bytes versus 2,186,490 previously,
about 12.5% less. Timing is not uniformly faster: final back-to-back warm medians
improved for dreads/afro but regressed for the small beard samples. Same-build
binary/JSON comparisons favor binary for head-inclusive previews; small borrowed-
head previews are essentially unchanged. Reports preserve both earlier samples
and the final comparison. Do not infer a universal load-time improvement.

No shader/fitting/export algorithms or runtime payload changed; all 197 Blender
files remain identical. No disk cache was added. The launcher targets 0.2.33;
0.2.32/0.2.31 are rollbacks. This remains a beta with the external QA gates below.

## 0.2.32 Beta: Editable and Validated Body-Fit Preview

Body Fit inputs now follow current loading/busy state rather than retaining the
disabled flag from when they were constructed. The real preview's controls are
editable after loading and recover after chest/Reset operations without rebuilding
the whole panel just to change editability.

Reloads merge only known, numeric, finite game-float drafts. Unfinished text,
nonfinite/overflow values and obsolete fields cannot enter renderer input.
Empty dimension inputs no longer become zero: the last valid preview stays,
the field is marked invalid, and Apply/Calculate wait for correction or Revert.
Valid negative and over-one values remain supported. Player-switch/Reset clears
old incomplete fields; ordinary same-player reload retains current incomplete
input without passing it to geometry. No export/conversion/Blender math changed.

Fifteen focused draft/lifecycle cases and all build gates pass. Real UI input
checks verify enabled controls, finite unchanged geometry during blank input,
correction, Revert and an unchanged player-file hash. Eleven isolated packaged
edit workflows pass, including bounded cleanup after transient Windows locks.
Stock and converted hair/beard visibility is measured across three angles, with
color/lightmap/tangent bindings inspected. Full Swap, automatic staging, staged
textures, Blender return/eye recalculation, five strand controls and recovery/
transport checks pass. All 197 Blender files and the runtime payload are unchanged.
The launcher targets 0.2.32; external beta QA below still remains.

## 0.2.31 Beta: Blender Session Ownership and Reset Recovery

The preview UI now tracks live Blender handoffs per normalized player path,
matching the backend. Switching to another player clears only the visible busy
state; returning restores that player's session/status. Late opening results,
errors and old-session events cannot replace a newer handoff. Terminal events
retire their own entry, including while their player is not selected. New Swap
does not forget backend-owned live Blender sessions.

New Swap now waits for pending body-fit, chest, headband, hair and config writes
even if preview cleanup fails. Failed cleanup retains the player, rereads its
data, reconciles already-saved values and keeps unapplied numeric/invalid-text
body-fit drafts. Controls recover even if the reread also fails; retry remains
available. New edits and staging are blocked during cleanup. Conversion math,
Blender scripts and runtime payload are unchanged.

All release gates and 83 Rust tests pass, including 12 staging/recovery, 26
player-edit and ten Blender UI cases. Ten isolated packaged control workflows
pass with delayed/mock responses and zero JS errors. Real 2K23 Full Swap and
auto-staging, textured Blender return with eyes recalculated, staged DDS priority,
cache/GPU reuse, catalog burst/cancellation, five strand rendering controls,
recovery and exact worker-output checks pass. The launcher targets 0.2.31.
This is a checked beta, not a stable WPF replacement; external QA below remains.

## 0.2.30 Beta: Source-Aware Previews and Safe Player Edits

This candidate includes the unreleased 0.2.29 cache/build improvements below.
Native preview resources now have explicit owners. Staged/Blender preparation
releases them after decoding; the catalog retains a 48-entry descriptor LRU and
pins active readers until they settle. Failed preparations roll back partial
registrations. Reset IDs are not reused, and in-memory cleanup does not take the
player-file mutation lock. Archive/head caches keep their existing byte limits.

Catalog revisions and native cache identity now also include game EXE/Oodle
metadata. Missing/moved installs retire the old native cache. Preparation-stage
fallbacks retain their native reason in bounded preview warnings; if both readers
fail, both reasons are reported. Mock lifecycle tests have completion watchdogs.

Staging and config edits now retain their player/request ownership through
inspection, reads, writes, reset and dialogs. Body-fit, chest correction and
headband completions cannot change another player's state or controls. Pending
reads do not supersede these active writes. Failed player selections preserve
valid edits on the current player; actual staging/reset retires them.

All 83 Rust tests and release gates pass, including nine staging, ten config and
21 player-edit cases. An isolated packaged registration test covers 600 distinct
hair/beard selections with three resources at a time and zero retained after
cleanup; it does not decode geometry. Packaged WebView checks cover eight edit
workflows with controlled delayed responses and zero JavaScript errors.

After Steam completed build 25761995, real-asset checks passed: all 6,492 catalog
rows match the original reader; 13 native/fallback assets, scoped cancellation,
64 descriptor identities with bounded ownership, staged DDS priority, geometry
and texture reuse, five strand rendering controls, Full Swap/automatic staging,
textured Blender return with eyes recalculated, recovery and worker transport.
Timing varies by asset; not every first native decode is faster. Two existing
low-detail mesh-reference warnings remain tracked. The desktop launcher now
targets 0.2.30. All 197 Blender files and the runtime payload remain unchanged.
Clean-machine, live game/roster and outstanding native-dialog acceptance remain
external beta gates; this is not yet a stable WPF replacement.

## Superseded 0.2.29 Candidate: Source-Aware Preview Caches

Accessory catalogs now carry a source revision derived from their watched game
paths, manifest/settings timestamps and sizes, and extractor presence. The bridge
and Hair tab invalidate obsolete descriptors, workers, asset/head caches and
displayed selections when that source changes, including the same asset ID in a
different game folder. Unchanged catalog checks preserve warm caches and selection.

Successful settings/repair commits cancel older preview work; failed settings
writes keep usable caches. Pending lists belong to their settings generation and
configured source. Older results cannot undo a newer source or refill its caches.
Catalog installation also requires a current, validated list and target context.

Release builds also fingerprint build-owned sources, tools, retained engine files,
shared Rust code and the generated frontend at verification checkpoints. Changed
inputs abort publication before replacing the sharing EXE. BUILD.json records
the input/frontend digests; this guard does not run at application startup.

The build, 73 Rust tests, twelve focused freshness cases, packaged source-switch
checks, all 6,451 catalog row comparisons, representative native/fallback assets,
Full Swap auto-staging and textured Blender return pass. Runtime payload and all
197 Blender files are unchanged. Final native preview/render/recovery checks are
pending: the installed game updated mid-verification, temporarily removing its
manifest/extractor. The desktop launcher remains on verified 0.2.28 meanwhile.

Freshness is checked on catalog refresh/tab entry/settings changes, not by a
continuous filesystem watcher. Edits to archive bytes without changed watched
metadata and resource-handle lifetime remain separate review items.

## Previous 0.2.28 Scoped Preview Cancellation

Staged and catalog previews now drain cancellation independently. Cached catalog
and fit-base decodes wait for their own stop barrier; independent Blender
preparation does not wait for unrelated preview cancellation. New Swap requests
both stops together, then waits for both and any Blender preparation before
cleaning its session.

Terminated worker events cannot submit new reads or cancel replacement workers.
Cancelled fallback/preparation responses cannot restart decoders or repopulate
descriptor/staged caches after Reset.

Sixteen focused lifecycle cases and packaged delayed-stop/reset checks pass.
The prior two-decode burst policy, donor caches, A-B-A selection, source texture
priority, Full Swap staging, approved rendering and textured Blender return
remain verified. Conversion math, runtime payload and Blender files are unchanged.
Game-root/manifest cache identity is addressed by the 0.2.29 candidate above.

## Previous 0.2.27 Responsive Catalog Selection

The preview worker retains an active decode but replaces obsolete queued catalog
selections with the newest one. Replaced requests settle as cancelled, so callers
do not hang. Untagged requests, staged previews and Blender reference preparation
retain their existing ordering and behavior; donor-head/frame caches stay warm.

In three local 12-selection burst samples, only two previews decode instead of
twelve, and final-selection latency is 445-466 ms versus 2,092-2,455 ms in the
saved 0.2.26 comparison. This measures rapid cached selection, not every preview
load. Final geometry/UV/texture hashes and native head/frame reuse match.

Returning to the already displayed A while B loads now keeps A selected and
rendered. Both old success and old failure are ignored, without duplicating an
unchanged active/idle selection. Real-renderer A-B-A, cancellation/resume,
native/fallback comparisons, staging, Blender and rendering checks pass.
The runtime payload and all Blender tools remain unchanged.

## Previous 0.2.26 Hair Request Ownership

Hair and facial-hair target slots now belong to the player, configuration and
category that requested them. Older results and failures cannot replace newer
slots or repopulate them after New Swap. Install waits for current slots, checks
that the selected slot is present, and ignores stale completion UI after a
selection change. Config edits and staging invalidate the old target selection.

Repeated callers share a pending catalog read. Switching category can start its
new request without publishing an older category's result or clearing its busy
state. Explicit refresh still rechecks the authoritative backend cache.

Sixteen focused request-ownership cases and packaged control checks pass.
Native hair/beard/headband installs, config preservation, body-fit values,
Full Swap auto-staging, textured Blender return, staged textures, strand
rendering, recovery and responsive layout pass. No conversion or renderer math
changed; the prepared runtime and all Blender tools are reused unchanged.
This remains a beta candidate pending the external release checks below.

## Previous 0.2.25 Binary Preview Transport

Registered preview archives and files now cross the native bridge as bounded
binary packets, then transfer to the decoder worker. Compatibility JSON reads
remain available. In a 14-archive sample, transport/cache bytes fall from 67.0 MB
to 50.2 MB, preserving exact contents, hashes and compatibility fields.
These are archive-transfer savings, not a claim about total preview latency.
Conversion code, staged-first resolution and approved rendering remain intact.

## Previous 0.2.24 Responsive Preview Workspace

The Preview tab now fits its available window height instead of putting a
520-pixel minimum viewport inside a shorter scrolling panel. Camera controls,
the configuration selector and status stay visible at the minimum window size.
Render Summary collapses into a drawer on compact windows, and the navigation
uses an icon rail when space is tight.

Body-fit controls sit beside the player instead of covering it. Camera controls
and the jersey toggle remain available while editing; closing the panel restores
the full toolbar. Refresh uses a labeled icon button. Rendering and conversion
math are unchanged.

Real-app checks pass at five sizes, plus six light/dark fit-editor cases.
Adjusted orbit/zoom preservation, Refresh, fullscreen/Escape, Full Swap staging,
staged textures, strand rendering, recovery and Blender 5.1 round trip pass.
The runtime payload and all 197 Blender files are unchanged from 0.2.23, so an
already-prepared runtime is reused.

## Previous 0.2.23 Runtime Packaging And Safety

The sharing EXE is 52.5 MB instead of 59.0 MB. Unused development packages are
excluded from its payload; the expanded engine is 86.9 MB instead of 109.7 MB,
with 2,402 fewer files. Production apps are not modified. All 197 Blender files
and every retained Python-runtime file are verified unchanged during packaging.

Python workers ignore unrelated Python installation settings. New prepared
runtimes carry file inventories for best-effort current/previous retention;
active, modified, user-referenced and older uninventoried folders stay protected.
External custom Blender tools are preserved. The first launch prepares this new
payload once; subsequent launches reuse it. Cached startup timing remains similar.

Standalone imports/codecs, runtime repair, lease protection and packaged retention
checks pass. Available 2K18/19/20/23/26 conversions, full UI/staging, staged textures,
five strand controls, ZIP/RAR handling, settings/recovery and the textured Blender
5.1 Edit Mode round trip pass. Fresh 2K25 and external QA remain outstanding.

## Previous 0.2.22 Preview Worker Ownership

Completed preview buffers now transfer to the UI without an extra requested
structured-clone copy. A full-player test moves 3.3 MB across 43 buffers; native
and fallback donor-head caches remain intact. Nine real preview cases match the
prior build's geometry and texture hashes. A controlled same-payload sample
measures 27.6 ms copying versus 18.7 ms transferring; overall load-time samples
vary, so this is not presented as a uniform whole-preview speedup.

The worker entry is now an owned module rather than generated by slicing and
replacing original worker source. Serial processing, cancellation, projection,
failure recovery and the existing parser/renderer remain in use.
The runtime payload and all 197 Blender files are unchanged.
All release gates and 62 Rust tests pass, along with full UI/staging, staged
textures, rendering/recovery and a textured Blender 5.1 Edit Mode round trip.

## Previous 0.2.21 Catalog Update

Initial hair/beard listing now uses the Rust manifest index shared with preview
extraction. All 6,451 installed asset rows retain their original names, order,
flags and fields. The list no longer constructs or transports thousands of
unused preview descriptors; selected-asset previews remain on demand.

On the development PC, the first hair read including manifest indexing takes
813 ms versus 1,886 ms through the original reader. Facial hair then reuses the
index in 30 ms versus 1,633 ms. Cached reads take 21/5 ms, and list payloads are
about 76% smaller. The original reader remains available as a fallback.

All release gates, 62 Rust tests, native cancellation/cache invalidation, fallback
UI refresh, Full Swap/staging, staged textures, rendering and recovery checks pass.
The runtime payload, conversion helpers, shaders and all 197 Blender files are
unchanged, so the previously prepared runtime is reused.

## Previous 0.2.20 Cache Update

Repeated backend accessory-list reads now reuse a bounded Rust cache. On the
development PC, hair reads fell from 1,871 ms to 81 ms and facial hair from
1,623 ms to 17 ms, retaining all 5,361/1,090 rows and response fields.
First-load parsing remains unchanged. The UI's duplicate per-category list cache
is removed: category changes and tab entry recheck authoritative inputs, saved
game folders refresh the list, and unchanged refreshes preserve selection.

Template saves and CLI language changes no longer discard unrelated player
geometry. Real player edits and Blender returns still invalidate it; New Swap
still releases preview payloads. Settings changes clear obsolete bridge resources.
The runtime payload, conversion helpers, approved shaders and all 197 Blender
files are unchanged, so an existing prepared runtime is reused.
All release gates and 59 Rust tests pass, alongside packaged catalog refresh,
staged-texture, Full Swap/automatic staging, rendering and recovery checks.

## Previous 0.2.19 Checkpoint

Player catalogs now refresh when their manifest/settings files change or preferred
game files disappear and return. Previously the Rust cache could keep old rows
until settings were saved or the app restarted. Private fixture checks compare
with the original parser and cover edits, same-size replacements, external settings
changes, fallback discovery and preferred-root restoration. Files edited during
loading are not cached under their newer state.

All 9,977 rows across the installed 2K18/19/23/27 catalogs preserve every response
field; unchanged cached reads take 6-11 ms on the development PC. All release gates,
56 Rust tests and the full UI/staging/recovery checks pass. Renderer/conversion
code and Blender tools are unchanged, and the existing prepared runtime is reused.

### Previous Uniform-Array Update

The native decoder now handles the eight-layer uniform arrays used by the static
jersey and shorts references. Four 40-segment region-ID maps were previously
rejected by the ordinary 32-segment cap. Array tables now have a per-layer bound
with a 256-segment ceiling; single-layer, decoded-byte and overlap limits remain.
All eight garment DDS files match the original reader byte-for-byte without
garment decode warnings. The 20 shared asset tests now run as a release gate
alongside the 31 app Rust tests.
Full UI/automatic staging, staged DDS precedence, cache reuse and the five
approved hair/beard GPU controls also pass. Other app binaries and all 197
bundled Blender files are unchanged; the prepared runtime payload is reused.

### Previous Staged-Texture Update

The native preview now prefers DDS data already embedded in staged IFFs before
following retained TXTR .tld references into shared manifest resources. This fixes
edited body, hair and beard textures being silently replaced with their originals.
TXTR range metadata remains unchanged. Nineteen deliberately edited DDS payloads
across six body/accessory archives retain exact hashes; the ten uploaded channels
also match the original WPF fallback. Removing a staged legs map falls back only
that region, and subsequent edits invalidate the preview cache.

The 0.2.17 update passed 31 Rust tests, all release gates, the full UI/automatic
staging check and five approved hair/beard GPU controls. That update changed no
renderer shader, conversion math, Blender script or shared Archive Studio library.

### Previous Diagnostic Update

Unexpected conversion-engine exits now report the command and Windows exit code
in decimal and hexadecimal, with bounded diagnostic tails. Only the latest
worker-failure.json is kept under the app's Logs folder. Normal validation errors
and intentional cancellation keep their existing behavior. Thirty Rust tests and
an isolated packaged-worker termination/recovery check pass; the earlier one-off
catalog startup exit is not claimed fixed without a deterministic cause.

Actual Windows IFF, folder and Blender EXE selections, cancellation and return
focus have been checked in 0.2.15. ZIP/RAR and hair-only filter interaction, plus
repeat checks on the latest build, await the user's next Computer Use approval.

Cancelling the main preview no longer interrupts catalog decoding, and cancelling
the catalog no longer affects the main preview or independent Blender-reference
work. Native cancellation uses category-scoped tokens and queued-request generations;
bridge cancellation drains only its own category. New Swap waits for a pending
Blender opening to settle before clearing session files. The real native regression
failed at three timings on 0.2.14 and passes on 0.2.15, with matching-category
cancellation and independent-reference controls. Twenty-six Rust tests pass.
The bundled runtime payload is identical to 0.2.14, allowing its prepared runtime
to be reused; no Blender scripts or rendering/conversion math changed.

Stock manifest headband fitting now prepares a bounded, temporary copy with decoded
referenced geometry buffers before calling the unchanged production fitter. The
actual player keeps its original compressed geometry and unrelated resources.
Stock png06827, converted-player fitting, native decoding guards and Blender Edit
Mode round trips pass. All 197 bundled Blender files remain unchanged.

All 46 WPF public API methods and 228 UI control IDs are retained. The legacy
backend launcher remains intentionally disabled. Completion callbacks can start
another swap, subscriber failures are isolated, immediate cancellation avoids
launching a worker, and cancelled workers report cancellation rather than an
unexplained engine exit. Independent Blender openings and Windows path aliases
are handled without losing pending requests.

Cancellation waits for a native registration acknowledgement and targets only
its own job ID. Late progress/cancel requests cannot affect a replacement swap.
Blender handoffs also have independent request IDs; an old close/error or RPC
failure cannot close a newly reopened player session.

Staged preview fallback resolves missing config textures and all active accessory
companions, works without a manifest when local files are present, and invalidates
its resource cache when the game manifest changes. Staged body maps still win over
manifest maps. Runtime integrity verification accepts Windows package overlays
while retaining enclosed-path, ancestor-link, length and checksum checks.

Native file dialogs belong to the app window and retain the WPF hair-only filter.
Browser downloads, extra windows and permission requests are denied. Failed
Blender preparation/launch removes only its own temporary assets; successful
handoffs retain their existing supervised lifetime. Blender scripts/extensions
and the approved strand renderer are unchanged.

Staged previews now respect regular-only and tangent-only accessory declarations,
including manifest fallback. Unpaired tangent-space hair is reconstructed against
the staged head, while paired regular hair keeps the approved rendering behavior.
A private CHEER afro fixture matches all 11,153 catalog vertices exactly. Three
converted beard slot layouts pass visible-pixel, texture-binding and rotation
checks; all five approved catalog hair/beard material checks also pass.

The October 15 follow-up verifies a combined 2K23 hair + beard swap with package
renaming. Its source hair has 15,262 vertices, exceeding the original target slot.
The newer highest-LOD policy intentionally differs from WPF's smaller selection:
source indices and both UV streams match exactly, source-position error stays
below 0.000004 units, and all distance levels retain the highest detail. The
other seven archives match WPF directly. All eight archives match an independent
original-converter reference with only that policy enabled; the renamed player
also renders with hair and beard color, lightmap and tangent maps.

Parity comparison now preserves duplicate game-document properties. Thirteen
comparison guards and all ten retained original-WPF conversion cases, plus the
final multi-config comparison, pass under the stricter parser. These are
verification changes; the approved renderer and bundled Blender tools are unchanged.
Three negative source-hair tests also reject deliberate index, position and UV changes.

Native WebView2 settings match the WPF restrictions. Actual isolated browser
and renderer crashes terminate owned conversion workers, exit with an error and
leave no final partial player. The latest diagnostic replaces one small JSON
file instead of accumulating crash logs. Twenty-six Rust tests pass.

The complete WPF production pathway (including its post-swap chest correction)
and Tauri generate matching 2K18/19/20/23/26 contents across 77 archives and 5,041
entries in ten cases, including 2K18/19 source-hair swaps, three more 2K23 beard
slot permutations and the legacy chest corrections. The regular-only permutation uses a private target
fixture; it is not claimed as a stock catalog layout. Full Swap and
both preview readers also pass with only Windows system directories on PATH.
That verifies packaged worker lookup, not a clean Windows installation: WebView2
and the .NET Framework requirements of compatibility helpers still need a
separate-machine check.

Thirty-eight inherited WPF 2K25/configuration fixture tests now run against the
native DDS builder as a release gate, with no Node process. Seven packaged Blender
eye-policy tests also pass. A real 2K26 output matches its source eye radii within
0.000025 units and retains all 332 target morph resources. These checks preserve
the stock-size/no-change policy and do not replace the pending in-game blink test.

Paired config/accessory tests now cover ten operations/checks on multi-config
Embiid and converted png6720 fixtures. The final content comparison covers 33
archives and 1,846 entries, with explicit allowances only for verified native
metadata/shared-texture packaging. Both backends protect shared headband slots.
The inherited stock png06827 compressed-position fitting failure is now resolved
with the private input-copy wrapper described above. Legacy headband transfer
changes vertex data while preserving target topology and the original source.

See PARITY-AUDIT.md for verified workflows and the remaining sign-off work.

The 0.2.3 audit fixes legacy hair selection in both bridge/backend loaders:
Full Swap uses the highest source LOD, expanding the target stream when needed.
A real 2K23 png6720 conversion retains all 16,213 source vertices; its triangle
indices, both encoded UV streams, texture payloads and TXTR Min/Max match the
source exactly. The preview now applies range-decoded hair/beard lightmaps while
preserving color-map alpha. This response only affects preview materials.
Blender return replacement holds the edit lock briefly rather than blocking app
edits for the entire Blender session. Return validation, backup rollback and
session path boundaries are covered by tests. All 197 Blender tool files remain
byte-identical to the production bundle.

## 0.2.4 Parity Audit

The complete production command surface is connected, apart from the intentionally
removed legacy/backend launcher. The feature audit now also verifies:

| Workflow | Verification |
| --- | --- |
| Full Swap and staging | Real 2K23 UI conversion; 2K18/25/26 native conversions |
| 2K25 source facial hair | Adem Bona detected and converted successfully |
| DDS conversion without Node | 54 native Rust cases byte-identical to the old builder |
| Configs | Create, edit, rename, defaults; original config remains unchanged |
| Body fit | 113 target fields, negative/over-one edits, unmatched values preserved |
| Hair and facial hair | Catalogs, original donor head, scoped installation and textures |
| Headbands | Native 2K27 item, geometry and configuration installation |
| Preview | Native/legacy texture and geometry parity, staged overrides, GPU reuse |
| Blender | Textured Edit Mode round trip; eye recalculation and rollback guards |
| New Swap | Failure recovery and cancellation; Blender handoff files preserved |
| Settings and templates | Persistence, normalization and corrupt-template recovery |
| Favorites and theme | Actual native UI filtering and reload persistence |
| CLI | Correct bundled looyh binary, localized language readback and input guards |
| Health | Every bundled file hash verified; cache repair preserves exports |

The 2K25 converter no longer depends on the Node DDS helper: it invokes this
sharing EXE's bounded native DDS builder and reuses the Archive Studio codec.
The normal app remains free of Node and .NET window hosts. Roster memory writes
remain guarded by highlighted-player identity, expected-value checks and readback.
Language tests restore the CLI's original preference.

Blender receives its own player snapshot and jersey copy. A return is rejected
if the staged player changed after the handoff, rather than overwriting newer
edits. New Swap does not delete a live Blender handoff.

## 0.2.5 Recovery And Archive Audit

Nested ZIP and RAR5 packages (stored and solid) now have automated parity checks
against the unpacked source, including companions and facial-hair detection.
A real RAR-sourced 2K23 facial-hair Full Swap passes with zero validation errors.
Unsafe paths, duplicate/multiple players, links and invalid inner IFFs are rejected
without leaving failed staging folders. New Swap preserves the final export.
WinRAR is used only to generate test fixtures; recipients use the bundled reader.

Missing cached Python, bridge or ready-marker files are now repaired from the
checksummed EXE automatically. Preparation is serialized across instances, and
active runtime leases block unsafe replacement. Settings and outputs survive
repair, and successful repair removes its temporary/backup folders. Backend
bootstrap measured 543 ms from the repaired cache, versus about 3 seconds for
first extraction/repair on the development PC. This host-only update reuses the
0.2.4 engine payload, so a healthy existing runtime needs no extraction.

Fourteen Rust tests and the runtime-recovery suite pass. Ten bundled legacy
regressions also pass, including 2K19 protected body routing and 2K20 manifest
offsets. At that audit, 2K19/2K20 were unavailable. The 0.2.10 checkpoint now tests
an installed 2K19 source and a complete saved 2K20 package. Live game/roster
acceptance still remains unverified.

Simulated roster tests cover all five writable fields across NBA2K25/26/27,
stale identity/value rejection, numeric bounds, and failed readback. They never
access game memory. Native UI tests confirm all eight tabs, textured rendering,
rotation, automatic staging, favorites/theme persistence and New Swap recovery.

## 0.2.6 Hair Catalog Performance

Catalog hair and facial hair now use Archive Studio's native Rust vertex/index
decoder, with the original JavaScript decoder retained as a fallback. Native
donor fingerprints no longer attempt to read a full virtual IFF through the
shared-texture-only route. Eight donor heads and their neutral tangent frames
are retained in a bounded, memory-only LRU, and released on New Swap/reset.
The JavaScript fallback also retains eight heads and reusable tangent frames.

Compact binary arrays preserve exact float/index values across the native
viewer boundary. Highest LOD, original head vertex IDs, scale-64 tangent
projection and existing texture/TXTR bindings are preserved. This changes
preview loading only, not exported assets, Blender tools or conversion math.
No new disk cache is created. The unchanged engine payload is reused.

tests/catalog-performance.cjs compares 13 real manifest hair/beard assets
against the original decoder, checks textures and geometry, cached frame reuse,
cache budgets, rotation and reset. tests/catalog-cache.cjs checks exact tangent
math, LRU eviction, clone isolation, same-size file edits and oversize bypass.
tests/catalog-load-time.cjs compares the same assets with 0.2.5 in isolated
sessions. Locally, first dreads loading fell from 4.84 s to 1.21 s, a tangent-space
beard from 2.40 s to 0.14 s, and curly afro from 3.08 s to 0.27 s. Warm selections
took 0.11-0.19 s. These measurements include asset preparation, texture decoding
and viewer transfer, but not GPU upload; timing varies by asset and machine.

## Storage And Packaging

The sharing EXE embeds the frontend and a checksummed engine payload. Engine files
are extracted once into Local AppData/CharacterModToolTauri/Runtime and reused.
Release packaging verifies its payload and all bundled Blender files before
atomically publishing the sharing EXE. A locked existing release is left intact.
Settings, templates and outputs are private to this app. Session extraction and
hair caches are removed on normal exit; stale sessions are recovered on startup.
The app never automatically removes another application's caches or Blender tools.

Blender 5.1, the relevant games and Microsoft Edge WebView2 Runtime remain required.
Node and Rust development tools are not required by recipients.

One sharing EXE is not the same as a fully self-contained runtime. The retained
compatibility decoders use Windows' .NET Framework CLR 4 (not the .NET SDK or a
.NET window host). Native texture helpers require VC++ v14 x64 DLLs; the audit
observed texconv loading them from System32 on this PC. Health now checks file
presence and clearly distinguishes it from successful loading or clean-PC proof.
Microsoft provides the [Framework installation guide](https://learn.microsoft.com/en-us/dotnet/framework/install/)
and [supported VC++ Redistributable downloads](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist).
No runtime installers or system changes are performed automatically.

## Development

### 0.2.7 Preview Diagnostics

Hair tangent textures now reconstruct TXTR ranges before anisotropy sampling.
Strand lightmaps modulate reflected diffuse and specular lighting without changing
color-map alpha. Shader composition, GPU compilation and 13 native/legacy catalog
cases pass. Preview diagnostics verify exported IFF hashes remain unchanged.
The png06572 tangent-space afro has 11,153 zero TEXCOORD1 pairs and populated
TEXCOORD0; the existing preview fallback already selects the populated channel.
The latest png2882 regular hair also has zero TEXCOORD1, but already uses TEXCOORD0.
Its remaining faceted highlights are not solved by changing that UV channel.

### 0.2.8 Strand Material Response

Textured hair and beard previews tint reflected highlights from the decoded color
atlas instead of treating dark fibers as white solid surfaces. This preview-only
approximation keeps anisotropy, TXTR decoding, lightmap response and atlas alpha.
It does not change exported IFFs, Blender operations or the other material roles.
The png2882 reproduction no longer has bright blue-gray card patches. Real GPU
checks verify visible, rotating geometry for the png06572 afro, straight/short hair,
and ordinary/tangent-space beards. Exact game-shader parity is not claimed.

### 0.2.9 Hair Alpha Coverage Trial

Textured hair and beard materials now use cutout depth instead of ordinary alpha
blending. Multisampled preview contexts use alpha-to-coverage for smoother strand
edges; non-MSAA contexts retain the same atlas mask with standard alpha testing.
The existing cutoff, atlas alpha, lightmap and tint response remain unchanged.
This affects both previewers only, not exported assets or Blender operations.

Run setup-dev.ps1 once, then build-release.ps1. Node/TypeScript dependencies are
build-only. Every build runs typecheck.cjs before compiling the frontend; failed
type checks prevent packaging. tests/contracts.types.ts checks incompatible asset
categories, incomplete swap requests and incorrect picker result types.
tests/conversion.ps1 verifies a real 2K23-to-2K27 conversion into a test folder.
tests/tauri-ui.cjs tests the real native frontend using a temporary debugging port
enabled only with --ui-test. Normal launches have no debugging port.
tests/native-preview.cjs checks native/legacy geometry, texture and TXTR parity,
cached refreshes and GPU geometry reuse against the actual Tauri window.
Full Swap completion also verifies that the output becomes the staged player.
After that UI test, tests/blender-roundtrip.cjs verifies the preserved Blender
handoff using an isolated output, preview textures and static jersey.
tests/highest-hair.cjs verifies the real hair full swap and automatic staging;
tests/compare-source-hair.py checks source/export buffers and texture payloads.
tests/blender-services.py covers managed returns and rollback without modifying
user exports. Full 1:1 parity is not claimed until the remaining generation and
in-game checks above have been completed.

Additional regression suites: tests/workflow-parity.cjs,
tests/generation-parity.cjs, tests/settings-roster.cjs, tests/ui-recovery.cjs
and tests/dds-parity.cjs. Archive/recovery coverage is in tests/archive-parity.cjs,
tests/runtime-recovery.cjs and tests/roster-guards.py. Editing and conversion tests use dedicated artifact
copies, not the user's latest exported player.
