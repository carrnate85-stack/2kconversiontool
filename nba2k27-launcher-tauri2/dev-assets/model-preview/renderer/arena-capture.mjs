// Pure court-relative calculations shared by the offline viewer and checks.
export function captureLightRig(view) {
  const center = view.target;
  const length = Number(view.courtLength);
  if (!Array.isArray(center) || center.length !== 3 || !center.every(Number.isFinite) || !Number.isFinite(length) || length <= 0) {
    throw new Error('Invalid court lighting reference.');
  }
  const anchors = view.basketAnchors || [];
  let longX = 0, longZ = 1;
  if (Array.isArray(view.longAxis) && view.longAxis.length === 3 && view.longAxis.every(Number.isFinite)) {
    const norm=Math.hypot(view.longAxis[0],view.longAxis[2]);
    if(norm>0){longX=view.longAxis[0]/norm;longZ=view.longAxis[2]/norm;}
  } else if (anchors.length >= 2) {
    const first = anchors[0], last = anchors.at(-1);
    const dx = last[0] - first[0], dz = last[2] - first[2];
    const norm = Math.hypot(dx, dz);
    if (norm > 1) { longX = dx / norm; longZ = dz / norm; }
  }
  const ceilingHeight = view.ceilingHeight === null || view.ceilingHeight === undefined
    ? Infinity : Number(view.ceilingHeight) - center[1];
  const height = Math.min(length * 0.8, ceilingHeight * 0.85);
  if (!(height > 20)) throw new Error('Insufficient ceiling clearance for the capture light.');
  return {
    keyPosition: [center[0] + length * (longZ * 0.12 + longX * 0.1), center[1] + height,
      center[2] + length * (-longX * 0.12 + longZ * 0.1)],
    probePosition: [center[0], center[1] + Math.min(250, height * 0.5), center[2]],
    shadowExtent: length * 0.8, shadowNear: 20, shadowFar: height + length * 3,
  };
}

export function captureSurfaceParameters(parameter, floor) {
  const numberOr = (value, fallback) => value === undefined || value === null || !Number.isFinite(Number(value)) ? fallback : Number(value);
  const clamp = (value, min, max) => Math.max(min, Math.min(max, value));
  return {
    roughness: clamp(numberOr(parameter.DetailDefaultRoughness ?? parameter.DefaultRoughness, floor ? 0.3 : 0.65), 0.08, 1),
    metalness: clamp(numberOr(parameter.DefaultMetalness, 0), 0, 1),
  };
}

// Some basket shaders store neutral EmissiveTint as scalar 1. Indexing that
// scalar as RGB creates NaNs that contaminate the entire reflection probe.
export function captureEmissionTint(value) {
  const rgb = Array.isArray(value) ? value : [value ?? 1, value ?? 1, value ?? 1];
  return [0, 1, 2].map(axis => Number.isFinite(Number(rgb[axis]))
    ? Math.max(0, Number(rgb[axis])) : 1);
}

