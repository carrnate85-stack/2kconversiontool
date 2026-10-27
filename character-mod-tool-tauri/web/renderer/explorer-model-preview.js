// Owned adapter based on the pinned, complete 2K Explorer renderer.
import * as THREE from './vendor/three.module.min.js';
import { DDSLoader } from './vendor/DDSLoader.js';
import { captureLightRig, captureSurfaceParameters, captureAuthoredCourtLights, captureAuthoredSun, captureDisplayParameters, captureEmissionTint } from './arena-capture.mjs';
import { configureArenaLayers } from './arena-layers.mjs';
import { configureScoreboardOverlay, configureCourtGraffiti } from './arena-overlays.mjs';
import { captureBasketFrame, catalogOverheadTarget, captureCreatedWoodTint } from './arena-catalog.mjs';
import { materialProfile, previewLightingProfile } from './render-profiles.mjs';
import { skinBoundaryResponse, skinLuminanceResponse } from '../skin-response.mjs';
import { configureStrandLightmap, configureStrandTangentRange, configureStrandSpecular, configureStrandCoverage } from '../strand-preview-fixes.mjs';
import { createStudioEnvironment, studioLightBalance, seamSmoothedNormals, vertexTangents, configureEyeIris, configureSkinShading } from '../studio-shading.mjs';


export function createExplorerRenderer(options = {}) {
const hostWindow = globalThis.window;
const window = {
  devicePixelRatio: hostWindow.devicePixelRatio,
  addEventListener: (...args) => hostWindow.addEventListener(...args),
  dispatchEvent: (event) => hostWindow.dispatchEvent(new CustomEvent(event.type, {
    detail: { ...(event.detail || {}), viewer: options.viewer || 'modelPreview' },
  })),
};
const canvas = options.canvas || document.getElementById('previewCanvas');
const scene = new THREE.Scene();
scene.background = options.transparent ? null : new THREE.Color(0x080b10);
scene.fog = new THREE.FogExp2(0x080b10, 0.00008);
const camera = new THREE.PerspectiveCamera(45, 1, 0.01, 1000000);
const renderer = new THREE.WebGLRenderer({ canvas, antialias: true, alpha: Boolean(options.transparent), powerPreference: 'high-performance' });
renderer.outputColorSpace = THREE.SRGBColorSpace;
renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, options.picker ? 1.25 : 2));

const ambientLight = new THREE.AmbientLight(0xffffff, 1.0);
scene.add(ambientLight);
const hemisphereLight = new THREE.HemisphereLight(0xe8f2ff, 0x54463f, 0);
scene.add(hemisphereLight);
const keyLight = new THREE.DirectionalLight(0xfff8f2, 0.85);
keyLight.position.set(2, 4, 3);
scene.add(keyLight);
scene.add(keyLight.target);
const fillLight = new THREE.DirectionalLight(0xc9dcff, 0);
fillLight.position.set(-4, 2, 3);
scene.add(fillLight, fillLight.target);
const rimLight = new THREE.DirectionalLight(0xff6a32, 0.22);
rimLight.position.set(-3, 1, -2);
scene.add(rimLight, rimLight.target);
const grid = new THREE.GridHelper(10000, 40, 0x4c5665, 0x20262f);
grid.material.opacity = 0.32;
grid.material.transparent = true;
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
let textureCache = [];
let textureItems = [];
let currentPayload = null;
let currentPreviewProfile = previewLightingProfile();
let captureEnvironment = null;
// Built once per renderer on first studio use; false after a failed build.
let studioEnvironment = null;
const captureLightGroup = new THREE.Group();
scene.add(captureLightGroup);
let drag = null;
let selectionHelper = null;
let loadGeneration = 0;
let textureBatchGeneration = 0;
const movementKeys = new Set();
let movementFrame = 0;
let movementTime = 0;
// Geometry-derived lookup data belongs to this load only, not a growing cache.
const skinBoundaryPairs = new Map();
let materialMetadataByName = new Map();
let cameraRenderFrame = 0;
let textureSampler = null;

function invalidateShadows() {
  // Static geometry/light rigs do not need another shadow pass just because
  // the viewer camera moved. Material/visibility/rig changes still invalidate.
  renderer.shadowMap.needsUpdate = true;
}

function scheduleCameraRender() {
  updateCamera(false);
  if (cameraRenderFrame) return;
  cameraRenderFrame = requestAnimationFrame(() => {
    cameraRenderFrame = 0;
    if (viewerIsActive()) { renderer.render(scene, camera); options.onCameraChange?.({ target: target.toArray(), yaw, pitch, distance, fittedDistance }); }
  });
}

function isPlacedArena(payload = currentPayload) {
  const stats = payload?.stats || {};
  return Number(stats.objectMatrices || 0) >= 20
    && Number(stats.instances || 0) > Number(stats.modelsDecoded || 0);
}

function configurePreviewLighting(payload = currentPayload) {
  currentPreviewProfile = previewLightingProfile(payload || {});
  captureLightGroup.clear();
  renderer.toneMapping = currentPreviewProfile.toneMapping === 'neutral'
    ? THREE.NeutralToneMapping : THREE.NoToneMapping;
  renderer.toneMappingExposure = currentPreviewProfile.exposure;
  scene.background?.set(0x080b10);
  scene.fog.color.set(0x080b10);
  camera.fov = 45;
  camera.updateProjectionMatrix();
  scene.environment = null;
  captureEnvironment?.dispose();
  captureEnvironment = null;
  renderer.shadowMap.enabled = false;
  renderer.localClippingEnabled = false;
  renderer.shadowMap.autoUpdate = true;
  invalidateShadows();
  keyLight.castShadow = false;
  keyLight.shadow.map?.dispose();
  keyLight.shadow.map = null;
  keyLight.position.set(2, 4, 3);
  keyLight.target.position.set(0, 0, 0);
  fillLight.position.set(-4, 2, 3);
  fillLight.target.position.set(0, 0, 0);
  rimLight.position.set(-3, 1, -2);
  rimLight.target.position.set(0, 0, 0);
  keyLight.color.set(0xfff8f2);
  fillLight.color.set(0xc9dcff);
  rimLight.color.set(0xff6a32);
  ambientLight.intensity = currentPreviewProfile.ambient;
  hemisphereLight.intensity = currentPreviewProfile.hemisphere;
  keyLight.intensity = currentPreviewProfile.key;
  fillLight.intensity = currentPreviewProfile.fill;
  rimLight.intensity = currentPreviewProfile.rim;
  scene.environmentIntensity = 1;
  const environment = currentPreviewProfile.kind === 'arena' ? null : studioEnvironmentTexture();
  if (environment) {
    // Reflections and image-based fill replace most of the flat ambient term.
    const balance = studioLightBalance(currentPreviewProfile);
    scene.environment = environment;
    scene.environmentIntensity = balance.environmentIntensity;
    ambientLight.intensity = balance.ambient;
    hemisphereLight.intensity = balance.hemisphere;
  }
}

function studioEnvironmentTexture() {
  if (studioEnvironment === null) {
    try { studioEnvironment = createStudioEnvironment(THREE, renderer); } catch { studioEnvironment = false; }
  }
  return studioEnvironment ? studioEnvironment.texture : null;
}

function configureStudioLights(bounds) {
  if (!currentPreviewProfile.shadows || bounds.isEmpty()) return;
  const center = bounds.getCenter(new THREE.Vector3());
  const size = bounds.getSize(new THREE.Vector3());
  const radius = Math.max(size.x, size.y, size.z, 0.01);
  const place = (light, direction) => {
    light.position.copy(center).addScaledVector(new THREE.Vector3(...direction).normalize(), radius * 2.6);
    light.target.position.copy(center);
    light.target.updateMatrixWorld(true);
  };
  place(keyLight, [1.5, 2.3, 2]);
  place(fillLight, [-2.1, 1.15, 1.3]);
  place(rimLight, [-1.4, 1.7, -2.2]);
  renderer.shadowMap.enabled = options.shadows !== false;
  // r185 replaces PCFSoftShadowMap with PCFShadowMap; radius sets the softness.
  renderer.shadowMap.type = THREE.PCFShadowMap;
  renderer.shadowMap.autoUpdate = false;
  invalidateShadows();
  keyLight.castShadow = options.shadows !== false;
  if (keyLight.shadow.mapSize.x !== 2048) { keyLight.shadow.map?.dispose(); keyLight.shadow.map = null; }
  keyLight.shadow.mapSize.set(2048, 2048);
  keyLight.shadow.radius = 3;
  keyLight.shadow.bias = -0.0002;
  keyLight.shadow.normalBias = radius * 0.0006;
  const shadowCamera = keyLight.shadow.camera;
  const extent = radius * 0.72;
  shadowCamera.left = shadowCamera.bottom = -extent;
  shadowCamera.right = shadowCamera.top = extent;
  shadowCamera.near = Math.max(0.01, radius * 0.05);
  shadowCamera.far = radius * 6;
  shadowCamera.updateProjectionMatrix();
  modelRoot.traverse((object) => {
    if (!object.isMesh) return;
    const materials = Array.isArray(object.material) ? object.material : [object.material];
    object.castShadow = materials.some((material) => material && !material.transparent);
    object.receiveShadow = true;
  });
}

function colorFor(name) {
  const normalized = String(name || '').toLowerCase();
  if (/eye(?:ball|white)|sclera/.test(normalized)) return new THREE.Color(0xd8d4c8);
  if (/iris|pupil/.test(normalized)) return new THREE.Color(0x332b26);
  if (/caruncle|teartube/.test(normalized)) return new THREE.Color(0x9a5d5c);
  if (/teeth/.test(normalized)) return new THREE.Color(0xd7d1bf);
  if (/tongue|mouth|gum/.test(normalized)) return new THREE.Color(0x7f4948);
  if (/facial.?hair|beard|brow|lash|(?:^|_)hair(?:_|$)/.test(normalized)) return new THREE.Color(0x292624);
  if (/(?:^|_)(?:skin|face|head|body)(?:_|$)/.test(normalized)) return new THREE.Color(0x9b7868);
  let hash = 2166136261;
  for (const char of String(name)) hash = Math.imul(hash ^ char.charCodeAt(0), 16777619);
  const color = new THREE.Color();
  color.setHSL(((hash >>> 0) % 360) / 360, 0.07, 0.52 + ((hash >>> 8) % 8) / 100);
  return color;
}

function updateCamera(draw = true) {
  if (draw && cameraRenderFrame) {
    cancelAnimationFrame(cameraRenderFrame);
    cameraRenderFrame = 0;
  }
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
  if (draw) { renderer.render(scene, camera); options.onCameraChange?.({ target: target.toArray(), yaw, pitch, distance, fittedDistance }); }
}

