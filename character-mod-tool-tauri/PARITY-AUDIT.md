# Historical WPF Parity Evidence

As of October 16, 2026, strict 1:1 parity is no longer the objective.
IMPROVEMENT-AUDIT.md governs the current feature, appearance, quality and
efficiency review. The results and external gates below record the prior audit;
they do not require retaining inefficient implementations or stop independent
improvement work.

Baseline: WPF Character Mod Tool 0.1.31.
Checkpoint: Tauri 0.2.19, October 15, 2026.
Status: external sign-off blocked; full 1:1 parity is not yet claimed.
Requested work window: 17:58:46 through 20:28:46 UTC (1:58:46-4:28:46 PM EDT).
Paused early at the user's request on October 11; resumed October 15.
The original cumulative 2.5-hour work limit was completed; the user then opened a new full-parity follow-up goal.

## Verified

| Area | Evidence |
| --- | --- |
| Public surface | 46 API methods present; 20 direct command/payload contracts checked |
| UI | 228 WPF control IDs retained; all eight production tabs tested |
| Full Swap | Real 2K23 beard conversion validates and automatically stages |
| Generations | Fresh 2K18/19/26 conversions and saved complete 2K20 source conversion validate |
| Legacy chest | Blender correction applied to 132/130/30 vertices for the 2K18/19/20 cases |
| Source preservation | Generation conversion source hashes unchanged |
| Configs | Create/edit/rename/default; scoped hair/beard/headband leave original config unchanged |
| Body fit | 113 fields; negative and over-one values persist without changing unrelated fields |
| Hair/headbands | Native hair/beard installation, textures and 2K27 headband config verified |
| Preview | Native/fallback positions, indices, UVs, DDS payloads and bindings match |
| Staged DDS precedence | Reproduced a 0.2.16 bug: native preview followed retained TXTR .tld references into the manifest instead of reading edited .dds data already embedded in staged IFFs. 0.2.17 prefers the enclosed DDS, retaining TXTR metadata and the native compressed-TLD path when no DDS exists. Nineteen edited DDS payloads across chest, torso, legs, male skin, hair and beard archives match exactly; the ten uploaded channels match the original WPF fallback, including range and binding. Single-region fallback, edit invalidation, source hashes and reset cleanup pass |
| Uniform array textures | Reproduced four missing 40-segment R8 uniform maps on 0.2.17. Their original DDS files have eight array layers. 0.2.18 bounds segment tables per layer, with a 256-segment ceiling and unchanged 32-segment ordinary-texture limit; size, overlap and complete-coverage checks remain. All eight jersey/shorts DDS files now match original-reader hashes exactly, with no garment warnings |
| Shared native decoder gates | Release builds now run the shared asset library's 20 tests, including array/single-layer/cap/truncated-header guards, plus all 31 app Rust tests. Only this app was rebuilt; other app binaries and bundled Blender tools were not changed |
| Catalog | 13 real native/fallback asset comparisons; source IDs and tangent frames preserved |
| Player catalog coherence | Reproduced 0.2.18 returning old rows after a private manifest edit while the original parser returned the new player. 0.2.19 checks manifest/settings state and resolved/preferred manifest/extractor presence before cache reuse. Edits, same-size replacements, external settings changes, missing/restored preferred files and extractor presence transitions pass; files edited during a load are not cached under the new state |
| Installed catalog cache coverage | Complete cached responses match all fields of the original worker response for 2K18 (1,857 rows), 2K19 (1,849), 2K23 (2,942) and 2K27 (3,329): 9,977 rows total. Cached reads take 6-11 ms without launching another worker. 2K20/25/26 manifests remain unavailable for this fresh listing check |
| Rendering | Afro, straight, fade and regular/tangent beard GPU checks pass; catalog overlay changes 10,721 pixels |
| Accessory slot coverage | Three more complete WPF/Tauri 2K23 swaps match: png4109 paired beard, png2200 stock tangent-only beard and a private regular-only target fixture; 21 archives and 1,158 entries, with source/target hashes unchanged |
| Staged accessory modes | Strict yes/no modes exclude opposite local and manifest companions; paired and unknown modes retain compatible fallback behavior; five Python preview tests and the Rust mode guard pass |
| Tangent-only main hair | Private png06572 CHEER tangent-only fixture reconstructs all 11,153 vertices identically to the native catalog; paired regular-hair selection is unchanged |
| Converted beard visibility | Paired, tangent-only and regular-only outputs show 1,945/344/1,657 changed beard pixels; color, lightmap and anisotropy maps, desktop/compact layout and rotation pass |
| Legacy source hair | Complete 2K18 and 2K19 png0005 source-hair Full Swaps match WPF across 16 more archives and 1,168 entries; original source/target hashes unchanged; both outputs visibly render with color, lightmap and anisotropy maps |
| Combined swap and package rename | 2K23 png4109 hair + beard to png28001 validates and renders. Seven non-hair archives match WPF exactly; the hair archive matches the original converter with the explicitly requested highest-LOD policy. All eight archives/391 entries match that policy-aware reference; source and target companion hashes are unchanged |
| Highest-LOD exception | Combined output retains all 15,262 source vertices, exact source triangle indices, both UV byte streams and UV metadata; maximum source-position error is 0.000003815 units. All distance levels use that geometry. The older WPF selection is deliberately not restored |
| Duplicate JSON properties | Comparator now preserves repeated property values and their relative order, distinguishes objects from arrays, and detects normalized-key collisions. Thirteen comparison guards pass; all ten retained original-WPF conversion cases and the final multi-config comparison pass again with this stricter parser |
| Highest-LOD negative guards | Three tests deliberately alter index, position and UV data; each change is rejected. The verified source-hair exception cannot silently accept those geometry changes |
| Cache | Cached staged decode ~23 ms; seven GPU meshes reused; reset clears cache payloads |
| Recovery | Native job-registration acknowledgement, request-specific cancellation, stale progress isolation; real worker cancellation and New Swap recovery |
| Preview cancellation isolation | Reproduced 0.2.14 failure at three timings: cancelling staged preview killed catalog geometry. Category-scoped native tokens and generation tickets fix it in 0.2.15; six real native tests cover cross-category survival, matching-category cancellation and independent reference work |
| Reference/reset ordering | Bridge tests verify worker scope tags, category-specific draining and independent Blender preparation; New Swap waits for preparation/opening to settle before clearing session files. All 26 Rust tests pass |
| 0.2.15 UI recovery | Full-swap staging/all eight tabs and a subsequent recovery run pass. Favorites/theme persist; cancelled selection clears cache; failed-swap recovery restores the button and a seven-mesh/16-texture preview. One earlier catalog startup worker ended without a result; direct catalog read and rerun passed, but its cause remains unproven |
| Blender | Textured Edit Mode round trip with jersey and recalculated eyes; rollback/stale-return guards; independent/reopened session IDs reject late events |
| Persistence | Settings/templates/favorites/theme tested; corrupt templates preserved with warning |
| CLI | Binary integrity, language readback and input guards; no live writes performed |
| Archives | ZIP and stored/solid RAR parity; unsafe paths, links and invalid packages rejected |
| Runtime | Missing Python/bridge/ready-marker repaired; settings/exports preserved; active leases protected |
| Host | Dialog owner/hair filter restored; browser downloads/windows/permissions denied; native context-menu/devtools/host-object/status/zoom settings read back |
| Native Windows interaction | Actual 0.2.15 source IFF selection/cancellation, replacement cancellation, supported/all-files menu, Blender EXE selection/current folder, output folder selection/cancellation and return focus pass in an isolated profile. Settings were cancelled without saving; ZIP/RAR and hair-only filter interaction remain pending |
| Worker diagnostics | Thirty Rust tests pass. Unexpected protocol/worker exits include command and decimal/hex Windows exit status, with bounded stderr/stdout tails in one replaced local log. A deliberately stopped packaged catalog worker reports its exit, then a fresh catalog succeeds; ordinary validation and expected cancellation do not replace the failure log |
| Catalog startup sampling | Twelve fresh 0.2.15 host cycles return all 5,361 hair / 1,090 beard assets. This narrows but does not explain the earlier one-off worker exit; no automatic retry or conversion changes were introduced |
| 0.2.16 packaged checks | Another twelve fresh-start catalog cycles pass. All eight UI tabs, seven-mesh/16-texture staged preview, automatic Full Swap staging, body-fit fields, hair/beard catalogs, favorites/theme and failure/cancellation recovery pass. Native cross-preview cancellation controls still pass; the prepared runtime hash is unchanged |
| 0.2.17 packaged checks | Thirty-one Rust tests and all release gates pass. Full Swap still validates with zero errors and automatically stages; all eight UI tabs, body-fit fields, both catalogs, desktop/compact nonblank rendering and rotation pass. The five approved afro/straight/fade/regular-beard/tangent-beard GPU controls pass. The runtime payload and all 197 Blender files are unchanged; shared Archive Studio code was not modified |
| 0.2.18 packaged checks | All release gates and 51 Rust tests (31 app + 20 shared) pass. Full Swap validates and automatically stages; all eight UI tabs, body-fit fields, both catalogs, desktop/compact rendering and rotation pass. Preview reports no garment-map warnings. The 19 staged DDS checks, direct WPF fallback comparisons, edit invalidation, zero-payload reset and category cancellation controls still pass; all five strand GPU visible-pixel counts are identical to the prior build. Runtime payload hash and all 197 Blender files are unchanged |
| 0.2.19 packaged checks | All release gates and 56 Rust tests (36 app + 20 shared) pass. Full Swap validates and automatically stages; all eight UI tabs, previews, body-fit fields and both accessory catalogs pass. Recovery verifies favorites/theme, cancellation/New Swap, zero retained preview payloads and failed-swap button restoration. Runtime payload hash and all 197 Blender files are unchanged; conversion and renderer code were not modified |
| Renderer crash | Actual browser and renderer crashes exit with code 1, reap owned conversion workers and publish no partial player |
| Output comparison | Complete WPF 0.1.31 and Tauri paths match ten 2K18/19/20/23/26 cases: 77 archives, 5,041 entries; includes legacy source hair, three beard slot permutations and legacy chest correction |
| Inherited regressions | 38 WPF configuration and 2K25 geometry/texture/beard fixture tests pass with the native Rust DDS builder; no Node process, skips or original settings writes |
| Modern eyes | Seven Blender policy tests pass; real 2K26 exported eye radii differ from source by under 0.000025 units and all 332 target morph resources remain unchanged |
| Paired config edits | Ten WPF/Tauri checks cover Embiid's multi-config create/clear/rename/default, body fit, scoped hair/beard, shared headband rejection, converted-player installation and legacy headband shape transfer; final comparison covers 33 archives and 1,846 entries. Transfer changes vertex data, preserves target topology and leaves its source unchanged |
| Stock headband fitting | Actual manifest png06827 succeeds using a private normalized fitting copy; all 349 unrelated player resources, including four original compressed geometry buffers, are byte-identical afterward; source package is unchanged |
| Fit input safety | Six wrapper/input tests and nine native decoder guards pass; bounded expansion, malformed sizes, partial-copy cleanup and restoration of the original fitter are checked |
| Package integrity | Sharing EXE footer, payload bounds, hash and metadata agree; all 197 Blender files match production byte-for-byte; verified on each package build |
| Publication | Candidate EXE is verified before atomic publication; a locked prior release is preserved, owned temporary build files are cleaned on failure |
| Startup reuse | 0.2.14 private runtime startup measured 2.72 s initially, 0.58 s cached and 0.54 s after repair; two active leases prevent destructive repair |
| Audit storage | Superseded migration builds/test copies removed: 1,964,610,227 bytes freed in this pass; production apps, Blender tools, exports and current comparison fixtures preserved; latest release, two rollback releases and the performance baseline retained |
| Resumed audit storage | Two superseded combined-swap test copies removed after the replacement passed, freeing another 292,141,624 bytes; latest verified source/baseline/output fixture retained |
| 0.2.16 storage | Removed superseded 0.2.11/0.2.12 test executables and unused native-picker archive fixtures, freeing 154,800,672 bytes. Retained 0.2.16, two rollback releases and the 0.2.5 performance baseline; production apps, exports and Blender tools unchanged |
| Redistribution | Full Swap, native/fallback previews and health pass with Windows-only PATH; clean-machine prerequisites remain unverified |

