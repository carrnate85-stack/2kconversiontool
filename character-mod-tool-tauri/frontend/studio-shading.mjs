// Preview-only studio shading: environment, surface frames, eyes and skin.
// Shader hooks chain onto any previous onBeforeCompile, like the strand fixes.

// Image-based light replaces most of the flat ambient/hemisphere fill. The
// remaining fill keeps unlit (environment-free) cavities from going black.
export const STUDIO_ENVIRONMENT = Object.freeze({ intensity: 0.62, ambientScale: 0.22, hemisphereScale: 0.38 });

export function studioLightBalance(profile = {}) {
  return {
    ambient: Number(profile.ambient || 0) * STUDIO_ENVIRONMENT.ambientScale,
    hemisphere: Number(profile.hemisphere || 0) * STUDIO_ENVIRONMENT.hemisphereScale,
    environmentIntensity: STUDIO_ENVIRONMENT.intensity,
  };
}

// Softboxes follow configureStudioLights directions (Y-up scene space), so
// reflections line up with the key/fill/rim highlights.
const SOFTBOXES = [
  { direction: [1.5, 2.3, 2], color: [1, 0.97, 0.94], intensity: 7, size: [3.2, 2.4] },
  { direction: [-2.1, 1.15, 1.3], color: [0.8, 0.87, 1], intensity: 2.4, size: [3.6, 2.6] },
  { direction: [-1.4, 1.7, -2.2], color: [1, 0.93, 0.88], intensity: 4.2, size: [1.4, 3.4] },
  { direction: [0, 1, 0.05], color: [1, 1, 1], intensity: 1.6, size: [5, 5] },
];

export function createStudioEnvironment(THREE, renderer) {
  const scene = new THREE.Scene();
  const disposables = [];
  const room = new THREE.SphereGeometry(10, 48, 24);
  const colors = new Float32Array(room.attributes.position.count * 3);
  // A dark dome keeps reflections as distinct softbox catchlights; a bright
  // uniform dome reads as a milky veil over eyes and flattens skin detail.
  const top = new THREE.Color(0.11, 0.12, 0.14), horizon = new THREE.Color(0.05, 0.05, 0.055), floor = new THREE.Color(0.02, 0.018, 0.016);
  const color = new THREE.Color();
  for (let index = 0; index < room.attributes.position.count; index++) {
    const height = room.attributes.position.getY(index) / 10;
    if (height >= 0) color.copy(horizon).lerp(top, Math.pow(height, 0.7));
    else color.copy(horizon).lerp(floor, Math.min(1, -height * 3));
    color.toArray(colors, index * 3);
  }
  room.setAttribute('color', new THREE.BufferAttribute(colors, 3));
  const roomMaterial = new THREE.MeshBasicMaterial({ vertexColors: true, side: THREE.BackSide, toneMapped: false });
  scene.add(new THREE.Mesh(room, roomMaterial));
  disposables.push(room, roomMaterial);
  for (const box of SOFTBOXES) {
    const geometry = new THREE.PlaneGeometry(box.size[0], box.size[1]);
    const material = new THREE.MeshBasicMaterial({ color: new THREE.Color(...box.color).multiplyScalar(box.intensity), side: THREE.DoubleSide, toneMapped: false });
    const panel = new THREE.Mesh(geometry, material);
    panel.position.set(...box.direction).normalize().multiplyScalar(7.5);
    panel.lookAt(0, 0, 0);
    scene.add(panel);
    disposables.push(geometry, material);
  }
  const generator = new THREE.PMREMGenerator(renderer);
  try {
    return generator.fromScene(scene, 0.035);
  } finally {
    generator.dispose();
    disposables.forEach((item) => item.dispose());
  }
}

