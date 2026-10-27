// Before/after renderer harness: the baseline (old/) and working-tree (new/) explorer
// renderers load the same payload side by side. See ../README.md for URL parameters.
import { loadPlayer } from './payload.mjs';

const params = new URLSearchParams(location.search);
const log = (...args) => { document.getElementById('log').textContent += args.join(' ') + '\n'; console.log(...args); };

// Players come from decode-player.cjs (JSON plus a binary sidecar).
const playerPayload = (name) => loadPlayer(name);

// Without ?player=, a synthetic head: face/chest groups split by a duplicated seam
// ring, two eyes and generated textures. Needs no game files.
function base64(bytes) {
  let text = '';
  for (let i = 0; i < bytes.length; i += 0x8000) text += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(text);
}
function rgbaTexture(name, binding, format, pixel, size = 256) {
  const bytes = new Uint8Array(size * size * 4);
  for (let y = 0; y < size; y++) for (let x = 0; x < size; x++) {
    bytes.set(pixel(x / size, y / size).map((channel) => Math.max(0, Math.min(255, Math.round(channel * 255)))), (y * size + x) * 4);
  }
  return { name, source: name, previewRgba: true, width: size, height: size, format, bytes: base64(bytes), binding };
}
function sphere(name, material, rings, segments, place, uv, groups) {
  const positions = [], uvs = [], indices = [];
  const ring = (from, to) => {
    const start = positions.length / 3, indexStart = indices.length;
    for (let r = from; r <= to; r++) for (let s = 0; s <= segments; s++) {
      const lat = Math.PI * r / rings, lon = 2 * Math.PI * s / segments;
      positions.push(...place(Math.sin(lat) * Math.cos(lon), Math.sin(lat) * Math.sin(lon), Math.cos(lat)));
      uvs.push(...uv(s / segments, r / rings, Math.sin(lat) * Math.cos(lon), Math.cos(lat)));
    }
    for (let r = 0; r < to - from; r++) for (let s = 0; s < segments; s++) {
      const a = start + r * (segments + 1) + s, b = a + segments + 1;
      indices.push(a, b, a + 1, b, b + 1, a + 1);
    }
    return { start: indexStart, count: indices.length - indexStart };
  };
  const built = groups.map(([groupMaterial, from, to]) => ({ material: groupMaterial, ...ring(from, to) }));
  return { name, sourceKind: 'main', positions: new Float32Array(positions), uvs: new Float32Array(uvs), indices: new Uint32Array(indices), groups: built };
}
function syntheticPayload() {
  const noise = (x, y) => { const s = Math.sin(x * 127.1 + y * 311.7) * 43758.5453; return s - Math.floor(s); };
  const skin = (tone) => (u, v) => { const n = noise(Math.floor(u * 256), Math.floor(v * 256)) * 0.05; return [tone[0] + n, tone[1] + n * 0.8, tone[2] + n * 0.6, 1]; };
  const pores = (u, v) => [0.5 + Math.sin(u * 2 * Math.PI * 40) * 0.175, 0.5 + Math.sin(v * 2 * Math.PI * 30) * 0.175, 0.9, 1];
  const textures = [
    rgbaTexture('face_color.dds', { target: 'face', channel: 'color' }, 'RGBA8_UNORM_SRGB', skin([0.55, 0.38, 0.3])),
    rgbaTexture('chest_color.dds', { target: 'chest', channel: 'color' }, 'RGBA8_UNORM_SRGB', skin([0.53, 0.37, 0.29])),
    rgbaTexture('face_normal.dds', { target: 'face', channel: 'normal' }, 'RGBA8_UNORM', pores),
    rgbaTexture('chest_normal.dds', { target: 'chest', channel: 'normal' }, 'RGBA8_UNORM', pores),
    rgbaTexture('eyewhite_color_00.dds', { target: 'eye', channel: 'color' }, 'RGBA8_UNORM_SRGB', () => [0.86, 0.84, 0.8, 1]),
  ];
  // ?eyerange=1: a range-packed eye white; decoding tints it, skipping it leaves it white.
  if (params.get('eyerange')) textures[4].range = { min: [0.5, 0, 0, 0], max: [1, 0.3, 0.3, 1], mask: [1, 1, 1, 0] };
  const head = sphere('head', 'face_mat', 96, 128, (x, y, z) => [x, y * 1.08, z * 1.2], (u, v) => [u, v], [['face_mat', 0, 60], ['chest_mat', 60, 96]]);
  // Planar eye UVs centred on the forward (-Y) pole put the iris at (0.5, 0.5).
  const eye = (name, cx) => sphere(name, `${name}_mat`, 32, 48, (x, y, z) => [cx + x * 0.16, -0.86 + y * 0.16, 0.32 + z * 0.16],
    (u, v, x, z) => [0.5 + x * 0.5, 0.5 + z * 0.5], [[`${name}_mat`, 0, 32]]);
  return {
    name: 'synthetic head', player: { irisColor: params.get('iris') || 'hazel', irisSize: 'regular' }, stats: {},
    meshes: [head, eye('l_eye', -0.36), eye('r_eye', 0.36)],
    materials: ['face_mat', 'chest_mat', 'l_eye_mat', 'r_eye_mat'].map((name) => ({ name, shader: /eye/.test(name) ? 'eye.fx' : 'skin.fx' })),
    textures,
  };
}

