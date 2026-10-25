import * as THREE from './vendor/three.module.min.js';
import { DDSLoader } from './vendor/DDSLoader.js';
import { materialProfile, previewLightingProfile } from './render-profiles.mjs';

const viewerParams = new URL(import.meta.url).searchParams;
const viewerGlobalName = viewerParams.get('global') || 'modelPreview';
const canvas = document.getElementById(viewerParams.get('canvas') || 'previewCanvas');
const transparentBackground = viewerParams.get('transparent') === '1';
const interactiveViewer = viewerParams.get('interactive') !== '0';
const pickerMode = viewerParams.get('picker') === '1';
const cameraBroadcast = viewerParams.get('broadcast') || '';
const cameraFollow = viewerParams.get('follow') || '';
const scene = new THREE.Scene();
scene.background = transparentBackground ? null : new THREE.Color(0x080b10);
scene.fog = new THREE.FogExp2(0x080b10, 0.00008);
const camera = new THREE.PerspectiveCamera(45, 1, 0.01, 1000000);
const renderer = new THREE.WebGLRenderer({ canvas, antialias: true, alpha: transparentBackground, powerPreference: 'high-performance' });
renderer.outputColorSpace = THREE.SRGBColorSpace;
renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, pickerMode ? 1.25 : 2));

const ambientLight = new THREE.AmbientLight(0xffffff, 1.0);
scene.add(ambientLight);
const hemisphereLight = new THREE.HemisphereLight(0xe8f2ff, 0x54463f, 0);
scene.add(hemisphereLight);
const keyLight = new THREE.DirectionalLight(0xfff8f2, 0.85);
keyLight.position.set(2, 4, 3);
scene.add(keyLight);
scene.add(keyLight.target);
const fillLight = new THREE.DirectionalLight(0xc9dcff, 0);
scene.add(fillLight, fillLight.target);
const rimLight = new THREE.DirectionalLight(0xff6a32, 0.22);
rimLight.position.set(-3, 1, -2);
scene.add(rimLight);
scene.add(rimLight.target);
const grid = new THREE.GridHelper(10000, 40, 0x4c5665, 0x20262f);
grid.material.opacity = 0.32;
grid.material.transparent = true;
grid.visible = !transparentBackground;
scene.add(grid);

const modelRoot = new THREE.Group();
// The decoder deliberately emits the workspace Blender import basis (Z-up).
// Three.js and its grid are Y-up, so rotate the complete placed scene once.
modelRoot.rotation.x = -Math.PI / 2;
scene.add(modelRoot);
const target = new THREE.Vector3();
const raycaster = new THREE.Raycaster();
const pointer = new THREE.Vector2();
let yaw = Math.PI / 4;
let pitch = Math.PI / 5;
let distance = 10;
let fittedDistance = 10;
let wireframe = false;
let eyelashesVisible = false;
let jerseyVisible = false;
let textureCache = [];
let textureItems = [];
let currentPayload = null;
let loadGeneration = 0;
let drag = null;
let selectionHelper = null;
const movementKeys = new Set();
let movementFrame = 0;
let movementTime = 0;
let cameraFollowFrame = 0;
let bodyFitContext = null;
let currentBodyFitValues = {};
let chestCorrectionEnabled = false;
let lastChestCorrection = null;
let studioLighting = true;
let clayMode = false;
let currentPreviewProfile = null;
let headOcclusionVisible = true;
let studioBounds = new THREE.Box3();

const BODY_FIT_REGIONS = {
  forearmupper: { low: 0.56, high: 0.70, kind: 'arm', amplitude: 0.07 },
  forearmlower: { low: 0.46, high: 0.61, kind: 'arm', amplitude: 0.07 },
  forearm: { low: 0.48, high: 0.68, kind: 'arm', amplitude: 0.075 },
  upperarm: { low: 0.61, high: 0.79, kind: 'arm', amplitude: 0.08 },
  trap: { low: 0.73, high: 0.84, kind: 'core', amplitude: 0.055 },
  neck: { low: 0.80, high: 0.90, kind: 'core', amplitude: 0.065 },
  chest: { low: 0.64, high: 0.78, kind: 'core', amplitude: 0.085 },
  breast: { low: 0.62, high: 0.76, kind: 'core', amplitude: 0.05 },
  abs: { low: 0.50, high: 0.66, kind: 'core', amplitude: 0.075 },
  head: { low: 0.87, high: 1.01, kind: 'core', amplitude: 0.04 },
  shoulder: { low: 0.68, high: 0.82, kind: 'shoulder', amplitude: 0.075 },
  hips: { low: 0.40, high: 0.55, kind: 'core', amplitude: 0.07 },
  glutes: { low: 0.39, high: 0.54, kind: 'core', amplitude: 0.065 },
  thigh: { low: 0.21, high: 0.45, kind: 'leg', amplitude: 0.08 },
  calf: { low: 0.04, high: 0.28, kind: 'leg', amplitude: 0.075 },
  calfupper: { low: 0.14, high: 0.31, kind: 'leg', amplitude: 0.07 },
  calflower: { low: 0.03, high: 0.19, kind: 'leg', amplitude: 0.065 },
};

function isPlacedArena(payload = currentPayload) {
  const stats = payload?.stats || {};
  return Number(stats.objectMatrices || 0) >= 20
    && Number(stats.instances || 0) > Number(stats.modelsDecoded || 0);
}

function configurePreviewLighting(payload = currentPayload) {
  renderer.shadowMap.enabled = false;
  keyLight.castShadow = false;
  if (studioLighting && payload) {
    currentPreviewProfile = previewLightingProfile(payload);
    renderer.toneMapping = THREE.NeutralToneMapping;
    renderer.toneMappingExposure = currentPreviewProfile.exposure;
    ambientLight.intensity = currentPreviewProfile.ambient;
    hemisphereLight.intensity = currentPreviewProfile.hemisphere;
    keyLight.intensity = currentPreviewProfile.key;
    fillLight.intensity = currentPreviewProfile.fill;
    rimLight.intensity = currentPreviewProfile.rim;
    rimLight.color.set(0xff6a32);
    return;
  }
  currentPreviewProfile = { id: 'legacy-preview' };
  renderer.toneMapping = THREE.NoToneMapping;
  renderer.toneMappingExposure = 1;
  hemisphereLight.intensity = fillLight.intensity = 0;
  keyLight.position.set(2, 4, 3);
  keyLight.target.position.set(0, 0, 0);
  rimLight.position.set(-3, 1, -2);
  rimLight.target.position.set(0, 0, 0);
  rimLight.color.set(0xff6a32);
  const playerPreview = Boolean(payload?.player);
  ambientLight.intensity = playerPreview ? 2.15 : 1.0;
  keyLight.intensity = playerPreview ? 1.75 : 0.85;
  rimLight.intensity = playerPreview ? 0.4 : 0.22;
}

function placeStudioLights() {
  if (!studioLighting) return;
  const catalog = pickerMode || transparentBackground;
  const center = catalog || studioBounds.isEmpty() ? target : studioBounds.getCenter(new THREE.Vector3());
  const size = studioBounds.getSize(new THREE.Vector3());
  const radius = catalog ? Math.max(fittedDistance * 0.6, 0.01) : Math.max(size.x, size.y, size.z, 0.01);
  for (const [light, direction] of [[keyLight, [1.5, 2.3, 2]], [fillLight, [-2.1, 1.15, 1.3]], [rimLight, [-1.4, 1.7, -2.2]]]) {
    light.position.copy(center).addScaledVector(new THREE.Vector3(...direction).normalize(), radius * 2.6);
    light.target.position.copy(center);
    light.target.updateMatrixWorld(true);
  }
  // Layered catalog canvases use head depth, not independent shadow passes.
  renderer.shadowMap.enabled = !catalog && !studioBounds.isEmpty();
  renderer.shadowMap.type = THREE.PCFSoftShadowMap;
  keyLight.castShadow = renderer.shadowMap.enabled;
  if (keyLight.castShadow) {
    keyLight.shadow.mapSize.set(1024, 1024);
    keyLight.shadow.bias = -0.0002;
    keyLight.shadow.normalBias = radius * 0.0006;
    const shadowCamera = keyLight.shadow.camera;
    shadowCamera.left = shadowCamera.bottom = -radius * 0.72;
    shadowCamera.right = shadowCamera.top = radius * 0.72;
    shadowCamera.near = Math.max(0.01, radius * 0.05);
    shadowCamera.far = radius * 6;
    shadowCamera.updateProjectionMatrix();
  }
}

function applyPreviewMaterialProfile(material) {
  if (material.userData.neutralHairFitBase || material.userData.neutralFacialHairFitBase) return;
  const metadata = currentPayload?.materials?.find((item) => item.name === material.name);
  const profile = materialProfile(material.name, metadata?.shader);
  material.userData.previewMaterialRole = profile.role;
  material.roughness = studioLighting ? profile.roughness : profile.role === 'strand' ? 0.48 : 0.72;
  material.metalness = studioLighting ? profile.metalness : 0.05;
  if (material.normalMap) material.normalScale.setScalar(studioLighting ? profile.normalScale : 0.7);
  if ('clearcoat' in material) {
    material.clearcoat = studioLighting ? profile.clearcoat || 0 : 0;
    material.clearcoatRoughness = profile.clearcoatRoughness || 0;
  }
}

function colorFor(name) {
  const normalized = String(name || '').toLowerCase();
  if (/eye(?:ball|white)|sclera/.test(normalized)) return new THREE.Color(0xd8d4c8);
  if (/iris|pupil/.test(normalized)) return new THREE.Color(0x332b26);
  if (/caruncle|teartube/.test(normalized)) return new THREE.Color(0x9a5d5c);
  if (/teeth/.test(normalized)) return new THREE.Color(0xd7d1bf);
  if (/tongue|mouth|gum/.test(normalized)) return new THREE.Color(0x7f4948);
  if (/hair|beard|brow|lash/.test(normalized)) return new THREE.Color(0x292624);
  if (/jersey|uniform|shorts/.test(normalized)) return new THREE.Color(0xd8dce1);
  if (/skin|face|head|body/.test(normalized)) return new THREE.Color(0x9b7868);
  let hash = 2166136261;
  for (const char of String(name)) hash = Math.imul(hash ^ char.charCodeAt(0), 16777619);
  const color = new THREE.Color();
  color.setHSL(((hash >>> 0) % 360) / 360, 0.07, 0.52 + ((hash >>> 8) % 8) / 100);
  return color;
}

function cameraState() {
  return { target: target.toArray(), yaw, pitch, distance, fittedDistance };
}