// Physical LED panels provide their own light. Shader identity, rather than
// names such as "scoretable", distinguishes the screen from its metal housing.
export function captureDisplayParameters(shader, parameter = {}, mapped = true, options = {}) {
  if (!mapped) return null;
  if (options.digits && /^scoreboard(?:_CLOD)?\.fx(?:#|$)/i.test(shader || '')) {
    return { kind: 'digits', emission: .6, surface: .1, brightnessSource: 'preview approximation',
      content: 'authored digit atlas; static zero, runtime values not reproduced' };
  }
  if (!/^led_CLOD\.fx(?:#|$)/i.test(shader || '')) return null;
  const value = (key, fallback) => parameter[key] == null || !Number.isFinite(Number(parameter[key]))
    ? fallback : Math.max(0, Math.min(2, Number(parameter[key])));
  return { kind: 'led', emission: value('LEDPart', .6), surface: value('NonLedPart', .1),
    content: 'authored static texture; runtime video not reproduced' };
}

export function captureAuthoredSun(lights, view) {
  const finite3 = value => Array.isArray(value) && value.length >= 3 && value.slice(0, 3).every(Number.isFinite);
  if (!finite3(view?.target) || !(Number(view.courtLength) > 0) || !Number.isFinite(Number(view.courtLength))) return null;
  for (const light of lights || []) {
    const a = light.Attribute || {};
    if (light.Type !== 'DIRECTIONAL' || Number(a.VC_IsActive) !== 1 || a.VC_BlackLight
      || !(Number(light.Intensity) > 0) || !Number.isFinite(Number(light.Intensity))
      || !finite3(a.VC_Direction) || !finite3(light.Color)) continue;
    const norm = Math.hypot(...a.VC_Direction.slice(0, 3));
    if (norm < 0.000001) continue;
    const direction = a.VC_Direction.slice(0, 3).map(value => value / norm);
    if (direction[1] >= -0.05) continue;
    return { name: light.name, direction, color: light.Color.slice(0, 3).map(value => Math.max(0, Math.min(1, value))),
      sourceIntensity: Number(light.Intensity), intensity: 0.9,
      position: view.target.map((value, axis) => value - direction[axis] * view.courtLength),
      castsShadows: Number(a.VC_CastsShadows) === 1 };
  }
  return null;
}

export function captureLayerWeights(mask, enabled = [0, 1, 2, 3]) {
  const values = [0, 1, 2].map(axis => enabled.includes(axis + 1)
    ? Math.max(0, Math.min(1, Number(mask[axis]) || 0)) : 0);
  const [r, g, b] = values;
  return [(1-r)*(1-g)*(1-b), r*(1-g)*(1-b), g*(1-b), b];
}

export function captureLayerPlan(parameter = {}, slots = []) {
  const number = (value, fallback) => value == null || !Number.isFinite(Number(value)) ? fallback : Number(value);
  const plan = [];
  for (let index = 0; index < 4; index++) {
    const prefix = `LAYER${index}_`;
    if (index && !slots.some(slot => slot.startsWith(prefix)) && !Object.keys(parameter).some(key => key.startsWith(prefix))) continue;
    const rgb = parameter[prefix + 'DefaultAlbedo'];
    const modulate = parameter[prefix + 'AlbedoModulate'] || [1, 1, 1];
    const transform = role => [number(parameter[prefix + role + 'ScaleX'], 1) * number(parameter[prefix + 'GlobalUScale'], 1),
      number(parameter[prefix + role + 'ScaleY'], 1) * number(parameter[prefix + 'GlobalVScale'], 1),
      number(parameter[prefix + role + 'OffsetX'], 0), number(parameter[prefix + role + 'OffsetY'], 0)];
    plan.push({ index, prefix, color: Array.isArray(rgb) ? rgb.slice(0, 3) : [1, 1, 1],
      hasColor: Array.isArray(rgb) || slots.includes(prefix + 'AlbedoMap'), modulate: modulate.slice(0, 3),
      roughness: Math.max(0.08, Math.min(1, number(parameter[prefix + 'DefaultRoughness'], 0.65))),
      colorTransform: transform('Albedo'), roughnessTransform: transform('Normal'), aoTransform: transform('ConeAngle') });
  }
  return plan;
}

// Use the authored line-light positions, directions, cones and relative energy.
// The engine's photometric units are normalized to a bounded center irradiance.
// Each line gets two samples; this is an approximation of its area emission.
export function captureAuthoredCourtLights(lights, view, options = {}) {
  const finite3 = value => Array.isArray(value) && value.length >= 3 && value.slice(0, 3).every(Number.isFinite);
  if (!finite3(view?.target) || !(Number(view.courtLength) > 0) || !Number.isFinite(Number(view.courtLength))) return [];
  const samples = [];
  for (const light of lights || []) {
    const a = light.Attribute || {};
    if (options.spots && light.Type === 'SPOT') {
      if (Number(a.VC_IsActive) !== 1 || a.VC_BlackLight || !(Number(light.Intensity) > 0)
        || !Number.isFinite(Number(light.Intensity)) || !finite3(light.Translate)
        || !finite3(a.VC_AimDirection) || !finite3(light.Color)) continue;
      const norm = Math.hypot(...a.VC_AimDirection.slice(0, 3));
      if (norm < 0.000001) continue;
      const angle = Math.max(0.05, Math.min(Math.PI / 2 - 0.01, Number(a.PENUMBRAANGLE) || Math.PI / 3));
      const inner = Math.max(0, Math.min(angle, Number(a.CONEANGLE) || 0));
      samples.push({ name: light.name, sourceType: 'spot', position: light.Translate.slice(0, 3),
        direction: a.VC_AimDirection.slice(0, 3).map(value => value / norm),
        color: light.Color.slice(0, 3).map(value => Math.max(0, Math.min(1, value))),
        angle, penumbra: 1 - inner / angle, distance: Math.max(0, Number(a.VC_FarPlane) || 0), intensity: Number(light.Intensity) });
      continue;
    }
    if (light.Type !== 'AREA' || Number(a.VC_IsActive) !== 1 || Number(a.VC_AreaLightType) !== 1
      || a.VC_BlackLight || !(Number(light.Intensity) > 0) || !Number.isFinite(Number(light.Intensity))
      || !finite3(a.EndPoint1) || !finite3(a.EndPoint2)
      || !finite3(a.Direction) || !finite3(light.Color)) continue;
    const directionLength = Math.hypot(...a.Direction.slice(0, 3));
    if (directionLength < 0.000001) continue;
    const direction = a.Direction.slice(0, 3).map(value => value / directionLength);
    const angle = Math.max(0.05, Math.min(Math.PI / 2 - 0.01, Number(a.Beta) || Math.PI / 3));
    const inner = Math.max(0, Math.min(angle, Number(a.Alpha) || 0));
    for (const fraction of [0.25, 0.75]) {
      const position = a.EndPoint1.slice(0, 3).map((value, axis) => value + (a.EndPoint2[axis] - value) * fraction);
      samples.push({ name: light.name, sourceType: 'line-area', position, direction,
        color: light.Color.slice(0, 3).map(value => Math.max(0, Math.min(1, value))),
        angle, penumbra: 1 - inner / angle, distance: Number.isFinite(Number(a.Radius)) ? Math.max(0, Number(a.Radius)) : 0,
        intensity: Number(light.Intensity) / 2 });
    }
  }
  // Avoid a shader with hundreds of lights. Nearby street lamps can have
  // short cutoffs and illuminate none of the court; rank outdoor samples by
  // their actual cone/cutoff contribution before spending the eight slots.
  const length = Number(view.courtLength);
  const distanceToCenter = item => Math.hypot(...item.position.map((value, axis) => value - view.target[axis]));
  const contribution = sample => {
    const delta = view.target.map((value, axis) => value - sample.position[axis]);
    const distance = Math.hypot(...delta);
    const dot = delta.reduce((sum, value, axis) => sum + value * sample.direction[axis], 0) / Math.max(1, distance);
    const outerCos = Math.cos(sample.angle), innerCos = Math.cos(sample.angle * (1 - sample.penumbra));
    const t = Math.max(0, Math.min(1, (dot - outerCos) / Math.max(0.000001, innerCos - outerCos)));
    const cone = t * t * (3 - 2 * t);
    const cutoff = sample.distance > 0 ? Math.max(0, 1 - (distance / sample.distance) ** 4) ** 2 : 1;
    return sample.intensity * cone * cutoff / Math.max(1, distance ** 2);
  };
  const eligible = options.spots ? samples.filter(sample => contribution(sample) > 0) : samples;
  eligible.sort((a, b) => options.spots
    ? contribution(b) - contribution(a) || distanceToCenter(a) - distanceToCenter(b)
    : distanceToCenter(a) - distanceToCenter(b));
  const selected = eligible.slice(0, 8);
  const centerEnergy = selected.reduce((sum, sample) => sum + contribution(sample), 0);
  if (!(centerEnergy > 0.000001) || !Number.isFinite(length) || !(length > 0)) return [];
  const scale = 0.6 / centerEnergy;
  // Bound extreme off-court sources whose center cone barely intersects.
  if (!Number.isFinite(scale) || scale > 100) return [];
  return selected.map(sample => ({ ...sample, intensity: sample.intensity * scale }));
}
