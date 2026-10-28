// Opt-in portrait additions (model-preview.js?portrait=1) on top of the shared
// renderer, which already shades 2K27 skin from the game's maps (roughness from
// *_roughcurvedense, bent-normal occlusion, the face detail normal, per-channel
// soft normals and wrapped diffuse, an oily lobe) and lights players with its
// studio environment. Portraits add procedural pores, the portrait garments
// (team color, logos, numbers, the jersey tuck) and the research lighting
// overrides.
import { DDSLoader } from './shared/vendor/DDSLoader.js';

// Procedural pore micro-surface (not a game texture), scaled per person by the
// appearance's "noise uv scale" (~31.9) and "face pore depth" (~3).
const PORE_FREQUENCY = 12;
const PORE_STRENGTH = 0.03;

export function configurePortraitSkin(context, material) {
  material.userData.portraitSkin = null;
  if (context.materialTextureTarget(material.name) !== 'face' || !material.map
    || material.userData.previewMaterialRole !== 'skin') return;
  material.userData.portraitSkin = { pores: true };
  const uniforms = {
    nbaPoreScale: { value: (Number(context.payload?.player?.poreScale) || 31.855) * PORE_FREQUENCY },
    nbaPoreStrength: { value: (Number(context.payload?.player?.poreDepth) || 3) * PORE_STRENGTH },
  };
  const previous = material.onBeforeCompile;
  const previousKey = material.customProgramCacheKey();
  material.onBeforeCompile = (shader) => {
    previous(shader);
    Object.assign(shader.uniforms, uniforms);
    shader.fragmentShader = `uniform float nbaPoreScale, nbaPoreStrength;
float nbaHash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }
float nbaNoise(vec2 p) { vec2 i = floor(p), f = fract(p); f = f * f * (3.0 - 2.0 * f);
  return mix(mix(nbaHash(i), nbaHash(i + vec2(1.0, 0.0)), f.x), mix(nbaHash(i + vec2(0.0, 1.0)), nbaHash(i + 1.0), f.x), f.y); }
` + shader.fragmentShader.replace('#include <emissivemap_fragment>', `
        #ifdef USE_NORMALMAP_TANGENTSPACE
          vec2 nbaPore = vMapUv * nbaPoreScale;
          vec2 nbaPoreSlope = vec2( nbaNoise( nbaPore + vec2( 0.35, 0.0 ) ) - nbaNoise( nbaPore - vec2( 0.35, 0.0 ) ),
            nbaNoise( nbaPore + vec2( 0.0, 0.35 ) ) - nbaNoise( nbaPore - vec2( 0.0, 0.35 ) ) );
          normal = normalize( normal - tbn * vec3( nbaPoreSlope * nbaPoreStrength, 0.0 ) );
        #endif
        #include <emissivemap_fragment>`);
  };
  material.customProgramCacheKey = () => `${previousKey}:nba-portrait-pores-v1`;
  material.needsUpdate = true;
}