// Vertices split along UV/material seams get the same smooth normal when
// they coincide and face within creaseAngleDegrees; hard edges stay hard.
// Each input is { positions, indices }; returns one Float32Array per mesh.
export function seamSmoothedNormals(meshes, { creaseAngleDegrees = 60, tolerance = null } = {}) {
  const sums = meshes.map(({ positions, indices }) => {
    const sum = new Float32Array(positions.length);
    for (let cursor = 0; cursor + 2 < indices.length; cursor += 3) {
      const a = indices[cursor] * 3, b = indices[cursor + 1] * 3, c = indices[cursor + 2] * 3;
      const abx = positions[b] - positions[a], aby = positions[b + 1] - positions[a + 1], abz = positions[b + 2] - positions[a + 2];
      const acx = positions[c] - positions[a], acy = positions[c + 1] - positions[a + 1], acz = positions[c + 2] - positions[a + 2];
      // The unnormalised cross product weights each face by its area.
      const nx = aby * acz - abz * acy, ny = abz * acx - abx * acz, nz = abx * acy - aby * acx;
      sum[a] += nx; sum[a + 1] += ny; sum[a + 2] += nz;
      sum[b] += nx; sum[b + 1] += ny; sum[b + 2] += nz;
      sum[c] += nx; sum[c + 1] += ny; sum[c + 2] += nz;
    }
    return sum;
  });
  const units = sums.map((sum) => {
    const unit = new Float32Array(sum.length);
    for (let offset = 0; offset < sum.length; offset += 3) {
      const length = Math.hypot(sum[offset], sum[offset + 1], sum[offset + 2]);
      if (length > 0) { unit[offset] = sum[offset] / length; unit[offset + 1] = sum[offset + 1] / length; unit[offset + 2] = sum[offset + 2] / length; }
    }
    return unit;
  });
  let step = tolerance;
  if (!(step > 0)) {
    let min = [Infinity, Infinity, Infinity], max = [-Infinity, -Infinity, -Infinity];
    for (const { positions } of meshes) for (let index = 0; index < positions.length; index += 3) {
      for (let axis = 0; axis < 3; axis++) {
        min[axis] = Math.min(min[axis], positions[index + axis]);
        max[axis] = Math.max(max[axis], positions[index + axis]);
      }
    }
    const diagonal = Math.hypot(max[0] - min[0], max[1] - min[1], max[2] - min[2]);
    step = Number.isFinite(diagonal) && diagonal > 0 ? diagonal * 1e-6 : 1e-6;
  }
  // Numeric spatial hash; buckets hold [meshIndex, vertexOffset] pairs.
  const buckets = new Map();
  const quantized = meshes.map(({ positions }) => {
    const cells = new Int32Array(positions.length);
    for (let index = 0; index < positions.length; index++) cells[index] = Math.round(positions[index] / step);
    return cells;
  });
  quantized.forEach((cells, meshIndex) => {
    for (let offset = 0; offset < cells.length; offset += 3) {
      const key = (Math.imul(cells[offset], 73856093) ^ Math.imul(cells[offset + 1], 19349663) ^ Math.imul(cells[offset + 2], 83492791)) >>> 0;
      let bucket = buckets.get(key);
      if (!bucket) buckets.set(key, bucket = []);
      bucket.push(meshIndex, offset);
    }
  });
  const crease = Math.cos(creaseAngleDegrees * Math.PI / 180);
  // Unshared vertices (most of them) keep their own unit normal.
  const results = units.map((unit) => unit.slice());
  for (const bucket of buckets.values()) {
    if (bucket.length === 2) continue;
    for (let left = 0; left < bucket.length; left += 2) {
      const meshIndex = bucket[left], offset = bucket[left + 1];
      const own = units[meshIndex];
      if (own[offset] === 0 && own[offset + 1] === 0 && own[offset + 2] === 0) continue;
      let x = 0, y = 0, z = 0;
      const cells = quantized[meshIndex];
      for (let right = 0; right < bucket.length; right += 2) {
        const otherMesh = bucket[right], otherOffset = bucket[right + 1];
        const otherCells = quantized[otherMesh], otherUnit = units[otherMesh];
        if (otherCells[otherOffset] !== cells[offset] || otherCells[otherOffset + 1] !== cells[offset + 1]
          || otherCells[otherOffset + 2] !== cells[offset + 2]) continue;
        // Zero (unused) normals fail this test as well.
        if (otherUnit[otherOffset] * own[offset] + otherUnit[otherOffset + 1] * own[offset + 1]
          + otherUnit[otherOffset + 2] * own[offset + 2] < crease) continue;
        const other = sums[otherMesh];
        x += other[otherOffset]; y += other[otherOffset + 1]; z += other[otherOffset + 2];
      }
      const length = Math.hypot(x, y, z) || 1;
      results[meshIndex][offset] = x / length; results[meshIndex][offset + 1] = y / length; results[meshIndex][offset + 2] = z / length;
    }
  }
  return results;
}

