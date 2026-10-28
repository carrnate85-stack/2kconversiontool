const boundaryCache = new WeakMap();

// Loose and converted player head boundaries can differ slightly from the
// retained body edge. Match nearby physical vertices only; never average
// unrelated regions of the skin atlases.
export function nearestSkinBoundaryPairs(payload, leftTarget, rightTarget, materialTarget) {
  if (!payload || !['face:chest', 'chest:torso', 'torso:legs'].includes(`${leftTarget}:${rightTarget}`)) return [];
  const key = `${leftTarget}:${rightTarget}`;
  let cached = boundaryCache.get(payload);
  if (!cached) { cached = new Map(); boundaryCache.set(payload, cached); }
  if (cached.has(key)) return cached.get(key);
  const left = [], right = [];
  let low = Infinity, high = -Infinity;
  for (const mesh of payload.meshes || []) {
    if (mesh.sourceKind === 'preview-jersey' || !mesh.uvs?.length) continue;
    const vertices = new Map();
    for (const group of mesh.groups || []) {
      const target = materialTarget(group.material);
      if (target !== leftTarget && target !== rightTarget) continue;
      const indices = vertices.get(target) || new Set();
      for (let i = group.start; i < group.start + group.count; i += 1) indices.add(mesh.indices[i]);
      vertices.set(target, indices);
    }
    for (const [target, indices] of vertices) for (const index of indices) {
      const position = Array.from(mesh.positions.slice(index * 3, index * 3 + 3));
      const uv = Array.from(mesh.uvs.slice(index * 2, index * 2 + 2));
      if (position.length !== 3 || uv.length !== 2 || [...position, ...uv].some(value => !Number.isFinite(value))) continue;
      low = Math.min(low, position[2]); high = Math.max(high, position[2]);
      (target === leftTarget ? left : right).push({ position, uv });
    }
  }
  const tolerance = Math.max((high - low) * 0.004, 0.001);
  const cell = position => position.map(value => Math.floor(value / tolerance));
  const grid = new Map();
  for (const point of right) {
    const gridKey = cell(point.position).join(',');
    const points = grid.get(gridKey) || [];
    points.push(point);
    grid.set(gridKey, points);
  }
  const pairs = [], used = new Set();
  for (const point of left) {
    const [x, y, z] = cell(point.position);
    let nearest = null, distance = tolerance * tolerance;
    for (let a = -1; a <= 1; a += 1) for (let b = -1; b <= 1; b += 1) for (let c = -1; c <= 1; c += 1) {
      for (const candidate of grid.get(`${x + a},${y + b},${z + c}`) || []) {
        const squared = point.position.reduce((sum, value, axis) => sum + (value - candidate.position[axis]) ** 2, 0);
        if (squared <= distance) { nearest = candidate; distance = squared; }
      }
    }
    if (!nearest) continue;
    const identity = point.position.map(value => Math.round(value / (tolerance * 0.1))).join(',');
    if (used.has(identity)) continue;
    used.add(identity);
    pairs.push({
      left: point.uv,
      right: nearest.uv,
      leftPosition: point.position,
      rightPosition: nearest.position,
      distance: Math.sqrt(distance),
    });
  }
  cached.set(key, pairs);
  return pairs;
}
