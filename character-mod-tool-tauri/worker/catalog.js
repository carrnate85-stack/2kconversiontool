import { native } from './native.js';
import { HeadPreviewCache } from './head-cache.js';
import { projectTangentSpaceMeshes, frameCacheStats } from './tangent-frame.js';
const { createIffPreview } = require('cmt-preview');
const heads = new HeadPreviewCache();

function hydrate(payload) {
  if (!payload) return null;
  for (const mesh of payload.meshes) {
    for (const [key, value] of Object.entries(mesh)) {
      if (!value || typeof value !== 'object' || typeof value.data !== 'string' || !['f32', 'u32', 'u16'].includes(value.kind)) continue;
      const binary = atob(value.data);
      const bytes = Uint8Array.from(binary, char => char.charCodeAt(0));
      const ArrayType = value.kind === 'f32' ? Float32Array : value.kind === 'u32' ? Uint32Array : Uint16Array;
      if (bytes.byteLength % ArrayType.BYTES_PER_ELEMENT) throw new Error('Truncated native geometry array.');
      mesh[key] = new ArrayType(bytes.buffer);
    }
    for (const key of ['positions', 'uvs', 'attachmentUvs', 'tangentOffsets', 'skinWeights', 'detailUvs', 'overlayUvs']) {
      if (Array.isArray(mesh[key])) mesh[key] = new Float32Array(mesh[key]);
    }
    for (const key of ['indices', 'sourceVertexIds', 'tangentAnchorIds', 'skinOffsets']) {
      if (Array.isArray(mesh[key])) mesh[key] = new Uint32Array(mesh[key]);
    }
    if (Array.isArray(mesh.skinBones)) mesh.skinBones = new Uint16Array(mesh.skinBones);
  }
  return payload;
}

export async function catalog(asset) {
  const started = performance.now();
  const includeBase = asset.includeBase !== false;
  let geometry, fallback;
  if (asset.native && asset.nativeGeometry !== false) {
    try { geometry = await native('native-catalog-geometry', { ...asset, includeBase }); }
    catch (error) { fallback = String(error); }
  }
  const hair = await createIffPreview(asset.geometryPath, {
    companionPaths: asset.itemPath ? [{ path: asset.itemPath, kind: 'item' }] : [],
    maxTriangles: 350000, maxModels: 80, preferHighestDetail: true,
    nativeGeometry: geometry ? hydrate(geometry.asset) : null,
  });
  let base, cached, tangentVertices;
  if (geometry) {
    base = hydrate(geometry.base);
    cached = geometry.stats.headCacheHit;
    tangentVertices = geometry.stats.tangentHairProjectedVertices;
  } else {
    const result = await heads.get(asset.sourcePlayerPath, filePath => createIffPreview(filePath, {
      hairFitHeadOnly: true, skipTextures: true, preferHighestDetail: true, maxTriangles: 120000, maxModels: 8,
    }), { clone: includeBase });
    cached = result.cached;
    base = includeBase ? result.value : null;
    tangentVertices = projectTangentSpaceMeshes(hair, result.value);
    hair.meshes.filter(mesh => mesh.tangentSpaceAttachment).forEach(mesh => {
      if (mesh.geometryKey) mesh.geometryKey += ':head:' + (result.value.geometryKey || asset.sourcePlayerPath);
    });
  }
  if (asset.tangentspace && !tangentVertices) throw new Error('No tangent-space vertices could attach to the original donor head.');
  if (fallback) hair.skipped.push('Native catalog geometry fallback: ' + fallback);
  const kind = asset.assetType === 'facialhair' ? 'facialhair-fit-base' : 'hair-fit-base';
  if (base) base.meshes.forEach(mesh => { mesh.sourceKind = kind; });
  const stats = { ...hair.stats, ...(geometry?.stats || {}), nativeGeometry: Boolean(geometry), headCacheHit: cached,
    tangentHairProjectedVertices: tangentVertices, facialHairProjectedVertices: asset.assetType === 'facialhair' ? tangentVertices : 0,
    fallbackFrameCache: geometry ? null : frameCacheStats(), pickerDecodeMs: Math.round((performance.now() - started) * 10) / 10 };
  return { sourcePng: asset.sourcePng, key: asset.key,
    base: base ? { ...base, name: `png${asset.sourcePng} fitting head`, player: { mode: 'hair-catalog-native-head', bodyFit: {}, jerseyAvailable: false }, activeItems: [] } : null,
    asset: { ...hair, name: `${asset.key} from png${asset.sourcePng}`, player: { mode: 'hair-catalog-asset', bodyFit: {}, jerseyAvailable: false }, activeItems: [asset.key], stats }, stats };
}