// Tangent-space hair and beards carry two UV sets: the shared strand-atlas UVs
// (color, alpha, tangent) and a unique per-card lightmap layout. The decoder
// labels the wider-spread set `uvs`, which for these meshes is the lightmap
// grid, so cards sampled tiny solid patches of the atlas and rendered as flat
// flakes. Atlas UVs give each triangle a real share of the strand artwork;
// lightmap cells are tiny. Pick the set whose median triangle UV area is
// clearly larger; the other set is the lightmap UV.
export function cardTextureUvs(uvs, alternateUvs, indices, margin = 4) {
  if (!uvs?.length || alternateUvs?.length !== uvs.length || !indices?.length) return uvs;
  const medianArea = (values) => {
    const triangles = Math.floor(indices.length / 3);
    const step = Math.max(1, Math.floor(triangles / 4096));
    const areas = [];
    for (let triangle = 0; triangle < triangles; triangle += step) {
      const a = indices[triangle * 3] * 2, b = indices[triangle * 3 + 1] * 2, c = indices[triangle * 3 + 2] * 2;
      areas.push(Math.abs((values[b] - values[a]) * (values[c + 1] - values[a + 1])
        - (values[c] - values[a]) * (values[b + 1] - values[a + 1])) / 2);
    }
    areas.sort((left, right) => left - right);
    return areas[Math.floor(areas.length / 2)] || 0;
  };
  return medianArea(alternateUvs) > medianArea(uvs) * margin ? alternateUvs : uvs;
}

// Per-vertex tangents (xyz + handedness w) from UV gradients. Degenerate UV
// triangles leave no gradient, so those vertices get any tangent orthogonal
// to the normal rather than a zero vector that would shade as NaN.
export function vertexTangents(positions, normals, uvs, indices, target = new Float32Array(positions.length / 3 * 4)) {
  const vertexCount = positions.length / 3;
  const tan1 = new Float32Array(vertexCount * 3), tan2 = new Float32Array(vertexCount * 3);
  for (let cursor = 0; cursor + 2 < indices.length; cursor += 3) {
    const a = indices[cursor], b = indices[cursor + 1], c = indices[cursor + 2];
    const e1x = positions[b * 3] - positions[a * 3], e1y = positions[b * 3 + 1] - positions[a * 3 + 1], e1z = positions[b * 3 + 2] - positions[a * 3 + 2];
    const e2x = positions[c * 3] - positions[a * 3], e2y = positions[c * 3 + 1] - positions[a * 3 + 1], e2z = positions[c * 3 + 2] - positions[a * 3 + 2];
    const s1 = uvs[b * 2] - uvs[a * 2], t1 = uvs[b * 2 + 1] - uvs[a * 2 + 1];
    const s2 = uvs[c * 2] - uvs[a * 2], t2 = uvs[c * 2 + 1] - uvs[a * 2 + 1];
    const determinant = s1 * t2 - s2 * t1;
    if (!Number.isFinite(determinant) || Math.abs(determinant) < 1e-20) continue;
    const r = 1 / determinant;
    const sx = (e1x * t2 - e2x * t1) * r, sy = (e1y * t2 - e2y * t1) * r, sz = (e1z * t2 - e2z * t1) * r;
    const tx = (e2x * s1 - e1x * s2) * r, ty = (e2y * s1 - e1y * s2) * r, tz = (e2z * s1 - e1z * s2) * r;
    for (let corner = 0; corner < 3; corner++) {
      const vertex = (corner === 0 ? a : corner === 1 ? b : c) * 3;
      tan1[vertex] += sx; tan1[vertex + 1] += sy; tan1[vertex + 2] += sz;
      tan2[vertex] += tx; tan2[vertex + 1] += ty; tan2[vertex + 2] += tz;
    }
  }
  for (let vertex = 0; vertex < vertexCount; vertex++) {
    const nx = normals[vertex * 3], ny = normals[vertex * 3 + 1], nz = normals[vertex * 3 + 2];
    let x = tan1[vertex * 3], y = tan1[vertex * 3 + 1], z = tan1[vertex * 3 + 2];
    const along = nx * x + ny * y + nz * z;
    x -= nx * along; y -= ny * along; z -= nz * along;
    let length = Math.hypot(x, y, z);
    if (!(length > 1e-12)) {
      // Any axis that is not parallel to the normal yields a valid frame.
      const ax = Math.abs(nx) < 0.9 ? 1 : 0, ay = ax ? 0 : 1;
      x = ay * nz; y = -ax * nz; z = ax * ny - ay * nx;
      length = Math.hypot(x, y, z) || 1;
    }
    x /= length; y /= length; z /= length;
    const cx = ny * z - nz * y, cy = nz * x - nx * z, cz = nx * y - ny * x;
    const handedness = cx * tan2[vertex * 3] + cy * tan2[vertex * 3 + 1] + cz * tan2[vertex * 3 + 2] < 0 ? -1 : 1;
    target[vertex * 4] = x; target[vertex * 4 + 1] = y; target[vertex * 4 + 2] = z; target[vertex * 4 + 3] = handedness;
  }
  return target;
}

