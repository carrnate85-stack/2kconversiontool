import assert from 'node:assert/strict';
import { studioLightBalance, STUDIO_ENVIRONMENT, seamSmoothedNormals, vertexTangents, cardTextureUvs, configureEyeIris, configureSkinShading, configureTearFilm, configureEyewearLens, EYEWEAR_LENS, configureEyeSurface, EYE_SURFACE, strandTint, flipsBodyNormalGreen, configureDetailNormal, configureGarmentLayering, GARMENT_LAYERING, previewGarmentKind, garmentCoverWeights, configureColorwayMask, SHOE_COLORWAY, configureOcclusionAlpha } from '../web/renderer/studio-shading.mjs';

const balance = studioLightBalance({ ambient: 0.4, hemisphere: 0.8 });
assert.ok(Math.abs(balance.ambient - 0.4 * STUDIO_ENVIRONMENT.ambientScale) < 1e-12);
assert.ok(Math.abs(balance.hemisphere - 0.8 * STUDIO_ENVIRONMENT.hemisphereScale) < 1e-12);
assert.equal(balance.environmentIntensity, STUDIO_ENVIRONMENT.intensity);

// Two quads meeting at x=0 with split (duplicated) seam vertices, tilted 40
// degrees apart: the seam is smoothed. A 90-degree fold stays a hard edge.
function hinge(angleDegrees) {
  const angle = angleDegrees * Math.PI / 180;
  const left = { positions: new Float32Array([-1, 0, 0, 0, 0, 0, 0, 1, 0, -1, 1, 0]), indices: new Uint32Array([0, 1, 2, 0, 2, 3]) };
  const x = Math.cos(angle), z = -Math.sin(angle);
  const right = { positions: new Float32Array([0, 0, 0, x, 0, z, x, 1, z, 0, 1, 0]), indices: new Uint32Array([0, 1, 2, 0, 2, 3]) };
  return seamSmoothedNormals([left, right]);
}
const smooth = hinge(40);
const seamLeft = Array.from(smooth[0].slice(3, 6)), seamRight = Array.from(smooth[1].slice(0, 3));
seamLeft.forEach((value, axis) => assert.ok(Math.abs(value - seamRight[axis]) < 1e-6, 'Split seam vertices share one normal'));
assert.ok(Math.abs(Math.hypot(...seamLeft) - 1) < 1e-6);
assert.ok(seamLeft[0] > 0.1, 'Seam normal leans between both faces');
assert.deepEqual(Array.from(smooth[0].slice(0, 3)).map(Math.round), [0, 0, 1], 'Non-seam vertex keeps its face normal');
const hard = hinge(90);
assert.deepEqual(Array.from(hard[0].slice(3, 6)).map(value => Math.round(value * 1e6) / 1e6), [0, 0, 1], 'Crease above threshold stays hard');
assert.deepEqual(Array.from(hard[1].slice(0, 3)).map(value => Math.round(value * 1e6) / 1e6), [1, 0, 0]);

// Unused vertices keep a zero normal rather than NaN.
const unused = seamSmoothedNormals([{ positions: new Float32Array([0, 0, 0, 1, 0, 0, 0, 1, 0, 5, 5, 5]), indices: new Uint32Array([0, 1, 2]) }]);
assert.deepEqual(Array.from(unused[0].slice(9)), [0, 0, 0]);

// Tangents follow +U, with handedness flipping on a mirrored UV layout.
const positions = new Float32Array([0, 0, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0]);
const normals = new Float32Array([0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1]);
const indices = new Uint32Array([0, 1, 2, 0, 2, 3]);
const straight = vertexTangents(positions, normals, new Float32Array([0, 0, 1, 0, 1, 1, 0, 1]), indices);
assert.deepEqual(Array.from(straight.slice(0, 4)), [1, 0, 0, 1]);
const mirrored = vertexTangents(positions, normals, new Float32Array([1, 0, 0, 0, 0, 1, 1, 1]), indices);
assert.deepEqual(Array.from(mirrored.slice(0, 4)), [-1, 0, 0, -1]);
const degenerate = vertexTangents(positions, normals, new Float32Array(8), indices);
for (let vertex = 0; vertex < 4; vertex++) {
  const tangent = degenerate.slice(vertex * 4, vertex * 4 + 3);
  assert.ok(Math.abs(Math.hypot(...tangent) - 1) < 1e-6, 'Degenerate UVs still get a unit tangent');
  assert.ok(Math.abs(tangent[2]) < 1e-6, 'Repaired tangent is orthogonal to the normal');
}

