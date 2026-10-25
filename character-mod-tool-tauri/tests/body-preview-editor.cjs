const assert = require('node:assert/strict');
const path = require('node:path');
const { pathToFileURL } = require('node:url');

(async () => {
  const root = path.resolve(__dirname, '..');
  const THREE = await import(pathToFileURL(path.resolve(root, '../character-mod-tool-webview2/Reference/src/renderer/vendor/three.module.min.js')));
  const { createBodyEditor } = await import(pathToFileURL(path.join(root, 'web/renderer/body-preview-editor.mjs')));
  const rings = (heights, radiusX, radiusY) => heights.flatMap(z => Array.from({ length: 8 }, (_, index) => {
    const angle = index * Math.PI / 4;
    return [Math.cos(angle) * radiusX, Math.sin(angle) * radiusY, z];
  }));
  const triangles = ringCount => {
    const result = [];
    for (let ring = 0; ring < ringCount - 1; ring += 1) for (let index = 0; index < 8; index += 1) {
      const next = (index + 1) % 8, a = ring * 8 + index, b = ring * 8 + next;
      const c = (ring + 1) * 8 + index, d = (ring + 1) * 8 + next;
      result.push(a, b, c, b, d, c);
    }
    return result;
  };
  const makeMesh = (positions, indices) => {
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute('position', new THREE.BufferAttribute(Float32Array.from(positions.flat()), 3));
    geometry.setIndex(indices);
    return new THREE.Mesh(geometry, new THREE.MeshBasicMaterial());
  };
  const bodyHeights = [0, 20, 40, 60, 70, 80, 100];
  const jerseyHeights = [58, 70, 82];
  const body = makeMesh(rings(bodyHeights, 10, 6), triangles(bodyHeights.length));
  const jersey = makeMesh(rings(jerseyHeights, 11, 7), triangles(jerseyHeights.length));
  const morphStride = jersey.geometry.getAttribute('position').array.length;
  const fitMorphDeltas = new Float32Array(morphStride * 2);
  for (let vertex = 0; vertex < morphStride / 3; vertex += 1) {
    fitMorphDeltas[vertex * 3] = 0.25;
    fitMorphDeltas[morphStride + vertex * 3 + 2] = -0.125;
  }
  const modelRoot = new THREE.Group(); modelRoot.add(body, jersey);
  const context = {
    THREE, modelRoot, renderer: { render() {} }, scene: {}, camera: {}, invalidateShadows() {},
    materialTextureTarget: () => 'chest',
    typed(value, Type) { return value ? (value instanceof Type ? value : new Type(value)) : new Type(); },
  };
  const editor = createBodyEditor(context);
  const bodyFit = { chestTHIN: 0, chestTHICK: 0, 'Spine2.tx': 0, 'Spine2.ty': 0, 'Spine2.tz': 0 };
  editor.initialize({ player: { bodyFit }, meshes: [
    { sourceKind: 'main', groups: [{ material: 'chest_shader', start: 0, count: body.geometry.index.count }] },
    { sourceKind: 'preview-jersey', groups: [{ material: 'jersey', start: 0, count: jersey.geometry.index.count }],
      fitMorphNames: ['chestTHICK', 'chestTHIN'], fitMorphDeltas },
  ] });
  const diagnostics = editor.diagnostics();
  assert.equal(diagnostics.method, 'native-jersey-morphs');
  assert.equal(diagnostics.nativeMorphTargets, 2);
  assert.equal(diagnostics.coverage, 1);
  const bodyBase = Float32Array.from(body.geometry.getAttribute('position').array);
  const jerseyBase = Float32Array.from(jersey.geometry.getAttribute('position').array);
  editor.setBodyFit({ ...bodyFit, 'Spine2.tx': 1.5 });
  const bodyAfter = body.geometry.getAttribute('position').array;
  const jerseyAfter = jersey.geometry.getAttribute('position').array;
  let checked = 0;
  for (let index = 8; index < 16; index += 1) {
    const bodyIndex = (bodyHeights.indexOf(70) * 8 + (index - 8)) * 3;
    const jerseyIndex = index * 3;
    const bodyDelta = [0, 1, 2].map(axis => bodyAfter[bodyIndex + axis] - bodyBase[bodyIndex + axis]);
    const jerseyDelta = [0, 1, 2].map(axis => jerseyAfter[jerseyIndex + axis] - jerseyBase[jerseyIndex + axis]);
    assert(Math.hypot(...bodyDelta) > 0.1, 'Fixture body did not deform');
    assert(Math.hypot(...bodyDelta.map((value, axis) => value - jerseyDelta[axis])) < 0.12,
      `Jersey did not follow body displacement: ${bodyDelta} versus ${jerseyDelta}`);
    checked += 1;
  }
  assert.equal(checked, 8);
  assert(editor.diagnostics().clearance.maximumClearanceDrift < 0.00001);
  editor.setBodyFit(bodyFit);
  editor.setBodyFit({ ...bodyFit, chestTHICK: 1 });
  assert.deepEqual(Array.from(body.geometry.getAttribute('position').array), Array.from(bodyBase),
    'THICK changed the body instead of jersey clearance');
  const thickJersey = jersey.geometry.getAttribute('position').array;
  const maximumJerseyDelta = Math.max(...thickJersey.map((value, index) => Math.abs(value - jerseyBase[index])));
  assert(maximumJerseyDelta > 0.05, 'THICK did not change jersey clearance');
  for (let vertex = 0; vertex < thickJersey.length / 3; vertex += 1) {
    assert(Math.abs(thickJersey[vertex * 3] - jerseyBase[vertex * 3] - 0.25) < 0.00001,
      'THICK did not apply the authored morph delta');
  }
  assert(editor.diagnostics().clearance.maximumClearanceDrift > 0.001,
    'THICK preserved clearance instead of changing garment fit');
  editor.setBodyFit({ ...bodyFit, chestTHICK: 1.5, chestTHIN: -0.5 });
  const combinedJersey = jersey.geometry.getAttribute('position').array;
  for (let vertex = 0; vertex < combinedJersey.length / 3; vertex += 1) {
    assert(Math.abs(combinedJersey[vertex * 3] - jerseyBase[vertex * 3] - 0.375) < 0.00001,
      'THICK did not retain its independent weight');
    assert(Math.abs(combinedJersey[vertex * 3 + 2] - jerseyBase[vertex * 3 + 2] - 0.0625) < 0.00001,
      'Negative THIN did not retain its independent weight');
  }
  assert.equal(editor.diagnostics().appliedMorphTargets, 2);
  editor.setBodyFit(bodyFit);
  assert.deepEqual(Array.from(body.geometry.getAttribute('position').array), Array.from(bodyBase));
  assert.deepEqual(Array.from(jersey.geometry.getAttribute('position').array), Array.from(jerseyBase));

  const fallbackBody = makeMesh(rings(bodyHeights, 10, 6), triangles(bodyHeights.length));
  const fallbackJersey = makeMesh(rings(jerseyHeights, 11, 7), triangles(jerseyHeights.length));
  const fallbackRoot = new THREE.Group(); fallbackRoot.add(fallbackBody, fallbackJersey);
  const fallbackEditor = createBodyEditor({ ...context, modelRoot: fallbackRoot });
  const fallbackBase = Float32Array.from(fallbackJersey.geometry.getAttribute('position').array);
  fallbackEditor.initialize({ player: { bodyFit }, meshes: [
    { sourceKind: 'main', groups: [{ material: 'chest_shader', start: 0, count: fallbackBody.geometry.index.count }] },
    { sourceKind: 'preview-jersey', groups: [{ material: 'jersey', start: 0, count: fallbackJersey.geometry.index.count }] },
  ] });
  fallbackEditor.setBodyFit({ ...bodyFit, chestTHICK: 1 });
  assert.equal(fallbackEditor.diagnostics().method, 'body-surface-follow-fallback');
  assert(Math.max(...fallbackJersey.geometry.getAttribute('position').array.map((value, index) => Math.abs(value - fallbackBase[index]))) > 0.05,
    'Morph-free garments lost the regional fallback');
  console.log(JSON.stringify({ checks: [
    'surface binding coverage',
    'dimension displacement follows the body',
    'native thick and thin morphs apply independently without changing the body',
    'negative and above-one native weights remain supported',
    'regional fallback remains available for morph-free garments',
    'baseline restoration',
  ], diagnostics }, null, 2));
})().catch(error => { console.error(error); process.exitCode = 1; });