function setCameraState(value) {
  if (!value || !Array.isArray(value.target)) return;
  target.fromArray(value.target);
  yaw = Number(value.yaw) || 0;
  pitch = Number(value.pitch) || 0;
  distance = Math.max(0.001, Number(value.distance) || distance);
  fittedDistance = Math.max(0.001, Number(value.fittedDistance) || fittedDistance);
  updateCamera(false);
  if (cameraFollow) {
    if (cameraFollowFrame) cancelAnimationFrame(cameraFollowFrame);
    cameraFollowFrame = requestAnimationFrame(() => {
      cameraFollowFrame = 0;
      renderer.render(scene, camera);
    });
  }
}

function updateCamera(notify = true) {
  pitch = Math.max(-Math.PI * 0.48, Math.min(Math.PI * 0.48, pitch));
  distance = Math.max(0.001, distance);
  const horizontal = Math.cos(pitch) * distance;
  camera.position.set(
    target.x + Math.sin(yaw) * horizontal,
    target.y + Math.sin(pitch) * distance,
    target.z + Math.cos(yaw) * horizontal,
  );
  camera.lookAt(target);
  camera.near = Math.max(0.001, distance / 5000);
  camera.far = Math.max(1000, distance * 100);
  camera.updateProjectionMatrix();
  placeStudioLights();
  renderer.render(scene, camera);
  if (notify && cameraBroadcast) {
    window.dispatchEvent(new CustomEvent(`model-preview:camera:${cameraBroadcast}`, { detail: cameraState() }));
  }
}

function viewerIsActive() {
  if (!interactiveViewer) return false;
  if (canvas.closest('[hidden]')) return false;
  const modal = canvas.closest('.modal-backdrop');
  return !modal || !modal.classList.contains('hidden');
}

function moveCameraFrame(time) {
  movementFrame = 0;
  if (!movementKeys.size || !viewerIsActive()) return;
  const delta = Math.min(0.05, Math.max(0, (time - movementTime) / 1000));
  movementTime = time;
  const forward = target.clone().sub(camera.position);
  forward.y = 0;
  if (forward.lengthSq() < 0.000001) forward.set(-Math.sin(yaw), 0, -Math.cos(yaw));
  forward.normalize();
  const right = new THREE.Vector3().crossVectors(forward, camera.up).normalize();
  const movement = new THREE.Vector3();
  if (movementKeys.has('KeyD') || movementKeys.has('ArrowRight')) movement.add(right);
  if (movementKeys.has('KeyA') || movementKeys.has('ArrowLeft')) movement.sub(right);
  if (movementKeys.has('KeyW') || movementKeys.has('ArrowUp') || movementKeys.has('Space')) movement.y += 1;
  if (movementKeys.has('KeyS') || movementKeys.has('ArrowDown') || movementKeys.has('KeyC')) movement.y -= 1;
  let cameraChanged = false;
  if (movement.lengthSq()) {
    const speed = movementKeys.has('ShiftLeft') || movementKeys.has('ShiftRight') ? 900 : 300;
    target.addScaledVector(movement.normalize(), speed * delta);
    cameraChanged = true;
  }
  let zoomDirection = 0;
  if (movementKeys.has('KeyQ')) zoomDirection -= 1;
  if (movementKeys.has('KeyE')) zoomDirection += 1;
  if (zoomDirection) {
    const zoomSpeed = movementKeys.has('ShiftLeft') || movementKeys.has('ShiftRight') ? 3.6 : 1.8;
    distance *= Math.exp(zoomDirection * zoomSpeed * delta);
    cameraChanged = true;
  }
  if (cameraChanged) updateCamera();
  movementFrame = requestAnimationFrame(moveCameraFrame);
}

function startCameraMovement() {
  if (movementFrame) return;
  movementTime = performance.now();
  movementFrame = requestAnimationFrame(moveCameraFrame);
}

function stopCameraMovement() {
  movementKeys.clear();
  if (movementFrame) cancelAnimationFrame(movementFrame);
  movementFrame = 0;
}

function resize() {
  const width = Math.max(1, canvas.clientWidth);
  const height = Math.max(1, canvas.clientHeight);
  renderer.setSize(width, height, false);
  camera.aspect = width / height;
  camera.updateProjectionMatrix();
  renderer.render(scene, camera);
}
new ResizeObserver(resize).observe(canvas);

function disposeRoot() {
  loadGeneration += 1;
  clearPick(false);
  const geometries = new Set();
  const materialsToDispose = new Set();
  while (modelRoot.children.length) {
    const child = modelRoot.children.pop();
    if (child.geometry) geometries.add(child.geometry);
    const materials = Array.isArray(child.material) ? child.material : [child.material];
    materials.filter(Boolean).forEach((material) => {
      disposeDerivedTextures(material);
      materialsToDispose.add(material);
    });
  }
  geometries.forEach((geometry) => geometry.dispose());
  materialsToDispose.forEach((material) => material.dispose());
  textureCache.forEach((texture) => texture?.dispose());
  textureCache = [];
  textureItems = [];
  bodyFitContext = null;
  currentBodyFitValues = {};
  chestCorrectionEnabled = false;
  lastChestCorrection = null;
}

function percentile(values, ratio) {
  if (!values.length) return 0;
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.min(sorted.length - 1, Math.max(0, Math.floor((sorted.length - 1) * ratio)))];
}

function smoothBand(value, low, high, feather = 0.035) {
  const rise = THREE.MathUtils.smoothstep(value, low - feather, low + feather);
  const fall = 1 - THREE.MathUtils.smoothstep(value, high - feather, high + feather);
  return rise * fall;
}

function bodyFitSignals(values = {}) {
  const pairs = new Map();
  for (const [key, rawValue] of Object.entries(values)) {
    const match = key.match(/^(.*?)(THIN|THICK)$/i);
    const value = Number(rawValue);
    if (!match || !Number.isFinite(value)) continue;
    const token = match[1].toLowerCase();
    const pair = pairs.get(token) || { thin: 0, thick: 0, thinKey: '', thickKey: '' };
    pair[match[2].toLowerCase()] = value;
    pair[`${match[2].toLowerCase()}Key`] = key;
    pairs.set(token, pair);
  }
  return pairs;
}

function bodyDimensionDeltas(values = {}) {
  if (!bodyFitContext) return [];
  const baseline = bodyFitContext.dimensionBaseline || {};
  const deltas = [];
  for (const [key, rawValue] of Object.entries(values)) {
    const match = key.match(/^(.*?)\.t([xyz])$/i);
    const value = Number(rawValue);
    const original = Number(baseline[key]);
    if (!match || !Number.isFinite(value) || !Number.isFinite(original)) continue;
    const delta = value - original;
    if (Math.abs(delta) <= 0.000001) continue;
    deltas.push({
      bone: match[1].toLowerCase(),
      axis: match[2].toLowerCase(),
      delta,
    });
  }
  return deltas;
}

function normalizedRigBoneName(value) {
  return String(value || '').toLowerCase().replace(/^def_/, '');
}

function bodyDimensionChangeGroups(values = {}) {
  if (!bodyFitContext) return [];
  const baselineEntries = new Map(
    Object.entries(bodyFitContext.dimensionBaseline || {}).map(([key, value]) => [key.toLowerCase(), Number(value)]),
  );
  const changes = new Map();
  for (const [key, rawValue] of Object.entries(values)) {
    const match = key.match(/^(.*?)\.t([xyz])$/i);
    if (!match) continue;
    const value = Number(rawValue);
    const original = baselineEntries.get(key.toLowerCase());
    if (!Number.isFinite(value) || !Number.isFinite(original)) continue;
    const bone = match[1].toLowerCase();
    const axis = 'xyz'.indexOf(match[2].toLowerCase());
    const change = changes.get(bone) || {
      bone,
      baseline: [0, 0, 0],
      delta: [0, 0, 0],
    };
    change.baseline[axis] = original;
    change.delta[axis] = value - original;
    changes.set(bone, change);
  }
  return [...changes.values()].filter((change) => change.delta.some((value) => Math.abs(value) > 0.000001));
}

function isRigDescendant(bones, candidate, ancestor) {
  let index = candidate;
  const visited = new Set();
  while (index >= 0 && index < bones.length && !visited.has(index)) {
    if (index === ancestor) return true;
    visited.add(index);
    index = Number(bones[index]?.parent ?? -1);
  }
  return false;
}

function gameVectorToPreview(vector) {
  return new THREE.Vector3(vector.x, -vector.z, vector.y);
}

function rigDimensionChanges(object, changes) {
  const bones = object.userData.bodyFitBones;
  const offsets = object.userData.bodyFitSkinOffsets;
  const skinBones = object.userData.bodyFitSkinBones;
  const skinWeights = object.userData.bodyFitSkinWeights;
  const vertexCount = object.userData.bodyFitBasePositions?.length / 3;
  if (!bones?.length || offsets?.length !== vertexCount + 1 || !skinBones?.length || !skinWeights?.length) return null;

  const byName = new Map(bones.map((bone, index) => [normalizedRigBoneName(bone.name), index]));
  const prepared = [];
  for (const change of changes) {
    const boneIndex = byName.get(change.bone);
    if (boneIndex === undefined) continue;
    const source = new THREE.Vector3(...change.baseline);
    const target = new THREE.Vector3(...(bones[boneIndex].translate || [0, 0, 0]));
    const delta = new THREE.Vector3(...change.delta);
    if (source.lengthSq() > 0.000001 && target.lengthSq() > 0.000001) {
      delta.applyQuaternion(new THREE.Quaternion().setFromUnitVectors(source.normalize(), target.normalize()));
    }
    const token = change.bone.replace(/^(?:left|right)/, '').replace(/_end$/, '');
    prepared.push({
      boneIndex,
      anchoredUpperBody: ['spine', 'spine1', 'spine2'].includes(token),
      delta: gameVectorToPreview(delta),
    });
  }
  if (!prepared.length) return null;

  const descendantCache = prepared.map((change) => {
    const flags = new Uint8Array(bones.length);
    for (let index = 0; index < bones.length; index += 1) {
      flags[index] = isRigDescendant(bones, index, change.boneIndex) ? 1 : 0;
    }
    return flags;
  });
  return { offsets, skinBones, skinWeights, prepared, descendantCache };
}

function applyRigDimensionDeltas(output, rig) {
  const vertexCount = rig.offsets.length - 1;
  for (let vertex = 0; vertex < vertexCount; vertex += 1) {
    const outputOffset = vertex * 3;
    for (let changeIndex = 0; changeIndex < rig.prepared.length; changeIndex += 1) {
      const change = rig.prepared[changeIndex];
      const descendants = rig.descendantCache[changeIndex];
      let weight = 0;
      for (let cursor = rig.offsets[vertex]; cursor < rig.offsets[vertex + 1]; cursor += 1) {
        const belongsToUpperChain = descendants[rig.skinBones[cursor]];
        const applies = change.anchoredUpperBody ? !belongsToUpperChain : belongsToUpperChain;
        if (applies) weight += rig.skinWeights[cursor];
      }
      if (change.anchoredUpperBody) weight *= -1;
      output[outputOffset] += change.delta.x * weight;
      output[outputOffset + 1] += change.delta.y * weight;
      output[outputOffset + 2] += change.delta.z * weight;
    }
  }
}

