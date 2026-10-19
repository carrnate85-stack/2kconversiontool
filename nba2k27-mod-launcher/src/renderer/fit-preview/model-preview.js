import * as THREE from './vendor/three.module.min.js';
import { DDSLoader } from './vendor/DDSLoader.js';

const canvas = document.getElementById('previewCanvas');
const scene = new THREE.Scene();
scene.background = new THREE.Color(0x080b10);
scene.fog = new THREE.FogExp2(0x080b10, 0.00008);
const camera = new THREE.PerspectiveCamera(45, 1, 0.01, 1000000);
const renderer = new THREE.WebGLRenderer({ canvas, antialias: true, alpha: false, powerPreference: 'high-performance' });
renderer.outputColorSpace = THREE.SRGBColorSpace;
renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));

const ambientLight = new THREE.AmbientLight(0xffffff, 1.0);
scene.add(ambientLight);
const keyLight = new THREE.DirectionalLight(0xfff8f2, 0.85);
keyLight.position.set(2, 4, 3);
scene.add(keyLight);
const rimLight = new THREE.DirectionalLight(0xff6a32, 0.22);
rimLight.position.set(-3, 1, -2);
scene.add(rimLight);
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
let drag = null;
let selectionHelper = null;
let loadGeneration = 0;
const movementKeys = new Set();
let movementFrame = 0;
let movementTime = 0;

function isPlacedArena(payload = currentPayload) {
  const stats = payload?.stats || {};
  return Number(stats.objectMatrices || 0) >= 20
    && Number(stats.instances || 0) > Number(stats.modelsDecoded || 0);
}

function configurePreviewLighting(payload = currentPayload) {
  const playerPreview = Boolean(payload?.player);
  ambientLight.intensity = playerPreview ? 2.15 : 1.0;
  keyLight.intensity = playerPreview ? 1.75 : 0.85;
  rimLight.intensity = playerPreview ? 0.4 : 0.22;
  // Clothing comparisons need neutral lighting, not the Explorer's orange rim.
  keyLight.color.setHex(payload?.prototype ? 0xffffff : 0xfff8f2);
  rimLight.color.setHex(payload?.prototype ? 0xffffff : 0xff6a32);
}

function colorFor(name) {
  const normalized = String(name || '').toLowerCase();
  if (/eye(?:ball|white)|sclera/.test(normalized)) return new THREE.Color(0xd8d4c8);
  if (/iris|pupil/.test(normalized)) return new THREE.Color(0x332b26);
  if (/caruncle|teartube/.test(normalized)) return new THREE.Color(0x9a5d5c);
  if (/teeth/.test(normalized)) return new THREE.Color(0xd7d1bf);
  if (/tongue|mouth|gum/.test(normalized)) return new THREE.Color(0x7f4948);
  if (/hair|beard|brow|lash/.test(normalized)) return new THREE.Color(0x292624);
  if (/skin|face|head|body/.test(normalized)) return new THREE.Color(0x9b7868);
  let hash = 2166136261;
  for (const char of String(name)) hash = Math.imul(hash ^ char.charCodeAt(0), 16777619);
  const color = new THREE.Color();
  color.setHSL(((hash >>> 0) % 360) / 360, 0.07, 0.52 + ((hash >>> 8) % 8) / 100);
  return color;
}

function updateCamera() {
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
  renderer.render(scene, camera);
}

