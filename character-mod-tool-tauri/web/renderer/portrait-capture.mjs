// Portrait capture (model-preview.js?portrait=1 → capturePortrait): head-fitted
// key-light shadows, 4x supersampled multisampled HDR render, screen-space
// ambient occlusion from the depth buffer, filmic tone mapping in the final
// composite, then a stepped downsample to the output size. ACES at exposure 1.0
// won the A/B: neutral ran orange, AgX washed skin out to gray.
const SUPERSAMPLE = 4;
// Head-and-shoulders shadow frustum half-size and map, in model units (cm).
const SHADOW_EXTENT = 30;
const SHADOW_MAP = 2048;

const SSAO_FRAGMENT = `
uniform sampler2D depthMap;
uniform mat4 projection, inverseProjection;
uniform vec2 texel;
uniform float radius, power;
varying vec2 vUv;
vec3 viewPosition(vec2 uv) {
  float depth = texture2D(depthMap, uv).x;
  vec4 clip = vec4(uv * 2.0 - 1.0, depth * 2.0 - 1.0, 1.0);
  vec4 view = inverseProjection * clip;
  return view.xyz / view.w;
}
float hash(vec2 p) { return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453); }
void main() {
  float depth = texture2D(depthMap, vUv).x;
  if (depth >= 1.0) { gl_FragColor = vec4(1.0); return; }
  vec3 position = viewPosition(vUv);
  vec3 normal = normalize(cross(dFdx(position), dFdy(position)));
  float angle = hash(vUv) * 6.2831853;
  float occlusion = 0.0;
  for (int i = 0; i < 16; i++) {
    float f = float(i) + 0.5;
    // Spiral of samples over the normal-oriented hemisphere.
    float a = angle + f * 2.39996;
    float r = sqrt(f / 16.0);
    vec3 tangent = normalize(abs(normal.z) < 0.9 ? cross(normal, vec3(0.0, 0.0, 1.0)) : cross(normal, vec3(1.0, 0.0, 0.0)));
    vec3 bitangent = cross(normal, tangent);
    vec3 dir = normalize(tangent * cos(a) * r + bitangent * sin(a) * r + normal * sqrt(max(0.0, 1.0 - r * r)));
    vec3 samplePoint = position + dir * radius * mix(0.2, 1.0, f / 16.0);
    vec4 projected = projection * vec4(samplePoint, 1.0);
    vec2 uv = projected.xy / projected.w * 0.5 + 0.5;
    float sceneZ = viewPosition(uv).z;
    float range = smoothstep(0.0, 1.0, radius / max(abs(position.z - sceneZ), 1e-4));
    occlusion += (sceneZ >= samplePoint.z + 0.02 * radius ? 1.0 : 0.0) * range;
  }
  gl_FragColor = vec4(vec3(pow(1.0 - occlusion / 16.0, power)), 1.0);
}`;

const BLUR_FRAGMENT = `
uniform sampler2D aoMap, depthMap;
uniform vec2 texel;
varying vec2 vUv;
void main() {
  float center = texture2D(depthMap, vUv).x, total = 0.0, weight = 0.0;
  for (int x = -2; x <= 2; x++) for (int y = -2; y <= 2; y++) {
    vec2 uv = vUv + vec2(float(x), float(y)) * texel;
    float w = 1.0 / (1.0 + abs(texture2D(depthMap, uv).x - center) * 4000.0);
    total += texture2D(aoMap, uv).x * w; weight += w;
  }
  gl_FragColor = vec4(vec3(total / weight), 1.0);
}`;

const COMPOSITE_FRAGMENT = `
uniform sampler2D colorMap, aoMap;
uniform float strength;
varying vec2 vUv;
void main() {
  vec4 color = texture2D(colorMap, vUv);
  color.rgb *= mix(1.0, texture2D(aoMap, vUv).x, strength);
  gl_FragColor = color;
  #include <tonemapping_fragment>
  #include <colorspace_fragment>
}`;

const VERTEX = 'varying vec2 vUv; void main() { vUv = uv; gl_Position = vec4(position.xy, 0.0, 1.0); }';