function viewerIsActive() {
  if (options.interactive === false || canvas.closest('[hidden]')) return false;
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
  if (movementKeys.has('ArrowUp')) movement.add(forward);
  if (movementKeys.has('ArrowDown')) movement.sub(forward);
  if (movementKeys.has('KeyD') || movementKeys.has('ArrowRight')) movement.add(right);
  if (movementKeys.has('KeyA') || movementKeys.has('ArrowLeft')) movement.sub(right);
  if (movementKeys.has('KeyW') || movementKeys.has('Space')) movement.y += 1;
  if (movementKeys.has('KeyS') || movementKeys.has('KeyC')) movement.y -= 1;
  let changed = false;
  if (movement.lengthSq()) {
    const speed = Math.max(fittedDistance, 0.01) * (movementKeys.has('ShiftLeft') || movementKeys.has('ShiftRight') ? 1.25 : 0.42);
    target.addScaledVector(movement.normalize(), speed * delta);
    changed = true;
  }
  const zoomDirection = (movementKeys.has('KeyE') ? 1 : 0) - (movementKeys.has('KeyQ') ? 1 : 0);
  if (zoomDirection) {
    const zoomRate = movementKeys.has('ShiftLeft') || movementKeys.has('ShiftRight') ? 4 : 2;
    distance *= Math.exp(zoomDirection * zoomRate * delta);
    changed = true;
  }
  if (changed) updateCamera();
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
  if (cameraRenderFrame) cancelAnimationFrame(cameraRenderFrame);
  cameraRenderFrame = 0;
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

function disposeRoot(keepTextures = new Set()) {
  if (textureSampler) {
    textureSampler.geometry.dispose(); textureSampler.material.dispose();
    textureSampler.target.dispose(); textureSampler = null;
  }
  skinBoundaryPairs.clear();
  clearPick(false, false);
  const geometries = new Set();
  const materialsToDispose = new Set();
  while (modelRoot.children.length) {
    const child = modelRoot.children.at(-1);
    modelRoot.remove(child);
    // InstancedMesh owns additional GPU buffers beyond its shared geometry.
    child.dispose?.();
    if (child.geometry) geometries.add(child.geometry);
    const materials = Array.isArray(child.material) ? child.material : [child.material];
    materials.filter(Boolean).forEach((material) => {
      disposeDerivedTextures(material);
      materialsToDispose.add(material);
    });
  }
  geometries.forEach((geometry) => geometry.dispose());
  materialsToDispose.forEach((material) => material.dispose());
  textureCache.forEach((texture) => { if (!keepTextures.has(texture)) texture?.dispose(); });
  textureCache = [];
  textureItems = [];
}

function clearPick(notify = true, draw = true) {
  if (selectionHelper) {
    scene.remove(selectionHelper);
    selectionHelper.geometry?.dispose();
    selectionHelper.material?.dispose();
    selectionHelper = null;
  }
  if (notify) window.dispatchEvent(new CustomEvent('model-preview:pick', { detail: null }));
  if (draw) renderer.render(scene, camera);
}

function typed(value, Type) {
  if (value instanceof Type) return value;
  if (ArrayBuffer.isView(value)) return new Type(value.buffer, value.byteOffset, value.byteLength / Type.BYTES_PER_ELEMENT);
  return new Type(value || []);
}

function indexArray(value) {
  // WebGL reserves 0xffff for primitive restart. Keep that index (and larger
  // ones) in a 32-bit buffer, but halve storage for ordinary small meshes.
  const indices = value instanceof Uint32Array ? value : new Uint32Array(value || []);
  for (const index of indices) if (index >= 65535) return indices;
  return new Uint16Array(indices);
}

function nextFrame() {
  return new Promise((resolve) => requestAnimationFrame(resolve));
}

function cancelLoad() {
  loadGeneration += 1;
}

function fitCamera(draw = true) {
  const bounds = new THREE.Box3().setFromObject(modelRoot);
  if (bounds.isEmpty()) {
    target.set(0, 0, 0);
    fittedDistance = distance = 10;
  } else {
    bounds.getCenter(target);
    const size = bounds.getSize(new THREE.Vector3());
    const radius = Math.max(size.x, size.y, size.z, 0.01);
    if (isPlacedArena()) target.set(0, 0, 0);
    fittedDistance = distance = radius / (2 * Math.tan(THREE.MathUtils.degToRad(camera.fov / 2))) * 1.35;
    grid.position.y = bounds.min.y;
    grid.scale.setScalar(Math.max(0.01, radius / 10000));
  }
  configureStudioLights(bounds);
  yaw = Math.PI / 4;
  pitch = Math.PI / 5;
  updateCamera(draw);
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

// Offline catalog captures share a court-relative camera, independent of the
// arena's roof/bowl bounds. Basket placement origins are at floor level.
function setArenaCaptureView(options = {}) {
  invalidateShadows();
  if (!currentPayload?.stats?.arena) throw new Error('Load an arena before setting a capture view.');
  stopCameraMovement();
  if (selectionHelper) clearPick(false);
  modelRoot.updateMatrixWorld(true);
  const anchors = [];
  for (const object of modelRoot.children) {
    if (!object.isInstancedMesh) continue;
    const names = object.userData.previewInstanceTargets || [];
    names.forEach((name, index) => {
      if (!/(?:^|:)basket$/i.test(name)) return;
      const matrix = new THREE.Matrix4();
      object.getMatrixAt(index, matrix);
      matrix.premultiply(object.matrixWorld);
      const point = new THREE.Vector3().setFromMatrixPosition(matrix);
      if (!anchors.some(other => other.distanceTo(point) < 1)) anchors.push(point);
    });
  }
  anchors.sort((a, b) => a.z - b.z || a.x - b.x);
  const basketFrame = captureBasketFrame(anchors.map(point=>point.toArray()));
  const anchored = Boolean(basketFrame);
  if (!anchored && options.sceneFallback) return setArenaSceneCaptureView(options);
  const center = anchored ? new THREE.Vector3(...basketFrame.center) : new THREE.Vector3();
  const longAxis = anchored ? new THREE.Vector3(...basketFrame.axis) : new THREE.Vector3(0, 0, 1);
  const sideAxis = new THREE.Vector3(longAxis.z, 0, -longAxis.x);
  const length = anchored ? basketFrame.length : 2865;
  const width = anchored ? basketFrame.width : length*.534;
  const azimuthDegrees = Number(options.azimuthDegrees ?? 245);
  const elevationDegrees = Number(options.elevationDegrees ?? 35);
  const distanceInCourtLengths = Number(options.distanceInCourtLengths ?? 1.3);
  if (![azimuthDegrees, elevationDegrees, distanceInCourtLengths].every(Number.isFinite)
    || elevationDegrees <= 0 || elevationDegrees >= 90 || distanceInCourtLengths <= 0) {
    throw new Error('Invalid arena capture camera.');
  }
  const azimuth = THREE.MathUtils.degToRad(azimuthDegrees);
  const horizontal = longAxis.clone().multiplyScalar(Math.cos(azimuth))
    .addScaledVector(sideAxis, Math.sin(azimuth));
  yaw = Math.atan2(horizontal.x, horizontal.z);
  pitch = THREE.MathUtils.degToRad(elevationDegrees);
  const requestedDistance = length * distanceInCourtLengths;
  distance = requestedDistance;
  const cutawayObjects = options.catalogCutaway ? applyCatalogRoofCutaway(center.y) : [];
  let ceilingHeight = null;
  for (const object of modelRoot.children) {
    if (!object.visible) continue;
    if (!/(?:^|[:_])(?:roof|ceiling)(?:_|$)/i.test(object.name.split(':').at(-1))) continue;
    const bounds = new THREE.Box3().setFromObject(object);
    const size = bounds.getSize(new THREE.Vector3());
    if (size.x < length * 0.5 || size.z < length * 0.5 || bounds.min.y <= center.y + 500) continue;
    ceilingHeight = Math.min(ceilingHeight ?? Infinity, bounds.min.y);
  }
  if (ceilingHeight !== null) distance = Math.min(distance, (ceilingHeight - center.y - 300) / Math.sin(pitch));
  // A very low gym cannot fit an elevated full-court view inside its roof
  // without an almost 180-degree lens. Use a documented roof cutaway instead.
  if (options.cutaway !== false && (distance < requestedDistance * 0.45 || options.catalogCutaway && distance < requestedDistance)) {
    distance = requestedDistance;
    for (const object of modelRoot.children) {
      if (!/roof|ceiling|truss|rafter|duct|pillar|light_hanging|lights_hanging|light_stage|light_spot/i.test(object.name)) continue;
      const bounds = new THREE.Box3().setFromObject(object);
      if (bounds.min.y <= center.y + 500 && !/truss|rafter|duct|pillar/i.test(object.name)) continue;
      object.visible = false;
      cutawayObjects.push(object.name);
    }
  }
  target.copy(center);
  // Keep the requested view angle inside a low gym roof. Widen the lens to
  // compensate for the shorter distance instead of photographing the roof.
  camera.fov = THREE.MathUtils.radToDeg(2 * Math.atan(Math.tan(THREE.MathUtils.degToRad(45 / 2)) * requestedDistance / distance));
  grid.visible = false;
  updateCamera(false);
  camera.updateMatrixWorld(true);
  const framingPoints = [];
  for (const x of [-1, 1]) for (const z of [-1, 1]) {
    framingPoints.push(center.clone().addScaledVector(sideAxis, x * width*.5).addScaledVector(longAxis, z * length * 0.502));
  }
  for (const end of [-1, 1]) for (const x of [-1, 1]) for (const height of [0, 600]) {
    framingPoints.push(center.clone().addScaledVector(longAxis, end * length * 0.63)
      .addScaledVector(sideAxis, x * 110).add(new THREE.Vector3(0, height, 0)));
  }
  if(anchors.length>2) for(const anchor of anchors) for(const x of [-110,110]) for(const height of [0,600])
    framingPoints.push(anchor.clone().addScaledVector(sideAxis,x).add(new THREE.Vector3(0,height,0)));
  let tangent = Math.tan(THREE.MathUtils.degToRad(camera.fov / 2));
  for (const point of framingPoints) {
    const view = point.clone().applyMatrix4(camera.matrixWorldInverse);
    if (view.z >= 0) throw new Error('The capture camera is too close to frame the court.');
    tangent = Math.max(tangent, Math.abs(view.x) / (-view.z * camera.aspect) * 1.08, Math.abs(view.y) / -view.z * 1.08);
  }
  camera.fov = THREE.MathUtils.radToDeg(2 * Math.atan(tangent));
  camera.updateProjectionMatrix();
  renderer.render(scene, camera);
  return {
    preset: 'elevated-corner-v2', azimuthDegrees, elevationDegrees,
    distanceInCourtLengths, courtLength: length, anchorSource: anchored ? 'basket placements' : 'NBA court fallback',
    basketAnchors: anchors.map(point => point.toArray()),
    courtWidth:width, longAxis:longAxis.toArray(),
    position: camera.position.toArray(), target: target.toArray(), fov: camera.fov,
    actualDistance: distance, ceilingHeight, ceilingAdjustment: distance < requestedDistance, cutawayObjects,
    frameBounds: framingPoints.map(point => point.clone().project(camera).toArray()),
  };
}

function capturePng() {
  renderer.render(scene, camera);
  return canvas.toDataURL('image/png');
}

function inspectCaptureVisibility() {
  camera.updateMatrixWorld(true);modelRoot.updateMatrixWorld(true);
  const visible=modelRoot.children.filter(object=>object.visible);
  return [[0,0],[-.4,-.3],[.4,-.3],[-.4,.3],[.4,.3]].map(([x,y])=>{
    raycaster.setFromCamera(new THREE.Vector2(x,y),camera);
    const hits=raycaster.intersectObjects(visible,false);
    return {ndc:[x,y],hits:hits.slice(0,3).map(hit=>({name:hit.object.name,distance:hit.distance,
      point:hit.point.toArray(),material:(Array.isArray(hit.object.material)?hit.object.material[hit.face?.materialIndex||0]:hit.object.material)?.name}))};
  });
}

function applyCatalogRoofCutaway(floorHeight = 0) {
  const names=[];
  for(const object of modelRoot.children){
    const leaf=object.name.split(':').at(-1);
    if(/basket|scorecube|scoreboard/i.test(leaf))continue;
    if(object.isInstancedMesh){
      const targets=object.userData.previewInstanceTargets || [];
      const overhead=targets.map((name,index)=>catalogOverheadTarget(name)?index:-1).filter(index=>index>=0);
      if(overhead.length){
        if(overhead.length===object.count){object.visible=false;names.push(object.name+' [overhead scene group]');continue;}
        // Shared geometry can also appear on lower walls. Remove only the
        // placements inside overhead groups, keeping those other instances.
        const empty=new THREE.Matrix4().makeScale(0,0,0);
        for(const index of overhead)object.setMatrixAt(index,empty);
        object.instanceMatrix.needsUpdate=true;object.computeBoundingBox();object.computeBoundingSphere();
        names.push(object.name+` [${overhead.length} overhead instances hidden]`);
      }
    }else if(catalogOverheadTarget(object.name)){
      object.visible=false;names.push(object.name+' [overhead scene group]');continue;
    }
    if(/roof|ceiling|truss|rafter|duct|hvac|domeframe|domeglass|(?:^|_)beam|pillar/i.test(leaf)){
      object.visible=false;names.push(object.name);
    }else if(/(?:^|_)shell(?:_|$)/i.test(leaf) || /@shell(?:s|_[^@]+)?_grp@/i.test(object.name)){
      // Some gyms bake roofs and walls into one shell. Clip its upper portion
      // rather than discarding the room's lower walls and floor together.
      object.userData.captureClipHeight=floorHeight+600;
      renderer.localClippingEnabled=true;names.push(object.name+' [upper shell clipped]');
    }
  }
  return names;
}

// Picker entries also include lobby/menu scenes without a pair of baskets.
// Fit their actual static bounds rather than implying a standard NBA court.
function setArenaSceneCaptureView(options = {}) {
  const cutawayObjects=options.catalogCutaway ? applyCatalogRoofCutaway() : [];
  let box = new THREE.Box3();
  for(const object of modelRoot.children)if(object.visible)box.union(new THREE.Box3().setFromObject(object));
  const courtBox=new THREE.Box3();
  for(const object of modelRoot.children){
    const materials=Array.isArray(object.material)?object.material:[object.material];
    if(!object.visible || !materials.some(material=>/^floor_/i.test(material.name)))continue;
    const floorBounds=new THREE.Box3().setFromObject(object),size=floorBounds.getSize(new THREE.Vector3());
    if(size.y<50 && size.x*size.z>1000000)courtBox.union(floorBounds);
  }
  const floorFraming=!courtBox.isEmpty();
  if(floorFraming){box=courtBox;box.max.y+=600;}
  if (box.isEmpty()) throw new Error('No static scene geometry was found.');
  const size = box.getSize(new THREE.Vector3());
  if (![...box.min.toArray(), ...box.max.toArray()].every(Number.isFinite)) throw new Error('Invalid static scene bounds.');
  target.copy(box.getCenter(new THREE.Vector3()));
  yaw = THREE.MathUtils.degToRad(Number(options.azimuthDegrees ?? 245));
  pitch = THREE.MathUtils.degToRad(Number(options.elevationDegrees ?? 35));
  distance = Math.max(size.x, size.y, size.z, 1) * 1.8;
  camera.fov = 45;
  grid.visible = false;
  updateCamera(false);
  camera.updateMatrixWorld(true);
  const corners=[];
  for(const x of [box.min.x,box.max.x]) for(const y of [box.min.y,box.max.y]) for(const z of [box.min.z,box.max.z]) {
    corners.push(new THREE.Vector3(x,y,z));
  }
  let tangent=Math.tan(THREE.MathUtils.degToRad(camera.fov/2));
  for(const corner of corners){
    const p=corner.clone().applyMatrix4(camera.matrixWorldInverse);
    if(p.z>=0) throw new Error('Static scene cannot be framed.');
    tangent=Math.max(tangent,Math.abs(p.x)/(-p.z*camera.aspect)*1.08,Math.abs(p.y)/-p.z*1.08);
  }
  camera.fov=THREE.MathUtils.radToDeg(2*Math.atan(tangent));camera.updateProjectionMatrix();
  return {preset:'static-scene-overview-v1',anchorSource:floorFraming?'authored floor bounds; no basket pair':'static scene bounds; no basket pair',
    azimuthDegrees:Number(options.azimuthDegrees??245),elevationDegrees:Number(options.elevationDegrees??35),
    target:target.toArray(),position:camera.position.toArray(),fov:camera.fov,courtLength:Math.max(size.x,size.z,1),
    ceilingHeight:null,basketAnchors:[],cutawayObjects,bounds:{min:box.min.toArray(),max:box.max.toArray()},
    frameBounds:corners.map(point=>point.clone().project(camera).toArray())};
}

function setArenaDisplayCaptureView(options = {}) {
  if (!currentPayload?.stats?.arena) throw new Error('Load an arena before capturing its physical scoreboard.');
  stopCameraMovement();
  modelRoot.updateMatrixWorld(true);
  const candidates = modelRoot.children.filter(object => object.visible
    && /scorecube|scoreboard/i.test(object.name) && !/spotlight|tarp|scoretable/i.test(object.name));
  const box = new THREE.Box3();
  const names = [];
  for (const object of candidates) {
    // Suspension cables share the scorecube model and can run all the way
    // through the roof. Fit the screen primitives, not that full-model bound.
    const materials = Array.isArray(object.material) ? object.material : [object.material];
    const screenGroups = object.geometry.groups.filter(group => materials[group.materialIndex]?.userData.arenaDisplay);
    if (!screenGroups.length) continue;
    const localBox = new THREE.Box3();
    const positions = object.geometry.getAttribute('position');
    const indices = object.geometry.index;
    for (const group of screenGroups) for (let cursor = group.start; cursor < group.start + group.count; cursor++) {
      localBox.expandByPoint(new THREE.Vector3().fromBufferAttribute(positions, indices ? indices.getX(cursor) : cursor));
    }
    const objectBox = new THREE.Box3();
    for (let index = 0; index < (object.isInstancedMesh ? object.count : 1); index++) {
      const matrix = new THREE.Matrix4();
      if (object.isInstancedMesh) object.getMatrixAt(index, matrix);
      matrix.premultiply(object.matrixWorld);
      objectBox.union(localBox.clone().applyMatrix4(matrix));
    }
    if (objectBox.isEmpty() || ![...objectBox.min.toArray(), ...objectBox.max.toArray()].every(Number.isFinite)) continue;
    box.union(objectBox);
    names.push(object.name);
  }
  if (box.isEmpty()) throw new Error('No physical scoreboard geometry was found.');
  const size = box.getSize(new THREE.Vector3());
  target.copy(box.getCenter(new THREE.Vector3()));
  yaw = THREE.MathUtils.degToRad(Number(options.azimuthDegrees ?? 245));
  pitch = THREE.MathUtils.degToRad(Number(options.elevationDegrees ?? 8));
  distance = Math.max(...size.toArray()) * 1.8;
  camera.fov = 45;
  grid.visible = false;
  updateCamera(false);
  camera.updateMatrixWorld(true);
  const corners = [];
  for (const x of [box.min.x, box.max.x]) for (const y of [box.min.y, box.max.y]) for (const z of [box.min.z, box.max.z]) {
    corners.push(new THREE.Vector3(x, y, z));
  }
  let tangent = Math.tan(THREE.MathUtils.degToRad(camera.fov / 2));
  for (const corner of corners) {
    const p = corner.clone().applyMatrix4(camera.matrixWorldInverse);
    if (p.z >= 0) throw new Error('Physical scoreboard capture cannot be framed.');
    tangent = Math.max(tangent, Math.abs(p.x) / (-p.z * camera.aspect) * 1.08, Math.abs(p.y) / -p.z * 1.08);
  }
  camera.fov = THREE.MathUtils.radToDeg(2 * Math.atan(tangent));
  camera.updateProjectionMatrix();
  return { preset: 'physical-scoreboard-v1', objects: names, target: target.toArray(), position: camera.position.toArray(),
    fov: camera.fov, bounds: { min: box.min.toArray(), max: box.max.toArray() },
    frameBounds: corners.map(p => p.clone().project(camera).toArray()) };
}

function configureCourtLogos(material, parameter) {
  const logos = [];
  for (const [index, item] of textureItems.entries()) {
    for (const binding of item.assignments || []) {
      if (binding.channel !== 'logo' || binding.material.toLowerCase() !== material.name.toLowerCase() || !textureCache[index]) continue;
      const prefix = binding.slot.replace(/Texture$/i, '');
      const angle = Number(parameter[`${prefix}Rotation`] || 0) * Math.PI / 180;
      const scaleX = Number(parameter[`${prefix}ScaleX`] ?? 25);
      const scaleY = Number(parameter[`${prefix}ScaleY`] ?? 25);
      logos.push({ slot: binding.slot, texture: textureCache[index],
        offset: [Number(parameter[`${prefix}OffsetX`] || 0), Number(parameter[`${prefix}OffsetY`] || 0)],
        scale: [scaleX, scaleY], angle, fullCourt: scaleX > 0 && scaleY > 0 && scaleX < 1 && scaleY < 1
          && /_court(?:\.|$)/i.test(item.name) && textureCache[index].image.width === textureCache[index].image.height * 2 });
    }
  }
  logos.sort((a, b) => a.slot.localeCompare(b.slot, undefined, { numeric: true }));
  if (!logos.length) return [];
  const previous = material.onBeforeCompile;
  const previousKey = material.customProgramCacheKey();
  material.onBeforeCompile = shader => {
    previous(shader);
    // TEXCOORD2 tiles every 128 game units. Use the court projection extent
    // declared by the floor material, rather than the wood texture's UVs.
    shader.uniforms.courtProjectionExtent = { value: Number(parameter.FloorIBLSize?.[2] || 4000) / 128 };
    shader.vertexShader = 'attribute vec2 courtUv; varying vec2 vCourtUv;\n' + shader.vertexShader;
    shader.vertexShader = shader.vertexShader.replace('#include <uv_vertex>', '#include <uv_vertex>\nvCourtUv = courtUv;');
    let declarations = 'varying vec2 vCourtUv; uniform float courtProjectionExtent;\n';
    let composition = '';
    logos.forEach((logo, index) => {
      shader.uniforms[`courtLogo${index}`] = { value: logo.texture };
      shader.uniforms[`courtTransform${index}`] = { value: new THREE.Vector4(...logo.offset, ...logo.scale) };
      shader.uniforms[`courtRotation${index}`] = { value: new THREE.Vector2(Math.cos(logo.angle), Math.sin(logo.angle)) };
      declarations += `uniform sampler2D courtLogo${index}; uniform vec4 courtTransform${index}; uniform vec2 courtRotation${index};\n`;
      composition += `{
        vec2 p = (vCourtUv - vec2(0.5)) ${logo.fullCourt ? '* 0.5' : '/ courtProjectionExtent'} - courtTransform${index}.xy;
        vec2 r = courtRotation${index};
        vec2 q = vec2(r.x*p.x + r.y*p.y, -r.y*p.x + r.x*p.y) * courtTransform${index}.zw + vec2(0.5);
        if (q.x >= 0.0 && q.x <= 1.0 && q.y >= 0.0 && q.y <= 1.0) {
          vec4 artwork = texture2D(courtLogo${index}, q);
          diffuseColor.rgb = mix(diffuseColor.rgb, artwork.rgb, artwork.a);
        }
      }\n`;
    });
    shader.fragmentShader = declarations + shader.fragmentShader;
    shader.fragmentShader = shader.fragmentShader.replace('#include <alphamap_fragment>', composition + '#include <alphamap_fragment>');
  };
  material.customProgramCacheKey = () => `${previousKey}:court-logos-v2:${logos.map(logo => Number(logo.fullCourt)).join(',')}`;
  return logos.map(({ slot, texture, offset, scale, angle, fullCourt }) => ({ slot, texture: texture.name, offset, scale, rotationDegrees: angle * 180 / Math.PI, fullCourt }));
}

function setArenaCaptureAppearance(definitions = {}, options = {}) {
  invalidateShadows();
  if (!currentPayload?.stats?.arena) throw new Error('Load an arena before setting capture materials.');
  if (options.lighting && !options.preserveLighting) {
    ambientLight.intensity = 0.85;
    keyLight.intensity = 0.7;
    keyLight.color.set(0xfffaf5);
    rimLight.intensity = 0.18;
    rimLight.color.set(0xdce8ff);
  }
  const changes = [];
  for (const object of modelRoot.children) {
    if (/^crowd_cards$/i.test(object.name)) object.visible = false;
    const originals = Array.isArray(object.material) ? object.material : [object.material];
    object.material = originals.map(original => {
      const definition = definitions[original.name.toLowerCase()] || {};
      const parameter = definition.parameter || {};
      const defaultColor = parameter.DetailDefaultAlbedo || parameter.DefaultAlbedo;
      const modulate = parameter.AlbedoModulate || parameter.ModulateAlbedo || parameter.LAYER0_AlbedoModulate;
      const color = new THREE.Color(1, 1, 1);
      const woodTint=captureCreatedWoodTint(original.name,definition.shader,parameter);
      if (woodTint) color.setRGB(...woodTint,THREE.SRGBColorSpace);
      else if (!original.map && Array.isArray(defaultColor)) color.setRGB(defaultColor[0] ?? 0, defaultColor[1] ?? 0, defaultColor[2] ?? 0, THREE.SRGBColorSpace);
      else if (!original.map) color.copy(original.color);
      if (Array.isArray(modulate)) color.multiply(new THREE.Color().setRGB(modulate[0] ?? 0, modulate[1] ?? 0, modulate[2] ?? 0, THREE.SRGBColorSpace));
      const floorShader = /^floor_/i.test(original.name) && /^floor\.fx(?:#|$)/i.test(definition.shader || '');
      const isLine = /:line_\d+_mat$/i.test(original.name) || (floorShader &&
        (/:floor_line_/i.test(original.name) || definition.resourceSlots?.includes('LineTexture')));
      const isFloorArea = /^floor_/i.test(original.name) && (/:area_\d+_mat$/i.test(original.name) || (floorShader && !isLine));
      const glass = /glass_backboard/i.test(original.name);
      const alpha = String(parameter.AlphaStyle?.Value || parameter.AlphaStyle || 'OPAQUE');
      const foliageCutout = /^speedtree_(?:leaves|fronds)\.fx/i.test(definition.shader || '');
      const cull = String(parameter.CullMode?.Value || parameter.CullMode || 'CW');
      const Material = options.lighting && !glass
        ? options.pbr ? THREE.MeshStandardMaterial : !isLine ? THREE.MeshPhongMaterial : THREE.MeshBasicMaterial
        : THREE.MeshBasicMaterial;
      const material = new Material({ name: original.name, map: original.map, color,
        side: cull === 'NONE' || foliageCutout ? THREE.DoubleSide : THREE.FrontSide,
        transparent: glass || isLine || /blend|transparent|translucent/i.test(alpha),
        opacity: glass ? 0.12 : Number(parameter.DefaultAlpha ?? 1),
        alphaTest: foliageCutout ? 0.35 : isLine || /mask|test|cutout/i.test(alpha) ? 0.05 : 0,
        depthWrite: !glass && !isLine, polygonOffset: isLine || isFloorArea,
        polygonOffsetFactor: isLine ? -4 : isFloorArea ? -2 : 0,
        polygonOffsetUnits: isLine ? -4 : isFloorArea ? -2 : 0,
      });
      if(floorShader && original.map && definition.resourceSlots?.includes('BaseTexture')
        && Array.isArray(parameter.BaseTextureScale) && Array.isArray(parameter.BaseTextureOffset)
        && [...parameter.BaseTextureScale.slice(0,2),...parameter.BaseTextureOffset.slice(0,2)].every(Number.isFinite)){
        // Created floors select the wood region of a shared atlas. Sampling
        // the whole atlas also paints its white padding onto the court.
        const derived=original.map.clone();
        derived.repeat.fromArray(parameter.BaseTextureScale);derived.offset.fromArray(parameter.BaseTextureOffset);
        if(object.geometry.getAttribute('graffitiUv')){
          object.geometry.setAttribute('uv1',object.geometry.getAttribute('graffitiUv'));derived.channel=1;
        }
        material.map=derived;material.userData.derivedTextures=[derived];
      }
      if(floorShader && isLine && original.map && definition.resourceSlots?.includes('LineTexture')
        && !parameter.CreatedFloor && object.geometry.getAttribute('detailUv')){
        // The explicit line atlas follows the ribbon coordinates on TEXCOORD2;
        // asphalt/wood UV0 repeats in world space and breaks the painted arcs.
        // CreatedFloor variants instead place their ribbon atlas on UV0.
        const derived=original.map.clone();derived.channel=2;
        object.geometry.setAttribute('uv2',object.geometry.getAttribute('detailUv'));
        material.map=derived;material.userData.derivedTextures=[derived];
      }
      if(Number.isFinite(object.userData.captureClipHeight))
        material.clippingPlanes=[new THREE.Plane(new THREE.Vector3(0,-1,0),object.userData.captureClipHeight)];
      if (material.isMeshStandardMaterial) {
        Object.assign(material, captureSurfaceParameters(parameter, isFloorArea || isLine));
        material.envMapIntensity = 0.7;
        const normal = boundArenaTexture(material.name, 'normal');
        if (normal) {
          material.normalMap = captureSurfaceTexture(normal, parameter, material);
          material.normalScale.set(0.12, 0.12);
        }
        const roughness = boundArenaTexture(material.name, 'roughness');
        if (roughness) {
          material.roughnessMap = captureSurfaceTexture(roughness, parameter, material);
          // The map already holds absolute roughness; avoid multiplying it by
          // the fallback value used for untextured surfaces.
          material.roughness = 1;
        }
        const ao = boundArenaTexture(material.name, 'ao');
        if (ao) {
          material.aoMap = captureSurfaceTexture(ao, parameter, material, 'ConeAngle');
          material.aoMapIntensity = 0.65;
        }
        const metalness = boundArenaTexture(material.name, 'metalness');
        if (metalness) {
          material.metalnessMap = captureSurfaceTexture(metalness, parameter, material, 'MetalDense');
          material.metalness = 1;
        }
      }
      if (material.isMeshPhongMaterial || material.isMeshStandardMaterial) {
        if (material.isMeshPhongMaterial) {
          material.shininess = isFloorArea ? 40 : 12;
          material.specular.set(isFloorArea ? 0x252525 : 0x101010);
        }
        const emissive = boundArenaTexture(material.name, 'emissive');
        if (emissive && Number(parameter.EmissiveIntensity || 0) > 0) {
          material.emissiveMap = emissive;
          const tint = captureEmissionTint(parameter.EmissiveTint);
          material.emissive.setRGB(tint[0], tint[1], tint[2], THREE.SRGBColorSpace);
          // Game photometric units are not Three.js units. Bound the preview
          // response so a lit fixture remains readable without blowing out.
          material.emissiveIntensity = Math.min(1.5, Math.log10(1 + Number(parameter.EmissiveIntensity)) / 3);
        }
        const display = captureDisplayParameters(definition.shader, parameter, Boolean(material.map),
          { digits: Boolean(definition.captureDigitAtlas) });
        if (display) {
          material.emissiveMap = material.map;
          material.emissive.copy(material.color);
          material.emissiveIntensity = display.emission;
          material.color.multiplyScalar(display.surface);
          if (material.isMeshStandardMaterial) { material.metalness = 0; material.roughness = 1; material.envMapIntensity = 0; }
          else material.specular.set(0x000000);
          material.userData.arenaDisplay = display;
          if (display.kind === 'digits') {
            material.transparent = true;
            material.alphaTest = .02;
            material.depthWrite = false;
          }
        }
      }
      configureTextureRange(material, definition.resourceSlots?.includes('CombinedLayerMaskMap') ? null : original.map);
      const layers = configureArenaLayers(material, definition, (channel, slot) => boundArenaTextureSlot(material.name, channel, slot),
        Boolean(object.geometry.getAttribute('detailUv')));
      const overlay = configureScoreboardOverlay(material, definition,
        boundArenaTextureSlot(material.name, 'overlay', 'AddOn1AlbedoMap'), Boolean(object.geometry.getAttribute('overlayUv')));
      const graffiti = configureCourtGraffiti(material, definition,
        boundArenaTextureSlot(material.name,'overlay','GraffitiAlbedoMap'),Boolean(object.geometry.getAttribute('graffitiUv')));
      const logos = object.geometry.getAttribute('courtUv') ? configureCourtLogos(material, parameter) : [];
      changes.push({ name: material.name, color: `#${material.color.getHexString()}`, map: material.map?.name || '',
        mapUvChannel:material.map?.channel ?? null,mapRepeat:material.map?.repeat.toArray() || null,mapOffset:material.map?.offset.toArray() || null,
        emissiveMap: material.emissiveMap?.name || '', normalMap: material.normalMap?.name || '',
        roughnessMap: material.roughnessMap?.name || '', transparent: material.transparent,
        aoMap: material.aoMap?.name || '', metalnessMap: material.metalnessMap?.name || '',
        roughness: material.roughness ?? null, metalness: material.metalness ?? null,
        lighting: material.isMeshStandardMaterial ? 'pbr' : material.isMeshPhongMaterial ? 'phong' : 'unlit', logos, layers });
      changes.at(-1).display = material.userData.arenaDisplay || null;
      changes.at(-1).overlay = overlay;
      changes.at(-1).graffiti = graffiti;
      disposeDerivedTextures(original);
      original.dispose();
      return material;
    });
  }
  return changes;
}

function setArenaCaptureLighting(view, definitions = {}, options = {}) {
  if (!currentPayload?.stats?.arena) throw new Error('Load an arena before setting capture lighting.');
  scene.environment = null;
  captureEnvironment?.dispose();
  captureEnvironment = null;
  setArenaCaptureAppearance(definitions, { lighting: true });
  captureLightGroup.clear();
  const authoredSun = options.outdoor ? captureAuthoredSun(options.sceneLights, view) : null;
  const authoredLights = captureAuthoredCourtLights(options.sceneLights, view, { spots: options.outdoor });
  for (const definition of authoredLights) {
    const light = new THREE.SpotLight(new THREE.Color().setRGB(...definition.color), definition.intensity,
      definition.distance, definition.angle, definition.penumbra, 2);
    light.name = definition.name;
    light.position.fromArray(definition.position);
    light.target.position.copy(light.position).add(new THREE.Vector3().fromArray(definition.direction));
    captureLightGroup.add(light, light.target);
  }
  if (authoredLights.length) {
    ambientLight.intensity = 0.75;
    keyLight.intensity = 0.25;
    rimLight.intensity = 0.12;
  }
  const center = new THREE.Vector3().fromArray(view.target);
  const length = view.courtLength;
  const rig = captureLightRig(view);
  keyLight.position.fromArray(authoredSun?.position || rig.keyPosition);
  if (authoredSun) {
    keyLight.color.setRGB(...authoredSun.color, THREE.SRGBColorSpace);
    keyLight.intensity = authoredSun.intensity;
    ambientLight.intensity = 0.25;
    rimLight.intensity = 0.08;
    captureLightGroup.add(new THREE.HemisphereLight(0xc9e4ff, 0x6e6253, 0.7));
    scene.background?.set(0x94bdd7);
    scene.fog.color.set(0x94bdd7);
  }
  const brightness = options.brightPreview ? { toneMapping:'ACESFilmic', exposure:1.5,
    ambientGain:1.8, hemisphereGain:1.15, shadowIntensity:.55 } : null;
  if (brightness) {
    renderer.toneMapping = THREE.ACESFilmicToneMapping;
    renderer.toneMappingExposure = brightness.exposure;
    ambientLight.intensity *= brightness.ambientGain;
    for (const light of captureLightGroup.children) if (light.isHemisphereLight) light.intensity *= brightness.hemisphereGain;
  }
  keyLight.target.position.copy(center);
  keyLight.target.updateMatrixWorld(true);
  renderer.shadowMap.enabled = options.shadows !== false;
  renderer.shadowMap.autoUpdate = true;
  renderer.shadowMap.type = THREE.PCFShadowMap;
  keyLight.shadow.radius = 1;
  keyLight.castShadow = renderer.shadowMap.enabled && (!authoredSun || authoredSun.castsShadows);
  keyLight.shadow.mapSize.set(2048, 2048);
  keyLight.shadow.bias = -0.00004;
  keyLight.shadow.normalBias = 1.5;
  keyLight.shadow.intensity = brightness?.shadowIntensity ?? .7;
  const shadowCamera = keyLight.shadow.camera;
  shadowCamera.left = shadowCamera.bottom = -rig.shadowExtent;
  shadowCamera.right = shadowCamera.top = rig.shadowExtent;
  shadowCamera.near = rig.shadowNear;
  shadowCamera.far = rig.shadowFar;
  shadowCamera.updateProjectionMatrix();
  const casters = [];
  const receivers = [];
  for (const object of modelRoot.children) {
    // These static court objects cast the preview's local shadows. Overhead
    // roofs and scorecubes would block this approximate light rig completely.
    object.castShadow = renderer.shadowMap.enabled && /basket|scoretable|chair_arenabench|rack_ball/i.test(object.name);
    object.receiveShadow = renderer.shadowMap.enabled && /floor_.*court|lowerbowl|upperbowl|bleacher|chair|basket|court_full_city|court_short_city|terrain_/i.test(object.name);
    if (object.castShadow) casters.push(object.name);
    if (object.receiveShadow) receivers.push(object.name);
  }
  renderer.render(scene, camera);
  // The capture is static; reuse this shadow map during all six cube views.
  renderer.shadowMap.autoUpdate = false;
  let environment = null;
  if (options.reflections !== false) {
    const cubeTarget = new THREE.WebGLCubeRenderTarget(128, { type: THREE.HalfFloatType });
    const probe = new THREE.CubeCamera(10, length * 5, cubeTarget);
    probe.position.fromArray(rig.probePosition);
    const generator = new THREE.PMREMGenerator(renderer);
    try {
      probe.update(renderer, scene);
      captureEnvironment?.dispose();
      captureEnvironment = generator.fromCubemap(cubeTarget.texture);
      scene.environment = captureEnvironment.texture;
      environment = { source: 'actual arena scene', position: probe.position.toArray(), cubeResolution: 128 };
    } finally {
      generator.dispose();
      cubeTarget.dispose();
    }
  }
  const appearance = setArenaCaptureAppearance(definitions, { lighting: true, pbr: true, preserveLighting: true });
  renderer.render(scene, camera);
  return { preset: authoredSun || options.outdoor ? 'court-lighting-v4' : authoredLights.length ? 'court-lighting-v3' : 'court-lighting-v2', keyPosition: keyLight.position.toArray(), shadowMapSize: keyLight.castShadow ? [2048, 2048] : null,
    shadowCasters: casters, shadowReceivers: receivers, environment, authoredSun, authoredLights, appearance, brightness,
    ambientIntensity:ambientLight.intensity };
}

function createDdsTexture(item) {
  try {
    const bytes = typeof Uint8Array.fromBase64 === 'function'
      ? Uint8Array.fromBase64(item.bytes)
      : Uint8Array.from(atob(item.bytes), c => c.charCodeAt(0));
    if (item.previewRgba) {
      if (bytes.length !== item.width * item.height * 4) return null;
      const texture = new THREE.DataTexture(bytes, item.width, item.height, THREE.RGBAFormat);
      texture.name = item.name;
      texture.userData.range = item.range || null;
      texture.colorSpace = item.range || item.format === 'RGBA8_UNORM' ? THREE.NoColorSpace : THREE.SRGBColorSpace;
      texture.wrapS = texture.wrapT = THREE.RepeatWrapping;
      texture.generateMipmaps = true;
      texture.minFilter = THREE.LinearMipmapLinearFilter;
      texture.magFilter = THREE.LinearFilter;
      texture.anisotropy = Math.min(8, renderer.capabilities.getMaxAnisotropy());
      texture.needsUpdate = true;
      return texture;
    }
    const parsed = new DDSLoader().parse(bytes.buffer, true);
    if (!parsed.format || !parsed.width || !parsed.height || parsed.isCubemap) return null;
    const texture = new THREE.CompressedTexture(parsed.mipmaps, parsed.width, parsed.height, parsed.format);
    texture.name = item.name;
    texture.userData.range = item.range || null;
    const declaredFormat = String(item.format || '').toUpperCase();
    texture.colorSpace = item.range || (declaredFormat && declaredFormat !== 'DDS' && !declaredFormat.includes('SRGB'))
      ? THREE.NoColorSpace
      : declaredFormat.includes('SRGB') || item.binding?.channel === 'color'
        || /basecolou?r|(?:^|_)color(?:_|\.)|albedo|diffuse|_c(?:\.|_)/i.test(`${item.source} ${item.name}`)
        ? THREE.SRGBColorSpace : THREE.NoColorSpace;
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
  if (/(?:^|_)hair(?:_|$)/.test(name)) return 'hair';
  if (/(?:^|_)face(?:_|$)/.test(name)) return 'face';
  if (/(?:^|_)(?:arms?|chest)(?:_|$)/.test(name)) return 'chest';
  if (/(?:^|_)(?:torso|body)(?:_|$)/.test(name)) return 'torso';
  if (/(?:^|_)(?:legs?|thigh|calf)(?:_|$)/.test(name)) return 'legs';
  if (/ball/.test(name)) return 'ball';
  return '';
}

function materialMetadata(materialName) {
  const normalized = String(materialName || '').toLowerCase();
  return materialMetadataByName.get(normalized) || null;
}

function previewMaterialProfile(materialName) {
  const metadata = materialMetadata(materialName);
  return materialProfile(materialName, metadata?.shader || '', currentPreviewProfile.kind);
}

function applyPreviewMaterialProfile(material, normalTexture = material.normalMap) {
  if (!material || material.userData.neutralFacialHairFitBase) return null;
  const profile = previewMaterialProfile(material.name);
  material.userData.previewMaterialRole = profile.role;
  if ('roughness' in material) material.roughness = profile.roughness;
  if ('metalness' in material) material.metalness = profile.metalness;
  if ('clearcoat' in material) material.clearcoat = profile.clearcoat || 0;
  if ('clearcoatRoughness' in material) material.clearcoatRoughness = profile.clearcoatRoughness ?? 0;
  if (normalTexture && material.normalScale) material.normalScale.set(profile.normalScale, profile.normalScale);
  if ('anisotropy' in material && profile.role !== 'strand') material.anisotropy = 0;
  if (profile.role === 'glass') {
    material.transparent = true;
    material.opacity = 0.28;
    material.depthWrite = false;
  }
  return profile;
}

function boundTexture(targetName, channel) {
  const index = textureItems.findIndex((item, itemIndex) => textureCache[itemIndex]
    && item.binding?.target === targetName && item.binding?.channel === channel);
  return index >= 0 ? textureCache[index] : null;
}

function boundTextureMatching(targetName, channel, pattern) {
  const index = textureItems.findIndex((item, itemIndex) => textureCache[itemIndex]
    && item.binding?.target === targetName && item.binding?.channel === channel
    && pattern.test(`${item.name} ${item.source}`));
  return index >= 0 ? textureCache[index] : null;
}

function boundStrandTangent(materialName, targetName) {
  const name = String(materialName || '').toLowerCase();
  let selected = -1, priority = Infinity, explicit = false;
  textureItems.forEach((item, index) => {
    if (!textureCache[index]) return;
    const assigned = item.assignments?.some(assignment => assignment.channel === 'tangent'
      && String(assignment.material || '').toLowerCase() === name);
    const fallback = item.binding?.target === targetName && item.binding?.channel === 'auxiliary'
      && /tangent/i.test(`${item.name} ${item.source}`);
    if (!assigned && !fallback) return;
    const sourcePriority = Number.isFinite(item.sourcePriority) ? item.sourcePriority : index;
    if (sourcePriority < priority || sourcePriority === priority && assigned && !explicit) {
      selected = index; priority = sourcePriority; explicit = Boolean(assigned);
    }
  });
  return selected < 0 ? null : textureCache[selected];
}

function boundArenaTexture(materialName, channel) {
  const normalized = String(materialName || '').toLowerCase();
  const matches = (item, itemIndex) => textureCache[itemIndex]
    && item.assignments?.some((assignment) => assignment.channel === channel
      && String(assignment.material || '').toLowerCase() === normalized);
  // An explicit standalone normal wins over a packed fallback regardless of
  // scene/container ordering. Layered maps remain outside this approximation.
  let index = channel === 'normal' ? textureItems.findIndex((item, itemIndex) =>
    item.captureResourceSlot === 'NormalMap' && matches(item, itemIndex)) : -1;
  if (index < 0) index = textureItems.findIndex(matches);
  return index >= 0 ? textureCache[index] : null;
}

function boundArenaTextureSlot(materialName, channel, slot) {
  const normalized = materialName.toLowerCase();
  const index = textureItems.findIndex((item, index) => textureCache[index] && item.assignments?.some(binding =>
    binding.material.toLowerCase() === normalized && binding.channel === channel && binding.slot === slot));
  return index < 0 ? null : textureCache[index];
}

function captureSurfaceTexture(texture, parameter, material, prefix = 'Normal') {
  const finite = (value, fallback) => value == null || !Number.isFinite(Number(value)) ? fallback : Number(value);
  const sx = finite(parameter[prefix + 'ScaleX'], 1), sy = finite(parameter[prefix + 'ScaleY'], 1);
  const ox = finite(parameter[prefix + 'OffsetX'], 0), oy = finite(parameter[prefix + 'OffsetY'], 0);
  if (sx === 1 && sy === 1 && ox === 0 && oy === 0) return texture;
  const derived = texture.clone();
  derived.repeat.set(sx, sy);
  derived.offset.set(ox, oy);
  material.userData.derivedTextures ||= [];
  material.userData.derivedTextures.push(derived);
  return derived;
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
  if (/^#?[0-9a-f]{6}$/.test(name)) return new THREE.Color(`#${name.replace('#', '')}`);
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

function configureTextureRange(material, texture) {
  const range = texture?.userData?.range || null;
  material.userData.textureRange = range;
  // A color-map swap can reuse the same compiled shader. Keep its uniform
  // objects alive and update their values even when onBeforeCompile is skipped.
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
  if (!textureSampler) {
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
    // A readback must sample this exact raw map, independent of the main
    // scene's material-program/color-space caches. Range maps use NoColorSpace.
    const sampleMaterial = new THREE.RawShaderMaterial({
      uniforms: { sampleMap: { value: null }, sampleTransform: { value: new THREE.Matrix3() } },
      vertexShader: `precision highp float;
        attribute vec3 position;
        attribute vec2 uv;
        uniform mat3 sampleTransform;
        varying vec2 sampleUv;
        void main() { sampleUv = (sampleTransform * vec3(uv, 1.0)).xy; gl_Position = vec4(position.xy, 0.0, 1.0); }`,
      fragmentShader: `precision highp float;
        uniform sampler2D sampleMap;
        varying vec2 sampleUv;
        void main() { gl_FragColor = texture2D(sampleMap, sampleUv); }`,
      toneMapped: false,
      depthTest: false,
      depthWrite: false,
    });
    const geometry = new THREE.PlaneGeometry(2, 2);
    sampleScene.add(new THREE.Mesh(geometry, sampleMaterial));
    textureSampler = { target: targetTexture, scene: sampleScene, camera: sampleCamera, material: sampleMaterial, geometry };
  }
  const sampler = textureSampler;
  if (texture.matrixAutoUpdate) texture.updateMatrix();
  sampler.material.uniforms.sampleTransform.value.copy(texture.matrix);
  sampler.material.uniforms.sampleMap.value = texture;
  sampler.material.uniformsNeedUpdate = true;
  const pixels = new Uint8Array(sampleSize * sampleSize * 4);
  const previousTarget = renderer.getRenderTarget();
  try {
    renderer.setRenderTarget(sampler.target);
    renderer.clear();
    renderer.render(sampler.scene, sampler.camera);
    renderer.readRenderTargetPixels(sampler.target, 0, 0, sampleSize, sampleSize, pixels);
  } catch {
    return null;
  } finally {
    renderer.setRenderTarget(previousTarget);
    sampler.material.uniforms.sampleMap.value = null;
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
  const key = `${leftTarget}:${rightTarget}`;
  if (skinBoundaryPairs.has(key)) return skinBoundaryPairs.get(key);
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
    // Integer keys at 0.001-unit resolution; toFixed() dominated this pass on bodies.
    const positionKey = (vertex) => Math.round(mesh.positions[vertex * 3] * 1000) + ','
      + Math.round(mesh.positions[vertex * 3 + 1] * 1000) + ',' + Math.round(mesh.positions[vertex * 3 + 2] * 1000);
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
  const resolvedPairs = pairs.length ? pairs : options.skinBoundaryPairs?.(currentPayload, leftTarget, rightTarget, materialTextureTarget) || pairs;
  skinBoundaryPairs.set(key, resolvedPairs);
  return resolvedPairs;
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
  return new THREE.Color(...(skinBoundaryResponse(reference, source, referenceResponse)
    || skinLuminanceResponse(reference, source, referenceResponse) || [1, 1, 1]));
}

function skinMaterialResponses() {
  if (!currentPayload?.player) return new Map();
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
  invalidateShadows();
  const materialResponses = skinMaterialResponses();
  modelRoot.traverse((object) => {
    const materials = Array.isArray(object.material) ? object.material : [object.material];
    materials.filter(Boolean).forEach((material) => {
      disposeDerivedTextures(material);
      const targetName = materialTextureTarget(material.name);
      const boundColorTexture = boundArenaTexture(material.name, 'color') || (targetName
        ? boundTexture(targetName, 'color')
        : null);
      const colorTexture = targetName === 'eye'
        ? eyeAtlasTexture(boundColorTexture, material)
        : boundColorTexture;
      const normalTexture = boundArenaTexture(material.name, 'normal') || (targetName
        ? boundTexture(targetName, 'normal')
        : null);
      const strandMaterial = targetName === 'hair' || targetName === 'facialhair';
      const tangentTexture = strandMaterial
        ? boundStrandTangent(material.name, targetName)
        : null;
      material.map = colorTexture;
      material.normalMap = normalTexture;
      // Strands already apply their own lightmap/occlusion response.
      material.aoMap = targetName && !strandMaterial && targetName !== 'eye'
        ? boundTextureMatching(targetName, 'auxiliary', /occlusion|(?:^|_)ao(?:_|\.)/i) : null;
      if (material.aoMap) material.aoMapIntensity = 1;
      if ('anisotropyMap' in material) {
        material.anisotropyMap = tangentTexture;
        material.anisotropy = tangentTexture ? previewMaterialProfile(material.name).anisotropy || 0 : 0;
      }
      configureTextureRange(material, colorTexture);
      configureStrandLightmap(material, strandMaterial
        ? boundTextureMatching(targetName, 'auxiliary', /lightmap|occlusion/i) : null);
      configureStrandTangentRange(material, tangentTexture, THREE.ShaderChunk.lights_physical_fragment);
      configureStrandSpecular(material, strandMaterial);
      configureEyeIris(material, targetName === 'eye' && Boolean(colorTexture),
        irisColor(currentPayload?.player?.irisColor), irisScale(currentPayload?.player?.irisSize));
      const profile = applyPreviewMaterialProfile(material, normalTexture);
      configureSkinShading(material, profile?.role === 'skin');
      material.transparent = profile?.role === 'glass'
        || Boolean(colorTexture) && (targetName === 'eyelash' || strandMaterial);
      material.alphaTest = targetName === 'eyelash' || strandMaterial ? profile?.alphaTest || 0 : 0;
      material.depthWrite = targetName !== 'eyelash' && profile?.role !== 'glass';
      material.forceSinglePass = strandMaterial;
      configureStrandCoverage(material, strandMaterial, renderer.getContext().getContextAttributes()?.antialias);
      material.color.copy(colorTexture && targetName !== 'eyelash'
        ? materialResponses.get(targetName) || new THREE.Color(0xffffff)
        : colorFor(material.name));
      material.userData.skinMaterialResponse = materialResponses.get(targetName)?.toArray() || null;
      if (currentPayload?.shoe && !colorTexture) material.color.set(0xaaaaaa);
      material.needsUpdate = true;
    });
  });
}

function setVectorAttribute(geometry, name, values, size) {
  // Reused geometry keeps its GPU buffer; only the contents change.
  const current = geometry.getAttribute(name);
  if (current?.array.length === values.length) { current.array.set(values); current.needsUpdate = true; }
  else geometry.setAttribute(name, new THREE.BufferAttribute(values, size));
}

// Studio meshes share one seam-aware normal pass (face/neck/body splits stay
// smooth across meshes and material groups), then get tangents for normal maps.
function prepareSurfaceFrames(payload) {
  if (payload?.stats?.arena) return;
  const meshes = modelRoot.children.filter((object) => object.isMesh && object.geometry.index
    && !object.geometry.userData.authoredNormals);
  // Instances repeat one local geometry, so only plain meshes share a space.
  const groups = [meshes.filter((object) => !object.isInstancedMesh),
    ...meshes.filter((object) => object.isInstancedMesh).map((object) => [object])];
  for (const group of groups.filter((entries) => entries.length)) {
    const normals = seamSmoothedNormals(group.map(({ geometry }) => ({
      positions: geometry.attributes.position.array, indices: geometry.index.array,
    })));
    group.forEach(({ geometry }, index) => setVectorAttribute(geometry, 'normal', normals[index], 3));
  }
  for (const { geometry, isMesh } of modelRoot.children) {
    if (!isMesh || !geometry.index || !geometry.attributes.uv || !geometry.attributes.normal) continue;
    setVectorAttribute(geometry, 'tangent', vertexTangents(geometry.attributes.position.array,
      geometry.attributes.normal.array, geometry.attributes.uv.array, geometry.index.array), 4);
  }
}

async function load(payload, onProgress = () => {}, options = {}) {
  const generation = ++loadGeneration;
  const retained = new Map(), retainedTextures = new Map();
  const meshKey = item => item.geometryKey ? [item.sourceKind, item.sourceName, item.name, item.geometryKey].join('|') : '';
  const wanted = new Set((payload.meshes || []).map(meshKey).filter(Boolean));
  for (const object of [...modelRoot.children]) {
    if (!object.isInstancedMesh && object.userData.geometryKey && wanted.has(object.userData.geometryKey)) {
      if (object.userData.bodyFitBasePositions) {
        object.geometry.attributes.position.array.set(object.userData.bodyFitBasePositions);
        object.geometry.attributes.position.needsUpdate = true;
      }
      modelRoot.remove(object); retained.set(object.userData.geometryKey, object);
    }
  }
  for (let i = 0; i < textureItems.length; i++) {
    const old = textureItems[i], texture = textureCache[i];
    if (!texture) continue;
    const next = (payload.textures || []).find(item => item.name === old.name && item.bytes === old.bytes
      && JSON.stringify(item.range) === JSON.stringify(old.range) && item.inputColorSpace === old.inputColorSpace);
    if (next) retainedTextures.set(next, texture);
  }
  let reusedGeometry = 0;
  try {
  currentPayload = payload;
  materialMetadataByName = new Map();
  for (const item of payload.materials || []) {
    const key = String(item.name || '').toLowerCase();
    if (!materialMetadataByName.has(key)) materialMetadataByName.set(key, item);
  }
  grid.visible = true;
  eyelashesVisible = false;
  configurePreviewLighting(payload);
  disposeRoot(new Set(retainedTextures.values()));
  const items = payload.meshes || [];
  let sliceStarted = performance.now();
  for (let itemIndex = 0; itemIndex < items.length; itemIndex += 1) {
    if (generation !== loadGeneration) throw new Error('Preview cancelled.');
    const item = items[itemIndex];
    const reuse = retained.get(meshKey(item));
    if (reuse) {
      retained.delete(meshKey(item)); modelRoot.add(reuse); reusedGeometry++;
      continue;
    }
    const neutralFacialHairFitBase = item.sourceKind === 'facialhair-fit-base';
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute('position', new THREE.BufferAttribute(typed(item.positions, Float32Array), 3));
    if (item.uvs?.length) geometry.setAttribute('uv', new THREE.BufferAttribute(typed(item.uvs, Float32Array), 2));
    if (item.courtUvs?.length) geometry.setAttribute('courtUv', new THREE.BufferAttribute(typed(item.courtUvs, Float32Array), 2));
    if (item.detailUvs?.length) geometry.setAttribute('detailUv', new THREE.BufferAttribute(typed(item.detailUvs, Float32Array), 2));
    if (item.overlayUvs?.length) geometry.setAttribute('overlayUv', new THREE.BufferAttribute(typed(item.overlayUvs, Float32Array), 2));
    if (item.graffitiUvs?.length) geometry.setAttribute('graffitiUv', new THREE.BufferAttribute(typed(item.graffitiUvs, Float32Array), 2));
    geometry.setIndex(new THREE.BufferAttribute(indexArray(item.indices), 1));
    if (item.normals?.length === item.positions?.length) {
      geometry.setAttribute('normal', new THREE.BufferAttribute(typed(item.normals, Float32Array), 3));
      geometry.userData.authoredNormals = true;
    } else if (payload.stats?.arena) {
      geometry.computeVertexNormals();
    }
    const materials = [];
    for (const group of item.groups || []) {
      const targetName = materialTextureTarget(group.material);
      const profile = previewMaterialProfile(group.material);
      const Material = targetName === 'hair' || targetName === 'facialhair' || profile.role === 'eye' || profile.role === 'glass'
        || profile.role === 'skin' && !neutralFacialHairFitBase
        ? THREE.MeshPhysicalMaterial
        : THREE.MeshStandardMaterial;
      const material = new Material({
        name: group.material,
        color: neutralFacialHairFitBase ? new THREE.Color(0x8e959c) : colorFor(group.material),
        roughness: neutralFacialHairFitBase ? 0.9 : profile.roughness,
        metalness: neutralFacialHairFitBase ? 0 : profile.metalness,
        side: THREE.DoubleSide,
        wireframe,
      });
      material.userData.neutralFacialHairFitBase = neutralFacialHairFitBase;
      applyPreviewMaterialProfile(material);
      const materialIndex = materials.length;
      materials.push(material);
      geometry.addGroup(group.start, group.count, materialIndex);
    }
    if (!materials.length) {
      const material = new THREE.MeshStandardMaterial({ color: 0xa6adb7, wireframe });
      applyPreviewMaterialProfile(material);
      materials.push(material);
    }
    const packedMatrices = typed(item.instanceMatrices, Float32Array);
    const packedCount = Math.floor(packedMatrices.length / 16);
    const legacyInstances = item.instances || [];
    const instanceCount = packedCount || legacyInstances.length;
    if (instanceCount) {
      const mesh = new THREE.InstancedMesh(geometry, materials, instanceCount);
      mesh.name = item.name;
      mesh.userData.previewInstances = legacyInstances;
      mesh.userData.previewInstanceNames = item.instanceNames || legacyInstances.map((instance) => instance.name);
      mesh.userData.previewInstanceTargets = item.instanceTargets || legacyInstances.map((instance) => instance.target);
      mesh.userData.previewModel = item.name;
      mesh.userData.previewGroups = item.groups || [];
      for (let index = 0; index < instanceCount; index += 1) {
        const source = packedCount ? packedMatrices : legacyInstances[index].matrix;
        const offset = packedCount ? index * 16 : 0;
        mesh.setMatrixAt(index, new THREE.Matrix4().fromArray(source, offset));
      }
      mesh.instanceMatrix.setUsage(THREE.StaticDrawUsage);
      mesh.instanceMatrix.needsUpdate = true;
      modelRoot.add(mesh);
    } else {
      const mesh = new THREE.Mesh(geometry, materials);
      mesh.name = item.name;
      mesh.userData.previewModel = item.name;
      mesh.userData.previewGroups = item.groups || [];
      modelRoot.add(mesh);
    }
    modelRoot.children.at(-1).userData.geometryKey = meshKey(item);
    if ((!options.deferRender || options.yieldPreparation) && performance.now() - sliceStarted >= 8 && itemIndex + 1 < items.length) {
      onProgress({ phase: 'meshes', completed: itemIndex + 1, total: items.length });
      await nextFrame();
      sliceStarted = performance.now();
    }
  }
  try {
    prepareSurfaceFrames(payload);
  } catch {
    for (const { geometry, isMesh } of modelRoot.children) if (isMesh && !geometry.attributes.normal) geometry.computeVertexNormals();
  }
  textureItems = payload.textures || [];
  textureCache = [];
  sliceStarted = performance.now();
  for (let index = 0; index < textureItems.length; index += 1) {
    if (generation !== loadGeneration) throw new Error('Preview cancelled.');
    const reused = retainedTextures.get(textureItems[index]);
    if (reused) retainedTextures.delete(textureItems[index]);
    textureCache.push(reused || createDdsTexture(textureItems[index]));
    if ((!options.deferRender || options.yieldPreparation) && performance.now() - sliceStarted >= 8 && index + 1 < textureItems.length) {
      onProgress({ phase: 'textures', completed: index + 1, total: textureItems.length });
      await nextFrame();
      sliceStarted = performance.now();
    }
  }
  applyAutomaticTextures();
  if (generation !== loadGeneration) throw new Error('Preview cancelled.');
  if (!options.deferRender) {
    setEyelashesVisible(false, false);
    if (isPlacedArena(payload)) { fitCamera(false); setArenaCourtView(); }
    else fitCamera();
  }
  return { decodedTextures: textureCache.filter(Boolean).length, reusedGeometry };
  } finally {
    for (const object of retained.values()) {
      object.geometry.dispose();
      for (const material of (Array.isArray(object.material) ? object.material : [object.material])) {
        disposeDerivedTextures(material); material.dispose();
      }
    }
    for (const texture of retainedTextures.values()) texture.dispose();
  }
}

async function loadArenaTextures(items, onProgress = () => {}, options = {}) {
  const generation = loadGeneration;
  const batch = ++textureBatchGeneration;
  const append = Boolean(options.append);
  const previousItems = textureItems, previousCache = textureCache;
  const existingNames = new Set((append ? previousItems : []).map((item) => String(item.name || '').toLowerCase()));
  const additions = (items || []).filter((item) => {
    const name = String(item?.name || '').toLowerCase();
    if (!name || existingNames.has(name)) return false;
    existingNames.add(name);
    return true;
  });
  const added = [];
  const created = [], stored = [];
  let committedItems, committedCache;
  const isCurrent = () => generation === loadGeneration && batch === textureBatchGeneration
    && textureItems === previousItems && textureCache === previousCache;
  let sliceStarted = performance.now();
  try {
  for (let index = 0; index < additions.length; index += 1) {
    if (!isCurrent()) throw new Error('Preview cancelled.');
    const item = additions[index];
    const texture = createDdsTexture(item);
    created.push(texture);
    const { bytes: _encodedBytes, ...storedItem } = item;
    const textureIndex = (append ? previousItems.length : 0) + stored.length;
    stored.push(storedItem);
    added.push({ index: textureIndex, item: storedItem, decoded: Boolean(texture) });
    if (performance.now() - sliceStarted >= 8 && index + 1 < additions.length) {
      onProgress({ completed: index + 1, total: additions.length });
      await nextFrame();
      sliceStarted = performance.now();
    }
  }
  onProgress({ completed: additions.length, total: additions.length });
  if (!isCurrent()) throw new Error('Preview cancelled.');
  textureItems = committedItems = append ? [...previousItems, ...stored] : stored;
  textureCache = committedCache = append ? [...previousCache, ...created] : created;
  applyAutomaticTextures();
  renderer.render(scene, camera);
  if (!append) previousCache.forEach(texture => texture?.dispose());
  return {
    decodedTextures: textureCache.filter(Boolean).length,
    addedTextures: added.filter((entry) => entry.decoded).length,
    added,
  };
  } catch (error) {
    if (committedItems && textureItems === committedItems && textureCache === committedCache) {
      textureItems = previousItems; textureCache = previousCache;
      try { applyAutomaticTextures(); } catch { /* Preserve the original upload/render error. */ }
    }
    created.forEach(texture => { if (!textureCache.includes(texture)) texture?.dispose(); });
    throw error;
  }
}

function replaceModelColors(update) {
  const names = new Set(update.replaceMaterials.map(n => n.toLowerCase()));
  const additions = update.textures.map(item => ({ item, texture: createDdsTexture(item) }));
  if (additions.some(a => !a.texture)) {
    additions.forEach(a => a.texture?.dispose());
    throw new Error('The new color texture could not be uploaded; previous textures retained.');
  }
  const keptItems = [], keptTextures = [], removed = [];
  textureItems.forEach((item, i) => {
    if (item.assignments?.some(a => a.channel === 'color' && names.has(a.material.toLowerCase()))) removed.push(textureCache[i]);
    else { keptItems.push(item); keptTextures.push(textureCache[i]); }
  });
  textureItems = [...keptItems, ...additions.map(a => a.item)];
  textureCache = [...keptTextures, ...additions.map(a => a.texture)];
  if (update.shoe) currentPayload.shoe = update.shoe;
  if ('apparelVariant' in update) currentPayload.apparelVariant = update.apparelVariant;
  currentPayload.textures = textureItems;
  currentPayload.stats.textures = textureItems.length;
  currentPayload.textureDependencies = [
    ...(currentPayload.textureDependencies || []).filter(item =>
      !item.assignments?.some(assignment => assignment.channel === 'color' && names.has(assignment.material.toLowerCase()))),
    ...update.textures.map(({ bytes: _bytes, data: _data, ...metadata }) => metadata),
  ];
  applyAutomaticTextures();
  removed.forEach(texture => texture?.dispose());
  renderer.render(scene, camera);
  return { decodedTextures: textureCache.filter(Boolean).length };
}

function dependencyDetails(material) {
  const materialName = String(material?.name || 'Unassigned');
  const normalizedMaterial = materialName.toLowerCase();
  const targetName = materialTextureTarget(materialName);
  const dependencies = (currentPayload?.textureDependencies || []).filter((item) => (
    item.assignments?.some((assignment) => String(assignment.material || '').toLowerCase() === normalizedMaterial)
      || (targetName && item.binding?.target === targetName)
  ));
  const appliedNames = new Map([
    ['color', material?.map?.name || ''],
    ['normal', material?.normalMap?.name || ''],
    ['auxiliary', material?.anisotropyMap?.name || ''],
  ]);
  return dependencies.map((dependency) => {
    const normalizedName = String(dependency.name || '').toLowerCase();
    const textureIndex = textureItems.findIndex((item) => String(item.name || '').toLowerCase() === normalizedName);
    const decoded = textureIndex >= 0 && Boolean(textureCache[textureIndex]);
    const assignment = dependency.assignments?.find(
      (item) => String(item.material || '').toLowerCase() === normalizedMaterial,
    );
    const channel = assignment?.channel || dependency.binding?.channel || 'auxiliary';
    const appliedName = appliedNames.get(channel) || '';
    return {
      name: dependency.name,
      resourceName: dependency.resourceName || dependency.name,
      source: dependency.source || '',
      sourceIff: dependency.sourceIff || '',
      width: Number(dependency.width || 0),
      height: Number(dependency.height || 0),
      mips: Number(dependency.mips || 0),
      format: dependency.format || 'DDS',
      channel,
      rangeCompressed: Boolean(dependency.range),
      loaded: decoded,
      applied: Boolean(appliedName) && (
        appliedName.toLowerCase() === normalizedName || appliedName.toLowerCase().includes(normalizedName)
      ),
      status: decoded ? 'loaded' : textureIndex >= 0 ? 'unsupported' : 'pending',
      textureIndex: decoded ? textureIndex : null,
    };
  });
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
  const hits = raycaster.intersectObjects(modelRoot.children, false);
  const hit = (options.filterHits ? options.filterHits(hits) : hits)[0];
  clearPick(false);
  if (!hit) {
    window.dispatchEvent(new CustomEvent('model-preview:pick', { detail: null }));
    renderer.render(scene, camera);
    return;
  }

  const object = hit.object;
  const materialIndex = Number.isInteger(hit.face?.materialIndex) ? hit.face.materialIndex : 0;
  const materials = Array.isArray(object.material) ? object.material : [object.material];
  const material = materials[materialIndex] || materials[0] || null;
  const group = object.userData.previewGroups?.[materialIndex] || null;
  const materialMetadata = currentPayload?.materials?.find(
    (item) => String(item.name || '').toLowerCase() === String(material?.name || '').toLowerCase(),
  );
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
      name: instance?.name || object.name,
      target: instance?.target || object.userData.previewModel || object.name,
      model: object.userData.previewModel || object.name,
      instance: instanceId,
      primitive: group?.mesh || '',
      material: material ? {
        name: material.name || 'Unassigned',
        shader: materialMetadata?.shader || '',
        source: materialMetadata?.source || '',
        textures: dependencyDetails(material),
      } : null,
    },
  }));
  renderer.render(scene, camera);
}

function setWireframe(value) {
  invalidateShadows();
  wireframe = Boolean(value);
  modelRoot.traverse((object) => {
    const materials = Array.isArray(object.material) ? object.material : [object.material];
    materials.filter(Boolean).forEach((material) => { material.wireframe = wireframe; material.needsUpdate = true; });
  });
  renderer.render(scene, camera);
}

function setEyelashesVisible(value, draw = true) {
  invalidateShadows();
  eyelashesVisible = Boolean(value);
  modelRoot.traverse((object) => {
    const materials = Array.isArray(object.material) ? object.material : [object.material];
    materials.filter(Boolean).forEach((material) => {
      if (materialTextureTarget(material.name) === 'eyelash') material.visible = eyelashesVisible;
    });
  });
  if (draw) renderer.render(scene, camera);
}

function setTexture(index) {
  invalidateShadows();
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
      // Inspecting one texture shows it raw, without the automatic extra maps.
      material.aoMap = null;
      if ('anisotropyMap' in material) { material.anisotropyMap = null; material.anisotropy = 0; }
      configureTextureRange(material, texture);
      applyPreviewMaterialProfile(material, null);
      material.color.copy(texture ? new THREE.Color(0xffffff) : colorFor(material.name));
      material.needsUpdate = true;
    });
  });
  renderer.render(scene, camera);
}

