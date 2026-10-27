import { createExplorerRenderer } from './explorer/model-preview.js';
import { createBodyEditor } from './body-preview-editor.mjs';
import { nearestSkinBoundaryPairs, normalPacking, configureNormalPacking } from './skin-preview-fixes.mjs';
import { configurePortraitSkin, configurePortraitLighting } from './portrait-skin.mjs';
import { capturePortrait } from './portrait-capture.mjs';

const params = new URL(import.meta.url).searchParams;
const viewerGlobalName = params.get('global') || 'modelPreview';
const canvas = document.getElementById(params.get('canvas') || 'previewCanvas');
const pickerMode = params.get('picker') === '1';
const transparentBackground = params.get('transparent') === '1';
const portraitMode = params.get('portrait') === '1';
const cameraBroadcast = params.get('broadcast') || '';
const cameraFollow = params.get('follow') || '';
let sourcePayload = null;
let loadGeneration = 0;
let pendingLoad = 0;
let studioLighting = true;
let clayMode = false;
let headOcclusionVisible = true;
let wireframe = false;
let gridVisible = !transparentBackground;

function visibleHit(hit) {
  if (!hit.object.visible || !hit.object.parent?.visible) return false;
  const materials = Array.isArray(hit.object.material) ? hit.object.material : [hit.object.material];
  return materials[hit.face?.materialIndex || 0]?.visible !== false;
}

const context = createExplorerRenderer({
  canvas,
  viewer: viewerGlobalName,
  picker: pickerMode,
  transparent: transparentBackground,
  interactive: params.get('interactive') !== '0',
  // Transparent captures omit studio shadows unless a caller opts in (shadows=1).
  shadows: params.has('shadows') ? params.get('shadows') === '1' : !pickerMode && !transparentBackground,
  skinBoundaryPairs: nearestSkinBoundaryPairs,
  onCameraChange: (state) => {
    if (cameraBroadcast) window.dispatchEvent(new CustomEvent('model-preview:camera:' + cameraBroadcast, { detail: state }));
  },
  filterHits: (hits) => {
    const visible = hits.filter(visibleHit);
    return visible[0]?.object.userData.previewOccluder ? [] : visible;
  },
});
const { THREE, modelRoot, renderer, scene, camera, target } = context;
const bodyEditor = createBodyEditor(context);

function draw() { renderer.render(scene, camera); }
function bounds() {
  const box = new THREE.Box3().setFromObject(modelRoot);
  return box.isEmpty() ? null : { min: box.min.toArray(), max: box.max.toArray() };
}

function setCameraState(value) {
  if (!value || !Array.isArray(value.target)) return;
  context.setCameraState({
    target: value.target,
    yaw: Number(value.yaw) || 0,
    pitch: Number(value.pitch) || 0,
    distance: Math.max(0.001, Number(value.distance) || context.cameraState().distance),
    fittedDistance: Math.max(0.001, Number(value.fittedDistance) || context.fittedDistance),
  });
  draw();
}

function fitCamera(extraBounds = null) {
  context.api.fitCamera();
  if (extraBounds?.min?.length === 3 && extraBounds?.max?.length === 3) {
    const box = new THREE.Box3().setFromObject(modelRoot).union(new THREE.Box3(
      new THREE.Vector3(...extraBounds.min), new THREE.Vector3(...extraBounds.max)));
    const size = box.getSize(new THREE.Vector3());
    const distance = Math.max(size.y, size.x / Math.max(camera.aspect, 0.1), size.z)
      / (2 * Math.tan(THREE.MathUtils.degToRad(camera.fov / 2))) * 1.35;
    setCameraState({ ...context.cameraState(), target: box.getCenter(new THREE.Vector3()).toArray(), distance, fittedDistance: distance });
  }
}