let passes = null;
function fullscreen(THREE, fragmentShader, uniforms, toneMapped = false) {
  const material = new THREE.ShaderMaterial({ vertexShader: VERTEX, fragmentShader, uniforms, toneMapped, depthTest: false, depthWrite: false });
  const scene = new THREE.Scene();
  scene.add(new THREE.Mesh(new THREE.PlaneGeometry(2, 2), material));
  return { scene, material };
}

// Re-aim the studio key light's shadow at the head and shoulders.
function fitHeadShadow(context, options = {}) {
  const { THREE, keyLight, renderer, target } = context;
  if (!keyLight.castShadow) return;
  const direction = keyLight.position.clone().sub(keyLight.target.position).normalize();
  keyLight.target.position.copy(target);
  keyLight.position.copy(target).addScaledVector(direction, SHADOW_EXTENT * 5);
  keyLight.target.updateMatrixWorld(true);
  const shadow = keyLight.shadow, camera = shadow.camera;
  camera.left = camera.bottom = -SHADOW_EXTENT;
  camera.right = camera.top = SHADOW_EXTENT;
  camera.near = 1; camera.far = SHADOW_EXTENT * 10;
  camera.updateProjectionMatrix();
  if (shadow.mapSize.x !== SHADOW_MAP) { shadow.mapSize.set(SHADOW_MAP, SHADOW_MAP); shadow.map?.dispose(); shadow.map = null; }
  shadow.bias = -0.0004;
  shadow.normalBias = SHADOW_EXTENT * 2 / SHADOW_MAP * 1.5;
  // Softbox-like penumbra: wide PCF, slightly lifted. VSM softened as well but
  // banded on suits and polos.
  const soft = options.shadowType === 'vsm';
  shadow.radius = options.shadowRadius ?? 12;
  shadow.blurSamples = 16;
  shadow.intensity = options.shadowIntensity ?? 0.8;
  if (soft) shadow.bias = -0.0001;
  const type = soft ? THREE.VSMShadowMap : THREE.PCFShadowMap;
  if (renderer.shadowMap.type !== type) { renderer.shadowMap.type = type; shadow.map?.dispose(); shadow.map = null; }
  renderer.shadowMap.needsUpdate = true;
}

// Two-pass hair: a solid alpha-tested core that writes depth, then the same
// strands alpha-blended on top (no depth writes) for soft edges and fine wisps.
function renderWithSoftHair(context, options) {
  const { renderer, scene, camera, modelRoot } = context;
  const strands = [], others = [];
  modelRoot.traverse((object) => {
    if (!object.isMesh || !object.visible) return;
    const materials = [object.material].flat().filter(Boolean);
    const hair = materials.filter((m) => m.userData.previewMaterialRole === 'strand' && m.map);
    if (hair.length) strands.push(...hair.map((material) => ({ material, saved: {
      alphaTest: material.alphaTest, transparent: material.transparent, depthWrite: material.depthWrite, alphaToCoverage: material.alphaToCoverage } })));
    if (hair.length !== materials.length || !hair.length) others.push(object);
  });
  if (!strands.length || options.softHair === false) { renderer.render(scene, camera); return; }
  const set = (values) => strands.forEach(({ material }) => { Object.assign(material, values); material.needsUpdate = true; });
  const autoClear = renderer.autoClear;
  try {
    set({ alphaTest: options.hairCore ?? 0.5, transparent: false, depthWrite: true, alphaToCoverage: false });
    renderer.render(scene, camera);
    others.forEach((object) => { object.userData.portraitHidden = true; object.visible = false; });
    set({ alphaTest: 0.004, transparent: true, depthWrite: false });
    renderer.autoClear = false;
    renderer.shadowMap.needsUpdate = false;
    renderer.render(scene, camera);
  } finally {
    renderer.autoClear = autoClear;
    others.forEach((object) => { if (object.userData.portraitHidden) { object.visible = true; delete object.userData.portraitHidden; } });
    strands.forEach(({ material, saved }) => { Object.assign(material, saved); material.needsUpdate = true; });
  }
}