if (options.interactive !== false) {
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
  scheduleCameraRender();
});
canvas.addEventListener('pointerup', (event) => {
  const shouldPick = drag?.button === 0 && !drag.moved;
  drag = null;
  if (shouldPick) pickObject(event.clientX, event.clientY);
});
canvas.addEventListener('pointercancel', () => { drag = null; });
canvas.addEventListener('wheel', (event) => {
  event.preventDefault();
  distance *= Math.exp(event.deltaY * 0.001);
  scheduleCameraRender();
}, { passive: false });

}
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

const api = {
  cancelLoad,
  colorHex: (name) => `#${colorFor(name).getHexString()}`,
  clearPick,
  fitCamera,
  load,
  loadArenaTextures,
  replaceModelColors,
  clearArenaTextures,
  resize,
  setTexture,
  setEyelashesVisible,
  setArenaCourtView,
  setArenaCaptureView,
  setArenaDisplayCaptureView,
  capturePng,
  inspectCaptureVisibility,
  setArenaCaptureAppearance,
  setArenaCaptureLighting,
  stopCameraMovement,
  setWireframe,
  stats: () => ({
    decodedTextures: textureCache.filter(Boolean).length,
    payload: currentPayload,
    previewProfile: currentPreviewProfile,
    fittedDistance,
    camera: camera.position.toArray(),
    target: target.toArray(),
    captureState: { environment: Boolean(scene.environment), shadows: renderer.shadowMap.enabled, authoredLights: captureLightGroup.children.length / 2,
      keyPosition: keyLight.position.toArray(), keyColor: keyLight.color.getHex(), cameraFov: camera.fov,
      toneMapping:renderer.toneMapping, exposure:renderer.toneMappingExposure, localClipping:renderer.localClippingEnabled },
    geometryIds: modelRoot.children.map(object => object.geometry?.uuid),
    materialTextures: modelRoot.children.flatMap((object) => {
      const materials = Array.isArray(object.material) ? object.material : [object.material];
      return materials.filter(Boolean).map((material) => ({
        name: material.name,
        map: material.map?.name || '',
        normalMap: material.normalMap?.name || '',
        roughnessMap: material.roughnessMap?.name || '',
        aoMap: material.aoMap?.name || '',
        lightmap: material.userData.strandLightmap || '',
        anisotropyMap: material.anisotropyMap?.name || '',
        tangentRangeCompressed: Boolean(material.userData.strandTangentRange),
        strandSpecularResponse: Boolean(material.userData.strandSpecularResponse),
        strandAlphaMode: material.userData.strandAlphaMode || null,
        alphaToCoverage: material.alphaToCoverage,
        alphaTest: material.alphaTest,
        transparent: material.transparent,
        depthWrite: material.depthWrite,
        metalnessMap: material.metalnessMap?.name || '',
        normalColorSpace: material.normalMap?.colorSpace ?? null,
        roughnessColorSpace: material.roughnessMap?.colorSpace ?? null,
        aoColorSpace: material.aoMap?.colorSpace ?? null,
        metalnessColorSpace: material.metalnessMap?.colorSpace ?? null,
        normalRepeat: material.normalMap?.repeat.toArray() || null,
        normalOffset: material.normalMap?.offset.toArray() || null,
        color: `#${material.color.getHexString()}`,
        role: material.userData.previewMaterialRole || '',
        roughness: material.roughness ?? null,
        metalness: material.metalness ?? null,
        skinResponse: material.userData.skinMaterialResponse || null,
        skinScatter: Boolean(material.userData.skinScatter),
        eyeIris: Boolean(material.userData.eyeIris),
        rangeCompressed: Boolean(material.userData.textureRange),
      }));
    }),
    renderer: renderer.capabilities.isWebGL2 ? 'WebGL 2' : 'WebGL 1',
  }),
};
resize();

return {
  api, THREE, canvas, scene, camera, renderer, modelRoot, grid, target,
  ambientLight, hemisphereLight, keyLight, fillLight, rimLight,
  applyAutomaticTextures, materialTextureTarget, boundTextureMatching, decodedTexturePixels, typed, boundaryUvPairs, boundaryColors,
  disposeRoot, configurePreviewLighting, configureStudioLights, updateCamera, pickObject, invalidateShadows, indexArray,
  get payload() { return currentPayload; }, set payload(value) { currentPayload = value; },
  get fittedDistance() { return fittedDistance; },
  cameraState: () => ({ target: target.toArray(), yaw, pitch, distance, fittedDistance }),
  setCameraState(value) {
    if (cameraRenderFrame) { cancelAnimationFrame(cameraRenderFrame); cameraRenderFrame = 0; }
    target.fromArray(value.target); yaw = value.yaw; pitch = value.pitch;
    distance = value.distance; fittedDistance = value.fittedDistance;
    updateCamera(false);
  },
  origin: '2k-explorer', sourceHash: 'aba5f8342ad7262fcd8db4a043705a3491df2e83bcb933254a4f9ff1ef39d41d',
};
}