function focusHead() {
  const box = new THREE.Box3().setFromObject(modelRoot);
  if (box.isEmpty()) return fitCamera();
  const center = box.getCenter(new THREE.Vector3()), size = box.getSize(new THREE.Vector3());
  const catalogHead = pickerMode && !transparentBackground;
  const portraitHeight = Math.max(size.y * (catalogHead ? 1.05 : 0.34),
    size.x / Math.max(camera.aspect, 0.1) * (catalogHead ? 1 : 0.3), 0.01);
  const distance = portraitHeight / (2 * Math.tan(THREE.MathUtils.degToRad(camera.fov / 2))) * 1.15;
  setCameraState({ target: [center.x, box.max.y - size.y * (catalogHead ? 0.46 : 0.14), center.z],
    yaw: 0, pitch: 0, distance, fittedDistance: distance });
  if (cameraBroadcast) window.dispatchEvent(new CustomEvent('model-preview:camera:' + cameraBroadcast, { detail: context.cameraState() }));
}

function setView(view) {
  if (view === 'head') return focusHead();
  if (view === 'fit') return fitCamera();
  if (!['front', 'side', 'back'].includes(view)) return;
  setCameraState({ ...context.cameraState(), yaw: view === 'side' ? Math.PI / 2 : view === 'back' ? Math.PI : 0, pitch: 0 });
  if (cameraBroadcast) window.dispatchEvent(new CustomEvent('model-preview:camera:' + cameraBroadcast, { detail: context.cameraState() }));
}

// Editor display modes surround Explorer's materials; skin decoding stays in Explorer.
function applyEditorDisplay() {
  context.applyAutomaticTextures();
  modelRoot.traverse((object) => {
    if (object.userData.previewOccluder) {
      object.castShadow = false;
      for (const material of object.material) material.wireframe = false;
      return;
    }
    const neutralHead = /^(?:hair|facialhair)-fit-base$/.test(object.userData.previewKind || '');
    const materials = Array.isArray(object.material) ? object.material : [object.material];
    for (const material of materials.filter(Boolean)) {
      const texture = material.normalMap;
      const pixels = texture && [THREE.RGB_BPTC_UNSIGNED_Format, THREE.RGBA_BPTC_Format].includes(texture.format)
        ? context.decodedTexturePixels(texture)?.pixels : null;
      configureNormalPacking(THREE, material, normalPacking(THREE, texture, pixels));
      if (portraitMode && !clayMode && !neutralHead) configurePortraitSkin(context, material);
    }
    if (!clayMode && !neutralHead) return;
    for (const material of materials.filter(Boolean)) {
      material.map = material.normalMap = null;
      if ('anisotropyMap' in material) material.anisotropyMap = null;
      material.color.setHex(0x8e959c);
      material.transparent = false;
      material.alphaTest = 0;
      material.depthWrite = true;
      material.roughness = 0.9;
      material.metalness = 0;
      material.needsUpdate = true;
    }
  });
  if (portraitMode && studioLighting) configurePortraitLighting(context, portraitLightingSettings);
  context.invalidateShadows();
  draw();
}

// Optional overrides for research A/B (envIntensity, ambient, hemisphere, jersey/shorts roughness).
const portraitLightingSettings = Object.fromEntries(['environment', 'ambient', 'hemisphere', 'garmentRoughness']
  .filter((key) => params.has(key)).map((key) => [key, Number(params.get(key))]));

function setStudioLighting(value) {
  studioLighting = Boolean(value);
  context.configurePreviewLighting();
  if (studioLighting) context.configureStudioLights(new THREE.Box3().setFromObject(modelRoot));
  else {
    renderer.toneMapping = THREE.NoToneMapping;
    renderer.toneMappingExposure = 1;
    renderer.shadowMap.enabled = context.keyLight.castShadow = false;
    context.ambientLight.intensity = sourcePayload?.player ? 2.15 : 1;
    context.hemisphereLight.intensity = context.fillLight.intensity = 0;
    context.keyLight.intensity = sourcePayload?.player ? 1.75 : 0.85;
    context.rimLight.intensity = sourcePayload?.player ? 0.4 : 0.22;
  }
  applyEditorDisplay();
}

