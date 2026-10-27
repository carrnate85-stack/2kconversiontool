// Regular (non-tangent-space) hair and beard meshes carry the per-card lightmap
// layout in TEXCOORD1, but the pinned preview decoder only keeps a second UV
// set for tangent-space meshes. Applied to the built worker bundle so regular
// strand meshes also ship it as attachmentUvs; the renderer then picks the
// atlas set and samples the lightmap with the other one.
const STRAND_CARDS = `        const strandCards = !tangentSpaceAttachment && Boolean(model.VertexFormat?.TEXCOORD1) && (() => {
          let inherited = "";
          const materials = (Array.isArray(model.Prim) ? model.Prim : []).map((primitive) => inherited = primitiveMaterial(primitive, inherited));
          return materials.length > 0 && materials.every((material) => /^(?:facial)?hair_/i.test(material));
        })();
`;

const REPLACEMENTS = [
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

function patchStrandUvs(source) {
  const crlf = source.includes('\r\n');
  let text = source.replace(/\r\n/g, '\n');
  if (text.includes('const strandCards = ')) return source;
  for (const [pinned, replace] of REPLACEMENTS) {
    const at = text.indexOf(pinned);
    if (at < 0 || text.indexOf(pinned, at + 1) >= 0) throw new Error(`Pinned preview UV hook changed: ${pinned.slice(0, 60)}`);
    text = text.slice(0, at) + replace(pinned) + text.slice(at + pinned.length);
  }
  return crlf ? text.replace(/\n/g, '\r\n') : text;
}

module.exports = { patchStrandUvs };