function bodyFitObjects(role) {
  return modelRoot.children.filter((object) => object.userData?.bodyFitRole === role);
}

function prepareBodyFitContext(baseValues = {}) {
  const bodyObjects = bodyFitObjects('body');
  const points = [];
  for (const object of bodyObjects) {
    const positions = object.userData.bodyFitBasePositions;
    const mask = object.userData.bodyFitMask;
    if (!positions || !mask) continue;
    for (let index = 0; index < mask.length; index += 1) {
      if (!mask[index]) continue;
      const offset = index * 3;
      points.push([positions[offset], positions[offset + 1], positions[offset + 2]]);
    }
  }
  if (!points.length) {
    bodyFitContext = null;
    return;
  }
  const xs = points.map((point) => point[0]);
  const ys = points.map((point) => point[1]);
  const zs = points.map((point) => point[2]);
  const minZ = percentile(zs, 0);
  const maxZ = percentile(zs, 1);
  const centerX = (percentile(xs, 0.02) + percentile(xs, 0.98)) / 2;
  const centerY = (percentile(ys, 0.02) + percentile(ys, 0.98)) / 2;
  const height = Math.max(0.001, maxZ - minZ);
  const midWidths = points
    .filter((point) => {
      const normalizedZ = (point[2] - minZ) / height;
      return normalizedZ >= 0.50 && normalizedZ <= 0.78;
    })
    .map((point) => Math.abs(point[0] - centerX));
  const torsoHalfWidth = Math.max(0.001, percentile(midWidths, 0.45));
  const legWidths = points
    .filter((point) => {
      const normalizedZ = (point[2] - minZ) / height;
      return normalizedZ >= 0.12 && normalizedZ <= 0.38;
    })
    .map((point) => Math.abs(point[0] - centerX));
  bodyFitContext = {
    bodyObjects,
    jerseyObjects: bodyFitObjects('jersey'),
    centerX,
    centerY,
    minZ,
    maxZ,
    height,
    torsoHalfWidth,
    legCenter: Math.max(torsoHalfWidth * 0.38, percentile(legWidths, 0.52)),
    dimensionBaseline: { ...baseValues },
  };
}

function bodyFitRegionWeight(token, definition, x, normalizedZ) {
  if (!bodyFitContext) return 0;
  const band = smoothBand(normalizedZ, definition.low, definition.high);
  if (!band) return 0;
  const lateral = THREE.MathUtils.smoothstep(
    Math.abs(x - bodyFitContext.centerX),
    bodyFitContext.torsoHalfWidth * 0.72,
    bodyFitContext.torsoHalfWidth * 1.48,
  );
  if (definition.kind === 'arm') return band * lateral;
  if (definition.kind === 'shoulder') return band * (0.35 + lateral * 0.65);
  if (definition.kind === 'leg') return band;
  if (token === 'head' || token === 'neck') return band;
  return band * (1 - lateral * 0.9);
}

function bodyDimensionWeight(bone, x, normalizedZ) {
  if (!bodyFitContext) return 0;
  const offsetX = x - bodyFitContext.centerX;
  const lateral = Math.abs(offsetX);
  const width = bodyFitContext.torsoHalfWidth;
  const left = bone.startsWith('left');
  const right = bone.startsWith('right');
  const side = left ? 1 : right ? -1 : 0;
  const sideWeight = side
    ? THREE.MathUtils.smoothstep(offsetX * side, -width * 0.2, width * 0.75)
    : 1;
  const central = 1 - THREE.MathUtils.smoothstep(lateral, width * 0.65, width * 1.7);
  const outer = THREE.MathUtils.smoothstep(lateral, width * 0.65, width * 1.9);
  const token = bone.replace(/^(?:left|right)/, '').replace(/_end$/, '');

  if (token === 'head') return smoothBand(normalizedZ, bone.endsWith('_end') ? 0.91 : 0.82, 1.01) * central;
  if (token === 'neck') return smoothBand(normalizedZ, 0.76, 0.91) * central;
  // The game keeps the upper torso anchored when changing a spine segment.
  // Its fit solver compensates through the chain below that joint.
  if (token === 'spine2') return 1 - THREE.MathUtils.smoothstep(normalizedZ, 0.7, 0.8);
  if (token === 'spine1') return 1 - THREE.MathUtils.smoothstep(normalizedZ, 0.61, 0.71);
  if (token === 'spine') return 1 - THREE.MathUtils.smoothstep(normalizedZ, 0.52, 0.62);
  if (token === 'hips') return smoothBand(normalizedZ, 0.31, 0.54) * central;
  if (token === 'shoulder') return smoothBand(normalizedZ, 0.67, 0.87) * (0.3 + outer * 0.7) * sideWeight;
  if (token === 'arm') return smoothBand(normalizedZ, 0.59, 0.82) * outer * sideWeight;
  if (token === 'forearm') {
    return smoothBand(normalizedZ, 0.53, 0.79)
      * THREE.MathUtils.smoothstep(lateral, width * 1.5, width * 3.4) * sideWeight;
  }
  if (token === 'hand') {
    return smoothBand(normalizedZ, 0.49, 0.76)
      * THREE.MathUtils.smoothstep(lateral, width * 2.5, width * 4.5) * sideWeight;
  }
  if (token === 'upleg') return smoothBand(normalizedZ, 0.19, 0.5) * sideWeight;
  if (token === 'leg') return smoothBand(normalizedZ, 0.05, 0.31) * sideWeight;
  if (token === 'foot') return smoothBand(normalizedZ, -0.01, 0.13) * sideWeight;
  if (token === 'toebase') return smoothBand(normalizedZ, -0.01, 0.075) * sideWeight;
  return 0;
}

function applyBodyDimensionDelta(output, offset, x, normalizedZ, change) {
  const weight = bodyDimensionWeight(change.bone, x, normalizedZ);
  if (!weight) return;
  const token = change.bone.replace(/^(?:left|right)/, '').replace(/_end$/, '');
  const side = change.bone.startsWith('left') ? 1 : change.bone.startsWith('right') ? -1 : 0;
  if (['spine', 'spine1', 'spine2'].includes(token)) {
    if (change.axis === 'x') output[offset] -= change.delta * weight;
    else if (change.axis === 'y') output[offset + 2] -= change.delta * weight;
    else output[offset + 1] += change.delta * weight;
    return;
  }
  if (change.axis === 'x') {
    output[offset] += change.delta * weight;
    return;
  }
  if (change.axis === 'z') {
    // Preview positions use X,-Z,Y relative to the game's XYZ basis.
    output[offset + 1] -= change.delta * weight;
    return;
  }
  if (['shoulder', 'arm', 'forearm', 'hand'].includes(token)) {
    output[offset] += change.delta * (side || 1) * weight;
  } else if (['upleg', 'leg'].includes(token)) {
    output[offset + 2] -= change.delta * weight;
  } else if (['foot', 'toebase'].includes(token)) {
    output[offset + 1] += change.delta * weight;
  } else {
    output[offset + 2] += change.delta * weight;
  }
}

function applyBodyFit(values = {}) {
  if (!bodyFitContext) return;
  currentBodyFitValues = { ...values };
  const signals = bodyFitSignals(values);
  const dimensionDeltas = bodyDimensionDeltas(values);
  const dimensionChanges = bodyDimensionChangeGroups(values);
  for (const object of [...bodyFitContext.bodyObjects, ...bodyFitContext.jerseyObjects]) {
    const base = object.userData.bodyFitBasePositions;
    const mask = object.userData.bodyFitMask;
    const attribute = object.geometry?.getAttribute('position');
    if (!base || !mask || !attribute) continue;
    const output = attribute.array;
    const response = object.userData.bodyFitRole === 'jersey' ? 0.45 : 1;
    const rig = rigDimensionChanges(object, dimensionChanges);
    for (let index = 0; index < mask.length; index += 1) {
      const offset = index * 3;
      const x = base[offset];
      const y = base[offset + 1];
      const z = base[offset + 2];
      output[offset] = x;
      output[offset + 1] = y;
      output[offset + 2] = z;
      if (!mask[index]) continue;
      const normalizedZ = (z - bodyFitContext.minZ) / bodyFitContext.height;
      let coreDelta = 0;
      let armDelta = 0;
      let armZDelta = 0;
      let legDelta = 0;
      for (const [token, definition] of Object.entries(BODY_FIT_REGIONS)) {
        const pair = signals.get(token);
        if (!pair) continue;
        const signal = THREE.MathUtils.clamp(pair.thick - pair.thin, -2, 2);
        if (!signal) continue;
        const weight = bodyFitRegionWeight(token, definition, x, normalizedZ);
        const delta = signal * definition.amplitude * weight * response;
        if (definition.kind === 'arm') {
          armDelta += delta;
          const regionCenterZ = bodyFitContext.minZ
            + bodyFitContext.height * ((definition.low + definition.high) / 2);
          armZDelta += (z - regionCenterZ) * delta;
        }
        else if (definition.kind === 'leg') legDelta += delta;
        else coreDelta += delta;
      }
      coreDelta = THREE.MathUtils.clamp(coreDelta, -0.24, 0.24);
      armDelta = THREE.MathUtils.clamp(armDelta, -0.2, 0.2);
      legDelta = THREE.MathUtils.clamp(legDelta, -0.2, 0.2);
      output[offset] += (x - bodyFitContext.centerX) * coreDelta;
      output[offset + 1] += (y - bodyFitContext.centerY) * (coreDelta + armDelta + legDelta);
      output[offset + 2] += armZDelta;
      if (legDelta) {
        const side = x < bodyFitContext.centerX ? -1 : 1;
        const legCenterX = bodyFitContext.centerX + side * bodyFitContext.legCenter;
        output[offset] += (x - legCenterX) * legDelta;
      }
      if (!rig) {
        for (const change of dimensionDeltas) applyBodyDimensionDelta(output, offset, x, normalizedZ, change);
      }
    }
    if (rig) applyRigDimensionDeltas(output, rig);
    attribute.needsUpdate = true;
    object.geometry.computeBoundingBox();
    object.geometry.computeBoundingSphere();
  }
  const chestCorrection = chestCorrectionEnabled ? applyChestCorrectionGeometry() : null;
  renderer.render(scene, camera);
  return chestCorrection;
}