## Remaining

- Fresh 2K25 manifest/source conversion check. Its former Steam folder no longer contains the manifest.
- In-game verification of converted models, blinking, hidden geometry, LOD visibility, accessories and physics.
- Live roster read/write verification with a highlighted player and EAC disabled; game is not currently running.
- Remaining native ZIP/RAR source selections and hair-only filter checks; repeat the completed native subset on the updated build when Computer Use approval is available.
- Clean-machine redistribution and missing-prerequisite checks.
- Broader source coverage per generation; the successful examples do not prove every player/config permutation.
- Investigate the one-off catalog startup worker exit observed during 0.2.15 UI recovery. A direct read returned 5,361 assets and the repeat UI run passed; this is not yet a deterministic reproduction or a closed issue.

## External Sign-Off Blockers

The user is away from the PC. These conditions remained unresolved across the
0.2.17, 0.2.18 and 0.2.19 follow-up turns; the current audit rechecked them.

- Neither configured Steam library has a current NBA 2K25 manifest. A complete source package is still needed for a fresh conversion comparison.
- No NBA 2K game is running, so live roster access and in-game model/blink/LOD/physics acceptance cannot be verified.
- Latest-build native dialog interaction requires the user's Computer Use approval. The previously passed 0.2.15 subset is retained as evidence, not treated as latest-build sign-off.
- Windows Sandbox is absent and no separate clean Windows test environment has been provided. Windows-only PATH testing is not substituted for that gate.
- The earlier one-off catalog startup exit has not reproduced in the retained cold-start/recovery checks. Bounded diagnostics are installed; an actual recurrence/log is needed to establish its cause.