function setClayMode(value) { clayMode = Boolean(value); applyEditorDisplay(); }
function setGridVisible(value) { gridVisible = Boolean(value); context.grid.visible = gridVisible && !transparentBackground; draw(); }
function setJerseyVisible(value) {
  for (const object of modelRoot.children) {
    if (object.userData.previewKind === 'preview-jersey') object.visible = Boolean(value);
  }
  context.invalidateShadows();
  draw();
}
function setMaterialTargetVisible(targetName, value) {
  modelRoot.traverse((object) => {
    const materials = Array.isArray(object.material) ? object.material : [object.material];
    for (const material of materials.filter(Boolean)) {
      if (context.materialTextureTarget(material.name) === targetName) material.visible = Boolean(value);
    }
  });
  context.invalidateShadows();
  draw();
}

function setOcclusionBase(payload) {
  for (const object of [...modelRoot.children]) {
    if (!object.userData.previewOccluder) continue;
    modelRoot.remove(object);
    object.geometry.dispose();
    for (const material of object.material) material.dispose();
  }
  for (const item of payload?.meshes || []) {
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute('position', new THREE.BufferAttribute(context.typed(item.positions, Float32Array), 3));
    geometry.setIndex(new THREE.BufferAttribute(context.indexArray(item.indices), 1));
    for (const group of item.groups || []) {
      if (/(?:^|_)eyeCapShape$/i.test(String(group.mesh || ''))) continue;
      geometry.addGroup(group.start, group.count, 0);
    }
    if (!geometry.groups.length) geometry.addGroup(0, geometry.index.count, 0);
    const material = new THREE.MeshBasicMaterial({ colorWrite: false, depthWrite: true, depthTest: true, side: THREE.DoubleSide });
    const object = new THREE.Mesh(geometry, [material]);
    object.userData.previewOccluder = true;
    object.visible = headOcclusionVisible;
    object.renderOrder = -1;
    modelRoot.add(object);
  }
  draw();
}
function setHeadOcclusionVisible(value) {
  headOcclusionVisible = Boolean(value);
  for (const object of modelRoot.children) if (object.userData.previewOccluder) object.visible = headOcclusionVisible;
  draw();
}

function preparePayload(payload) {
  // Explorer binds the first available texture: supply staged-first ordering.
  const textures = (payload.textures || []).map((item, index) => ({ item, index }))
    .sort((left, right) => (left.item.sourcePriority ?? left.index) - (right.item.sourcePriority ?? right.index))
    .map(({ item }) => item);
  const meshes = (payload.meshes || []).map((item) => ({ ...item, groups: (item.groups || [])
    .filter((group) => !/(?:^|_)eyeCapShape$/i.test(String(group.mesh || ''))) }));
  return { ...payload, textures, meshes };
}

async function load(payload, onProgress = () => {}) {
  const generation = ++loadGeneration;
  const previous = pendingLoad ? null : sourcePayload;
  pendingLoad = generation;
  try {
  const prepared = preparePayload(payload);
  if (previous?.geometryKey && previous.geometryKey === payload.geometryKey) {
    context.payload = prepared;
    const result = previous.textureKey === payload.textureKey
      ? { decodedTextures: context.api.stats().decodedTextures ?? payload.textures?.length ?? 0 }
      : await context.api.loadArenaTextures(prepared.textures, onProgress);
    if (generation !== loadGeneration) return { cancelled: true };
    bodyEditor.setBodyFit(prepared.player?.bodyFit || {});
    applyEditorDisplay(); draw();
    sourcePayload = payload;
    return { ...result, reusedGeometry: prepared.meshes.length };
  }
  const result = await context.api.load(prepared, onProgress, { deferRender: true, yieldPreparation: true });
  if (generation !== loadGeneration) return { cancelled: true };
  sourcePayload = payload;
  for (let index = 0; index < prepared.meshes.length; index++) {
    const item = prepared.meshes[index], object = modelRoot.children[index];
    object.userData.previewKind = item.sourceKind || '';
    object.userData.previewSource = item.sourceName || '';
  }
  // Catalog heads and hair layers never use the body-fit editor.
  if (!String(payload.player?.mode || '').startsWith('hair-catalog-')) bodyEditor.initialize(prepared);
  applyEditorDisplay();
  context.api.setEyelashesVisible(false);
  setJerseyVisible(false);
  fitCamera();
  setGridVisible(gridVisible);
  if (!studioLighting) setStudioLighting(false);
  window.dispatchEvent(new CustomEvent('model-preview:loaded', { detail: { viewer: viewerGlobalName } }));
  return result;
  } catch (error) {
    if (generation !== loadGeneration) return { cancelled: true };
    clear();
    if (error.message === 'Preview cancelled.') return { cancelled: true };
    throw error;
  } finally { if (pendingLoad === generation) pendingLoad = 0; }
}