function jerseyFrontSurfaceGrid(cellSize = 0.55) {
  if (!bodyFitContext?.jerseyObjects.length) return null;
  const triangles = new Map();
  const lowZ = bodyFitContext.minZ + bodyFitContext.height * 0.60;
  const highZ = bodyFitContext.minZ + bodyFitContext.height * 0.83;
  const maxX = bodyFitContext.torsoHalfWidth * 1.45;
  for (const object of bodyFitContext.jerseyObjects) {
    const positions = object.geometry?.getAttribute('position')?.array;
    const indices = object.geometry?.getIndex()?.array;
    if (!positions || !indices) continue;
    for (let cursor = 0; cursor + 2 < indices.length; cursor += 3) {
      const vertexIndices = [indices[cursor], indices[cursor + 1], indices[cursor + 2]];
      const vertices = vertexIndices.map((index) => {
        const offset = index * 3;
        return {
          x: positions[offset],
          y: positions[offset + 1],
          z: positions[offset + 2],
        };
      });
      const minX = Math.min(...vertices.map((vertex) => vertex.x));
      const maxTriangleX = Math.max(...vertices.map((vertex) => vertex.x));
      const minTriangleZ = Math.min(...vertices.map((vertex) => vertex.z));
      const maxTriangleZ = Math.max(...vertices.map((vertex) => vertex.z));
      if (maxTriangleZ < lowZ || minTriangleZ > highZ) continue;
      if (minX > bodyFitContext.centerX + maxX || maxTriangleX < bodyFitContext.centerX - maxX) continue;
      if (vertices.every((vertex) => vertex.y > bodyFitContext.centerY)) continue;

      const [a, b, c] = vertices;
      const denominator = ((b.z - c.z) * (a.x - c.x)) + ((c.x - b.x) * (a.z - c.z));
      if (Math.abs(denominator) < 0.000001) continue;
      const ab = new THREE.Vector3(b.x - a.x, b.y - a.y, b.z - a.z);
      const ac = new THREE.Vector3(c.x - a.x, c.y - a.y, c.z - a.z);
      const normal = ab.cross(ac).normalize();
      const triangle = { a, b, c, denominator, normalY: Math.abs(normal.y) };
      const minGridX = Math.floor(minX / cellSize);
      const maxGridX = Math.floor(maxTriangleX / cellSize);
      const minGridZ = Math.floor(minTriangleZ / cellSize);
      const maxGridZ = Math.floor(maxTriangleZ / cellSize);
      if (maxGridX - minGridX > 64 || maxGridZ - minGridZ > 64) continue;
      for (let gridZ = minGridZ; gridZ <= maxGridZ; gridZ += 1) {
        for (let gridX = minGridX; gridX <= maxGridX; gridX += 1) {
          const key = `${gridX}:${gridZ}`;
          const bucket = triangles.get(key) || [];
          bucket.push(triangle);
          triangles.set(key, bucket);
        }
      }
    }
  }
  return triangles.size ? { triangles, cellSize } : null;
}

function jerseyFrontAt(grid, x, z) {
  const triangleBucket = grid.triangles.get(
    `${Math.floor(x / grid.cellSize)}:${Math.floor(z / grid.cellSize)}`,
  ) || [];
  let surface = null;
  for (const triangle of triangleBucket) {
    const { a, b, c, denominator } = triangle;
    const weightA = (((b.z - c.z) * (x - c.x)) + ((c.x - b.x) * (z - c.z))) / denominator;
    const weightB = (((c.z - a.z) * (x - c.x)) + ((a.x - c.x) * (z - c.z))) / denominator;
    const weightC = 1 - weightA - weightB;
    if (weightA < -0.0001 || weightB < -0.0001 || weightC < -0.0001) continue;
    const y = (weightA * a.y) + (weightB * b.y) + (weightC * c.y);
    if (!surface || y < surface.y) surface = { y, normalY: triangle.normalY };
  }
  return surface;
}

function applyChestCorrectionGeometry() {
  lastChestCorrection = null;
  if (!bodyFitContext) return null;
  const jerseyGrid = jerseyFrontSurfaceGrid();
  if (!jerseyGrid) return null;
  const clearance = 0.08;
  const maxCorrection = 4.0;
  let corrected = 0;
  let peak = 0;
  let total = 0;
  const patches = [];
  for (const object of bodyFitContext.bodyObjects) {
    const positions = object.geometry?.getAttribute('position');
    const torsoMask = object.userData.bodyTorsoMask;
    if (!positions || !torsoMask) continue;
    const output = positions.array;
    const deltas = [];
    for (let index = 0; index < torsoMask.length; index += 1) {
      if (!torsoMask[index]) continue;
      const offset = index * 3;
      const x = output[offset];
      const y = output[offset + 1];
      const z = output[offset + 2];
      if (y > bodyFitContext.centerY) continue;
      const normalizedZ = (z - bodyFitContext.minZ) / bodyFitContext.height;
      if (normalizedZ < 0.60 || normalizedZ > 0.82) continue;
      const lateralDistance = Math.abs(x - bodyFitContext.centerX);
      if (lateralDistance > bodyFitContext.torsoHalfWidth * 1.02) continue;
      const jersey = jerseyFrontAt(jerseyGrid, x, z);
      if (!jersey) continue;
      const tangentClearance = clearance / THREE.MathUtils.clamp(jersey.normalY || 1, 0.6, 1);
      const penetration = (jersey.y + tangentClearance) - y;
      if (penetration <= 0.001) continue;
      const correction = Math.min(penetration, maxCorrection);
      output[offset + 1] += correction;
      deltas.push([index, correction]);
      corrected += 1;
      peak = Math.max(peak, correction);
      total += correction;
    }
    positions.needsUpdate = true;
    object.geometry.computeBoundingBox();
    object.geometry.computeBoundingSphere();
    if (deltas.length) {
      patches.push({
        name: object.name,
        vertexCount: torsoMask.length,
        deltas,
      });
    }
  }
  lastChestCorrection = {
    corrected,
    peak,
    average: corrected ? total / corrected : 0,
    patches,
  };
  return lastChestCorrection;
}

function setChestCorrection(value) {
  chestCorrectionEnabled = Boolean(value);
  if (!chestCorrectionEnabled) lastChestCorrection = null;
  return applyBodyFit(currentBodyFitValues || {});
}

function getChestCorrectionPatch() {
  if (!chestCorrectionEnabled) return null;
  return lastChestCorrection;
}

function regionMeasurements(role, token, definition) {
  if (!bodyFitContext) return [];
  const samples = [];
  for (const object of bodyFitObjects(role)) {
    const positions = object.geometry?.getAttribute('position')?.array;
    const mask = object.userData.bodyFitMask;
    if (!positions || !mask) continue;
    for (let index = 0; index < mask.length; index += 1) {
      if (!mask[index]) continue;
      const offset = index * 3;
      const x = positions[offset];
      const y = positions[offset + 1];
      const z = positions[offset + 2];
      const normalizedZ = (z - bodyFitContext.minZ) / bodyFitContext.height;
      if (bodyFitRegionWeight(token, definition, x, normalizedZ) < 0.35) continue;
      if (definition.kind === 'arm') {
        const centerZ = bodyFitContext.minZ + bodyFitContext.height * ((definition.low + definition.high) / 2);
        samples.push(Math.hypot(y - bodyFitContext.centerY, z - centerZ));
      } else if (definition.kind === 'leg') {
        const side = x < bodyFitContext.centerX ? -1 : 1;
        const centerX = bodyFitContext.centerX + side * bodyFitContext.legCenter;
        samples.push(Math.hypot(x - centerX, y - bodyFitContext.centerY));
      } else {
        samples.push(Math.hypot(x - bodyFitContext.centerX, y - bodyFitContext.centerY));
      }
    }
  }
  return samples;
}

function suggestBodyFit(values = {}) {
  if (!bodyFitContext?.jerseyObjects.length) return null;
  applyBodyFit(values);
  const suggested = { ...values };
  const pairs = bodyFitSignals(values);
  let evaluated = 0;
  let changed = 0;
  for (const [token, pair] of pairs) {
    const definition = BODY_FIT_REGIONS[token];
    if (!definition || token === 'head') continue;
    const bodySamples = regionMeasurements('body', token, definition);
    const jerseySamples = regionMeasurements('jersey', token, definition);
    if (bodySamples.length < 20 || jerseySamples.length < 20) continue;
    const bodyRadius = percentile(bodySamples, 0.92);
    const jerseyRadius = percentile(jerseySamples, 0.92);
    if (!bodyRadius || !jerseyRadius) continue;
    evaluated += 1;
    const clearance = jerseyRadius / bodyRadius;
    if (clearance >= 1.025) continue;
    let adjustment = THREE.MathUtils.clamp((1.025 - clearance) / definition.amplitude, 0.015, 0.18);
    if (pair.thickKey && pair.thick > 0) {
      const reduction = Math.min(pair.thick, adjustment);
      const next = Number((pair.thick - reduction).toFixed(6));
      if (next !== suggested[pair.thickKey]) changed += 1;
      suggested[pair.thickKey] = next;
      adjustment -= reduction;
    }
    if (pair.thinKey && adjustment > 0) {
      const next = Number(Math.min(2, pair.thin + adjustment).toFixed(6));
      if (next !== suggested[pair.thinKey]) changed += 1;
      suggested[pair.thinKey] = next;
    }
  }
  return { suggested, evaluated, changed };
}

function clearPick(notify = true) {
  if (selectionHelper) {
    scene.remove(selectionHelper);
    selectionHelper.geometry?.dispose();
    selectionHelper.material?.dispose();
    selectionHelper = null;
  }
  if (notify) window.dispatchEvent(new CustomEvent('model-preview:pick', { detail: { viewer: viewerGlobalName } }));
  renderer.render(scene, camera);
}

function typed(value, Type) {
  if (value instanceof Type) return value;
  if (ArrayBuffer.isView(value)) return new Type(value.buffer, value.byteOffset, value.byteLength / Type.BYTES_PER_ELEMENT);
  return new Type(value || []);
}

function nextFrame() {
  return new Promise((resolve) => requestAnimationFrame(resolve));
}

function fitCamera(extraBounds = null) {
  const bounds = new THREE.Box3().setFromObject(modelRoot);
  if (extraBounds?.min?.length === 3 && extraBounds?.max?.length === 3) {
    bounds.union(new THREE.Box3(new THREE.Vector3(...extraBounds.min), new THREE.Vector3(...extraBounds.max)));
  }
  studioBounds.copy(bounds);
  if (bounds.isEmpty()) {
    target.set(0, 0, 0);
    fittedDistance = distance = 10;
  } else {
    bounds.getCenter(target);
    const size = bounds.getSize(new THREE.Vector3());
    const radius = Math.max(size.x, size.y, size.z, 0.01);
    if (isPlacedArena()) target.set(0, 0, 0);
    fittedDistance = distance = Math.max(size.y, size.x / Math.max(camera.aspect, 0.1), size.z)
      / (2 * Math.tan(THREE.MathUtils.degToRad(camera.fov / 2))) * 1.35;
    grid.position.y = bounds.min.y;
    grid.scale.setScalar(Math.max(0.01, radius / 10000));
  }
  yaw = Math.PI / 4;
  pitch = Math.PI / 5;
  updateCamera();
}