// The decoded eye textures carry the sclera only, so the iris is drawn around
// the cornea, which sits at the centre of the eyeball's 0-1 UVs.
// Color and size are uniforms: one program for all players.
// The wet cornea should show small, crisp catchlights. A rough clearcoat
// smeared the key softbox into a white band across the iris.
export const EYE_SURFACE = Object.freeze({ environmentIntensity: 0.3, clearcoatRoughness: 0.05 });

// Runs after the material profile, which resets clearcoat roughness.
export function configureEyeSurface(material) {
  if (!material.userData.eyeIris) return;
  material.envMapIntensity = EYE_SURFACE.environmentIntensity;
  if ('clearcoatRoughness' in material) material.clearcoatRoughness = EYE_SURFACE.clearcoatRoughness;
}

export function configureEyeIris(material, enabled, color, scale) {
  material.userData.eyeIris = Boolean(enabled && material.map);
  if (!material.userData.eyeIris) return;
  const uniforms = material.userData.eyeIrisUniforms ||= { nbaIrisColor: { value: null }, nbaIrisScale: { value: 0.54 } };
  uniforms.nbaIrisColor.value = color;
  uniforms.nbaIrisScale.value = scale;
  const previous = material.onBeforeCompile;
  const previousKey = material.customProgramCacheKey();
  material.onBeforeCompile = (shader) => {
    previous(shader);
    Object.assign(shader.uniforms, uniforms);
    shader.fragmentShader = 'uniform vec3 nbaIrisColor;\nuniform float nbaIrisScale;\n' + shader.fragmentShader;
    // Runs after map/range decoding (which may have replaced map_fragment).
    shader.fragmentShader = shader.fragmentShader.replace('#include <color_fragment>', `#ifdef USE_MAP
  vec2 nbaEyeOffset = vMapUv - vec2( 0.5 );
  float nbaIrisDistance = length( nbaEyeOffset ) / nbaIrisScale;
  float nbaIrisMask = 1.0 - smoothstep( 0.185, 0.255, nbaIrisDistance );
  float nbaPupilMask = 1.0 - smoothstep( 0.052, 0.085, nbaIrisDistance );
  float nbaIrisAngle = atan( nbaEyeOffset.y, nbaEyeOffset.x );
  float nbaIrisRadial = clamp( ( nbaIrisDistance - 0.07 ) / 0.16, 0.0, 1.0 );
  float nbaIrisFibers = 0.88 + 0.07 * cos( nbaIrisAngle * 28.0 + sin( nbaIrisAngle * 7.0 ) * 1.7 )
    + 0.05 * cos( nbaIrisAngle * 61.0 + nbaIrisRadial * 9.0 );
  float nbaLimbalRing = smoothstep( 0.15, 0.225, nbaIrisDistance );
  float nbaCollarette = 1.0 - smoothstep( 0.0, 0.05, abs( nbaIrisDistance - 0.11 ) );
  vec3 nbaIris = nbaIrisColor * nbaIrisFibers * mix( 1.08, 0.5, nbaLimbalRing ) * ( 1.0 + 0.18 * nbaCollarette );
  diffuseColor.rgb = mix( diffuseColor.rgb, nbaIris, nbaIrisMask );
  diffuseColor.rgb = mix( diffuseColor.rgb, vec3( 0.012 ), nbaPupilMask );
#endif
#include <color_fragment>`);
  };
  material.customProgramCacheKey = () => previousKey + ':nba-eye-iris-v4';
}