function clear() {
  ++loadGeneration;
  pendingLoad = 0;
  context.api.stopCameraMovement();
  context.api.cancelLoad();
  sourcePayload = context.payload = null;
  context.disposeRoot();
  context.configurePreviewLighting();
  fitCamera();
  window.dispatchEvent(new CustomEvent('model-preview:cleared', { detail: { viewer: viewerGlobalName } }));
}

window[viewerGlobalName] = {
  ...context.api,
  clear, load, fitCamera, focusHead, setView, setStudioLighting, setClayMode, setGridVisible,
  setJerseyVisible, setMaterialTargetVisible, setOcclusionBase, setHeadOcclusionVisible,
  cameraState: context.cameraState, setCameraState,
  bounds,
  setVisible(value) { modelRoot.visible = Boolean(value); context.invalidateShadows(); draw(); },
  pickObject: context.pickObject,
  setWireframe(value) {
    wireframe = Boolean(value);
    context.api.setWireframe(value);
    for (const object of modelRoot.children) {
      if (object.userData.previewOccluder) for (const material of object.material) material.wireframe = false;
    }
    draw();
  },
  // Portrait mode: head-fitted shadows, 4x supersampling, SSAO and filmic tone mapping.
  capturePortrait: portraitMode ? (options) => capturePortrait(context, options) : undefined,
  setBodyFit: bodyEditor.setBodyFit,
  setChestCorrection: bodyEditor.setChestCorrection,
  getChestCorrectionPatch: bodyEditor.getChestCorrectionPatch,
  suggestBodyFit: bodyEditor.suggestBodyFit,
  stats() {
    const stats = context.api.stats();
    const normalCorrections = new Map(modelRoot.children.flatMap(object => {
      const materials = Array.isArray(object.material) ? object.material : [object.material];
      return materials.filter(material => material?.userData.normalPacking).map(material => [material.name, material.userData.normalPacking]);
    }));
    return { ...stats, payload: sourcePayload, profile: stats.previewProfile,
      rendererOrigin: context.origin, rendererSourceHash: context.sourceHash,
      studioLighting, clayMode, shadows: renderer.shadowMap.enabled,
      occlusionMeshes: modelRoot.children.filter(object => object.userData.previewOccluder && object.visible).length,
      skinBoundary: sourcePayload?.player ? {
        faceChestPairs: context.boundaryUvPairs('face', 'chest').length,
        faceChestColors: context.boundaryColors('face', 'chest'),
      } : null,
      materialTextures: stats.materialTextures.map((material) => ({
        ...material,
        normalPacking: normalCorrections.get(material.name) || null,
        inputColorSpace: sourcePayload?.textures?.find(item => item.name === material.map)?.inputColorSpace || null,
      })),
      bodyFitJersey: bodyEditor.diagnostics(),
    };
  },
};
if (cameraFollow) window.addEventListener('model-preview:camera:' + cameraFollow, event => setCameraState(event.detail));
setGridVisible(gridVisible);
