// Strand lightmaps use their own unique-per-card UV layout, not the shared
// strand atlas UVs. uvAttribute names the geometry attribute holding it; null
// means the mesh has none, and sampling with atlas UVs would scatter dark cells.
export function configureStrandLightmap(material, texture, uvAttribute = 'uv') {
  material.userData.strandLightmap = uvAttribute ? texture?.name || null : null;
  material.userData.strandLightmapSkipped = Boolean(texture && material.map && !uvAttribute);
  if (!texture || !material.map || !uvAttribute) return;
  texture.updateMatrix();
  const range = texture.userData?.range;
  const uniforms = material.userData.strandLightmapUniforms ||= {
    nbaStrandLightmap: { value: null },
    nbaStrandLightTransform: { value: null },
    nbaStrandLightMin: { value: new Float32Array(4) },
    nbaStrandLightMax: { value: new Float32Array(4) },
    nbaStrandLightMask: { value: new Float32Array(4) },
  };
  uniforms.nbaStrandLightmap.value = texture;
  uniforms.nbaStrandLightTransform.value = texture.matrix;
  uniforms.nbaStrandLightMin.value.set(range?.min || [0, 0, 0, 0]);
  uniforms.nbaStrandLightMax.value.set(range?.max || [1, 1, 1, 1]);
  uniforms.nbaStrandLightMask.value.set(range?.mask || [0, 0, 0, 0]);
  const previous = material.onBeforeCompile;
  const previousKey = material.customProgramCacheKey();
  material.onBeforeCompile = shader => {
    previous(shader);
    Object.assign(shader.uniforms, uniforms);
    shader.vertexShader = (uvAttribute === 'uv' ? '' : `attribute vec2 ${uvAttribute};\n`)
      + 'uniform mat3 nbaStrandLightTransform; varying vec2 vNbaStrandLightUv;\n' + shader.vertexShader;
    shader.vertexShader = shader.vertexShader.replace('#include <uv_vertex>',
      `#include <uv_vertex>\nvNbaStrandLightUv = (nbaStrandLightTransform * vec3(${uvAttribute}, 1.0)).xy;`);
    shader.fragmentShader = `uniform sampler2D nbaStrandLightmap;
uniform vec4 nbaStrandLightMin, nbaStrandLightMax, nbaStrandLightMask;
varying vec2 vNbaStrandLightUv;
` + shader.fragmentShader;
    shader.fragmentShader = shader.fragmentShader.replace('#include <color_fragment>', `#include <color_fragment>
vec4 nbaStrandLightSample = texture2D(nbaStrandLightmap, vNbaStrandLightUv);
vec4 nbaStrandLightDecoded = nbaStrandLightMin + nbaStrandLightSample * (nbaStrandLightMax - nbaStrandLightMin);
vec3 nbaStrandResponse = mix(nbaStrandLightSample, nbaStrandLightDecoded, nbaStrandLightMask).rgb;
nbaStrandResponse = clamp(nbaStrandResponse, 0.0, 1.0);`);
    shader.fragmentShader = shader.fragmentShader.replace('#include <lights_fragment_end>', `#include <lights_fragment_end>
reflectedLight.directDiffuse *= nbaStrandResponse;
reflectedLight.indirectDiffuse *= nbaStrandResponse;
reflectedLight.directSpecular *= nbaStrandResponse;
reflectedLight.indirectSpecular *= nbaStrandResponse;`);
  };
  material.customProgramCacheKey = () => previousKey + ':nba-strand-lightmap-v2' + (uvAttribute === 'uv' ? '' : ':' + uvAttribute);
}

export function configureStrandTangentRange(material, texture, physicalChunk) {
  const range = texture?.userData?.range;
  material.userData.strandTangentRange = Boolean(range);
  if (!range || !material.anisotropyMap || !physicalChunk) return;
  const uniforms = material.userData.strandTangentUniforms ||= {
    nbaStrandTangentMin: { value: new Float32Array(4) },
    nbaStrandTangentMax: { value: new Float32Array(4) },
    nbaStrandTangentMask: { value: new Float32Array(4) },
  };
  uniforms.nbaStrandTangentMin.value.set(range.min);
  uniforms.nbaStrandTangentMax.value.set(range.max);
  uniforms.nbaStrandTangentMask.value.set(range.mask);
  const previous = material.onBeforeCompile;
  const previousKey = material.customProgramCacheKey();
  const sample = 'vec3 anisotropyPolar = texture2D( anisotropyMap, vAnisotropyMapUv ).rgb;';
  if (!physicalChunk.includes(sample)) throw new Error('Strand tangent shader hook changed.');
  const decodedChunk = physicalChunk.replace(sample, `vec3 nbaStrandTangentSample = texture2D( anisotropyMap, vAnisotropyMapUv ).rgb;
vec3 nbaStrandTangentDecoded = nbaStrandTangentMin.rgb + nbaStrandTangentSample * (nbaStrandTangentMax.rgb - nbaStrandTangentMin.rgb);
vec3 anisotropyPolar = mix(nbaStrandTangentSample, nbaStrandTangentDecoded, nbaStrandTangentMask.rgb);`);
  material.onBeforeCompile = shader => {
    previous(shader);
    Object.assign(shader.uniforms, uniforms);
    shader.fragmentShader = 'uniform vec4 nbaStrandTangentMin, nbaStrandTangentMax, nbaStrandTangentMask;\n' + shader.fragmentShader;
    shader.fragmentShader = shader.fragmentShader.replace('#include <lights_physical_fragment>', decodedChunk);
  };
  material.customProgramCacheKey = () => previousKey + ':nba-strand-tangent-range-v1';
}

export function configureStrandSpecular(material, enabled) {
  material.userData.strandSpecularResponse = Boolean(enabled && material.map);
  if (!material.userData.strandSpecularResponse) return;
  const previous = material.onBeforeCompile;
  const previousKey = material.customProgramCacheKey();
  material.onBeforeCompile = shader => {
    previous(shader);
    // A fiber response keeps dark atlases from reflecting as white solid cards.
    shader.fragmentShader = shader.fragmentShader.replace('#include <lights_fragment_end>', `#include <lights_fragment_end>
vec3 nbaStrandSpecularTint = sqrt(clamp(diffuseColor.rgb, 0.0, 1.0));
reflectedLight.directSpecular *= nbaStrandSpecularTint;
reflectedLight.indirectSpecular *= nbaStrandSpecularTint;`);
  };
  material.customProgramCacheKey = () => previousKey + ':nba-strand-specular-v1';
}

export function configureStrandCoverage(material, enabled, multisampled) {
  material.alphaToCoverage = false;
  material.userData.strandAlphaMode = null;
  if (!enabled || !material.map || !(material.alphaTest > 0)) return;
  // Cutout depth represents covered strands, not a translucent card surface.
  material.transparent = false;
  material.depthWrite = true;
  material.forceSinglePass = true;
  material.alphaToCoverage = Boolean(multisampled);
  material.userData.strandAlphaMode = material.alphaToCoverage ? 'coverage' : 'cutout';
}