function hookMaterial(extra = {}) {
  return { map: {}, userData: {}, onBeforeCompile(shader) { shader.basePreserved = true; }, customProgramCacheKey: () => 'range-base', ...extra };
}
// The iris must chain onto range decoding (which replaces map_fragment).
const eye = hookMaterial();
const color = { r: 0.1, g: 0.2, b: 0.3 };
configureEyeIris(eye, true, color, 0.46);
const eyeShader = { uniforms: {}, fragmentShader: 'void main() {\n#include <color_fragment>\n}' };
eye.onBeforeCompile(eyeShader);
assert.equal(eyeShader.basePreserved, true, 'Iris keeps the texture-range hook');
assert.equal(eyeShader.uniforms.nbaIrisColor.value, color);
assert.equal(eyeShader.uniforms.nbaIrisScale.value, 0.46);
assert.ok(eyeShader.fragmentShader.indexOf('nbaIrisMask') < eyeShader.fragmentShader.indexOf('#include <color_fragment>'));
assert.equal(eye.customProgramCacheKey(), 'range-base:nba-eye-iris-v4', 'Iris color is a uniform, not part of the program key');
const uniforms = eye.userData.eyeIrisUniforms;
eye.onBeforeCompile = () => {}; eye.customProgramCacheKey = () => '';
configureEyeIris(eye, true, { r: 1, g: 1, b: 1 }, 0.64);
assert.equal(eye.userData.eyeIrisUniforms, uniforms);
assert.equal(uniforms.nbaIrisScale.value, 0.64);
const untextured = hookMaterial({ map: null });
configureEyeIris(untextured, true, color, 0.5);
assert.equal(untextured.userData.eyeIris, false);
assert.equal(untextured.customProgramCacheKey(), 'range-base');

const skin = hookMaterial({ sheen: 0, sheenRoughness: 1, specularIntensity: 1, sheenColor: { setRGB(...rgb) { this.rgb = rgb; } } });
configureSkinShading(skin, true);
assert.ok(skin.sheen > 0 && skin.specularIntensity < 1 && skin.sheenColor.rgb.length === 3);
const skinShader = { uniforms: {}, fragmentShader: '#include <lights_physical_pars_fragment>\n#include <lights_fragment_begin>' };
skin.onBeforeCompile(skinShader);
assert.equal(skinShader.basePreserved, true);
assert.ok(skinShader.fragmentShader.includes('#define RE_Direct RE_Direct_NbaSkin'));
assert.ok(skinShader.fragmentShader.indexOf('RE_Direct_NbaSkin') > skinShader.fragmentShader.indexOf('#include <lights_physical_pars_fragment>'));
assert.equal(skin.customProgramCacheKey(), 'range-base:nba-skin-scatter-v1');
const standard = hookMaterial();
configureSkinShading(standard, true);
assert.equal(standard.userData.skinScatter, false, 'Standard materials (no sheen) are left unchanged');
assert.equal(standard.customProgramCacheKey(), 'range-base');

// The face detail normal adds its XY after three's normal chunk, which stays in
// place for the wrapper's packing hook; no base normal map means no detail.
const detailFace = hookMaterial(); detailFace.normalMap = { name: 'face_normal_o' };
assert.equal(configureDetailNormal(detailFace, { name: 'face_detailnormal_o' }), true);
const detailShader = { uniforms: {}, fragmentShader: '#include <normal_fragment_maps>\n#include <lights_fragment_begin>' };
detailFace.onBeforeCompile(detailShader);
assert.equal(detailShader.basePreserved, true);
assert.ok(detailShader.fragmentShader.includes('#include <normal_fragment_maps>'), 'Normal chunk kept for the packing hook');
assert.ok(detailShader.fragmentShader.indexOf('nbaDetailXY') > detailShader.fragmentShader.indexOf('#include <normal_fragment_maps>'));
assert.equal(detailShader.uniforms.nbaDetailNormalMap.value.name, 'face_detailnormal_o');
assert.equal(detailFace.customProgramCacheKey(), 'range-base:nba-detail-normal-v1');
const noBase = hookMaterial();
assert.equal(configureDetailNormal(noBase, { name: 'face_detailnormal_o' }), false);
assert.equal(noBase.customProgramCacheKey(), 'range-base');

