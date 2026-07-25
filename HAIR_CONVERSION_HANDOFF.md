# Hair Conversion Engineering Handoff

Last updated: 2026-07-24

Current app version: `1.0.133-beta`

## Scope

The Character Mod Tool Hair tab now converts external NBA 2K23 and NBA 2K25
head-hair geometry into an existing NBA 2K26 appearance hair slot.

The conversion button is:

`Convert 2K23 / 2K25 IFF`

The source generation is detected from the SCNE vertex layout. The user does
not choose the generation manually.

Full Swap also exposes `Include Hair Swap`, automatic source-companion
detection, a manual source override, and a target appearance-slot selector.
That path stages the converted geometry with the complete output package and
does not write to the live mods folder.

## Main Files

- `character_mod_tool.py`
  - Adds the Hair-tab conversion button.
  - Adds `CharacterModTool.convert_external_hair()`.
  - Uses the currently open appearance IFF and selected hair slot as the
    NBA 2K26 target.
- `hair_tools/hair_picker.py`
  - Adds source generation detection and the shared installation workflow.
  - Important functions:
    - `external_hair_identity()`
    - `external_hair_conversion_plan()`
    - `validate_converted_external_hair()`
    - `convert_external_hair_to_output()`
    - `convert_external_hair_to_target()`
- `hair_tools/convert_2k23_hair_to_2k26_lod0.py`
  - Converts legacy segmented `.model` geometry to modern two-stream geometry.
- `hair_tools/convert_2k25_hair_to_2k26_static.py`
  - Repackages modern 2K25 geometry into a static NBA 2K26 shell.

## Shared Output Rules

- The destination is the selected appearance slot:
  `mods/char/sig/png####_geo_<target_key>.iff`.
- The target shell always comes from the NBA 2K26 game archives.
- Loose FIBA or other external hair folders are not used as source catalogs.
- Only full-detail geometry is referenced.
- All vertices are assigned to static head bone 48.
- The matrix-weight buffer is four zero bytes.
- Existing target files are backed up under the Character Mod Tool user-data
  `hair_picker_backups` directory.
- The target slot's existing NBA 2K26 item/texture file is retained.

## NBA 2K23 Conversion

Detection requirements:

- `POSITION0` is `R16G16B16A16_SNORM`.
- Separate `BINORMAL0` and `TANGENT0` channels exist.
- The model references a legacy `.model` binary.

Conversion behavior:

- Selects the highest authored legacy LOD that fits the target shell.
- If every authored LOD is larger than the target shell, uses the smallest
  authored LOD and expands the rebuilt 2K26 stream metadata to fit it.
- Converts quantized positions to `R32G32B32_FLOAT`.
- Rebuilds the packed NBA 2K26 tangent frame from legacy tangent/binormal data.
- Preserves source UV bytes and source UV Offset/Scale metadata.
- Pads unused vertices to the target shell's stream count when required.
- Emits one LOD and one primitive index range.

The target-count fitting code supports both multi-LOD NBA 2K26 templates and
static templates that do not contain a `Lods` field.

## NBA 2K25 Conversion

Detection requirements:

- `POSITION0` is `R32G32B32_FLOAT`.
- `TANGENTFRAME0` exists.
- Modern `VertexStream` and `IndexBuffer` descriptors exist.

Observed 2K25 differences:

- Two vertex streams are stored under duplicate `VertexBuffer` object keys.
- Hair commonly contains six LOD index ranges.
- The tested Wemby hair used compression metadata value 93.
- Its dominant head bone was 46 rather than NBA 2K26 bone 48.

Conversion behavior:

- Preserves source positions, tangent frames, and both UV channels.
- Selects the first/full-detail `LodList` range.
- Converts duplicate stream objects to an explicit two-stream array.
- Removes extra LOD references.
- Rewrites every weight to static bone 48.
- Uses the native NBA 2K26 target SCNE, material, mesh, tweaks, and archive
  structure.

## Verification

Full backend tests passed without writing to the live mods directory:

- NBA 2K25 source:
  `png8033_geo_hair_mid_fade.iff`
  - Target: `png8033_geo_hair_mid_fade.iff`
  - 10,420 vertices
  - 15,630 indices
  - Confirmed working in NBA 2K26 by the user.
- NBA 2K23 source:
  `png4503_hair_01.iff`
  - Target: `png4090_geo_hair_01.iff`
  - Selected `hihead_LODShape1`
  - 12,502 geometry vertices padded to 15,080 target stream vertices
  - 18,738 indices

The following checks passed:

- Python syntax validation for the app, backend, and both converters.
- Character Mod Tool `--package-self-test`.
- ZIP integrity and native member headers.
- Exactly two correctly sized vertex streams.
- Index buffer size, CRC, and vertex range.
- All output weights equal `48 << 8`.
- Four-byte matrix-weight buffer.

Test outputs are stored in:

`C:\Users\carrn\OneDrive\Documents\Hair Transfer\character_mod_tool_integration_tests`

## Known Limitations

- External conversion currently supports head hair only, not facial hair.
- This integrated workflow converts geometry only.
- It intentionally keeps the target slot's existing NBA 2K26 item textures.
- The target appearance slot must have a native NBA 2K26 geo entry in the
  manifest so it can provide the target shell.
- The source filename must begin with `png####_`.
- Full Swap hair conversion supports NBA 2K23 and NBA 2K25 head hair only.

## Resume Checklist

1. Open `character_mod_tool.py` and `hair_tools/hair_picker.py`.
2. Run the source package self-test:
   `.release_venv\Scripts\python.exe -B character_mod_tool.py --package-self-test`
3. Use the Hair tab with an open NBA 2K26 appearance IFF.
4. Select its target hair slot.
5. Click `Convert 2K23 / 2K25 IFF`.
6. Test any future changes with one source from each generation.
