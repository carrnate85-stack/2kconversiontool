import assert from 'node:assert/strict';
import { studioLightBalance, STUDIO_ENVIRONMENT, seamSmoothedNormals, vertexTangents, cardTextureUvs, configureEyeIris, configureSkinShading, configureTearFilm } from '../web/renderer/studio-shading.mjs';

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

// Card UVs (each quad covers a strand) beat an attachment layout that pins each card to one face spot.
const cardIndices = new Uint32Array([0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7]);
const cardUvs = new Float32Array([0, 0, 0.1, 0, 0.1, 0.5, 0, 0.5, 0.2, 0, 0.3, 0, 0.3, 0.5, 0.2, 0.5]);
const attachmentLayout = new Float32Array([0.10, 0.10, 0.101, 0.10, 0.101, 0.102, 0.10, 0.102, 0.90, 0.90, 0.901, 0.90, 0.901, 0.902, 0.90, 0.902]);
assert.equal(cardTextureUvs(attachmentLayout, cardUvs, cardIndices), cardUvs, 'Swapped decoder labels are corrected');
assert.equal(cardTextureUvs(cardUvs, attachmentLayout, cardIndices), cardUvs, 'Correct labels are kept');
assert.equal(cardTextureUvs(cardUvs, null, cardIndices), cardUvs, 'Meshes without a second set are untouched');
assert.equal(cardTextureUvs(cardUvs, cardUvs.map(value => value * 0.8), cardIndices), cardUvs, 'Similar sets need a clear margin to switch');

// The tear line becomes a translucent film; the caruncle and other parts are untouched.
const tearLine = { name: 'teartube_shader', color: { setRGB(...rgb) { this.rgb = rgb; } }, transparent: false, opacity: 1, depthWrite: true, roughness: 0.7, metalness: 0.03, userData: {} };
assert.equal(configureTearFilm(tearLine), true);
assert.ok(tearLine.transparent && tearLine.opacity < 0.5 && !tearLine.depthWrite && tearLine.roughness < 0.2 && tearLine.metalness === 0);
for (const name of ['teartubecaruncle_shader', 'mouth_shader', 'face_shader']) {
  const other = { name, transparent: false, opacity: 1, depthWrite: true, userData: {} };
  assert.equal(configureTearFilm(other), false);
  assert.ok(!other.transparent && other.opacity === 1 && other.depthWrite, `${name} stays opaque`);
}

console.log('Studio shading: light balance, seam/crease normals, tangent handedness and repair, chained iris uniforms and skin scatter passed.');
