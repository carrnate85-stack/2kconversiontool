// Opt-in portrait shading (model-preview.js?portrait=1). Uses the game's own
// skin maps, range-decoded with their TXTR Min/Max, which the default viewer
// leaves unbound: *_roughcurvedense R = roughness, *_bentnormal A = ambient
// occlusion, face_detailnormal RG = fine (pore/wrinkle) normal detail. All
// share the color map's UVs. Skin also gets a subsurface approximation:
// per-channel softened normals and wrapped diffuse (red scatters furthest).
import { DDSLoader } from './explorer/vendor/DDSLoader.js';

const SKIN_TARGETS = new Set(['face', 'chest', 'torso', 'legs']);
const DETAIL_STRENGTH = 0.7;
const DIRECT_OCCLUSION = 0.15;
// Share of the detailed (mapped) normal in R/G/B diffuse; the rest is the smooth surface.
const SKIN_NORMAL_MIX = [0.3, 0.65, 0.85];
// Diffuse wrap past the terminator per channel.
const SKIN_WRAP = [0.5, 0.2, 0.12];
// Second, tighter specular lobe for skin's oily top layer.
const SKIN_OIL_ROUGHNESS = 0.5;
const SKIN_OIL_WEIGHT = 0.25;
// Procedural pore micro-surface (not a game texture), scaled per person by the
// appearance's "noise uv scale" (~31.9) and "face pore depth" (~3).
const PORE_FREQUENCY = 12;
const PORE_STRENGTH = 0.03;

function rangeUniforms(texture) {
  const range = texture?.userData?.range;
  return {
    min: { value: new Float32Array(range?.min || [0, 0, 0, 0]) },
    max: { value: new Float32Array(range?.max || [1, 1, 1, 1]) },
    mask: { value: new Float32Array(range?.mask || [0, 0, 0, 0]) },
  };
}