// The teartube mesh is the wet tear line along the lower lid; drawn opaque with
// a placeholder flesh color it reads as a red rim. The caruncle stays opaque.
export const TEAR_FILM = Object.freeze({ opacity: 0.18, roughness: 0.06, color: [0.95, 0.9, 0.9] });

export function configureTearFilm(material) {
  const film = /(?:^|_)teartube(?:_|$)/i.test(String(material.name || ''));
  material.userData.tearFilm = film;
  if (!film) return false;
  material.color?.setRGB?.(...TEAR_FILM.color);
  material.transparent = true;
  material.opacity = TEAR_FILM.opacity;
  material.depthWrite = false;
  if ('roughness' in material) material.roughness = TEAR_FILM.roughness;
  if ('metalness' in material) material.metalness = 0;
  return true;
}

// Eyewear (goggles, glasses) color maps carry an opacity mask: frame opaque,
// lens transparent. A flat material opacity either hid the eyes behind solid
// lenses or, multiplied with the mask, erased the lenses. Use the mask with a
// minimum lens opacity and a glossy surface so lenses read as clear plastic.
export const EYEWEAR_LENS = Object.freeze({ minimumOpacity: 0.22, roughness: 0.12 });

export function configureEyewearLens(material, enabled) {
  material.userData.eyewearLens = Boolean(enabled && material.map);
  if (!material.userData.eyewearLens) return false;
  material.transparent = true;
  material.opacity = 1;
  material.depthWrite = true;
  if ('roughness' in material) material.roughness = EYEWEAR_LENS.roughness;
  if ('metalness' in material) material.metalness = 0;
  const uniforms = material.userData.eyewearLensUniforms ||= { nbaLensMinimumOpacity: { value: EYEWEAR_LENS.minimumOpacity } };
  const previous = material.onBeforeCompile;
  const previousKey = material.customProgramCacheKey();
  material.onBeforeCompile = (shader) => {
    previous(shader);
    Object.assign(shader.uniforms, uniforms);
    shader.fragmentShader = 'uniform float nbaLensMinimumOpacity;\n' + shader.fragmentShader;
    shader.fragmentShader = shader.fragmentShader.replace('#include <alphamap_fragment>',
      'diffuseColor.a = max( diffuseColor.a, nbaLensMinimumOpacity );\n#include <alphamap_fragment>');
  };
  material.customProgramCacheKey = () => previousKey + ':nba-eyewear-lens-v1';
  return true;
}

// Skin: sheen for the peach-fuzz rim, a lower dielectric F0, and wrapped,
// red-shifted diffuse past the terminator as a cheap subsurface stand-in.
export const SKIN_SHADING = Object.freeze({
  sheen: 0.2, sheenRoughness: 0.55, sheenColor: [0.75, 0.55, 0.48],
  specularIntensity: 0.6, scatterWrap: 0.45, scatterColor: [0.62, 0.2, 0.12],
});

export function configureSkinShading(material, enabled) {
  material.userData.skinScatter = Boolean(enabled && 'sheen' in material);
  if (!material.userData.skinScatter) return;
  material.sheen = SKIN_SHADING.sheen;
  material.sheenRoughness = SKIN_SHADING.sheenRoughness;
  material.sheenColor?.setRGB?.(...SKIN_SHADING.sheenColor);
  material.specularIntensity = SKIN_SHADING.specularIntensity;
  const uniforms = material.userData.skinScatterUniforms ||= {
    nbaSkinScatterColor: { value: new Float32Array(SKIN_SHADING.scatterColor) },
    nbaSkinScatterWrap: { value: SKIN_SHADING.scatterWrap },
  };
  const previous = material.onBeforeCompile;
  const previousKey = material.customProgramCacheKey();
  material.onBeforeCompile = (shader) => {
    previous(shader);
    Object.assign(shader.uniforms, uniforms);
    shader.fragmentShader = 'uniform vec3 nbaSkinScatterColor;\nuniform float nbaSkinScatterWrap;\n' + shader.fragmentShader;
    shader.fragmentShader = shader.fragmentShader.replace('#include <lights_physical_pars_fragment>', `#include <lights_physical_pars_fragment>
void RE_Direct_NbaSkin( const in IncidentLight directLight, const in vec3 geometryPosition, const in vec3 geometryNormal, const in vec3 geometryViewDir, const in vec3 geometryClearcoatNormal, const in PhysicalMaterial material, inout ReflectedLight reflectedLight ) {
  RE_Direct_Physical( directLight, geometryPosition, geometryNormal, geometryViewDir, geometryClearcoatNormal, material, reflectedLight );
  float nbaSkinNdl = dot( geometryNormal, directLight.direction );
  float nbaSkinWrapped = saturate( ( nbaSkinNdl + nbaSkinScatterWrap ) / ( 1.0 + nbaSkinScatterWrap ) );
  float nbaSkinBand = max( nbaSkinWrapped - saturate( nbaSkinNdl ), 0.0 );
  reflectedLight.directDiffuse += directLight.color * nbaSkinBand * nbaSkinScatterColor * BRDF_Lambert( material.diffuseColor );
}
#undef RE_Direct
#define RE_Direct RE_Direct_NbaSkin`);
  };
  material.customProgramCacheKey = () => previousKey + ':nba-skin-scatter-v1';
}