// Garment layering pushes skin depth only, after three's projection, from one
// shared vec2 uniform (jersey, footwear) weighted by per-vertex cover; materials
// it doesn't apply to are untouched.
const layering = { value: { x: GARMENT_LAYERING.depth, y: GARMENT_LAYERING.depth } };
const skinUnderGarment = hookMaterial();
assert.equal(configureGarmentLayering(skinUnderGarment, layering, true), true);
const layeringShader = { uniforms: {}, vertexShader: '#include <project_vertex>\n#include <worldpos_vertex>', fragmentShader: '' };
skinUnderGarment.onBeforeCompile(layeringShader);
assert.equal(layeringShader.basePreserved, true);
assert.equal(layeringShader.uniforms.nbaGarmentDepth, layering, 'Shared uniform: toggling never recompiles');
assert.ok(layeringShader.vertexShader.indexOf('gl_Position.z =') > layeringShader.vertexShader.indexOf('#include <project_vertex>'));
assert.ok(!/gl_Position\.(?:xy|x|y)\s*=/.test(layeringShader.vertexShader), 'Only depth moves');
assert.ok(layeringShader.vertexShader.includes('dot( nbaGarmentCover, nbaGarmentDepth )'), 'Only covered skin is pushed');
assert.equal(skinUnderGarment.customProgramCacheKey(), 'range-base:nba-garment-layering-v2');
const face = hookMaterial();
assert.equal(configureGarmentLayering(face, layering, false), false);
assert.equal(face.customProgramCacheKey(), 'range-base');
const garment = (sourceName) => previewGarmentKind({ sourceKind: 'preview-jersey', sourceName });
assert.equal(garment('clothing/geo/dynamic/shoes_special_twokgeneric_geo.iff'), 'footwear');
assert.equal(garment('clothing/geo/simple/female_sock_crew_occluded_by_special_twokgeneric_geo.iff'), 'footwear');
assert.equal(garment('char/jersey_nike_wishbone.iff'), 'jersey');
assert.equal(garment('char/female_shorts_nike.iff'), 'jersey');
assert.equal(previewGarmentKind({ sourceKind: 'main', sourceName: 'png00110.iff' }), null);
// Torso skin next to the jersey is covered; an arm far from it is not, and a
// foot near footwear gets only the footwear channel.
const cover = garmentCoverWeights(new Float32Array([0, 0, 0, 40, 0, 0, 0, -90, 0]), [
  { kind: 'jersey', positions: new Float32Array([1, 2, 1]) },
  { kind: 'footwear', positions: new Float32Array([0, -88, 1]) },
]);
assert.deepEqual(Array.from(cover), [1, 0, 0, 0, 0, 1]);

// A colorway mask recolors after the map step (which range decoding rewrites)
// and before vertex colors; no map means nothing to recolor.
const shoe = hookMaterial();
assert.equal(configureColorwayMask(shoe, SHOE_COLORWAY), true);
const shoeShader = { uniforms: {}, fragmentShader: 'RANGE_DECODED_MAP\n#include <color_fragment>' };
shoe.onBeforeCompile(shoeShader);
assert.equal(shoeShader.basePreserved, true);
assert.ok(shoeShader.fragmentShader.indexOf('nbaMaskSum') < shoeShader.fragmentShader.indexOf('#include <color_fragment>'));
assert.ok(shoeShader.fragmentShader.includes(SHOE_COLORWAY.blue.map((value) => value.toFixed(4)).join(', ')));
assert.equal(shoe.customProgramCacheKey(), 'range-base:nba-colorway-v1');
const bare = hookMaterial({ map: null });
assert.equal(configureColorwayMask(bare, SHOE_COLORWAY), false);
assert.equal(bare.customProgramCacheKey(), 'range-base');

// Bent-normal occlusion reads the aoMap's alpha in place of red, raw.
const aoChunk = 'float ambientOcclusion = ( texture2D( aoMap, vAoMapUv ).r - 1.0 ) * aoMapIntensity + 1.0;';
const bodyAo = hookMaterial({ aoMap: { name: 'chest_bentnormal' } });
assert.equal(configureOcclusionAlpha(bodyAo, true, aoChunk), true);
const aoShader = { uniforms: {}, fragmentShader: '#include <aomap_fragment>' };
bodyAo.onBeforeCompile(aoShader);
assert.equal(aoShader.basePreserved, true);
assert.ok(aoShader.fragmentShader.includes('texture2D( aoMap, vAoMapUv ).a') && !aoShader.fragmentShader.includes('.r -'));
assert.equal(bodyAo.customProgramCacheKey(), 'range-base:nba-occlusion-alpha-v1');
const plainAo = hookMaterial({ aoMap: { name: 'legs_occlusion' } });
assert.equal(configureOcclusionAlpha(plainAo, false, aoChunk), false, 'Dedicated occlusion maps keep red');
assert.equal(configureOcclusionAlpha(hookMaterial(), true, aoChunk), false, 'No aoMap, nothing to read');

