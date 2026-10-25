const label = mesh => [mesh.sourceName, ...(mesh.groups || []).map(group => group.material)].join(' ');
const facialHair = mesh => Boolean(mesh.tangentOffsets?.length) && /facial.?hair|beard|goatee|scruff/i.test(label(mesh));
const filename = mesh => String(mesh.sourceName || '').replace(/\\/g, '/').split('/').pop().toLowerCase();

export function selectTangentMeshes(meshes, mode) {
  if (mode === 'all') return meshes.filter(mesh => mesh.tangentSpaceAttachment);
  if (mode !== 'staged') return meshes.filter(facialHair);
  const ordinary = new Set(meshes.filter(mesh => !mesh.tangentSpaceAttachment).map(filename));
  return meshes.filter(mesh => facialHair(mesh) || mesh.tangentSpaceAttachment
    && /hair/i.test(label(mesh))
    && !ordinary.has(filename(mesh).replace(/_tangentspace\.iff$/, '.iff')));
}
