const HEADER_LIMIT = 4 * 1024 * 1024;
const BODY_LIMIT = 128 * 1024 * 1024;
const GEOMETRY_KEYS = ['positions', 'uvs', 'attachmentUvs', 'tangentOffsets', 'skinWeights',
  'detailUvs', 'overlayUvs', 'courtUvs', 'graffitiUvs', 'indices', 'sourceVertexIds',
  'tangentAnchorIds', 'skinOffsets', 'skinBones'];
const UINT_KEYS = new Set(['indices', 'sourceVertexIds', 'tangentAnchorIds', 'skinOffsets']);

function geometryPart(metadata, buffer, start, size) {
  if (!metadata || typeof metadata !== 'object' || Array.isArray(metadata)
    || !Array.isArray(metadata.meshes) || metadata.meshes.length > 1000) throw new Error('Invalid binary geometry metadata.');
  const source = new Uint8Array(buffer, start, size), arrays = [];
  let offset = 0;
  for (const mesh of metadata.meshes) {
    if (!mesh || typeof mesh !== 'object' || Array.isArray(mesh)) throw new Error('Invalid binary geometry mesh.');
    for (const key of GEOMETRY_KEYS) {
      const row = mesh[key]; if (row == null) continue;
      const kind = key === 'skinBones' ? 'u16' : UINT_KEYS.has(key) ? 'u32' : 'f32';
      const ArrayType = kind === 'f32' ? Float32Array : kind === 'u32' ? Uint32Array : Uint16Array;
      if (typeof row !== 'object' || row.kind !== kind || !Number.isSafeInteger(row.offset)
        || !Number.isSafeInteger(row.size) || row.offset !== offset || row.size < 0
        || row.size % ArrayType.BYTES_PER_ELEMENT || row.size > size - offset || 'data' in row) {
        throw new Error('Invalid binary geometry array range or type.');
      }
      const end = offset + row.size, next = Math.ceil(end / 4) * 4;
      if (next > size) throw new Error('Truncated binary geometry padding.');
      for (let index = end; index < next; index++) if (source[index] !== 0) throw new Error('Invalid binary geometry padding.');
      arrays.push({ mesh, key, ArrayType, offset, length: row.size / ArrayType.BYTES_PER_ELEMENT });
      offset = next;
    }
  }
  if (offset !== size) throw new Error('Trailing binary geometry data.');
  // Separate parts prevent a cached donor head from retaining an unrelated asset.
  const owned = buffer.slice(start, start + size);
  for (const row of arrays) row.mesh[row.key] = new row.ArrayType(owned, row.offset, row.length);
  return metadata;
}

export function unpackPreviewPacket(buffer) {
  if (!(buffer instanceof ArrayBuffer) || buffer.byteLength < 12 || buffer.byteLength > BODY_LIMIT + HEADER_LIMIT + 12) {
    throw new Error('Invalid binary preview packet size.');
  }
  const bytes = new Uint8Array(buffer);
  if (String.fromCharCode(...bytes.subarray(0, 8)) !== 'CMTBIN01') throw new Error('Unknown binary preview packet version.');
  const length = new DataView(buffer).getUint32(8, true);
  if (length > HEADER_LIMIT || length > bytes.length - 12) throw new Error('Truncated binary preview metadata.');
  const metadata = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes.subarray(12, 12 + length)));
  if (!metadata || typeof metadata !== 'object' || Array.isArray(metadata)) throw new Error('Invalid binary preview metadata.');
  const start = 12 + length, size = bytes.length - start;
  if (size > BODY_LIMIT) throw new Error('Binary preview data exceeds budget.');
  if (metadata.packetKind === 'file') {
    if (metadata.size !== size) throw new Error('Binary preview file size differs.');
    return { data: bytes.subarray(start) };
  }
  if (metadata.packetKind === 'catalog-geometry') {
    if (!Number.isSafeInteger(metadata.assetBytes) || !Number.isSafeInteger(metadata.baseBytes)
      || metadata.assetBytes < 0 || metadata.baseBytes < 0 || metadata.assetBytes % 4 || metadata.baseBytes % 4
      || metadata.assetBytes > size || metadata.baseBytes !== size - metadata.assetBytes
      || metadata.base == null && metadata.baseBytes !== 0 || !metadata.stats || typeof metadata.stats !== 'object' || Array.isArray(metadata.stats)) {
      throw new Error('Invalid binary geometry segment sizes.');
    }
    return { asset: geometryPart(metadata.asset, buffer, start, metadata.assetBytes),
      base: metadata.base == null ? null : geometryPart(metadata.base, buffer, start + metadata.assetBytes, metadata.baseBytes), stats: metadata.stats };
  }
  if (metadata.packetKind !== 'archive' || !Array.isArray(metadata.members) || metadata.members.length > 20000) {
    throw new Error('Invalid binary preview archive.');
  }
  let offset = 0;
  const names = new Set();
  const members = metadata.members.map(row => {
    if (!row || typeof row.name !== 'string' || names.has(row.name) || !Number.isSafeInteger(row.size)
        || row.size < 0 || row.offset !== offset || row.size > size - offset) {
      throw new Error('Invalid binary preview member range.');
    }
    names.add(row.name);
    const data = bytes.subarray(start + offset, start + offset + row.size);
    offset += row.size;
    return { name: row.name, size: row.size, data };
  });
  if (offset !== size) throw new Error('Trailing binary preview data.');
  const { packetKind, ...archive } = metadata;
  return { ...archive, members };
}