// Per-player hair/beard color from the FxTweakables color record. 2K multiplies
// the strand albedo by it; (0,0,0) means no tint is set, so it reads as absent.
export function strandTint(value) {
  if (!Array.isArray(value) || value.length !== 3) return null;
  if (!value.every((channel) => Number.isFinite(channel) && channel >= 0 && channel <= 1)) return null;
  return value.some((channel) => channel > 0) ? value.slice() : null;
}

// The male body's torso normal maps (fpglobal, every muscle type) carry the
// opposite green (bitangent) direction to its chest maps, which left a zigzag
// seam where the two materials meet. The mesh's own TANGENTFRAME0 frames match
// the UV-derived ones on every body material, so the flip is in the torso maps
// themselves. Child bodies share the male layout and take the same maps. The
// flip follows the map's source (native prep labels it "body normals
// (char/fpglobal.iff)"), not the player: female_skin torso maps match their
// chest maps, and flipping them creates the seam.
export function flipsBodyNormalGreen(targetName, textureSource) {
  return targetName === 'torso' && /fpglobal/i.test(String(textureSource || ''));
}

// A player's face_normal_o is only 512x512; the pores and fine wrinkles live in
// the 2048x2048 face_detailnormal_o over the same UVs. Its XY is added to the
// base normal's in tangent space (scaled like the base), after three's normal
// chunk so the wrapper's normal-packing hook still finds that chunk.
export const DETAIL_NORMAL = Object.freeze({ strength: 0.6 });

export function configureDetailNormal(material, texture, strength = DETAIL_NORMAL.strength) {
  material.userData.detailNormal = texture?.name || null;
  if (!texture || !material.normalMap) return false;
  const uniforms = material.userData.detailNormalUniforms ||= {
    nbaDetailNormalMap: { value: null },
    nbaDetailNormalStrength: { value: strength },
  };
  uniforms.nbaDetailNormalMap.value = texture;
  uniforms.nbaDetailNormalStrength.value = strength;
  const previous = material.onBeforeCompile;
  const previousKey = material.customProgramCacheKey();
  material.onBeforeCompile = (shader) => {
    previous(shader);
    Object.assign(shader.uniforms, uniforms);
    shader.fragmentShader = 'uniform sampler2D nbaDetailNormalMap;\nuniform float nbaDetailNormalStrength;\n' + shader.fragmentShader;
    shader.fragmentShader = shader.fragmentShader.replace('#include <normal_fragment_maps>', `#include <normal_fragment_maps>
#ifdef USE_NORMALMAP_TANGENTSPACE
  vec2 nbaDetailXY = texture2D( nbaDetailNormalMap, vNormalMapUv ).xy * 2.0 - 1.0;
  normal = normalize( tbn * vec3( mapN.xy + nbaDetailXY * normalScale * nbaDetailNormalStrength, mapN.z ) );
#endif`);
  };
  material.customProgramCacheKey = () => `${previousKey}:nba-detail-normal-v1`;
  return true;
}