function viewerIsActive() {
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
    const speed = movementKeys.has('ShiftLeft') || movementKeys.has('ShiftRight') ? 900 : 300;
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
  clearPick(false);
  const geometries = new Set();
  const materialsToDispose = new Set();
  while (modelRoot.children.length) {
    const child = modelRoot.children.pop();
    if (child.geometry) geometries.add(child.geometry);
    const materials = Array.isArray(child.material) ? child.material : [child.material];
    materials.filter(Boolean).forEach((material) => {
      disposeDerivedTextures(material);
      if (material.userData.neutralFacialHairFitBase) {
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
      materialsToDispose.add(material);
    });
  }
  geometries.forEach((geometry) => geometry.dispose());
  materialsToDispose.forEach((material) => material.dispose());
  textureCache.forEach((texture) => texture?.dispose());
  textureCache = [];
  textureItems = [];
}

function clearPick(notify = true) {
  if (selectionHelper) {
    scene.remove(selectionHelper);
    selectionHelper.geometry?.dispose();
    selectionHelper.material?.dispose();
    selectionHelper = null;
  }
  if (notify) window.dispatchEvent(new CustomEvent('model-preview:pick', { detail: null }));
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

function cancelLoad() {
  loadGeneration += 1;
}

function fitCamera() {
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
  yaw = Math.PI / 4;
  pitch = Math.PI / 5;
  updateCamera();
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
  if (/hair/.test(name)) return 'hair';
  if (/face/.test(name)) return 'face';
  if (/arms?|chest/.test(name)) return 'chest';
  if (/torso|body/.test(name)) return 'torso';
  if (/legs?|thigh|calf/.test(name)) return 'legs';
  if (/ball/.test(name)) return 'ball';
  return '';
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
  const palette = material.userData.garmentPalette;
  material.userData.textureRange = range;
  material.onBeforeCompile = range || palette ? (shader) => {
    shader.uniforms.mapRangeMin = { value: new THREE.Vector4(...(range?.min || [0,0,0,0])) };
    shader.uniforms.mapRangeMax = { value: new THREE.Vector4(...(range?.max || [1,1,1,1])) };
    shader.uniforms.mapRangeMask = { value: new THREE.Vector4(...(range?.mask || [0,0,0,0])) };
    if(palette){
      const linear=rgb=>new THREE.Color().setRGB(...rgb.map(v=>v/255),THREE.SRGBColorSpace);
      shader.uniforms.garmentPrimary={value:linear(palette.primary)};
      shader.uniforms.garmentSecondary={value:linear(palette.secondary)};
    }
    shader.fragmentShader = shader.fragmentShader
      .replace(
        '#include <map_pars_fragment>',
        '#include <map_pars_fragment>\nuniform vec4 mapRangeMin;\nuniform vec4 mapRangeMax;\nuniform vec4 mapRangeMask;' + (palette ? '\nuniform vec3 garmentPrimary;\nuniform vec3 garmentSecondary;' : ''),
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
          ${palette ? 'sampledDiffuseColor.rgb = sampledDiffuseColor.r * garmentPrimary + sampledDiffuseColor.g * garmentSecondary + vec3(sampledDiffuseColor.b);' : ''}
          diffuseColor *= sampledDiffuseColor;
        #endif`,
      );
  } : () => {};
  material.customProgramCacheKey = () => (range ? 'nba2k27-texture-range-v1' : '') + (palette ? '-clothing-two-region-v1' : '');
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
    const materials = Array.isArray(object.material) ? object.material : [object.material];
    materials.filter(Boolean).forEach((material) => {
      disposeDerivedTextures(material);
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
      material.map = colorTexture;
      material.normalMap = normalTexture;
      if ('anisotropyMap' in material) {
        material.anisotropyMap = tangentTexture;
        material.anisotropy = tangentTexture ? 0.7 : 0;
      }
      configureTextureRange(material, colorTexture);
      if (targetName === 'eye') configureEyeIris(material, colorTexture);
      if (normalTexture) material.normalScale.set(0.7, 0.7);
      material.transparent = Boolean(colorTexture) && (targetName === 'eyelash' || strandMaterial);
      material.alphaTest = targetName === 'eyelash' ? 0.08 : strandMaterial ? 0.035 : 0;
      material.depthWrite = targetName !== 'eyelash';
      material.forceSinglePass = strandMaterial;
      material.color.copy(colorTexture && targetName !== 'eyelash'
        ? materialResponses.get(targetName) || new THREE.Color(0xffffff)
        : material.userData.neutralFacialHairFitBase ? new THREE.Color(0x8e959c) : colorFor(material.name));
      material.userData.skinMaterialResponse = materialResponses.get(targetName)?.toArray() || null;
      material.needsUpdate = true;
    });
  });
}

