const boundaryCache = new WeakMap();

// Converted head boundaries can differ slightly from the retained body edge.
// Match nearby physical vertices, never unrelated averages from the atlases.
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
      for (let i = group.start; i < group.start + group.count; i++) indices.add(mesh.indices[i]);
      vertices.set(target, indices);
    }
    for (const [target, indices] of vertices) for (const i of indices) {
      const position = Array.from(mesh.positions.slice(i * 3, i * 3 + 3));
      const uv = Array.from(mesh.uvs.slice(i * 2, i * 2 + 2));
      if (position.length !== 3 || uv.length !== 2 || [...position, ...uv].some(v => !Number.isFinite(v))) continue;
      low = Math.min(low, position[2]); high = Math.max(high, position[2]);
      (target === leftTarget ? left : right).push({ position, uv });
    }
  }
  const tolerance = Math.max((high - low) * 0.004, 0.001);
  const cell = position => position.map(value => Math.floor(value / tolerance));
  const grid = new Map();
  for (const point of right) {
    const key = cell(point.position).join(',');
    const points = grid.get(key) || []; points.push(point); grid.set(key, points);
  }
  const pairs = [], used = new Set();
  for (const point of left) {
    const [x, y, z] = cell(point.position);
    let nearest = null, distance = tolerance * tolerance;
    for (let a = -1; a <= 1; a++) for (let b = -1; b <= 1; b++) for (let c = -1; c <= 1; c++) {
      for (const candidate of grid.get(`${x + a},${y + b},${z + c}`) || []) {
        const squared = point.position.reduce((sum, value, axis) => sum + (value - candidate.position[axis]) ** 2, 0);
        if (squared <= distance) { nearest = candidate; distance = squared; }
      }
    }
    if (!nearest) continue;
    const identity = point.position.map(v => Math.round(v / (tolerance * 0.1))).join(',');
    if (used.has(identity)) continue;
    used.add(identity);
    pairs.push({ left: point.uv, right: nearest.uv, leftPosition: point.position,
      rightPosition: nearest.position, distance: Math.sqrt(distance) });
  }
  cached.set(key, pairs);
  return pairs;
}

export function normalPacking(THREE, texture, pixels) {
  if (!texture) return null;
  if (texture.format === THREE.RED_GREEN_RGTC2_Format) return 'xy';
  if (texture.format === THREE.SIGNED_RED_GREEN_RGTC2_Format) return 'signed-xy';
  if (![THREE.RGB_BPTC_UNSIGNED_Format, THREE.RGBA_BPTC_Format].includes(texture.format) || !pixels?.length) return null;
  // 2K BC6H and legacy BC7 face normals can store XY only; Min/Max are source bounds, not
  // color-range compression. Confirm the omitted channel from the DDS pixels.
  for (let i = 2; i < pixels.length; i += 4) if (pixels[i] !== 0) return null;
  return 'xy';
}

export function configureNormalPacking(THREE, material, packing) {
  material.userData.normalPacking = packing;
  if (!packing) return;
  const baseCompile = material.onBeforeCompile;
  const baseKey = material.customProgramCacheKey();
  material.onBeforeCompile = shader => {
    baseCompile.call(material, shader);
    shader.fragmentShader = shader.fragmentShader.replace('#include <normalmap_pars_fragment>', `
      #include <normalmap_pars_fragment>
      vec3 decodeGameNormal(vec3 sampleValue) {
        vec2 xy = ${packing === 'signed-xy' ? 'sampleValue.xy' : 'sampleValue.xy * 2.0 - 1.0'};
        return vec3(xy, sqrt(max(0.0, 1.0 - dot(xy, xy))));
      }
    `).replace('#include <normal_fragment_maps>', THREE.ShaderChunk.normal_fragment_maps.replaceAll(
      'texture2D( normalMap, vNormalMapUv ).xyz * 2.0 - 1.0',
      'decodeGameNormal(texture2D( normalMap, vNormalMapUv ).xyz)',
    ));
  };
  material.customProgramCacheKey = () => `${baseKey}:nba2k-normal-${packing}-v1`;
  material.needsUpdate = true;
}