function focusHead() {
  const bounds = new THREE.Box3().setFromObject(modelRoot);
  if (bounds.isEmpty()) {
    fitCamera();
    return;
  }
  const center = bounds.getCenter(new THREE.Vector3());
  const size = bounds.getSize(new THREE.Vector3());
  const catalogHead = pickerMode && !transparentBackground;
  target.set(center.x, bounds.max.y - size.y * (catalogHead ? 0.46 : 0.14), center.z);
  const portraitHeight = Math.max(size.y * (catalogHead ? 1.05 : 0.34), size.x / Math.max(camera.aspect, 0.1) * (catalogHead ? 1 : 0.3), 0.01);
  fittedDistance = distance = portraitHeight
    / (2 * Math.tan(THREE.MathUtils.degToRad(camera.fov / 2))) * 1.15;
  yaw = 0;
  pitch = 0;
  updateCamera();
}

function setView(view) {
  if (view === 'head') return focusHead();
  if (view === 'fit') return fitCamera();
  if (!['front', 'side', 'back'].includes(view)) return;
  yaw = view === 'side' ? Math.PI / 2 : view === 'back' ? Math.PI : 0;
  pitch = 0;
  updateCamera();
}

function setStudioLighting(value) {
  studioLighting = Boolean(value);
  configurePreviewLighting();
  applyAutomaticTextures();
  updateCamera();
}

function setClayMode(value) {
  clayMode = Boolean(value);
  applyAutomaticTextures();
  renderer.render(scene, camera);
}

function setGridVisible(value) {
  grid.visible = Boolean(value) && !transparentBackground;
  renderer.render(scene, camera);
}

function setOcclusionBase(payload) {
  for (const mesh of [...modelRoot.children]) {
    if (!mesh.userData.previewOccluder) continue;
    modelRoot.remove(mesh);
    mesh.geometry.dispose();
    for (const material of mesh.material) material.dispose();
  }
  // Separate canvases cannot share a depth buffer. Draw the cached donor head
  // into the asset layer's depth buffer without writing its color or alpha.
  for (const item of payload?.meshes || []) {
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute('position', new THREE.BufferAttribute(typed(item.positions, Float32Array), 3));
    geometry.setIndex(new THREE.BufferAttribute(typed(item.indices, Uint32Array), 1));
    const groups = item.groups || [];
    for (const group of groups) {
      if (/(?:^|_)eyeCapShape$/i.test(String(group.mesh || ''))) continue;
      geometry.addGroup(group.start, group.count, 0);
    }
    if (groups.length && !geometry.groups.length) { geometry.dispose(); continue; }
    if (!groups.length) geometry.addGroup(0, geometry.index.count, 0);
    const material = new THREE.MeshBasicMaterial({ colorWrite: false, depthWrite: true, depthTest: true, side: THREE.DoubleSide });
    const mesh = new THREE.Mesh(geometry, [material]);
    mesh.userData.previewOccluder = true;
    mesh.renderOrder = -1;
    mesh.visible = headOcclusionVisible;
    modelRoot.add(mesh);
  }
  renderer.render(scene, camera);
}

function setHeadOcclusionVisible(value) {
  headOcclusionVisible = Boolean(value);
  for (const mesh of modelRoot.children) if (mesh.userData.previewOccluder) mesh.visible = headOcclusionVisible;
  renderer.render(scene, camera);
}

function basketWorldPosition() {
  modelRoot.updateMatrixWorld(true);
  const candidates = [];
  for (const object of modelRoot.children) {
    const legacy = object.userData.previewInstances || [];
    const names = object.userData.previewInstanceNames || legacy.map((instance) => instance.name);
    const targets = object.userData.previewInstanceTargets || legacy.map((instance) => instance.target);
    names.forEach((name, index) => {
      if (!/(?:^|[:@_])basket(?:\d|$)|rimr/i.test(`${name || ''} ${targets[index] || ''}`)) return;
      const matrix = new THREE.Matrix4();
      object.getMatrixAt(index, matrix);
      matrix.premultiply(object.matrixWorld);
      candidates.push(new THREE.Vector3().setFromMatrixPosition(matrix));
    });
  }
  candidates.sort((a, b) => a.z - b.z);
  return candidates[0] || new THREE.Vector3(0, 0, -1200);
}

function setArenaCourtView() {
  if (!isPlacedArena()) {
    fitCamera();
    return;
  }
  const basket = basketWorldPosition();
  const cameraHeight = 100;
  target.set(basket.x, cameraHeight, basket.z);
  const cameraX = 0;
  const cameraZ = 0;
  const dx = cameraX - target.x;
  const dz = cameraZ - target.z;
  yaw = Math.atan2(dx, dz);
  pitch = 0;
  distance = Math.max(200, Math.hypot(dx, dz));
  updateCamera();
}

function createDdsTexture(item) {
  try {
    const binary = atob(item.bytes);
    const bytes = new Uint8Array(binary.length);
    for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index);
    const parsed = new DDSLoader().parse(bytes.buffer, true);
    if (!parsed.format || !parsed.width || !parsed.height || parsed.isCubemap) return null;
    const texture = new THREE.CompressedTexture(parsed.mipmaps, parsed.width, parsed.height, parsed.format);
    texture.name = item.name;
    texture.userData.range = item.range || null;
    texture.userData.inputColorSpace = item.inputColorSpace || 'none';
    texture.colorSpace = !item.range && item.inputColorSpace === 'srgb'
      ? THREE.SRGBColorSpace
      : THREE.NoColorSpace;
    texture.minFilter = parsed.mipmapCount > 1 ? THREE.LinearMipmapLinearFilter : THREE.LinearFilter;
    texture.magFilter = THREE.LinearFilter;
    texture.wrapS = texture.wrapT = THREE.RepeatWrapping;
    texture.anisotropy = Math.min(8, renderer.capabilities.getMaxAnisotropy());
    texture.needsUpdate = true;
    return texture;
  } catch {
    return null;
  }
}

function materialTextureTarget(materialName) {
  const name = String(materialName || '').toLowerCase();
  if (/(?:^|_)[lr]_eye(?:_|$)|eyeball|sclera|iris/.test(name)) return 'eye';
  if (/eyelash|(?:^|_)lash(?:_|$)/.test(name)) return 'eyelash';
  if (/facial.?hair|beard/.test(name)) return 'facialhair';
  if (/hair/.test(name)) return 'hair';
  if (/face/.test(name)) return 'face';
  if (/arms?|chest/.test(name)) return 'chest';
  if (/torso|body/.test(name)) return 'torso';
  if (/legs?|thigh|calf/.test(name)) return 'legs';
  if (/ball/.test(name)) return 'ball';
  return '';
}

function boundTexture(targetName, channel) {
  let index = -1;
  let priority = Number.POSITIVE_INFINITY;
  textureItems.forEach((item, itemIndex) => {
    if (!textureCache[itemIndex]
      || item.binding?.target !== targetName || item.binding?.channel !== channel) return;
    const candidatePriority = Number.isFinite(item.sourcePriority) ? item.sourcePriority : itemIndex;
    if (candidatePriority >= priority) return;
    index = itemIndex;
    priority = candidatePriority;
  });
  return index >= 0 ? textureCache[index] : null;
}

function boundTextureMatching(targetName, channel, pattern) {
  let index = -1;
  let priority = Number.POSITIVE_INFINITY;
  textureItems.forEach((item, itemIndex) => {
    if (!textureCache[itemIndex]
      || item.binding?.target !== targetName || item.binding?.channel !== channel
      || !pattern.test(`${item.name} ${item.source}`)) return;
    const candidatePriority = Number.isFinite(item.sourcePriority) ? item.sourcePriority : itemIndex;
    if (candidatePriority >= priority) return;
    index = itemIndex;
    priority = candidatePriority;
  });
  return index >= 0 ? textureCache[index] : null;
}

function boundArenaTexture(materialName, channel) {
  const normalized = String(materialName || '').toLowerCase();
  const index = textureItems.findIndex((item, itemIndex) => textureCache[itemIndex]
    && item.assignments?.some((assignment) => assignment.channel === channel
      && String(assignment.material || '').toLowerCase() === normalized));
  return index >= 0 ? textureCache[index] : null;
}

function disposeDerivedTextures(material) {
  for (const texture of material.userData.derivedTextures || []) texture.dispose();
  material.userData.derivedTextures = [];
}

function eyeAtlasTexture(texture, material) {
  if (!texture || !/eyewhite/i.test(texture.name)) return texture;
  const derived = texture.clone();
  const rightEye = /(?:^|_)r_eye(?:_|$)/i.test(material.name);
  derived.name = `${texture.name} · ${rightEye ? 'right' : 'left'} eye quadrant`;
  derived.wrapS = THREE.ClampToEdgeWrapping;
  derived.wrapT = THREE.ClampToEdgeWrapping;
  derived.repeat.set(0.5, 0.5);
  // DDS textures are uploaded without a Y flip. The usable eye-white/iris
  // color is stored in the lower image row, which is v=0.5 in this basis.
  derived.offset.set(rightEye ? 0.5 : 0, 0.5);
  derived.needsUpdate = true;
  material.userData.derivedTextures.push(derived);
  return derived;
}

function irisColor(value) {
  const name = String(value || 'brown').trim().toLowerCase();
  const colors = {
    black: 0x211811,
    brown: 0x5a2f17,
    dark_brown: 0x351d12,
    light_brown: 0x86512a,
    amber: 0xa66a22,
    hazel: 0x786735,
    green: 0x4f7448,
    blue: 0x477aa3,
    gray: 0x76818b,
    grey: 0x76818b,
  };
  return new THREE.Color(colors[name.replace(/[^a-z0-9]+/g, '_')] || colors.brown);
}

function irisScale(value) {
  const name = String(value || 'regular').trim().toLowerCase();
  if (/small|tiny/.test(name)) return 0.46;
  if (/large|big/.test(name)) return 0.64;
  return 0.54;
}

function configureEyeIris(material, texture) {
  if (!texture) return;
  const color = irisColor(currentPayload?.player?.irisColor);
  const scale = irisScale(currentPayload?.player?.irisSize);
  material.onBeforeCompile = (shader) => {
    shader.uniforms.nbaIrisColor = { value: color };
    shader.uniforms.nbaIrisScale = { value: scale };
    shader.fragmentShader = shader.fragmentShader
      .replace('#include <map_pars_fragment>', '#include <map_pars_fragment>\nuniform vec3 nbaIrisColor;\nuniform float nbaIrisScale;')
      .replace(
        '#include <map_fragment>',
        `#ifdef USE_MAP
          vec4 sampledDiffuseColor = texture2D( map, vMapUv );
          #ifdef DECODE_VIDEO_TEXTURE
            sampledDiffuseColor = sRGBTransferEOTF( sampledDiffuseColor );
          #endif
          vec2 nbaEyeUv = fract( vMapUv * 2.0 );
          float nbaIrisDistance = length( nbaEyeUv - vec2( 0.5 ) ) / nbaIrisScale;
          float nbaIrisMask = 1.0 - smoothstep( 0.185, 0.255, nbaIrisDistance );
          float nbaPupilMask = 1.0 - smoothstep( 0.052, 0.085, nbaIrisDistance );
          float nbaIrisRays = 0.92 + 0.08 * cos( atan( nbaEyeUv.y - 0.5, nbaEyeUv.x - 0.5 ) * 28.0 );
          sampledDiffuseColor.rgb = mix( sampledDiffuseColor.rgb, nbaIrisColor * nbaIrisRays, nbaIrisMask );
          sampledDiffuseColor.rgb = mix( sampledDiffuseColor.rgb, vec3( 0.012 ), nbaPupilMask );
          diffuseColor *= sampledDiffuseColor;
        #endif`,
      );
  };
  material.customProgramCacheKey = () => `nba2k27-eye-iris-v2-${color.getHexString()}-${scale}`;
}