export function configurePortraitSkin(context, material) {
  const target = context.materialTextureTarget(material.name);
  material.userData.portraitSkin = null;
  if (!SKIN_TARGETS.has(target) || !material.map || material.userData.previewMaterialRole !== 'skin') return;
  // Per-character config maps precede male_skin's generic ones in binding order.
  const rough = context.boundTextureMatching(target, 'auxiliary', /roughcurvedense/i);
  const occlusion = context.boundTextureMatching(target, 'auxiliary', /bentnormal/i);
  const detail = target === 'face' ? context.boundTextureMatching('face', 'normal', /detailnormal/i) : null;
  material.userData.portraitSkin = { roughness: rough?.name || '', occlusion: occlusion?.name || '', detail: detail?.name || '', subsurface: true };
  const { THREE } = context;
  const directLine = 'reflectedLight.directDiffuse += irradiance * BRDF_Lambert( material.diffuseContribution );';
  const physicalChunk = THREE.ShaderChunk.lights_physical_pars_fragment;
  if (!physicalChunk.includes(directLine)) throw new Error('Skin diffuse shader hook changed.');
  const skinChunk = physicalChunk.replace(directLine, `vec3 nbaSkinNL = mix( vec3( dot( nbaSmoothNormal, directLight.direction ) ),
      vec3( dot( geometryNormal, directLight.direction ) ), nbaSkinNormalMix );
    vec3 nbaSkinDiffuse = saturate( ( nbaSkinNL + nbaSkinWrap ) / ( 1.0 + nbaSkinWrap ) );
    reflectedLight.directDiffuse += nbaSkinDiffuse * directLight.color * BRDF_Lambert( material.diffuseContribution );
    PhysicalMaterial nbaOilLayer = material;
    nbaOilLayer.roughness = max( 0.12, material.roughness * nbaSkinOilRoughness );
    reflectedLight.directSpecular += irradiance * BRDF_GGX( directLight.direction, geometryViewDir, geometryNormal, nbaOilLayer ) * nbaSkinOilWeight;`);
  const uniforms = {
    nbaRoughMap: { value: rough }, nbaOcclusionMap: { value: occlusion }, nbaDetailMap: { value: detail },
    nbaDetailStrength: { value: DETAIL_STRENGTH }, nbaDirectOcclusion: { value: DIRECT_OCCLUSION },
    nbaSkinNormalMix: { value: new THREE.Vector3(...SKIN_NORMAL_MIX) }, nbaSkinWrap: { value: new THREE.Vector3(...SKIN_WRAP) },
    nbaSkinOilRoughness: { value: SKIN_OIL_ROUGHNESS }, nbaSkinOilWeight: { value: SKIN_OIL_WEIGHT },
    nbaPoreScale: { value: (Number(context.payload?.player?.poreScale) || 31.855) * PORE_FREQUENCY },
    nbaPoreStrength: { value: (Number(context.payload?.player?.poreDepth) || 3) * PORE_STRENGTH },
  };
  for (const [prefix, texture] of [['nbaRough', rough], ['nbaOcclusion', occlusion], ['nbaDetail', detail]]) {
    const range = rangeUniforms(texture);
    uniforms[prefix + 'Min'] = range.min; uniforms[prefix + 'Max'] = range.max; uniforms[prefix + 'Mask'] = range.mask;
  }
  const previous = material.onBeforeCompile;
  const previousKey = material.customProgramCacheKey();
  const flags = `${rough ? 'R' : ''}${occlusion ? 'O' : ''}${detail ? 'D' : ''}`;
  material.onBeforeCompile = (shader) => {
    previous(shader);
    Object.assign(shader.uniforms, uniforms);
    const decode = (prefix) => `mix(${prefix}Sample, ${prefix}Min + ${prefix}Sample * (${prefix}Max - ${prefix}Min), ${prefix}Mask)`;
    if (!shader.fragmentShader.includes('#include <lights_physical_pars_fragment>')) throw new Error('Skin lighting include missing.');
    shader.fragmentShader = shader.fragmentShader
      .replace('#include <lights_physical_pars_fragment>', `uniform vec3 nbaSkinNormalMix, nbaSkinWrap;
uniform float nbaSkinOilRoughness, nbaSkinOilWeight;
vec3 nbaSmoothNormal;
${skinChunk}`)
      .replace('#include <lights_fragment_begin>', 'nbaSmoothNormal = nonPerturbedNormal;\n#include <lights_fragment_begin>');
    shader.fragmentShader = `${rough ? 'uniform sampler2D nbaRoughMap; uniform vec4 nbaRoughMin, nbaRoughMax, nbaRoughMask;\n' : ''}`
      + `${occlusion ? 'uniform sampler2D nbaOcclusionMap; uniform vec4 nbaOcclusionMin, nbaOcclusionMax, nbaOcclusionMask; uniform float nbaDirectOcclusion;\n' : ''}`
      + `${detail ? `uniform sampler2D nbaDetailMap; uniform vec4 nbaDetailMin, nbaDetailMax, nbaDetailMask; uniform float nbaDetailStrength, nbaPoreScale, nbaPoreStrength;
float nbaHash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }
float nbaNoise(vec2 p) { vec2 i = floor(p), f = fract(p); f = f * f * (3.0 - 2.0 * f);
  return mix(mix(nbaHash(i), nbaHash(i + vec2(1.0, 0.0)), f.x), mix(nbaHash(i + vec2(0.0, 1.0)), nbaHash(i + 1.0), f.x), f.y); }
` : ''}`
      + shader.fragmentShader;
    if (rough) {
      shader.fragmentShader = shader.fragmentShader.replace('#include <roughnessmap_fragment>', `#include <roughnessmap_fragment>
        vec4 nbaRoughSample = texture2D( nbaRoughMap, vMapUv );
        roughnessFactor = clamp( ${decode('nbaRough')}.r, 0.2, 1.0 );`);
    }
    if (detail) {
      shader.fragmentShader = shader.fragmentShader.replace('#include <emissivemap_fragment>', `
        #ifdef USE_NORMALMAP_TANGENTSPACE
          vec4 nbaDetailSample = texture2D( nbaDetailMap, vMapUv );
          vec2 nbaDetailXY = ( ${decode('nbaDetail')}.xy * 2.0 - 1.0 ) * nbaDetailStrength;
          normal = normalize( normal + tbn * vec3( nbaDetailXY, 0.0 ) );
          vec2 nbaPore = vMapUv * nbaPoreScale;
          vec2 nbaPoreSlope = vec2( nbaNoise( nbaPore + vec2( 0.35, 0.0 ) ) - nbaNoise( nbaPore - vec2( 0.35, 0.0 ) ),
            nbaNoise( nbaPore + vec2( 0.0, 0.35 ) ) - nbaNoise( nbaPore - vec2( 0.0, 0.35 ) ) );
          normal = normalize( normal - tbn * vec3( nbaPoreSlope * nbaPoreStrength, 0.0 ) );
        #endif
        #include <emissivemap_fragment>`);
    }
    if (occlusion) {
      shader.fragmentShader = shader.fragmentShader.replace('#include <aomap_fragment>', `#include <aomap_fragment>
        vec4 nbaOcclusionSample = texture2D( nbaOcclusionMap, vMapUv );
        float nbaOcclusion = clamp( ${decode('nbaOcclusion')}.a, 0.0, 1.0 );
        reflectedLight.indirectDiffuse *= nbaOcclusion;
        reflectedLight.indirectSpecular *= nbaOcclusion;
        reflectedLight.directDiffuse *= mix( 1.0, nbaOcclusion, nbaDirectOcclusion );
        reflectedLight.directSpecular *= mix( 1.0, nbaOcclusion, nbaDirectOcclusion );`);
    }
  };
  material.customProgramCacheKey = () => `${previousKey}:nba-portrait-skin-v3-${flags}`;
  material.needsUpdate = true;
}

