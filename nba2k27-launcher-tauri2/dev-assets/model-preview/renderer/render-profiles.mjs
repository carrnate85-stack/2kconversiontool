const text = (value) => String(value || '').toLowerCase();

// Preview-only regional response from paired seam samples. Never clamp the
// channels independently: a clipped blue gain alongside full red/green gains
// introduces a new hue. Reject the entire unreliable response instead.
export function skinBoundaryResponse(reference, source, referenceResponse = [1, 1, 1]) {
  if (![reference, source, referenceResponse].every(values => Array.isArray(values)
    && values.length === 3 && values.every(value => Number.isFinite(value) && value >= 0))) return null;
  const response = source.map((value, channel) => reference[channel] * referenceResponse[channel] / Math.max(value, 0.005));
  // Keep finite preview gain bounds, not guessed skin colors. Real female
  // neck samples require ~4.5x blue; the old per-channel 2.5 cap tinted them.
  return response.every(value => value >= 0.3 && value <= 8) ? response : null;
}

// Authored range reconstruction must precede the display sRGB transform.
// Per-channel and data maps also need untransformed source samples.
export function previewTextureUsesLinearSamples(item = {}, channel = 'rgb') {
  return Boolean(item.range) || channel !== 'rgb'
    || /normal|tangent|bump|mask|rough|metal|occlusion|lightmap|bc[456]/.test(`${item.name} ${item.format}`.toLowerCase());
}

export function previewKind(payload = {}) {
  if (payload?.stats?.arena) return 'arena';
  if (payload?.player) return 'player';
  if (payload?.shoe) return 'shoe';
  const haystack = [
    payload?.name,
    payload?.file,
    ...(payload?.materials || []).flatMap((material) => [material.name, material.shader]),
    ...(payload?.textures || []).flatMap((texture) => [texture.name, texture.source, texture.binding?.target]),
  ].map(text).join(' ');
  if (/(?:^|[\\/_])ball(?:_|[\\/.])|\bball_(?:gameplay|official)/.test(haystack)) return 'ball';
  if (/(?:^|[\\/_\s])clothing[\\/]|\b(?:jersey|shorts|uniform|warmup|socks?|shoes?)_/.test(haystack)) return 'apparel';
  if (/facial.?hair|beard|hair_(?:geo|shader)|_hair(?:_|\.)/.test(haystack)) return 'strand';
  return 'object';
}

const STUDIO_PROFILES = Object.freeze({
  player: Object.freeze({ id: 'studio-player-v2', ambient: 0.42, hemisphere: 0.82, key: 2.15, fill: 0.72, rim: 0.52, exposure: 1.03 }),
  shoe: Object.freeze({ id: 'studio-product-v2', ambient: 0.34, hemisphere: 0.74, key: 2.35, fill: 0.88, rim: 0.62, exposure: 1.04 }),
  apparel: Object.freeze({ id: 'studio-garment-v2', ambient: 0.4, hemisphere: 0.82, key: 2.2, fill: 0.82, rim: 0.54, exposure: 1.03 }),
  ball: Object.freeze({ id: 'studio-ball-v2', ambient: 0.3, hemisphere: 0.68, key: 2.4, fill: 0.78, rim: 0.66, exposure: 1.02 }),
  strand: Object.freeze({ id: 'studio-strand-v2', ambient: 0.38, hemisphere: 0.75, key: 2.25, fill: 0.82, rim: 0.68, exposure: 1.04 }),
  object: Object.freeze({ id: 'studio-object-v2', ambient: 0.38, hemisphere: 0.78, key: 2.2, fill: 0.82, rim: 0.56, exposure: 1.03 }),
});

export function previewLightingProfile(payload = {}) {
  const kind = previewKind(payload);
  if (kind === 'arena') return Object.freeze({
    id: 'arena-neutral-v1', kind, ambient: 1, hemisphere: 0, key: 0.85, fill: 0, rim: 0.22,
    exposure: 1, toneMapping: 'none', shadows: false,
  });
  return Object.freeze({
    ...STUDIO_PROFILES[kind],
    kind,
    toneMapping: 'neutral',
    shadows: true,
  });
}

export function materialRole(name, shader = '', kind = 'object') {
  const value = `${text(name)} ${text(shader)}`;
  if (/armpit.?hair/.test(value)) return 'strand';
  if (/eyelash|(?:^|[_\s])lash(?:[_\s]|$)/.test(value)) return 'eyelash';
  if (/facial.?hair|beard|(?:^|[_\s:])hair(?:[_\s:]|$)|hair_shader/.test(value)) return 'strand';
  if (/(?:^|[_\s])(?:l|r)?_?eye(?:s|ball|white)?(?:[_\s]|$)|sclera|iris|cornea/.test(value)) return 'eye';
  if (/glass|crystal|transparent|translucent/.test(value)) return 'glass';
  if (/emissive|led|display|screen|lightbulb|light_glow/.test(value)) return 'emissive';
  if ((kind === 'player' || /skin\.fx|character_skin|skin_shader/.test(value))
    && /(?:^|[_\s:])(?:skin|face|head|arms?|chest|torso|body|legs?|thigh|calf)(?:[_\s:]|$)/.test(value)) return 'skin';
  if (/metal|chrome|steel|aluminum|aluminium|buckle|zipper|eyelet/.test(value)) return 'metal';
  if (/rubber|outsole|midsole|sole(?:_|\s|$)|tread/.test(value)) return 'rubber';
  if (kind === 'ball' || /(?:^|[_\s])ball(?:[_\s]|$)/.test(value)) return 'ball';
  if (kind === 'apparel' || /cloth|fabric|jersey|shorts|uniform|sock|lace|mesh/.test(value)) return 'fabric';
  if (kind === 'shoe') return 'shoe';
  return 'generic';
}

const MATERIAL_PROFILES = Object.freeze({
  skin: Object.freeze({ roughness: 0.64, metalness: 0, normalScale: 0.58 }),
  eye: Object.freeze({ roughness: 0.2, metalness: 0, normalScale: 0.35, clearcoat: 0.72, clearcoatRoughness: 0.16 }),
  eyelash: Object.freeze({ roughness: 0.72, metalness: 0, normalScale: 0.35, alphaTest: 0.08 }),
  strand: Object.freeze({ roughness: 0.5, metalness: 0, normalScale: 0.62, alphaTest: 0.035, anisotropy: 0.7 }),
  fabric: Object.freeze({ roughness: 0.86, metalness: 0, normalScale: 0.62 }),
  rubber: Object.freeze({ roughness: 0.76, metalness: 0, normalScale: 0.68 }),
  metal: Object.freeze({ roughness: 0.34, metalness: 0.78, normalScale: 0.5 }),
  glass: Object.freeze({ roughness: 0.12, metalness: 0, normalScale: 0.35, clearcoat: 1, clearcoatRoughness: 0.08 }),
  emissive: Object.freeze({ roughness: 0.62, metalness: 0, normalScale: 0.45 }),
  ball: Object.freeze({ roughness: 0.7, metalness: 0, normalScale: 0.72 }),
  shoe: Object.freeze({ roughness: 0.68, metalness: 0, normalScale: 0.72 }),
  generic: Object.freeze({ roughness: 0.72, metalness: 0.03, normalScale: 0.6 }),
});

export function materialProfile(name, shader = '', kind = 'object') {
  const role = materialRole(name, shader, kind);
  return Object.freeze({ role, ...MATERIAL_PROFILES[role] });
}