// Preview garments are fitted at their vertices, so skin still bulges through
// between them (a female player's bust through the jersey); the game hides body
// faces under garments instead. Body skin within coverRadius of a garment is
// drawn as if pushed `depth` further from the camera while that garment shows:
// depth only, so its screen position and shading are unchanged and nothing opens
// a hole. A per-vertex cover weight (jersey/shorts, footwear) keeps uncovered
// skin, such as the arms, where it is.
export const GARMENT_LAYERING = Object.freeze({ depth: 2.5, coverRadius: 6 });

// Preview garments arrive as 'preview-jersey' companions; shoes and socks are
// told apart by their source file (shoes_*, *_sock_*).
export function previewGarmentKind(item) {
  if (item?.sourceKind !== 'preview-jersey') return null;
  const leaf = String(item.sourceName || '').split(/[\\/]/).pop().toLowerCase();
  return /^(?:shoes_|(?:male_|female_|child_)?sock_)/.test(leaf) ? 'footwear' : 'jersey';
}

// Per body vertex: 1 when a jersey/shorts (x) or footwear (y) vertex lies within
// coverRadius. Positions are model-space xyz arrays.
export function garmentCoverWeights(bodyPositions, garments, radius = GARMENT_LAYERING.coverRadius) {
  const cover = new Float32Array(bodyPositions.length / 3 * 2);
  const grids = [new Map(), new Map()];
  // Numeric cell keys (cells offset into 0..2047 per axis): string keys made this
  // the slowest step of the load.
  const cellKey = (cx, cy, cz) => ((cx + 1024) * 2048 + (cy + 1024)) * 2048 + (cz + 1024);
  const key = (x, y, z) => cellKey(Math.floor(x / radius), Math.floor(y / radius), Math.floor(z / radius));
  for (const { kind, positions } of garments) {
    const grid = grids[kind === 'footwear' ? 1 : 0];
    for (let i = 0; i < positions.length; i += 3) {
      const cell = key(positions[i], positions[i + 1], positions[i + 2]);
      let bucket = grid.get(cell);
      if (!bucket) grid.set(cell, bucket = []);
      bucket.push(positions[i], positions[i + 1], positions[i + 2]);
    }
  }
  const squared = radius * radius;
  for (let v = 0; v < bodyPositions.length / 3; v++) {
    const x = bodyPositions[v * 3], y = bodyPositions[v * 3 + 1], z = bodyPositions[v * 3 + 2];
    const cx = Math.floor(x / radius), cy = Math.floor(y / radius), cz = Math.floor(z / radius);
    grids.forEach((grid, channel) => {
      if (!grid.size) return;
      for (let dx = -1; dx <= 1 && !cover[v * 2 + channel]; dx++) for (let dy = -1; dy <= 1 && !cover[v * 2 + channel]; dy++) for (let dz = -1; dz <= 1; dz++) {
        const bucket = grid.get(cellKey(cx + dx, cy + dy, cz + dz));
        if (!bucket) continue;
        let hit = false;
        for (let i = 0; i < bucket.length; i += 3) {
          const ex = bucket[i] - x, ey = bucket[i + 1] - y, ez = bucket[i + 2] - z;
          if (ex * ex + ey * ey + ez * ez <= squared) { hit = true; break; }
        }
        if (hit) { cover[v * 2 + channel] = 1; break; }
      }
    });
  }
  return cover;
}

// `uniform.value` is a vec2 of depths (jersey/shorts, footwear), shared by every
// body material so toggling garments never recompiles; geometry supplies the
// per-vertex nbaGarmentCover weights (missing means uncovered).
export function configureGarmentLayering(material, uniform, enabled) {
  material.userData.garmentLayering = Boolean(enabled && uniform);
  if (!material.userData.garmentLayering) return false;
  const previous = material.onBeforeCompile;
  const previousKey = material.customProgramCacheKey();
  material.onBeforeCompile = (shader) => {
    previous(shader);
    shader.uniforms.nbaGarmentDepth = uniform;
    shader.vertexShader = 'uniform vec2 nbaGarmentDepth;\nattribute vec2 nbaGarmentCover;\n' + shader.vertexShader.replace('#include <project_vertex>', `#include <project_vertex>
float nbaGarmentPush = dot( nbaGarmentCover, nbaGarmentDepth );
if ( nbaGarmentPush > 0.0 ) {
  vec4 nbaBehindGarment = projectionMatrix * ( mvPosition + vec4( 0.0, 0.0, - nbaGarmentPush, 0.0 ) );
  gl_Position.z = nbaBehindGarment.z / nbaBehindGarment.w * gl_Position.w;
}`);
  };
  material.customProgramCacheKey = () => `${previousKey}:nba-garment-layering-v2`;
  return true;
}