async function load(payload, onProgress = () => {}) {
  const generation = ++loadGeneration;
  currentPayload = payload;
  eyelashesVisible = false;
  configurePreviewLighting(payload);
  disposeRoot();
  const items = payload.meshes || [];
  for (let itemIndex = 0; itemIndex < items.length; itemIndex += 1) {
    if (generation !== loadGeneration) throw new Error('Preview cancelled.');
    const item = items[itemIndex];
    const neutralFacialHairFitBase = ['facialhair-fit-base','fit-base'].includes(item.sourceKind);
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute('position', new THREE.BufferAttribute(typed(item.positions, Float32Array), 3));
    if (item.uvs?.length) geometry.setAttribute('uv', new THREE.BufferAttribute(typed(item.uvs, Float32Array), 2));
    geometry.setIndex(new THREE.BufferAttribute(typed(item.indices, Uint32Array), 1));
    geometry.computeVertexNormals();
    const materials = [];
    for (const group of item.groups || []) {
      const targetName = materialTextureTarget(group.material);
      const Material = targetName === 'hair' || targetName === 'facialhair'
        ? THREE.MeshPhysicalMaterial
        : THREE.MeshStandardMaterial;
      const material = new Material({
        name: group.material,
        color: neutralFacialHairFitBase ? new THREE.Color(0x8e959c) : colorFor(group.material),
        roughness: neutralFacialHairFitBase ? 0.9 : targetName === 'hair' || targetName === 'facialhair' ? 0.48 : 0.72,
        metalness: neutralFacialHairFitBase ? 0 : 0.05,
        side: THREE.DoubleSide,
        wireframe,
      });
      material.userData.neutralFacialHairFitBase = neutralFacialHairFitBase;
      material.userData.garmentPalette = group.garmentPalette || null;
      const materialIndex = materials.length;
      materials.push(material);
      geometry.addGroup(group.start, group.count, materialIndex);
    }
    if (!materials.length) materials.push(new THREE.MeshStandardMaterial({ color: 0xa6adb7, wireframe }));
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
    if ((itemIndex + 1) % 12 === 0 && itemIndex + 1 < items.length) {
      onProgress({ phase: 'meshes', completed: itemIndex + 1, total: items.length });
      await nextFrame();
    }
  }
  textureItems = payload.textures || [];
  textureCache = [];
  for (let index = 0; index < textureItems.length; index += 1) {
    if (generation !== loadGeneration) throw new Error('Preview cancelled.');
    textureCache.push(createDdsTexture(textureItems[index]));
    if ((index + 1) % 4 === 0 && index + 1 < textureItems.length) {
      onProgress({ phase: 'textures', completed: index + 1, total: textureItems.length });
      await nextFrame();
    }
  }
  applyAutomaticTextures();
  if (generation !== loadGeneration) throw new Error('Preview cancelled.');
  setEyelashesVisible(false);
  fitCamera();
  if (isPlacedArena(payload)) setArenaCourtView();
  return { decodedTextures: textureCache.filter(Boolean).length };
}

async function loadArenaTextures(items, onProgress = () => {}, options = {}) {
  const generation = loadGeneration;
  const append = Boolean(options.append);
  if (!append) {
    textureCache.forEach((texture) => texture?.dispose());
    textureItems = [];
    textureCache = [];
  }
  const existingNames = new Set(textureItems.map((item) => String(item.name || '').toLowerCase()));
  const additions = (items || []).filter((item) => {
    const name = String(item?.name || '').toLowerCase();
    if (!name || existingNames.has(name)) return false;
    existingNames.add(name);
    return true;
  });
  const added = [];
  for (let index = 0; index < additions.length; index += 1) {
    if (generation !== loadGeneration) throw new Error('Preview cancelled.');
    const item = additions[index];
    const texture = createDdsTexture(item);
    const { bytes: _encodedBytes, ...storedItem } = item;
    const textureIndex = textureItems.length;
    textureItems.push(storedItem);
    textureCache.push(texture);
    added.push({ index: textureIndex, item: storedItem, decoded: Boolean(texture) });
    if ((index + 1) % 4 === 0 && index + 1 < additions.length) {
      onProgress({ completed: index + 1, total: additions.length });
      await nextFrame();
    }
  }
  applyAutomaticTextures();
  renderer.render(scene, camera);
  return {
    decodedTextures: textureCache.filter(Boolean).length,
    addedTextures: added.filter((entry) => entry.decoded).length,
    added,
  };
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
  const hit = raycaster.intersectObjects(modelRoot.children, false)[0];
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
  wireframe = Boolean(value);
  modelRoot.traverse((object) => {
    const materials = Array.isArray(object.material) ? object.material : [object.material];
    materials.filter(Boolean).forEach((material) => { material.wireframe = wireframe; material.needsUpdate = true; });
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
  if (shouldPick) pickObject(event.clientX, event.clientY);
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

window.modelPreview = {
  cancelLoad,
  colorHex: (name) => `#${colorFor(name).getHexString()}`,
  clearPick,
  fitCamera,
  load,
  loadArenaTextures,
  clearArenaTextures,
  resize,
  setTexture,
  setEyelashesVisible,
  setArenaCourtView,
  stopCameraMovement,
  setWireframe,
  stats: () => ({
    payload: currentPayload,
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
      }));
    }),
    renderer: renderer.capabilities.isWebGL2 ? 'WebGL 2' : 'WebGL 1',
  }),
};
resize();
