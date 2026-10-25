// Player/strand profiles from the exporter's render-profiles.mjs.
export function previewLightingProfile(payload = {}) {
  const strand = String(payload.player?.mode || '').startsWith('hair-catalog-');
  return strand
    ? { id: 'studio-strand-v2', ambient: 0.38, hemisphere: 0.75, key: 2.25, fill: 0.82, rim: 0.68, exposure: 1.04 }
    : { id: 'studio-player-v2', ambient: 0.42, hemisphere: 0.82, key: 2.15, fill: 0.72, rim: 0.52, exposure: 1.03 };
}

export function materialProfile(name, shader = '') {
  const value = `${name || ''} ${shader || ''}`.toLowerCase();
  if (/eyelash|(?:^|[_\s])lash(?:[_\s]|$)/.test(value)) return { role: 'eyelash', roughness: 0.72, metalness: 0, normalScale: 0.35 };
  if (/facial.?hair|beard|(?:^|[_\s:])hair(?:[_\s:]|$)|hair_shader/.test(value)) return { role: 'strand', roughness: 0.5, metalness: 0, normalScale: 0.62 };
  if (/(?:^|[_\s])(?:l|r)?_?eye(?:ball|white)?(?:[_\s]|$)|sclera|iris|cornea/.test(value)) return { role: 'eye', roughness: 0.2, metalness: 0, normalScale: 0.35, clearcoat: 0.72, clearcoatRoughness: 0.16 };
  if (/(?:^|[_\s:])(?:skin|face|head|arms?|chest|torso|body|legs?|thigh|calf)(?:[_\s:]|$)/.test(value)) return { role: 'skin', roughness: 0.64, metalness: 0, normalScale: 0.58 };
  if (/cloth|fabric|jersey|shorts|uniform|sock/.test(value)) return { role: 'fabric', roughness: 0.86, metalness: 0, normalScale: 0.62 };
  return { role: 'generic', roughness: 0.72, metalness: 0.03, normalScale: 0.6 };
}
