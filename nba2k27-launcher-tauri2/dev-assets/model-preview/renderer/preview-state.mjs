// Preview-only coordinate/camera selection. Never edit decoded source arrays.
export function strandTextureCoordinates(mesh = {}) {
  const original = { uvs: mesh.uvs, semantic: mesh.textureUvSemantic || 'TEXCOORD0' };
  const atlas = mesh.attachmentUvs;
  const cards = mesh.groups?.length && mesh.groups.every(group =>
    /facial.?hair|beard|(?:^|_)hair(?:_|$)/i.test(String(group.material || '')));
  // Tangent-space cards store their strand atlas in UV0. UV1 is a separate
  // surface/attachment mapping, even when populated; it is not the color atlas.
  if (!cards || !mesh.tangentAnchorIds?.length || mesh.textureUvSemantic !== 'TEXCOORD1'
    || !atlas || atlas.length !== mesh.uvs?.length || atlas.length < 6 || atlas.length % 2
    || !atlas.every(Number.isFinite)
    || !atlas.some((value, index) => Math.abs(value - atlas[index % 2]) > 1e-6)) return original;
  return { uvs: atlas, semantic: 'TEXCOORD0' };
}

export function copyPreviewCamera(value) {
  if (!value || !Array.isArray(value.target) || value.target.length !== 3
    || !value.target.every(Number.isFinite)
    || !['yaw','pitch','distance','fittedDistance','fov'].every(key => Number.isFinite(value[key]))
    || value.distance < 0.001 || value.fittedDistance <= 0 || value.fov <= 0 || value.fov >= 180
    || Math.abs(value.pitch) > Math.PI * 0.48) return null;
  return { target: [...value.target], yaw: value.yaw, pitch: value.pitch,
    distance: value.distance, fittedDistance: value.fittedDistance, fov: value.fov };
}