// Soft photo-studio image lighting from the same key/fill/rim directions as
// the exporter's studio rig, replacing its flat ambient/hemisphere fill.
// Generic portrait garments (preview-jersey companions): untextured fabric.
const GARMENT_MATERIALS = [
  [/jersey/i, { color: 0x5b6472, roughness: 0.62 }],
  [/shorts/i, { color: 0x5b6472, roughness: 0.66 }],
  [/polo/i, { color: 0x1c2333, roughness: 0.88 }],
  [/blazer/i, { color: 0x2b2e34, roughness: 0.8 }],
  [/bdshirt|shirt/i, { color: 0xe9e9ec, roughness: 0.72 }],
  [/tie/i, { color: 0x2a3550, roughness: 0.45 }],
];
// Game garment normals (folds, seams, piping, buttons) from
// payload.portraitGarmentTextures.
const garmentTextures = new Map();
// A plain loop: Uint8Array.from(atob(s), callback) took ~0.5 s per jersey's textures.
function decodeBase64(data) {
  const text = atob(data), out = new Uint8Array(text.length);
  for (let i = 0; i < text.length; i += 1) out[i] = text.charCodeAt(i);
  return out;
}
function garmentTexture(THREE, item, colorSpace = THREE.NoColorSpace, wrap = THREE.RepeatWrapping) {
  const key = `${item.name}|${colorSpace}|${wrap}`;
  if (garmentTextures.has(key)) return garmentTextures.get(key);
  // Raw bytes when the caller has them (jersey previews); base64 otherwise.
  const bytes = item.bytes ?? decodeBase64(item.data);
  const whole = bytes.byteOffset === 0 && bytes.byteLength === bytes.buffer.byteLength ? bytes.buffer : bytes.slice().buffer;
  const parsed = new DDSLoader().parse(whole, true);
  const texture = new THREE.CompressedTexture(parsed.mipmaps, parsed.width, parsed.height, parsed.format);
  Object.assign(texture, { name: item.name, colorSpace, magFilter: THREE.LinearFilter, anisotropy: 8,
    minFilter: parsed.mipmapCount > 1 ? THREE.LinearMipmapLinearFilter : THREE.LinearFilter });
  texture.wrapS = texture.wrapT = wrap;
  texture.needsUpdate = true;
  garmentTextures.set(key, texture);
  return texture;
}
// Team uniforms (char/uniforms/<file>): the jersey/shorts color map replaces the
// tint, and chest/NBA logos are composited over it in the shader, placed by the
// uniform RDAT's scale_and_offset [sx, sy, ox, oy], a scale about the texture
// center plus an offset: logoUv = (uv - 0.5 - o) * s + 0.5 (checked against the
// Lakers 1978 home wordmark, which lands centered on the chest).
const MAP_LINE = '#include <map_fragment>';
// Numbers use the uniform's font atlas (clothing/<file>_font.iff): up to two
// glyphs ({u, w} in atlas width, aspect = glyph width / atlas height), laid out
// centered in the placement box at glyph height = box height, with spacing in
// box widths between them.
// The renderer reuses materials across loads, so the hook is rebuilt from the
// material's original one whenever its overlay set changes.
function configureGarmentOverlays(THREE, material, overlays) {
  const signature = overlays.map((o) => `${o.channel}:${o.name}:${o.scaleAndOffset}:${(o.glyphs || []).map((g) => g.u).join('/')}:${o.spacing ?? ''}`).join(',');
  if (material.userData.nbaOverlaySignature === signature) return;
  const base = material.userData.nbaOverlayBase ||= { compile: material.onBeforeCompile, key: material.customProgramCacheKey() };
  material.userData.nbaOverlaySignature = signature;
  material.needsUpdate = true;
  if (!overlays.length) {
    material.onBeforeCompile = base.compile;
    material.customProgramCacheKey = () => base.key;
    return;
  }
  const previous = base.compile, previousKey = base.key;
  const uniforms = overlays.map((item) => {
    const glyphs = (item.glyphs || []).slice(0, 2);
    return {
      number: item.channel === 'number',
      map: { value: garmentTexture(THREE, item, THREE.SRGBColorSpace, THREE.ClampToEdgeWrapping) },
      place: { value: new THREE.Vector4(...item.scaleAndOffset) },
      turn: { value: (Number(item.rotate) || 0) * Math.PI / 180 },
      glyphRects: { value: new THREE.Vector4(glyphs[0]?.u ?? 0, glyphs[0]?.w ?? 0, glyphs[1]?.u ?? 0, glyphs[1]?.w ?? 0) },
      glyphLayout: { value: new THREE.Vector4(glyphs[0]?.aspect ?? 0, glyphs[1]?.aspect ?? 0, glyphs.length, Number(item.spacing) || 0) },
    };
  });
  material.onBeforeCompile = (shader, renderer) => {
    previous.call(material, shader, renderer);
    uniforms.forEach((u, i) => Object.assign(shader.uniforms, { [`nbaOverlay${i}`]: u.map, [`nbaOverlayPlace${i}`]: u.place, [`nbaOverlayTurn${i}`]: u.turn,
      [`nbaGlyphRects${i}`]: u.glyphRects, [`nbaGlyphLayout${i}`]: u.glyphLayout }));
    const declare = uniforms.map((_, i) => `uniform sampler2D nbaOverlay${i}; uniform vec4 nbaOverlayPlace${i}; uniform float nbaOverlayTurn${i}; uniform vec4 nbaGlyphRects${i}; uniform vec4 nbaGlyphLayout${i};`).join('\n');
    const blend = uniforms.map((u, i) => `{
      vec2 c = ( vMapUv - 0.5 - nbaOverlayPlace${i}.zw ) * nbaOverlayPlace${i}.xy + 0.5;
      vec2 r = mat2( cos( nbaOverlayTurn${i} ), -sin( nbaOverlayTurn${i} ), sin( nbaOverlayTurn${i} ), cos( nbaOverlayTurn${i} ) ) * ( c - 0.5 ) + 0.5;
      ${u.number ? `
      // Glyph widths in box units (the box is 1/sx by 1/sy of the texture).
      float aspect = nbaOverlayPlace${i}.x / nbaOverlayPlace${i}.y;
      float w0 = nbaGlyphLayout${i}.x * aspect, w1 = nbaGlyphLayout${i}.z > 1.5 ? nbaGlyphLayout${i}.y * aspect : 0.0;
      float gap = nbaGlyphLayout${i}.z > 1.5 ? nbaGlyphLayout${i}.w : 0.0;
      float x = r.x - 0.5 + ( w0 + gap + w1 ) * 0.5;
      vec2 atlas = vec2( -1.0 );
      if ( r.y >= 0.0 && r.y <= 1.0 ) {
        if ( x >= 0.0 && x <= w0 ) atlas = vec2( nbaGlyphRects${i}.x + x / w0 * nbaGlyphRects${i}.y, r.y );
        else if ( w1 > 0.0 && x >= w0 + gap && x <= w0 + gap + w1 ) atlas = vec2( nbaGlyphRects${i}.z + ( x - w0 - gap ) / w1 * nbaGlyphRects${i}.w, r.y );
      }
      if ( atlas.x >= 0.0 ) {
        vec4 glyph = texture2D( nbaOverlay${i}, atlas );
        diffuseColor.rgb = mix( diffuseColor.rgb, glyph.rgb, glyph.a );
      }` : `
      if ( all( greaterThanEqual( r, vec2( 0.0 ) ) ) && all( lessThanEqual( r, vec2( 1.0 ) ) ) ) {
        vec4 logo = texture2D( nbaOverlay${i}, r );
        diffuseColor.rgb = mix( diffuseColor.rgb, logo.rgb, logo.a );
      }`}
    }`).join('\n');
    shader.fragmentShader = `${declare}\n${shader.fragmentShader.replace(MAP_LINE, `${MAP_LINE}\n${blend}`)}`;
  };
  // Programs depend only on the overlay structure (values are uniforms), so
  // garment sets with the same logos/numbers layout share one compiled program.
  const structure = overlays.map((o) => o.channel).join(',');
  material.customProgramCacheKey = () => `${previousKey}|nba-garment-overlays-${structure}`;
}
// A renderer that loads one garment set after another (jersey previews) keeps
// only the current payload's textures; earlier ones are released.
let garmentPayload = null;
function releaseStaleGarmentTextures(payload) {
  if (payload === garmentPayload) return;
  garmentPayload = payload;
  const current = new Set((payload?.portraitGarmentTextures || []).map((item) => item.name));
  for (const [key, texture] of garmentTextures) {
    if (current.has(key.split('|')[0])) continue;
    texture.dispose();
    garmentTextures.delete(key);
  }
}
// payload.tuckJersey: a jersey shown without a body hangs over the shorts, so it
// is clipped a little below the top of the shorts' waistband (world height).
const TUCK_INSET = 3;
function jerseyTuckHeight(context) {
  if (!context.payload?.tuckJersey) return null;
  const { THREE } = context, bounds = new THREE.Box3();
  context.modelRoot.traverse((object) => {
    if (object.userData.previewKind === 'preview-jersey' && [object.material].flat().some((m) => /shorts/i.test(m?.name || ''))) bounds.expandByObject(object, true);
  });
  return bounds.isEmpty() ? null : bounds.max.y - TUCK_INSET;
}
function configureGarmentTextures(context, material) {
  const { THREE } = context;
  releaseStaleGarmentTextures(context.payload);
  const items = (context.payload?.portraitGarmentTextures || []).filter((item) => material.name.toLowerCase().includes(item.material));
  const normal = items.find((item) => item.channel === 'normal');
  if (normal) {
    material.normalMap = garmentTexture(THREE, normal);
    material.normalScale.set(1, 1);
  }
  const color = items.find((item) => item.channel === 'color');
  if (color) {
    material.map = garmentTexture(THREE, color, THREE.SRGBColorSpace);
    material.color.setHex(0xffffff);
  }
  const overlays = items.filter((item) => ['logo', 'number'].includes(item.channel) && Array.isArray(item.scaleAndOffset));
  configureGarmentOverlays(THREE, material, material.map ? overlays : []);
  material.needsUpdate = true;
}
// Richer procedural iris on top of the exporter's flat iris disc: radial
// fibers, a lighter collarette, darker toward the pupil, a dark limbal ring,
// and a slight sclera falloff toward the lids.
const IRIS_LINE = 'sampledDiffuseColor.rgb = mix( sampledDiffuseColor.rgb, nbaIrisColor * nbaIrisRays, nbaIrisMask );';
function configurePortraitEye(material) {
  const previous = material.onBeforeCompile, previousKey = material.customProgramCacheKey();
  material.onBeforeCompile = (shader) => {
    previous(shader);
    if (!shader.fragmentShader.includes(IRIS_LINE)) return;
    shader.fragmentShader = shader.fragmentShader.replace(IRIS_LINE, `
      vec2 nbaIrisOffset = nbaEyeUv - vec2( 0.5 );
      float nbaIrisAngle = atan( nbaIrisOffset.y, nbaIrisOffset.x );
      float nbaFibers = 0.78 + 0.22 * sin( nbaIrisAngle * 85.0 + 2.0 * sin( nbaIrisAngle * 19.0 ) ) * ( 0.65 + 0.35 * sin( nbaIrisDistance * 70.0 ) );
      float nbaCollarette = smoothstep( 0.085, 0.11, nbaIrisDistance ) * ( 1.0 - smoothstep( 0.11, 0.145, nbaIrisDistance ) );
      vec3 nbaIris = nbaIrisColor * nbaFibers * ( 1.0 + 0.4 * nbaCollarette );
      nbaIris *= mix( 0.72, 1.0, smoothstep( 0.06, 0.15, nbaIrisDistance ) );
      nbaIris *= mix( 1.0, 0.3, smoothstep( 0.165, 0.235, nbaIrisDistance ) );
      sampledDiffuseColor.rgb *= mix( 1.0, 0.82, smoothstep( 0.32, 0.5, length( nbaIrisOffset ) ) );
      sampledDiffuseColor.rgb = mix( sampledDiffuseColor.rgb, nbaIris, nbaIrisMask );`);
  };
  material.customProgramCacheKey = () => `${previousKey}:nba-portrait-iris-v1`;
  material.needsUpdate = true;
}
let studioEnvironment = null;
const averagedLightmaps = new WeakMap();
export function configurePortraitLighting(context, settings = {}) {
  const { THREE, renderer, scene } = context;
  if (!studioEnvironment) {
    const room = new THREE.Scene();
    const material = (value) => new THREE.MeshBasicMaterial({ color: new THREE.Color(...value), side: THREE.DoubleSide });
    room.add(new THREE.Mesh(new THREE.SphereGeometry(50, 32, 16), material([0.035, 0.036, 0.04])));
    const panel = (direction, size, value) => {
      const mesh = new THREE.Mesh(new THREE.PlaneGeometry(size[0], size[1]), material(value));
      mesh.position.copy(new THREE.Vector3(...direction).normalize().multiplyScalar(20));
      mesh.lookAt(0, 0, 0);
      room.add(mesh);
    };
    // Photo-studio sized softboxes: small enough for compact eye catchlights.
    panel([1.5, 2.3, 2], [8, 10], [14, 13.4, 12.8]);
    panel([-2.1, 1.15, 1.3], [7, 9], [3.0, 3.2, 3.5]);
    panel([-1.4, 1.7, -2.2], [4, 11], [5.5, 5.3, 5.0]);
    panel([0, 3, 0.2], [12, 12], [1.2, 1.2, 1.22]);
    panel([0, -3, 1], [40, 40], [0.12, 0.105, 0.09]);
    const generator = new THREE.PMREMGenerator(renderer);
    studioEnvironment = generator.fromScene(room, 0.035);
    generator.dispose();
    room.traverse((object) => { object.geometry?.dispose(); object.material?.dispose(); });
  }
  scene.environment = studioEnvironment.texture;
  scene.environmentIntensity = settings.environment ?? 0.5;
  // Hair lightmaps are scalp-space clump maps sampled with TEXCOORD1 (the
  // strand's attachment position). Where the mesh has that set (lightmapUv),
  // point the existing strand hook at it. Otherwise the exporter would sample
  // with the color UVs and stamp a cell pattern over the cards, so feed it one
  // texel holding the map's mean raw value (Min/Max still decode).
  const meanLightmap = (texture) => {
    if (averagedLightmaps.has(texture)) return averagedLightmaps.get(texture);
    const pixels = context.decodedTexturePixels(texture)?.pixels;
    const mean = [0, 0, 0];
    if (pixels?.length) for (let i = 0; i < pixels.length; i += 4) for (let c = 0; c < 3; c += 1) mean[c] += pixels[i + c];
    const count = pixels?.length ? pixels.length / 4 : 0;
    const texel = count ? mean.map((sum) => Math.round(sum / count)) : [255, 255, 255];
    const averaged = Object.assign(new THREE.DataTexture(new Uint8Array([...texel, 255]), 1, 1), { needsUpdate: true });
    averagedLightmaps.set(texture, averaged);
    return averaged;
  };
  const tuckAt = jerseyTuckHeight(context);
  context.modelRoot.traverse((object) => {
    for (const material of [object.material].flat().filter(Boolean)) {
      if (object.userData.previewKind === 'preview-jersey') {
        const garment = GARMENT_MATERIALS.find(([pattern]) => pattern.test(material.name))?.[1];
        if (garment) {
          // A payload tint (e.g. the team's primary uniform color) overrides the default.
          const tint = context.payload?.portraitGarmentColor;
          material.color.setHex(Number.isInteger(tint) && /jersey/i.test(material.name) ? tint : garment.color);
          material.roughness = garment.roughness;
          material.metalness = 0;
          configureGarmentTextures(context, material);
          // Tucked: the jersey's tail ends inside the shorts' waistband.
          const tuck = tuckAt !== null && /jersey/i.test(material.name);
          material.clippingPlanes = tuck ? [new THREE.Plane(new THREE.Vector3(0, 1, 0), -tuckAt)] : null;
          if (tuck) context.renderer.localClippingEnabled = true;
        }
      }
      // Wet clearcoated eyes would otherwise mirror the whole studio over the iris.
      if ('envMapIntensity' in material) material.envMapIntensity = material.userData.previewMaterialRole === 'eye' ? 0.6 : 1;
      if (material.userData.previewMaterialRole === 'eye' && material.map) configurePortraitEye(material);
      const strand = material.userData.strandLightmapUniforms;
      const lightmap = strand?.nbaStrandLightmap.value;
      if (!lightmap || lightmap.isDataTexture) continue;
      if (object.geometry?.getAttribute('lightmapUv')) {
        const sample = 'vNbaStrandLightUv = (nbaStrandLightTransform * vec3(uv, 1.0)).xy;';
        const previous = material.onBeforeCompile;
        const previousKey = material.customProgramCacheKey();
        material.onBeforeCompile = (shader) => {
          previous(shader);
          if (!shader.vertexShader.includes(sample)) throw new Error('Strand lightmap UV hook changed.');
          shader.vertexShader = 'attribute vec2 lightmapUv;\n' + shader.vertexShader
            .replace(sample, 'vNbaStrandLightUv = (nbaStrandLightTransform * vec3(lightmapUv, 1.0)).xy;');
        };
        material.customProgramCacheKey = () => `${previousKey}:nba-lightmap-uv1`;
        material.needsUpdate = true;
      } else strand.nbaStrandLightmap.value = meanLightmap(lightmap);
    }
  });
  context.ambientLight.intensity = settings.ambient ?? 0.15;
  context.hemisphereLight.intensity = settings.hemisphere ?? 0.35;
}