function configureTextureRange(material, texture) {
  const range = texture?.userData?.range || null;
  material.userData.textureRange = range;
  // Cached programs retain these objects when a config or surface changes.
  const uniforms = material.userData.textureRangeUniforms ||= {
    mapRangeMin: { value: new THREE.Vector4() },
    mapRangeMax: { value: new THREE.Vector4() },
    mapRangeMask: { value: new THREE.Vector4() },
  };
  if (range) {
    uniforms.mapRangeMin.value.fromArray(range.min);
    uniforms.mapRangeMax.value.fromArray(range.max);
    uniforms.mapRangeMask.value.fromArray(range.mask);
  }
  material.onBeforeCompile = range ? (shader) => {
    Object.assign(shader.uniforms, uniforms);
    shader.fragmentShader = shader.fragmentShader
      .replace(
        '#include <map_pars_fragment>',
        '#include <map_pars_fragment>\nuniform vec4 mapRangeMin;\nuniform vec4 mapRangeMax;\nuniform vec4 mapRangeMask;',
      )
      .replace(
        '#include <map_fragment>',
        `#ifdef USE_MAP
          vec4 sampledDiffuseColor = texture2D( map, vMapUv );
          #ifdef DECODE_VIDEO_TEXTURE
            sampledDiffuseColor = sRGBTransferEOTF( sampledDiffuseColor );
          #endif
          vec4 rangeDecodedColor = mix( mapRangeMin, mapRangeMax, sampledDiffuseColor );
          sampledDiffuseColor = mix( sampledDiffuseColor, rangeDecodedColor, mapRangeMask );
          diffuseColor *= sampledDiffuseColor;
        #endif`,
      );
  } : () => {};
  material.customProgramCacheKey = () => (range ? 'nba2k27-texture-range-v1' : '');
}

function configureHairLightmap(material, texture) {
  material.userData.hairLightmap = texture?.name || null;
  if (!texture) return;
  const previousCompile = material.onBeforeCompile;
  const previousCacheKey = material.customProgramCacheKey;
  const range = texture.userData?.range || {
    min: [0, 0, 0, 0],
    max: [1, 1, 1, 1],
    mask: [0, 0, 0, 0],
  };
  material.onBeforeCompile = (shader) => {
    previousCompile(shader);
    shader.uniforms.nbaHairLightmap = { value: texture };
    shader.uniforms.nbaHairLightMin = { value: new THREE.Vector4(...range.min) };
    shader.uniforms.nbaHairLightMax = { value: new THREE.Vector4(...range.max) };
    shader.uniforms.nbaHairLightMask = { value: new THREE.Vector4(...range.mask) };
    shader.fragmentShader = shader.fragmentShader.replace(
      '#include <map_pars_fragment>',
      '#include <map_pars_fragment>\nuniform sampler2D nbaHairLightmap;\nuniform vec4 nbaHairLightMin;\nuniform vec4 nbaHairLightMax;\nuniform vec4 nbaHairLightMask;',
    );
    if (shader.fragmentShader.includes('diffuseColor *= sampledDiffuseColor;')) {
      shader.fragmentShader = shader.fragmentShader.replace(
        'diffuseColor *= sampledDiffuseColor;',
        `vec4 nbaHairLightSample = texture2D( nbaHairLightmap, vMapUv );
        vec4 nbaHairLightDecoded = mix( nbaHairLightMin, nbaHairLightMax, nbaHairLightSample );
        nbaHairLightSample = mix( nbaHairLightSample, nbaHairLightDecoded, nbaHairLightMask );
        sampledDiffuseColor.rgb *= nbaHairLightSample.rgb;
        diffuseColor *= sampledDiffuseColor;`,
      );
    } else {
      shader.fragmentShader = shader.fragmentShader.replace(
        '#include <map_fragment>',
        `#include <map_fragment>
        vec4 nbaHairLightSample = texture2D( nbaHairLightmap, vMapUv );
        vec4 nbaHairLightDecoded = mix( nbaHairLightMin, nbaHairLightMax, nbaHairLightSample );
        nbaHairLightSample = mix( nbaHairLightSample, nbaHairLightDecoded, nbaHairLightMask );
        diffuseColor.rgb *= nbaHairLightSample.rgb;`,
      );
    }
  };
  material.customProgramCacheKey = () => `${previousCacheKey()}-nba2k27-hair-lightmap-v1`;
}

function median(values) {
  if (!values.length) return 1;
  values.sort((left, right) => left - right);
  const middle = Math.floor(values.length / 2);
  return values.length % 2 ? values[middle] : (values[middle - 1] + values[middle]) / 2;
}

function decodedTexturePixels(texture) {
  if (!texture?.userData?.range) return null;
  if (texture.userData.decodedPixels) return texture.userData.decodedPixels;
  const sampleSize = 256;
  const targetTexture = new THREE.WebGLRenderTarget(sampleSize, sampleSize, {
    format: THREE.RGBAFormat,
    type: THREE.UnsignedByteType,
    depthBuffer: false,
    stencilBuffer: false,
  });
  targetTexture.texture.colorSpace = THREE.NoColorSpace;
  targetTexture.texture.minFilter = THREE.NearestFilter;
  targetTexture.texture.magFilter = THREE.NearestFilter;
  const sampleScene = new THREE.Scene();
  const sampleCamera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0, 2);
  sampleCamera.position.z = 1;
  const sampleMaterial = new THREE.MeshBasicMaterial({ map: texture, toneMapped: false });
  sampleScene.add(new THREE.Mesh(new THREE.PlaneGeometry(2, 2), sampleMaterial));
  const pixels = new Uint8Array(sampleSize * sampleSize * 4);
  const previousTarget = renderer.getRenderTarget();
  try {
    renderer.setRenderTarget(targetTexture);
    renderer.clear();
    renderer.render(sampleScene, sampleCamera);
    renderer.readRenderTargetPixels(targetTexture, 0, 0, sampleSize, sampleSize, pixels);
  } catch {
    return null;
  } finally {
    renderer.setRenderTarget(previousTarget);
    sampleScene.children[0].geometry.dispose();
    sampleMaterial.dispose();
    targetTexture.dispose();
  }
  texture.userData.decodedPixels = { pixels, size: sampleSize, range: texture.userData.range };
  return texture.userData.decodedPixels;
}

function sampleDecodedTexture(texture, uv) {
  const decoded = decodedTexturePixels(texture);
  if (!decoded || !uv) return null;
  const wrap = (value) => ((Number(value) % 1) + 1) % 1;
  const x = Math.min(decoded.size - 1, Math.floor(wrap(uv[0]) * decoded.size));
  const y = Math.min(decoded.size - 1, Math.floor(wrap(uv[1]) * decoded.size));
  const offset = (y * decoded.size + x) * 4;
  return [0, 1, 2].map((channel) => {
    const sample = decoded.pixels[offset + channel] / 255;
    return decoded.range.mask[channel]
      ? decoded.range.min[channel] + sample * (decoded.range.max[channel] - decoded.range.min[channel])
      : sample;
  });
}

function boundaryUvPairs(leftTarget, rightTarget) {
  const pairs = [];
  for (const mesh of currentPayload?.meshes || []) {
    const verticesByTarget = new Map();
    for (const group of mesh.groups || []) {
      const targetName = materialTextureTarget(group.material);
      if (targetName !== leftTarget && targetName !== rightTarget) continue;
      const vertices = verticesByTarget.get(targetName) || new Set();
      for (let cursor = group.start; cursor < group.start + group.count; cursor += 1) {
        vertices.add(mesh.indices[cursor]);
      }
      verticesByTarget.set(targetName, vertices);
    }
    const leftVertices = verticesByTarget.get(leftTarget);
    const rightVertices = verticesByTarget.get(rightTarget);
    if (!leftVertices?.size || !rightVertices?.size) continue;
    const positionKey = (vertex) => [0, 1, 2]
      .map((axis) => Number(mesh.positions[vertex * 3 + axis]).toFixed(3)).join(',');
    const rightByPosition = new Map([...rightVertices].map((vertex) => [positionKey(vertex), vertex]));
    for (const leftVertex of leftVertices) {
      const rightVertex = rightByPosition.get(positionKey(leftVertex));
      if (rightVertex === undefined) continue;
      pairs.push({
        left: [mesh.uvs[leftVertex * 2], mesh.uvs[leftVertex * 2 + 1]],
        right: [mesh.uvs[rightVertex * 2], mesh.uvs[rightVertex * 2 + 1]],
      });
    }
  }
  return pairs;
}

function boundaryColors(leftTarget, rightTarget) {
  const leftTexture = boundTexture(leftTarget, 'color');
  const rightTexture = boundTexture(rightTarget, 'color');
  const samples = boundaryUvPairs(leftTarget, rightTarget).map((pair) => ({
    left: sampleDecodedTexture(leftTexture, pair.left),
    right: sampleDecodedTexture(rightTexture, pair.right),
  })).filter((sample) => sample.left && sample.right);
  if (!samples.length) return null;
  return {
    left: [0, 1, 2].map((channel) => median(samples.map((sample) => sample.left[channel]))),
    right: [0, 1, 2].map((channel) => median(samples.map((sample) => sample.right[channel]))),
  };
}

function responseColor(reference, source, referenceResponse = [1, 1, 1]) {
  return new THREE.Color(...source.map((value, channel) => THREE.MathUtils.clamp(
    (reference[channel] * referenceResponse[channel]) / Math.max(value, 0.005),
    0.3,
    2.5,
  )));
}

function skinMaterialResponses() {
  if (!currentPayload?.player) return new Map();
  if (String(currentPayload.player.mode || '').startsWith('hair-catalog-')) return new Map();
  const responses = new Map();
  const chestTorso = boundaryColors('chest', 'torso');
  const chestResponse = chestTorso ? responseColor(chestTorso.right, chestTorso.left) : new THREE.Color(1, 1, 1);
  responses.set('chest', chestResponse);
  const torsoLegs = boundaryColors('torso', 'legs');
  if (torsoLegs) responses.set('legs', responseColor(torsoLegs.left, torsoLegs.right));
  const faceChest = boundaryColors('face', 'chest');
  if (faceChest) responses.set('face', responseColor(faceChest.right, faceChest.left, chestResponse.toArray()));
  return responses;
}