Final 0.2.19 package verification again confirms the payload hash and all 197
unchanged Blender files. Current UI reports retain all eight tabs and 228 control
IDs, zero swap validation errors, automatic staging, 113 body-fit fields, both
accessory catalogs, nonblank desktop/compact rendering, rotation and successful
failure/cancellation recovery. These results do not close the external gates.

## Next Sign-Off Steps

1. Provide the current 2K25 install/manifest or a complete player package with its face and accessory companions. Run a fresh full swap, including source facial hair, and compare against WPF.
2. Check the converted models in-game: blinking, hidden geometry, near/far gameplay LODs, hair physics, and accessory/config switching. Desktop and Blender rendering do not prove these game behaviors.
3. With the game running without EAC and a disposable roster player highlighted, verify live roster reads and explicitly authorized reversible field writes/readback.
4. Finish native ZIP/RAR and hair-only filter checks and repeat on the latest build. Actual 0.2.15 IFF/source cancellation, folder and EXE selection/current-location/return-focus checks have passed.
5. Test the sharing EXE on a separate clean Windows installation, including prerequisite detection, first runtime extraction, cached launch and Blender integration.
6. Broaden player/config samples across generations, including stock and custom modern heads. Existing examples establish tested compatibility, not exhaustive equivalence for every asset.