async function run(version, source) {
  const { createExplorerRenderer } = await import(`./${version}/explorer/model-preview.js`);
  const fixes = await import('./skin-preview-fixes.mjs');
  // Same options and normal-map decoding as the app's model-preview.js wrapper.
  const context = createExplorerRenderer({ canvas: document.getElementById(`canvas-${version}`), viewer: version,
    skinBoundaryPairs: fixes.nearestSkinBoundaryPairs });
  const started = performance.now();
  await context.api.load(structuredClone(source));
  const { THREE } = context;
  context.modelRoot.traverse((object) => {
    if (params.get('jersey') === '0' && object.name === 'player') object.visible = false;
    for (const material of (Array.isArray(object.material) ? object.material : [object.material]).filter(Boolean)) {
      const texture = material.normalMap;
      const pixels = texture && [THREE.RGB_BPTC_UNSIGNED_Format, THREE.RGBA_BPTC_Format].includes(texture.format)
        ? context.decodedTexturePixels(texture)?.pixels : null;
      fixes.configureNormalPacking(THREE, material, fixes.normalPacking(THREE, texture, pixels));
    }
  });
  context.invalidateShadows();
  const yaw = Number(params.get('yaw') ?? 0.35), pitch = Number(params.get('pitch') ?? 0.12);
  const state = context.cameraState();
  context.setCameraState({ ...state, yaw, pitch, distance: state.fittedDistance * Number(params.get('zoom') ?? 0.72) });
  if (params.get('focus') === 'eyes') {
    // Aim at the eye material groups themselves; the head bounds sit on the body axis.
    const box = new THREE.Box3(), point = new THREE.Vector3();
    context.modelRoot.updateMatrixWorld(true);
    context.modelRoot.traverse((object) => {
      if (!object.isMesh || object.isInstancedMesh) return;
      const materials = Array.isArray(object.material) ? object.material : [object.material];
      const { index, attributes } = object.geometry;
      for (const group of object.geometry.groups) {
        if (!/(?:^|_)[lr]_eye(?:_|$)/i.test(materials[group.materialIndex]?.name || '')) continue;
        for (let cursor = group.start; cursor < group.start + group.count; cursor++) {
          box.expandByPoint(point.fromBufferAttribute(attributes.position, index ? index.getX(cursor) : cursor).applyMatrix4(object.matrixWorld));
        }
      }
    });
    if (!box.isEmpty()) {
      const size = box.getSize(new THREE.Vector3());
      context.setCameraState({ ...state, yaw, pitch, target: box.getCenter(new THREE.Vector3()).toArray(),
        distance: Math.max(size.x, size.y, size.z) * Number(params.get('eyeZoom') ?? 1.6) });
    }
  }
  if (params.get('focus') === 'head') {
    const box = new THREE.Box3().setFromObject(context.modelRoot);
    const height = box.max.y - box.min.y, center = box.getCenter(new THREE.Vector3());
    context.setCameraState({ ...state, yaw, pitch, target: [center.x, box.max.y - height * Number(params.get('headY') ?? 0.065), center.z],
      distance: height * Number(params.get('headZoom') ?? 0.22) });
  }
  context.renderer.render(context.scene, context.camera);
  const failed = context.renderer.info.programs.filter((program) => program.diagnostics && !program.diagnostics.runnable);
  log(version, 'loaded in', (performance.now() - started).toFixed(0), 'ms; programs', context.renderer.info.programs.length,
    'failed', failed.length, 'glError', context.renderer.getContext().getError(), 'environment', Boolean(context.scene.environment));
  window[`ctx_${version}`] = context;
}

// Fine-detail score for a canvas region: mean absolute Laplacian of luminance.
// region is [x, y, width, height] as fractions, origin bottom-left.
window.measureDetail = (version, region = [0.1, 0.15, 0.5, 0.4]) => {
  const context = window[`ctx_${version}`];
  context.renderer.render(context.scene, context.camera);
  const gl = context.renderer.getContext(), w = gl.drawingBufferWidth, h = gl.drawingBufferHeight;
  const [x0, y0, rw, rh] = [region[0] * w, region[1] * h, region[2] * w, region[3] * h].map(Math.floor);
  const pixels = new Uint8Array(rw * rh * 4);
  gl.readPixels(x0, y0, rw, rh, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
  const L = (x, y) => { const i = (y * rw + x) * 4; return 0.2126 * pixels[i] + 0.7152 * pixels[i + 1] + 0.0722 * pixels[i + 2]; };
  let detail = 0, mean = 0, n = 0;
  for (let y = 1; y < rh - 1; y++) for (let x = 1; x < rw - 1; x++) {
    detail += Math.abs(4 * L(x, y) - L(x - 1, y) - L(x + 1, y) - L(x, y - 1) - L(x, y + 1)); mean += L(x, y); n++;
  }
  return { detail: detail / n, meanLuminance: mean / n };
};

window.harnessReady = (async () => {
  try {
    const player = params.get('player');
    const source = player ? await playerPayload(player) : syntheticPayload();
    const only = params.get('only');
    if (only !== 'new') await run('old', source);
    if (only !== 'old') await run('new', source);
    // ?headshot=809 shows the game's own headshot (player-extract headshot) as a color reference.
    if (params.get('headshot')) {
      const { showHeadshot } = await import('./headshot.mjs');
      await showHeadshot(window.ctx_new || window.ctx_old, `players/headshot-${params.get('headshot')}.dds`);
    }
    log('done');
  } catch (error) { log('ERROR', error.stack || error); }
})();