// Clothing "color" maps such as the generic shoe's are RGB region masks the game
// fills from a colorway (red upper, green laces and outsole edge, blue midsole on
// shoes_twokgeneric). Each channel's weight picks its colorway color; linear.
export const SHOE_COLORWAY = Object.freeze({ red: [0.8, 0.8, 0.79], green: [0.74, 0.74, 0.73], blue: [0.03, 0.03, 0.032] });

export function configureColorwayMask(material, colorway) {
  material.userData.colorwayMask = Boolean(colorway && material.map);
  if (!material.userData.colorwayMask) return false;
  const color = (rgb) => `vec3( ${rgb.map((value) => value.toFixed(4)).join(', ')} )`;
  const previous = material.onBeforeCompile;
  const previousKey = material.customProgramCacheKey();
  material.onBeforeCompile = (shader) => {
    previous(shader);
    // After the map step (which range decoding rewrites), before vertex colors.
    shader.fragmentShader = shader.fragmentShader.replace('#include <color_fragment>', `#ifdef USE_MAP
  float nbaMaskSum = diffuseColor.r + diffuseColor.g + diffuseColor.b;
  diffuseColor.rgb = nbaMaskSum > 0.05
    ? ( diffuseColor.r * ${color(colorway.red)} + diffuseColor.g * ${color(colorway.green)} + diffuseColor.b * ${color(colorway.blue)} ) / nbaMaskSum
    : ${color(colorway.red)};
#endif
#include <color_fragment>`);
  };
  material.customProgramCacheKey = () => `${previousKey}:nba-colorway-v1`;
  return true;
}

// Body bent-normal maps (and the player's face_bentnormal_o) carry ambient
// occlusion in alpha: dark in the armpits, under the jaw, around the eyes and
// nostrils. Bound as the aoMap, alpha is read in place of red. Its TXTR Min/Max
// are bounds, not range compression (the chest's darkest raw alpha, 171/255, is
// its Min of 0.67), so it is read raw. three applies the aoMap to ambient and
// environment light only: contact shadows that leave the key light alone.
export function configureOcclusionAlpha(material, enabled, aomapChunk) {
  const sample = 'texture2D( aoMap, vAoMapUv ).r';
  material.userData.occlusionAlpha = Boolean(enabled && material.aoMap && aomapChunk?.includes(sample));
  if (!material.userData.occlusionAlpha) return false;
  const chunk = aomapChunk.replace(sample, 'texture2D( aoMap, vAoMapUv ).a');
  const previous = material.onBeforeCompile;
  const previousKey = material.customProgramCacheKey();
  material.onBeforeCompile = (shader) => {
    previous(shader);
    shader.fragmentShader = shader.fragmentShader.replace('#include <aomap_fragment>', chunk);
  };
  material.customProgramCacheKey = () => `${previousKey}:nba-occlusion-alpha-v1`;
  return true;
}

// A female player's scanned chest color is black only where the sports-bra cups
// go, so the cut-out fill alone drew two gray ovals. The band extends it into a
// bandeau around the torso: from a little below the cups to their top (z is up),
// within 1.6x the cups' half-width so the arms stay bare. Returns
// [bottom, top, centerX, halfWidth] for the fill shader, or null without cups.
export const GARMENT_BAND = Object.freeze({ below: 3, above: 0.5, widthScale: 1.6, edge: 0.6, minimumCupVertices: 20 });

export function garmentBandBounds(positions, cups, band = GARMENT_BAND) {
  if (cups.length < band.minimumCupVertices) return null;
  let low = Infinity, high = -Infinity, centerX = 0;
  for (const v of cups) {
    low = Math.min(low, positions[v * 3 + 2]);
    high = Math.max(high, positions[v * 3 + 2]);
    centerX += positions[v * 3];
  }
  centerX /= cups.length;
  let halfWidth = 0;
  for (const v of cups) halfWidth = Math.max(halfWidth, Math.abs(positions[v * 3] - centerX));
  return [low - band.below, high + band.above, centerX, halfWidth * band.widthScale];
}