// Card UVs (each quad covers a strand) beat an attachment layout that pins each card to one face spot.
const cardIndices = new Uint32Array([0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7]);
const cardUvs = new Float32Array([0, 0, 0.1, 0, 0.1, 0.5, 0, 0.5, 0.2, 0, 0.3, 0, 0.3, 0.5, 0.2, 0.5]);
const attachmentLayout = new Float32Array([0.10, 0.10, 0.101, 0.10, 0.101, 0.102, 0.10, 0.102, 0.90, 0.90, 0.901, 0.90, 0.901, 0.902, 0.90, 0.902]);
assert.equal(cardTextureUvs(attachmentLayout, cardUvs, cardIndices), cardUvs, 'Swapped decoder labels are corrected');
assert.equal(cardTextureUvs(cardUvs, attachmentLayout, cardIndices), cardUvs, 'Correct labels are kept');
assert.equal(cardTextureUvs(cardUvs, null, cardIndices), cardUvs, 'Meshes without a second set are untouched');
assert.equal(cardTextureUvs(cardUvs, cardUvs.map(value => value * 0.8), cardIndices), cardUvs, 'Similar sets need a clear margin to switch');

// Eye surface: crisp catchlights only on iris-configured eyes.
const cornea = { userData: { eyeIris: true }, envMapIntensity: 1, clearcoatRoughness: 0.16 };
configureEyeSurface(cornea);
assert.equal(cornea.envMapIntensity, EYE_SURFACE.environmentIntensity);
assert.equal(cornea.clearcoatRoughness, EYE_SURFACE.clearcoatRoughness);
const notEye = { userData: {}, envMapIntensity: 1, clearcoatRoughness: 0.16 };
configureEyeSurface(notEye);
assert.equal(notEye.envMapIntensity, 1);

// Eyewear keeps its texture mask with a minimum lens opacity, chained after earlier hooks.
const goggles = hookMaterial({ transparent: false, opacity: 0.28, depthWrite: false, roughness: 0.72, metalness: 0.03 });
assert.equal(configureEyewearLens(goggles, true), true);
assert.ok(goggles.transparent && goggles.opacity === 1 && goggles.depthWrite && goggles.roughness < 0.2 && goggles.metalness === 0);
const lensShader = { uniforms: {}, fragmentShader: '#include <color_fragment>\n#include <alphamap_fragment>' };
goggles.onBeforeCompile(lensShader);
assert.equal(lensShader.basePreserved, true);
assert.ok(lensShader.fragmentShader.indexOf('max( diffuseColor.a, nbaLensMinimumOpacity )') < lensShader.fragmentShader.indexOf('#include <alphamap_fragment>'));
assert.equal(lensShader.uniforms.nbaLensMinimumOpacity.value, EYEWEAR_LENS.minimumOpacity);
assert.equal(goggles.customProgramCacheKey(), 'range-base:nba-eyewear-lens-v1');
const plainLens = hookMaterial({ map: null });
assert.equal(configureEyewearLens(plainLens, true), false, 'Untextured eyewear is left alone');

// The tear line becomes a translucent film; the caruncle and other parts are untouched.
const tearLine = { name: 'teartube_shader', color: { setRGB(...rgb) { this.rgb = rgb; } }, transparent: false, opacity: 1, depthWrite: true, roughness: 0.7, metalness: 0.03, userData: {} };
assert.equal(configureTearFilm(tearLine), true);
assert.ok(tearLine.transparent && tearLine.opacity < 0.5 && !tearLine.depthWrite && tearLine.roughness < 0.2 && tearLine.metalness === 0);
for (const name of ['teartubecaruncle_shader', 'mouth_shader', 'face_shader']) {
  const other = { name, transparent: false, opacity: 1, depthWrite: true, userData: {} };
  assert.equal(configureTearFilm(other), false);
  assert.ok(!other.transparent && other.opacity === 1 && other.depthWrite, `${name} stays opaque`);
}

assert.deepEqual(strandTint([0.8039, 0.451, 0.2431]), [0.8039, 0.451, 0.2431]);
assert.deepEqual(strandTint([1, 1, 1]), [1, 1, 1], 'Neutral tint passes through');
for (const unset of [null, undefined, [0, 0, 0], [0.5, 0.5], [0.5, NaN, 0.5], [1.2, 0.5, 0.5], [-0.1, 0.5, 0.5], 'brown']) {
  assert.equal(strandTint(unset), null, `${JSON.stringify(unset)} reads as no tint`);
}

assert.equal(flipsBodyNormalGreen('torso', 'body normals (char/fpglobal.iff)'), true, 'Male/child maps from fpglobal');
assert.equal(flipsBodyNormalGreen('torso', 'body normals (char/female_skin.iff)'), false, 'Female torso maps already match the chest');
assert.equal(flipsBodyNormalGreen('chest', 'body normals (char/fpglobal.iff)'), false);
assert.equal(flipsBodyNormalGreen('torso', undefined), false, 'Unknown sources are left alone');

console.log('Studio shading: light balance, seam/crease normals, tangent handedness and repair, chained iris uniforms, skin scatter, strand tint, torso normal flip, face detail normal, garment layering, shoe colorway and occlusion alpha passed.');