function applyAutomaticTextures() {
  const materialResponses = skinMaterialResponses();
  modelRoot.traverse((object) => {
    if (object.userData.previewOccluder) return;
    const materials = Array.isArray(object.material) ? object.material : [object.material];
    materials.filter(Boolean).forEach((material) => {
      disposeDerivedTextures(material);
      if (material.userData.neutralHairFitBase || material.userData.neutralFacialHairFitBase) {
        material.map = null;
        material.normalMap = null;
        if ('anisotropyMap' in material) {
          material.anisotropyMap = null;
          material.anisotropy = 0;
        }
        material.transparent = false;
        material.alphaTest = 0;
        material.depthWrite = true;
        material.color.setHex(0x8e959c);
        material.roughness = 0.9;
        material.metalness = 0;
        material.needsUpdate = true;
        return;
      }
      const targetName = materialTextureTarget(material.name);
      const boundColorTexture = targetName
        ? boundTexture(targetName, 'color')
        : boundArenaTexture(material.name, 'color');
      const colorTexture = targetName === 'eye'
        ? eyeAtlasTexture(boundColorTexture, material)
        : boundColorTexture;
      const normalTexture = targetName
        ? boundTexture(targetName, 'normal')
        : boundArenaTexture(material.name, 'normal');
      const strandMaterial = targetName === 'hair' || targetName === 'facialhair';
      const tangentTexture = strandMaterial
        ? boundTextureMatching(targetName, 'auxiliary', /tangent/i)
        : null;
      const lightmapTexture = strandMaterial
        ? boundTextureMatching(targetName, 'auxiliary', /lightmap|occlusion/i)
        : null;
      material.map = colorTexture;
      material.normalMap = normalTexture;
      if ('anisotropyMap' in material) {
        material.anisotropyMap = tangentTexture;
        material.anisotropy = tangentTexture ? 0.7 : 0;
      }
      configureTextureRange(material, colorTexture);
      if (strandMaterial) configureHairLightmap(material, lightmapTexture);
      if (targetName === 'eye') configureEyeIris(material, colorTexture);
      if (normalTexture) material.normalScale.set(0.7, 0.7);
      material.transparent = Boolean(colorTexture) && (targetName === 'eyelash' || strandMaterial);
      material.alphaTest = targetName === 'eyelash' ? 0.08 : strandMaterial ? 0.035 : 0;
      material.depthWrite = targetName !== 'eyelash';
      material.forceSinglePass = strandMaterial;
      material.color.copy(colorTexture && targetName !== 'eyelash'
        ? materialResponses.get(targetName) || new THREE.Color(0xffffff)
        : colorFor(material.name));
      material.userData.skinMaterialResponse = materialResponses.get(targetName)?.toArray() || null;
      applyPreviewMaterialProfile(material);
      if (clayMode) {
        material.map = material.normalMap = null;
        if ('anisotropyMap' in material) material.anisotropyMap = null;
        material.color.setHex(0x8e959c);
        material.transparent = false;
        material.alphaTest = 0;
        material.depthWrite = true;
      }
      material.needsUpdate = true;
    });
    object.castShadow = materials.some((material) => material && !material.transparent);
    object.receiveShadow = true;
  });
}

async function load(payload, onProgress = () => {}) {
  currentPayload = payload;
  eyelashesVisible = false;
  configurePreviewLighting(payload);
  disposeRoot();
  const generation = loadGeneration;
  const items = payload.meshes || [];
  for (let itemIndex = 0; itemIndex < items.length; itemIndex += 1) {
    const item = items[itemIndex];
    const neutralHairFitBase = item.sourceKind === 'hair-fit-base';
    const neutralFacialHairFitBase = item.sourceKind === 'facialhair-fit-base';
    const neutralFitBase = neutralHairFitBase || neutralFacialHairFitBase;
    const geometry = new THREE.BufferGeometry();
    const positions = typed(item.positions, Float32Array);
    const indices = typed(item.indices, Uint32Array);
    geometry.setAttribute('position', new THREE.BufferAttribute(positions, 3));
    if (item.uvs?.length) geometry.setAttribute('uv', new THREE.BufferAttribute(typed(item.uvs, Float32Array), 2));
    geometry.setIndex(new THREE.BufferAttribute(indices, 1));
    geometry.computeVertexNormals();
    const materials = [];
    const previewGroups = [];
    for (const group of item.groups || []) {
      // Eye caps are internal eye-assembly occlusion helpers. Their SCNE
      // primitives inherit face_shader, but the game does not present them as
      // opaque skin. Drawing them that way creates the circular eye bulge seen
      // in Preview after Blender has already positioned the eyes correctly.
      if (/(?:^|_)eyeCapShape$/i.test(String(group.mesh || ''))) continue;
      const targetName = materialTextureTarget(group.material);
      const Material = targetName === 'hair' || targetName === 'facialhair' || targetName === 'eye'
        ? THREE.MeshPhysicalMaterial
        : THREE.MeshStandardMaterial;
      const material = new Material({
        name: group.material,
        color: neutralFitBase ? new THREE.Color(0x8e959c) : colorFor(group.material),
        roughness: neutralFitBase ? 0.9 : targetName === 'hair' || targetName === 'facialhair' ? 0.48 : 0.72,
        metalness: neutralFitBase ? 0 : 0.05,
        side: THREE.DoubleSide,
        wireframe,
      });
      material.userData.neutralHairFitBase = neutralHairFitBase;
      material.userData.neutralFacialHairFitBase = neutralFacialHairFitBase;
      if (targetName === 'facialhair') {
        material.polygonOffset = true;
        material.polygonOffsetFactor = -1;
        material.polygonOffsetUnits = -1;
      }
      const materialIndex = materials.length;
      materials.push(material);
      previewGroups.push(group);
      geometry.addGroup(group.start, group.count, materialIndex);
    }
    if (!materials.length) materials.push(new THREE.MeshStandardMaterial({ color: 0xa6adb7, wireframe }));
    const packedMatrices = typed(item.instanceMatrices, Float32Array);
    const packedCount = Math.floor(packedMatrices.length / 16);
    const legacyInstances = item.instances || [];
    const instanceCount = packedCount || legacyInstances.length;
    const bodyFitEnabled = !String(payload.player?.mode || '').startsWith('hair-catalog-');
    const bodyFitMask = new Uint8Array(bodyFitEnabled ? Math.floor(positions.length / 3) : 0);
    const bodyChestMask = new Uint8Array(bodyFitEnabled ? Math.floor(positions.length / 3) : 0);
    const bodyTorsoMask = new Uint8Array(bodyFitEnabled ? Math.floor(positions.length / 3) : 0);
    const bodyFitRole = item.sourceKind === 'preview-jersey' ? 'jersey' : 'body';
    if (!bodyFitEnabled) {
      // Catalog heads and hair layers never use the body-fit editor.
    } else if (bodyFitRole === 'jersey') {
      bodyFitMask.fill(1);
    } else {
      for (const group of item.groups || []) {
        const targetName = materialTextureTarget(group.material);
        if (!['face', 'chest', 'torso', 'legs'].includes(targetName)) continue;
        for (let cursor = group.start; cursor < group.start + group.count; cursor += 1) {
          bodyFitMask[indices[cursor]] = 1;
          if (targetName === 'chest') bodyChestMask[indices[cursor]] = 1;
          if (targetName === 'torso') bodyTorsoMask[indices[cursor]] = 1;
        }
      }
    }
    let previewMesh;
    if (instanceCount) {
      const mesh = new THREE.InstancedMesh(geometry, materials, instanceCount);
      mesh.name = item.name;
      mesh.userData.previewInstances = legacyInstances;
      mesh.userData.previewInstanceNames = item.instanceNames || legacyInstances.map((instance) => instance.name);
      mesh.userData.previewInstanceTargets = item.instanceTargets || legacyInstances.map((instance) => instance.target);
      mesh.userData.previewModel = item.name;
      mesh.userData.previewKind = item.sourceKind || '';
      mesh.userData.previewSource = item.sourceName || '';
      for (let index = 0; index < instanceCount; index += 1) {
        const source = packedCount ? packedMatrices : legacyInstances[index].matrix;
        const offset = packedCount ? index * 16 : 0;
        mesh.setMatrixAt(index, new THREE.Matrix4().fromArray(source, offset));
      }
      mesh.instanceMatrix.setUsage(THREE.StaticDrawUsage);
      mesh.instanceMatrix.needsUpdate = true;
      modelRoot.add(mesh);
      previewMesh = mesh;
    } else {
      const mesh = new THREE.Mesh(geometry, materials);
      mesh.name = item.name;
      mesh.userData.previewModel = item.name;
      mesh.userData.previewKind = item.sourceKind || '';
      mesh.userData.previewSource = item.sourceName || '';
      modelRoot.add(mesh);
      previewMesh = mesh;
    }
    previewMesh.userData.previewGroups = previewGroups;
    if (bodyFitMask.some((value) => value)) {
      previewMesh.userData.bodyFitRole = bodyFitRole;
      previewMesh.userData.bodyFitMask = bodyFitMask;
      previewMesh.userData.bodyChestMask = bodyChestMask;
      previewMesh.userData.bodyTorsoMask = bodyTorsoMask;
      previewMesh.userData.bodyFitBasePositions = Float32Array.from(positions);
      previewMesh.userData.bodyFitBones = item.bones || [];
      previewMesh.userData.bodyFitSkinOffsets = typed(item.skinOffsets, Uint32Array);
      previewMesh.userData.bodyFitSkinBones = typed(item.skinBones, Uint16Array);
      previewMesh.userData.bodyFitSkinWeights = typed(item.skinWeights, Float32Array);
    }
    if ((itemIndex + 1) % 12 === 0 && itemIndex + 1 < items.length) {
      onProgress({ phase: 'meshes', completed: itemIndex + 1, total: items.length });
      await nextFrame();
      if (generation !== loadGeneration) return { cancelled: true };
    }
  }
  prepareBodyFitContext(payload.player?.bodyFit || {});
  applyBodyFit(payload.player?.bodyFit || {});
  textureItems = payload.textures || [];
  textureCache = [];
  for (let index = 0; index < textureItems.length; index += 1) {
    textureCache.push(createDdsTexture(textureItems[index]));
    if ((index + 1) % 4 === 0 && index + 1 < textureItems.length) {
      onProgress({ phase: 'textures', completed: index + 1, total: textureItems.length });
      await nextFrame();
      if (generation !== loadGeneration) return { cancelled: true };
    }
  }
  applyAutomaticTextures();
  setEyelashesVisible(false);
  setJerseyVisible(false);
  fitCamera();
  if (isPlacedArena(payload)) setArenaCourtView();
  window.dispatchEvent(new CustomEvent('model-preview:loaded', { detail: { viewer: viewerGlobalName } }));
  return { decodedTextures: textureCache.filter(Boolean).length };
}

