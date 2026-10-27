// Pinned fixes applied to the built preview worker bundle (its decoder source
// lives outside this repository). Each hook must match exactly once.

// Regular (non-tangent-space) hair and beard meshes carry the per-card lightmap
// layout in TEXCOORD1, but the decoder only kept a second UV set for
// tangent-space meshes. Regular strand meshes now ship it as attachmentUvs; the
// renderer picks the atlas set and samples the lightmap with the other one.
const STRAND_CARDS = `        const strandCards = !tangentSpaceAttachment && Boolean(model.VertexFormat?.TEXCOORD1) && (() => {
          let inherited = "";
          const materials = (Array.isArray(model.Prim) ? model.Prim : []).map((primitive) => inherited = primitiveMaterial(primitive, inherited));
          return materials.length > 0 && materials.every((material) => /^(?:facial)?hair_/i.test(material));
        })();
`;

const STRAND_REPLACEMENTS = [
  [
    'const tangentSpaceAttachment = String(position?.Format || "").toUpperCase() === "R16G16B16A16_UINT" && Boolean(model.VertexFormat?.POSITION1 && model.VertexFormat?.TEXCOORD0 && model.VertexFormat?.TEXCOORD1);\n',
    (match) => match + STRAND_CARDS,
  ],
  [
    'const uvDescriptors = tangentSpaceAttachment ? [model.VertexFormat?.TEXCOORD1, model.VertexFormat?.TEXCOORD0] : [model.VertexFormat?.TEXCOORD0];',
    () => 'const uvDescriptors = tangentSpaceAttachment ? [model.VertexFormat?.TEXCOORD1, model.VertexFormat?.TEXCOORD0] : strandCards ? [model.VertexFormat?.TEXCOORD0, model.VertexFormat?.TEXCOORD1] : [model.VertexFormat?.TEXCOORD0];',
  ],
  [
    'uvOptions.sort((left, right) => right.score - left.score);',
    () => 'if (tangentSpaceAttachment) uvOptions.sort((left, right) => right.score - left.score);',
  ],
  [
    'const attachmentOption = tangentSpaceAttachment ? uvOptions.find(',
    () => 'const attachmentOption = tangentSpaceAttachment || strandCards ? uvOptions.find(',
  ],
  [
    'attachmentUvs: tangentSpaceAttachment ? new Float32Array(attachmentUvs) : null,',
    () => 'attachmentUvs: tangentSpaceAttachment || strandCards ? new Float32Array(attachmentUvs) : null,',
  ],
];

// Player previews were capped at 24 textures / 16 MB and filled first with maps
// the renderer never samples (bent normal, cone angle, rdmo, region, roughness-
// curvature, sweat, wrinkle and detail normals, secondary body-normal variants
// such as avg_ripped), so hair lightmap/tangent maps and accessory textures were
// dropped on nearly every player. Unused maps are now skipped and the cap
// raised; arena previews keep their own limits. Chest/torso body normals come
// from native preparation, picked by muscle type, so the skin IFF's own
// avg_normal/avg_ripped copies are skipped. A player's own face_detailnormal_o
// (2048x2048, the face's high-frequency detail over a 512x512 face_normal_o) is
// kept; the generic skin IFF copies are not.
const UNUSED_TEXTURE = String.raw`      function previewTextureUnused(item) {
        const identity = ${'`${item.name || ""} ${item.source || ""}`'};
        const channels = item.assignments?.length ? item.assignments.map((assignment) => assignment.channel) : [item.binding?.channel];
        if (channels.some((channel) => channel === "color" || channel === "tangent")) return false;
        if (channels.includes("normal")) return /detailnormal/i.test(identity) && (!/^face_detailnormal_o\./i.test(item.name || "") || /_skin\.iff/i.test(identity)) || /_normal_avg_/i.test(identity) && !item.externalPath;
        return !/occlusion|(?:^|_)ao(?:_|\.)|lightmap|tangent/i.test(identity);
      }
`;

const TEXTURE_REPLACEMENTS = [
  ['var MAX_TEXTURE_BYTES = 16 * 1024 * 1024;', () => 'var MAX_TEXTURE_BYTES = 32 * 1024 * 1024;'],
  ['const maxTextureCount = includeArenaTextures ? 128 : 24;', () => 'const maxTextureCount = includeArenaTextures ? 128 : 40;'],
  ['      function previewTextureScore(item) {\n', (match) => UNUSED_TEXTURE + match],
  [
    '        for (const texture of prioritized) {\n',
    (match) => match + '          if (!includeArenaTextures && previewTextureUnused(texture)) continue;\n',
  ],
];

// Every player scene carries the same ~330 named morphs: FACS expressions and
// their combination correctives, joint-pose correctives (elbow, wrist, finger,
// knee) and THIN/THICK fit shapes. All are zero at rest, so the neutral scanned
// mesh is already the player's likeness; the old note implied otherwise.
const MORPH_NOTE = [
  [
    "skipped.unshift(`${morphTargets} facial morph target(s) detected. The preview currently shows the neutral base mesh; NBA 2K's math-node morph evaluation is not applied yet.`);",
    () => "skipped.unshift(`${morphTargets} morph target(s) detected (facial expressions and pose correctives). The preview shows the neutral rest pose, which already carries the player's likeness.`);",
  ],
];

// A player config (png#####_config_<look>.iff) can carry its own chest/legs
// color (png01801 beard/buzz, png02444 headphone/headband_fury/short_afro: a
// different tattoo era). Only one body color per region loads, ranked by
// container, so the main IFF always won; the selected config's now does.
const CONFIG_BODY_COLOR = [
  [
    '(bodyColorTextureTarget(a) && bodyColorTextureTarget(b) ? (a.container ?? Number.MAX_SAFE_INTEGER) - (b.container ?? Number.MAX_SAFE_INTEGER) : 0)',
    () => '(bodyColorTextureTarget(a) && bodyColorTextureTarget(b) ? bodyColorRank(a) - bodyColorRank(b) : 0)',
  ],
  [
    '      function bodyColorTextureTarget(texture) {\n',
    (match) => '      function bodyColorRank(texture) {\n        return /^config:/i.test(String(texture.source || "")) ? -1 : texture.container ?? Number.MAX_SAFE_INTEGER;\n      }\n' + match,
  ],
];

function apply(text, marker, replacements) {
  if (text.includes(marker)) return text;
  for (const [pinned, replace] of replacements) {
    const at = text.indexOf(pinned);
    if (at < 0 || text.indexOf(pinned, at + 1) >= 0) throw new Error(`Pinned preview worker hook changed: ${pinned.slice(0, 60)}`);
    text = text.slice(0, at) + replace(pinned) + text.slice(at + pinned.length);
  }
  return text;
}

function patchWorkerBundle(source) {
  const crlf = source.includes('\r\n');
  let text = source.replace(/\r\n/g, '\n');
  text = apply(text, 'const strandCards = ', STRAND_REPLACEMENTS);
  text = apply(text, 'function previewTextureUnused(', TEXTURE_REPLACEMENTS);
  text = apply(text, 'morph target(s) detected (facial expressions', MORPH_NOTE);
  text = apply(text, 'function bodyColorRank(', CONFIG_BODY_COLOR);
  return crlf ? text.replace(/\n/g, '\r\n') : text;
}

module.exports = { patchWorkerBundle };
