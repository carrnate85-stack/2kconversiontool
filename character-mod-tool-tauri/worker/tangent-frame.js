// Preserve the verified projection math; cache only its immutable donor frame.
const entries = new Map();
const anonymous = new WeakMap();
const LIMIT = 8, BUDGET = 24 * 1024 * 1024;
let bytes = 0, builds = 0, hits = 0;
export function frameCacheStats() { return { entries: entries.size, bytes, budget: BUDGET, builds, hits }; }
function frameFor(baseMesh) {
  const key = baseMesh.geometryKey;
  const hit = key ? entries.get(key) : anonymous.get(baseMesh);
  if (hit) {
    hits++;
    if (key) { entries.delete(key); entries.set(key, hit); }
    return hit;
  }
  const faceVertices = new Set();
  const normals = new Float32Array(baseMesh.positions.length);
  const tangents = new Float32Array(baseMesh.positions.length);
  for (const group of baseMesh.groups || []) {
    if (!/face|head|(?:^|_)ears?(?:_|$)/i.test(group.material)) continue;
    for (let cursor = group.start; cursor < group.start + group.count; cursor += 3) {
      const triangle = [baseMesh.indices[cursor], baseMesh.indices[cursor + 1], baseMesh.indices[cursor + 2]];
      triangle.forEach((vertex) => faceVertices.add(vertex));
      const [a, b, c] = triangle.map((vertex) => [
        baseMesh.positions[vertex * 3],
        baseMesh.positions[vertex * 3 + 1],
        baseMesh.positions[vertex * 3 + 2],
      ]);
      const ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
      const ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
      const normal = [
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
      ];
      const [uvA, uvB, uvC] = triangle.map((vertex) => [
        baseMesh.uvs[vertex * 2],
        baseMesh.uvs[vertex * 2 + 1],
      ]);
      const duvAB = [uvB[0] - uvA[0], uvB[1] - uvA[1]];
      const duvAC = [uvC[0] - uvA[0], uvC[1] - uvA[1]];
      const divisor = duvAB[0] * duvAC[1] - duvAB[1] * duvAC[0];
      const tangent = Math.abs(divisor) > 1e-10
        ? [
          (ab[0] * duvAC[1] - ac[0] * duvAB[1]) / divisor,
          (ab[1] * duvAC[1] - ac[1] * duvAB[1]) / divisor,
          (ab[2] * duvAC[1] - ac[2] * duvAB[1]) / divisor,
        ]
        : ab;
      for (const vertex of triangle) {
        normals[vertex * 3] += normal[0];
        normals[vertex * 3 + 1] += normal[1];
        normals[vertex * 3 + 2] += normal[2];
        tangents[vertex * 3] += tangent[0];
        tangents[vertex * 3 + 1] += tangent[1];
        tangents[vertex * 3 + 2] += tangent[2];
      }
    }
  }
  if (!faceVertices.size) return null;
  for (const vertex of faceVertices) {
    const offset = vertex * 3;
    const length = Math.hypot(normals[offset], normals[offset + 1], normals[offset + 2]) || 1;
    normals[offset] /= length;
    normals[offset + 1] /= length;
    normals[offset + 2] /= length;
    const tangentLength = Math.hypot(tangents[offset], tangents[offset + 1], tangents[offset + 2]) || 1;
    tangents[offset] /= tangentLength;
    tangents[offset + 1] /= tangentLength;
    tangents[offset + 2] /= tangentLength;
  }
  const sourceVertexLookup = new Map();
  baseMesh.sourceVertexIds?.forEach((sourceVertex, vertex) => sourceVertexLookup.set(sourceVertex, vertex));

  const value = { normals, tangents, sourceVertexLookup };
  const cost = normals.byteLength + tangents.byteLength + sourceVertexLookup.size * 32;
  builds++;
  if (key && cost <= BUDGET) {
    while (entries.size >= LIMIT || bytes + cost > BUDGET) {
      const oldest = entries.keys().next().value;
      const old = entries.get(oldest);
      bytes -= old.cost;
      entries.delete(oldest);
    }
    value.cost = cost; bytes += cost; entries.set(key, value);
  } else if (!key) anonymous.set(baseMesh, value);
  return value;
}
export function projectTangentSpaceMeshes(preview, basePreview) {
  const baseMesh = (basePreview?.meshes || []).find((mesh) => (
    !mesh.attachmentUvs?.length
    && (mesh.groups || []).some((group) => /face|head|(?:^|_)ears?(?:_|$)/i.test(group.material))
  ));
  if (!baseMesh?.positions?.length || !baseMesh?.uvs?.length) return 0;


  const frame = frameFor(baseMesh);
  if (!frame) return 0;
  const { normals, tangents, sourceVertexLookup } = frame;

  let projected = 0;
  for (const mesh of preview?.meshes || []) {
    if (!mesh.tangentOffsets?.length || !mesh.tangentAnchorIds?.length) continue;
    const positions = new Float32Array(mesh.tangentOffsets.length);
    let matched = 0;
    for (let vertex = 0; vertex < mesh.tangentAnchorIds.length; vertex += 1) {
      const match = sourceVertexLookup.get(mesh.tangentAnchorIds[vertex]);
      if (match === undefined) continue;
      const offset = vertex * 3;
      const anchor = match * 3;
      const localX = mesh.tangentOffsets[offset] * 64;
      const localY = mesh.tangentOffsets[offset + 1] * 64;
      const localZ = mesh.tangentOffsets[offset + 2] * 64;
      const tangent = [tangents[anchor], tangents[anchor + 1], tangents[anchor + 2]];
      const normal = [normals[anchor], normals[anchor + 1], normals[anchor + 2]];
      const bitangent = [
        normal[1] * tangent[2] - normal[2] * tangent[1],
        normal[2] * tangent[0] - normal[0] * tangent[2],
        normal[0] * tangent[1] - normal[1] * tangent[0],
      ];
      const bitangentLength = Math.hypot(...bitangent) || 1;
      bitangent[0] /= bitangentLength;
      bitangent[1] /= bitangentLength;
      bitangent[2] /= bitangentLength;
      for (let axis = 0; axis < 3; axis += 1) {
        positions[offset + axis] = baseMesh.positions[anchor + axis]
          + tangent[axis] * localX
          + bitangent[axis] * localY
          + normal[axis] * localZ;
      }
      matched += 1;
    }
    if (!matched) continue;
    mesh.positions = positions;
    mesh.tangentProjected = true;
    mesh.tangentProjectionMode = 'anchor-tangent-frame';
    projected += matched;
  }
  return projected;
}