async function loadArenaTextures(items, onProgress = () => {}) {
  textureCache.forEach((texture) => texture?.dispose());
  textureItems = items || [];
  textureCache = [];
  for (let index = 0; index < textureItems.length; index += 1) {
    textureCache.push(createDdsTexture(textureItems[index]));
    if ((index + 1) % 4 === 0 && index + 1 < textureItems.length) {
      onProgress({ completed: index + 1, total: textureItems.length });
      await nextFrame();
    }
  }
  applyAutomaticTextures();
  renderer.render(scene, camera);
  return { decodedTextures: textureCache.filter(Boolean).length };
}

function clearArenaTextures() {
  textureCache.forEach((texture) => texture?.dispose());
  textureItems = [];
  textureCache = [];
  applyAutomaticTextures();
  renderer.render(scene, camera);
}

function pickObject(clientX, clientY) {
  const rect = canvas.getBoundingClientRect();
  if (!rect.width || !rect.height) return;
  pointer.set(
    ((clientX - rect.left) / rect.width) * 2 - 1,
    -((clientY - rect.top) / rect.height) * 2 + 1,
  );
  modelRoot.updateMatrixWorld(true);
  raycaster.setFromCamera(pointer, camera);
  const hit = modelRoot.visible && raycaster.intersectObjects(modelRoot.children, false).find((candidate) => {
    const object = candidate.object;
    const materials = Array.isArray(object.material) ? object.material : [object.material];
    return object.visible && materials[candidate.face?.materialIndex || 0]?.visible;
  });
  clearPick(false);
  if (!hit) {
    window.dispatchEvent(new CustomEvent('model-preview:pick', { detail: { viewer: viewerGlobalName } }));
    renderer.render(scene, camera);
    return;
  }
  if (hit.object.userData.previewOccluder) return false;

  const object = hit.object;
  const materialIndex = hit.face?.materialIndex || 0;
  const materials = Array.isArray(object.material) ? object.material : [object.material];
  const material = materials[materialIndex];
  const group = object.userData.previewGroups?.[materialIndex];
  const metadata = currentPayload?.materials?.find((item) => item.name === material?.name);
  const instanceId = Number.isInteger(hit.instanceId) ? hit.instanceId : null;
  const legacyInstance = instanceId === null ? null : object.userData.previewInstances?.[instanceId];
  const instance = instanceId === null ? null : {
    name: object.userData.previewInstanceNames?.[instanceId] || legacyInstance?.name,
    target: object.userData.previewInstanceTargets?.[instanceId] || legacyInstance?.target,
  };
  object.geometry.computeBoundingBox();
  const bounds = object.geometry.boundingBox.clone();
  if (instanceId === null) {
    bounds.applyMatrix4(object.matrixWorld);
  } else {
    const matrix = new THREE.Matrix4();
    object.getMatrixAt(instanceId, matrix);
    matrix.premultiply(object.matrixWorld);
    bounds.applyMatrix4(matrix);
  }
  selectionHelper = new THREE.Box3Helper(bounds, 0xff6a32);
  scene.add(selectionHelper);
  window.dispatchEvent(new CustomEvent('model-preview:pick', {
    detail: {
      viewer: viewerGlobalName,
      name: instance?.name || object.name,
      target: instance?.target || object.userData.previewModel || object.name,
      model: object.userData.previewModel || object.name,
      instance: instanceId,
      primitive: group?.mesh || '',
      material: material ? {
        name: material.name,
        shader: metadata?.shader || '',
        textures: ['map', 'normalMap', 'anisotropyMap'].filter((slot) => material[slot]).map((slot) => ({
          channel: slot, name: material[slot].name,
          source: textureItems.find((item) => item.name === material[slot].name)?.source || '',
          rangeCompressed: Boolean(material[slot].userData.range),
        })),
        lightmap: material.userData.hairLightmap || '',
      } : null,
    },
  }));
  renderer.render(scene, camera);
  return true;
}

function setWireframe(value) {
  wireframe = Boolean(value);
  modelRoot.traverse((object) => {
    if (object.userData.previewOccluder) return;
    const materials = Array.isArray(object.material) ? object.material : [object.material];
    materials.filter(Boolean).forEach((material) => { material.wireframe = wireframe; material.needsUpdate = true; });
  });
  renderer.render(scene, camera);
}

function setMaterialTargetVisible(targetName, visible) {
  modelRoot.traverse((object) => {
    const materials = Array.isArray(object.material) ? object.material : [object.material];
    materials.filter(Boolean).forEach((material) => {
      if (materialTextureTarget(material.name) === targetName) material.visible = Boolean(visible);
    });
  });
  renderer.render(scene, camera);
}

function setEyelashesVisible(value) {
  eyelashesVisible = Boolean(value);
  modelRoot.traverse((object) => {
    const materials = Array.isArray(object.material) ? object.material : [object.material];
    materials.filter(Boolean).forEach((material) => {
      if (materialTextureTarget(material.name) === 'eyelash') material.visible = eyelashesVisible;
    });
  });
  renderer.render(scene, camera);
}

function setJerseyVisible(value) {
  jerseyVisible = Boolean(value);
  modelRoot.traverse((object) => {
    if (object.userData?.previewKind === 'preview-jersey') object.visible = jerseyVisible;
  });
  renderer.render(scene, camera);
}

function setTexture(index) {
  if (index === '' || index === null) {
    applyAutomaticTextures();
    renderer.render(scene, camera);
    return;
  }
  const texture = index === '' || index === null ? null : textureCache[Number(index)] || null;
  modelRoot.traverse((object) => {
    const materials = Array.isArray(object.material) ? object.material : [object.material];
    materials.filter(Boolean).forEach((material) => {
      material.map = texture;
      material.normalMap = null;
      configureTextureRange(material, texture);
      material.color.copy(texture ? new THREE.Color(0xffffff) : colorFor(material.name));
      material.needsUpdate = true;
    });
  });
  renderer.render(scene, camera);
}

canvas.addEventListener('contextmenu', (event) => event.preventDefault());
canvas.addEventListener('pointerdown', (event) => {
  canvas.setPointerCapture(event.pointerId);
  drag = {
    x: event.clientX,
    y: event.clientY,
    originX: event.clientX,
    originY: event.clientY,
    button: event.button,
    moved: false,
  };
});
canvas.addEventListener('pointermove', (event) => {
  if (!drag) return;
  const dx = event.clientX - drag.x;
  const dy = event.clientY - drag.y;
  if (Math.hypot(event.clientX - drag.originX, event.clientY - drag.originY) > 4) drag.moved = true;
  drag.x = event.clientX;
  drag.y = event.clientY;
  if (drag.button === 2) {
    const forward = target.clone().sub(camera.position).normalize();
    const right = new THREE.Vector3().crossVectors(forward, camera.up).normalize();
    const up = new THREE.Vector3().crossVectors(right, forward).normalize();
    const scale = distance * 0.0015;
    target.addScaledVector(right, -dx * scale).addScaledVector(up, dy * scale);
  } else {
    yaw -= dx * 0.006;
    pitch += dy * 0.006;
  }
  updateCamera();
});
canvas.addEventListener('pointerup', (event) => {
  const shouldPick = drag?.button === 0 && !drag.moved;
  drag = null;
  if (shouldPick) {
    const pickedAsset = cameraBroadcast && window.hairAssetPreview?.pickObject(event.clientX, event.clientY);
    if (!pickedAsset) pickObject(event.clientX, event.clientY);
  }
});
canvas.addEventListener('pointercancel', () => { drag = null; });
canvas.addEventListener('wheel', (event) => {
  event.preventDefault();
  distance *= Math.exp(event.deltaY * 0.001);
  updateCamera();
}, { passive: false });

const movementCodes = new Set([
  'KeyW', 'KeyA', 'KeyS', 'KeyD', 'KeyQ', 'KeyE', 'KeyC', 'Space',
  'ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'ShiftLeft', 'ShiftRight',
]);
window.addEventListener('keydown', (event) => {
  if (!movementCodes.has(event.code) || !viewerIsActive()) return;
  if (/^(INPUT|SELECT|TEXTAREA|BUTTON)$/.test(event.target?.tagName || '')) return;
  movementKeys.add(event.code);
  event.preventDefault();
  startCameraMovement();
});
window.addEventListener('keyup', (event) => {
  if (!movementCodes.has(event.code)) return;
  movementKeys.delete(event.code);
  event.preventDefault();
});
window.addEventListener('blur', stopCameraMovement);

if (cameraFollow) {
  window.addEventListener(`model-preview:camera:${cameraFollow}`, (event) => setCameraState(event.detail));
}

window[viewerGlobalName] = {
  clear: () => {
    currentPayload = null;
    disposeRoot();
    configurePreviewLighting(null);
    fitCamera();
    window.dispatchEvent(new CustomEvent('model-preview:cleared', { detail: { viewer: viewerGlobalName } }));
  },
  colorHex: (name) => `#${colorFor(name).getHexString()}`,
  clearPick,
  cameraState,
  setCameraState,
  fitCamera,
  focusHead,
  setView,
  setStudioLighting,
  setClayMode,
  setGridVisible,
  setOcclusionBase,
  setHeadOcclusionVisible,
  pickObject,
  bounds: () => {
    const bounds = new THREE.Box3().setFromObject(modelRoot);
    return bounds.isEmpty() ? null : { min: bounds.min.toArray(), max: bounds.max.toArray() };
  },
  setVisible: (value) => { modelRoot.visible = Boolean(value); renderer.render(scene, camera); },
  load,
  loadArenaTextures,
  clearArenaTextures,
  resize,
  setTexture,
  setBodyFit: applyBodyFit,
  setChestCorrection,
  getChestCorrectionPatch,
  suggestBodyFit,
  setEyelashesVisible,
  setJerseyVisible,
  setArenaCourtView,
  stopCameraMovement,
  setWireframe,
  setMaterialTargetVisible,
  stats: () => ({
    payload: currentPayload,
    profile: currentPreviewProfile,
    studioLighting,
    clayMode,
    shadows: renderer.shadowMap.enabled,
    occlusionMeshes: modelRoot.children.filter((object) => object.userData.previewOccluder && object.visible).length,
    fittedDistance,
    camera: camera.position.toArray(),
    target: target.toArray(),
    materialTextures: modelRoot.children.flatMap((object) => {
      const materials = Array.isArray(object.material) ? object.material : [object.material];
      return materials.filter(Boolean).map((material) => ({
        name: material.name,
        map: material.map?.name || '',
        normalMap: material.normalMap?.name || '',
        color: `#${material.color.getHexString()}`,
        rangeCompressed: Boolean(material.userData.textureRange),
        inputColorSpace: material.map?.userData?.inputColorSpace || null,
        skinResponse: material.userData.skinMaterialResponse || null,
      }));
    }),
  }),
};
resize();