// Generic portrait garments (preview-jersey companions): untextured fabric.
const GARMENT_MATERIALS = [
  // Basketball uniforms are matte mesh knit: at roughness 0.62 they read as
  // glossy next to the game's own store renders (research override: garmentRoughness).
  [/jersey/i, { color: 0x5b6472, roughness: 0.88, uniform: true }],
  [/shorts/i, { color: 0x5b6472, roughness: 0.88, uniform: true }],
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
// The renderer reuses materials across loads (jerseys of the same cut), so the
// hook is rebuilt from the material's original one whenever its overlay set
// changes. three.js only runs onBeforeCompile when the material has no program
// for its cache key yet. A material that already compiled this overlay structure
// keeps the uniform objects bound at that compile, so they are kept per
// material and updated in place. Fresh objects drew the previous jersey's
// number and logos, e.g. a home number over the away jersey of the same cut.
function configureGarmentOverlays(THREE, material, overlays, renderer) {
  const signature = overlays.map((o) => `${o.channel}:${o.name}:${o.scaleAndOffset}:${(o.glyphs || []).map((g) => g.u).join('/')}:${o.spacing ?? ''}`).join(',');
  // The base renderer re-applies its texture hook to reused materials, replacing
  // this one (configureNormalPacking wraps it instead, which keeps it). A
  // replaced hook drew no overlays, so they are rebuilt on the hook in place now,
  // even when the overlay set is unchanged (the 76ers 1997 away and alternate
  // share their logo and font).
  const installed = material.userData.nbaOverlayInstalled, key = material.customProgramCacheKey();
  const intact = installed && (material.onBeforeCompile === installed.compile || (key !== installed.key && key.startsWith(installed.key)));
  if (!intact) {
    material.userData.nbaOverlayBase = { compile: material.onBeforeCompile, key };
    material.userData.nbaOverlaySignature = undefined;
  }
  if (material.userData.nbaOverlaySignature === signature) return bindGarmentOverlays(renderer, material);
  const base = material.userData.nbaOverlayBase;
  material.userData.nbaOverlaySignature = signature;
  material.needsUpdate = true;
  if (!overlays.length) {
    material.onBeforeCompile = base.compile;
    material.customProgramCacheKey = () => base.key;
    material.userData.nbaOverlayActive = [];
    material.userData.nbaOverlayInstalled = { compile: material.onBeforeCompile, key: base.key };
    return;
  }
  const previous = base.compile, previousKey = base.key;
  const kept = material.userData.nbaOverlayUniforms ||= [];
  const uniforms = overlays.map((item, i) => {
    const glyphs = (item.glyphs || []).slice(0, 2);
    const u = kept[i] ||= { map: { value: null }, place: { value: new THREE.Vector4() }, turn: { value: 0 },
      glyphRects: { value: new THREE.Vector4() }, glyphLayout: { value: new THREE.Vector4() } };
    u.map.value = garmentTexture(THREE, item, THREE.SRGBColorSpace, THREE.ClampToEdgeWrapping);
    u.place.value.set(...item.scaleAndOffset);
    u.turn.value = (Number(item.rotate) || 0) * Math.PI / 180;
    u.glyphRects.value.set(glyphs[0]?.u ?? 0, glyphs[0]?.w ?? 0, glyphs[1]?.u ?? 0, glyphs[1]?.w ?? 0);
    u.glyphLayout.value.set(glyphs[0]?.aspect ?? 0, glyphs[1]?.aspect ?? 0, glyphs.length, Number(item.spacing) || 0);
    return { number: item.channel === 'number', ...u };
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
  material.userData.nbaOverlayActive = uniforms;
  material.userData.nbaOverlayInstalled = { compile: material.onBeforeCompile, key: material.customProgramCacheKey() };
  bindGarmentOverlays(renderer, material);
}
// three.js binds a material's uniforms when it compiles, and a material that
// switches back to a program it compiled earlier keeps the latest compile's
// uniforms. Those may lack the overlays (the 76ers 2007 home sampled unbound
// textures as garbage), so the current overlay uniforms are written into the
// material's uniform table directly.
function bindGarmentOverlays(renderer, material) {
  const table = renderer?.properties.get(material).uniforms;
  if (!table) return;
  (material.userData.nbaOverlayActive || []).forEach((u, i) => Object.assign(table, { [`nbaOverlay${i}`]: u.map, [`nbaOverlayPlace${i}`]: u.place,
    [`nbaOverlayTurn${i}`]: u.turn, [`nbaGlyphRects${i}`]: u.glyphRects, [`nbaGlyphLayout${i}`]: u.glyphLayout }));
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
  configureGarmentOverlays(THREE, material, material.map ? overlays : [], context.renderer);
  material.needsUpdate = true;
}

// Research overrides (?environment=&ambient=&hemisphere=&garmentRoughness=) and
// the portrait garments; the studio environment is the shared renderer's.
export function configurePortraitLighting(context, settings = {}) {
  const { scene } = context;
  if (settings.environment !== undefined && scene.environment) scene.environmentIntensity = settings.environment;
  const tuckAt = jerseyTuckHeight(context);
  context.modelRoot.traverse((object) => {
    for (const material of [object.material].flat().filter(Boolean)) {
      if (object.userData.previewKind === 'preview-jersey') {
        const garment = GARMENT_MATERIALS.find(([pattern]) => pattern.test(material.name))?.[1];
        if (garment) {
          // A payload tint (e.g. the team's primary uniform color) overrides the default
          // for the jersey and the viewer's shorts.
          const tint = context.payload?.portraitGarmentColor;
          material.color.setHex(Number.isInteger(tint) && /jersey|shorts/i.test(material.name) ? tint : garment.color);
          material.roughness = garment.uniform ? settings.garmentRoughness ?? garment.roughness : garment.roughness;
          material.metalness = 0;
          configureGarmentTextures(context, material);
          // Tucked: the jersey's tail ends inside the shorts' waistband.
          const tuck = tuckAt !== null && /jersey/i.test(material.name);
          material.clippingPlanes = tuck ? [new context.THREE.Plane(new context.THREE.Vector3(0, 1, 0), -tuckAt)] : null;
          if (tuck) context.renderer.localClippingEnabled = true;
        }
      }
      // Wet clearcoated eyes would otherwise mirror the whole studio over the iris.
      if ('envMapIntensity' in material) material.envMapIntensity = material.userData.previewMaterialRole === 'eye' ? 0.6 : 1;
    }
  });
  if (settings.ambient !== undefined) context.ambientLight.intensity = settings.ambient;
  if (settings.hemisphere !== undefined) context.hemisphereLight.intensity = settings.hemisphere;
}