Native Computer Use checks resumed after the Windows Update prompt cleared.
The verified subset is recorded in artifacts/native-dialog-report.json.
The updated executable requires a fresh app approval; the user is away from the
PC, so further native checks are deferred. No update/restart settings were changed.

## Intentional Differences

- Node backend and .NET window host are replaced by Rust/Tauri; compatibility format helpers remain bundled.
- The requested legacy/backend tab is removed.
- Archive Studio native extraction/caching and the approved preview-only strand response/coverage remain in use.
- The newer source-hair highest-LOD selection is retained; source hair is separately checked against original geometry/UV/texture data rather than regressed to older selection behavior.
- Exact game-shader evaluation and facial morph animation are not claimed by the neutral preview.
- Staged previews honor declared single-geometry modes instead of resurrecting an opposite companion from the manifest. Unpaired tangent-space hair is reconstructed in the main preview; paired regular hair retains its approved appearance. These are preview-only improvements over the inherited behavior.
- Native accessory extraction retains binary TXTR metadata and may embed shared DDS dependencies that WPF leaves in the manifest. Paired config comparisons decode metadata, normalize returned unique IDs and supported present-buffer aliases, and verify embedded DDS against reconstructed WPF payload hashes. Geometry/texture buffers are otherwise compared exactly; thirteen comparison guard tests pass.

Detailed test reports are in artifacts/*-report.json. Tests use private copies or
temporary fixtures, not the user's latest export. Blender tools are verified
against the production bundle before delivering the sharing executable.