export function capturePortrait(context, options = {}) {
  const { THREE, renderer, scene, camera } = context;
  const size = options.size || 512, scale = options.supersample || SUPERSAMPLE, width = size * scale;
  fitHeadShadow(context, options);
  const toneMappings = { agx: THREE.AgXToneMapping, neutral: THREE.NeutralToneMapping, aces: THREE.ACESFilmicToneMapping };
  const previous = { toneMapping: renderer.toneMapping, exposure: renderer.toneMappingExposure, pixelRatio: renderer.getPixelRatio(), target: renderer.getRenderTarget() };
  renderer.toneMapping = toneMappings[options.tone || 'aces'] ?? THREE.ACESFilmicToneMapping;
  renderer.toneMappingExposure = options.exposure ?? 1.0;
  if (!passes || passes.width !== width) {
    passes?.dispose();
    const depthTexture = new THREE.DepthTexture(width, width, THREE.UnsignedIntType);
    const color = new THREE.WebGLRenderTarget(width, width, { type: THREE.HalfFloatType, samples: 4, depthTexture });
    const ao = new THREE.WebGLRenderTarget(width, width), blur = new THREE.WebGLRenderTarget(width, width);
    const texel = new THREE.Vector2(1 / width, 1 / width);
    const ssao = fullscreen(THREE, SSAO_FRAGMENT, { depthMap: { value: depthTexture }, projection: { value: new THREE.Matrix4() }, inverseProjection: { value: new THREE.Matrix4() }, texel: { value: texel }, radius: { value: 2.5 }, power: { value: 1.6 } });
    const smooth = fullscreen(THREE, BLUR_FRAGMENT, { aoMap: { value: ao.texture }, depthMap: { value: depthTexture }, texel: { value: texel } });
    const composite = fullscreen(THREE, COMPOSITE_FRAGMENT, { colorMap: { value: color.texture }, aoMap: { value: blur.texture }, strength: { value: 0.75 } }, true);
    const quadCamera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0, 1);
    passes = { width, color, ao, blur, ssao, smooth, composite, quadCamera,
      dispose() { for (const t of [color, ao, blur]) t.dispose(); depthTexture.dispose(); for (const p of [ssao, smooth, composite]) p.material.dispose(); } };
  }
  const { color, ao, blur, ssao, smooth, composite, quadCamera } = passes;
  try {
    camera.updateMatrixWorld(true);
    ssao.material.uniforms.projection.value.copy(camera.projectionMatrix);
    ssao.material.uniforms.inverseProjection.value.copy(camera.projectionMatrixInverse);
    ssao.material.uniforms.radius.value = options.aoRadius ?? 2.5;
    composite.material.uniforms.strength.value = options.ao ?? 0.75;
    renderer.setPixelRatio(1);
    renderer.setSize(width, width, false);
    renderer.setRenderTarget(color); renderer.setClearColor(0x000000, 0); renderer.clear(); renderWithSoftHair(context, options);
    renderer.setRenderTarget(ao); renderer.render(ssao.scene, quadCamera);
    renderer.setRenderTarget(blur); renderer.render(smooth.scene, quadCamera);
    renderer.setRenderTarget(null); renderer.clear(); renderer.render(composite.scene, quadCamera);
    // Stepped halving keeps hair and lashes smooth when shrinking 4x.
    let source = renderer.domElement, current = width;
    while (current > size) {
      const next = Math.max(size, current / 2);
      const step = Object.assign(document.createElement('canvas'), { width: next, height: next });
      const context2d = step.getContext('2d');
      context2d.imageSmoothingQuality = 'high';
      context2d.drawImage(source, 0, 0, next, next);
      source = step; current = next;
    }
    return source.toDataURL('image/png');
  } finally {
    renderer.toneMapping = previous.toneMapping;
    renderer.toneMappingExposure = previous.exposure;
    renderer.setRenderTarget(previous.target);
    renderer.setPixelRatio(previous.pixelRatio);
    const canvas = renderer.domElement;
    renderer.setSize(canvas.clientWidth || size, canvas.clientHeight || size, false);
  }
}
