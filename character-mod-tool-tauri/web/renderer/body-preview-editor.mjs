// Existing Character Mod Tool geometry editor, separated from rendering.
import { previewGarmentKind } from './shared/studio-shading.mjs';

export function createBodyEditor(context) {
const { THREE, modelRoot, renderer: sourceRenderer, scene, camera, materialTextureTarget, typed } = context;
// Keep geometry calculations intact while invalidating shadows after deformation.
const renderer = { render(scene, camera) { context.invalidateShadows(); sourceRenderer.render(scene, camera); } };
let bodyFitContext = null;
let currentBodyFitValues = {};
let chestCorrectionEnabled = false;
let lastChestCorrection = null;
let jerseyFitDiagnostics = null;
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

// Body vertices bucketed by integer cell (nested maps). String cell keys and a
// copied, fully sorted candidate list per jersey vertex made this the slowest
// step of a player load (~0.4 s for a full body and jersey).
function prepareJerseySurfaceBindings() {
  const garmentObjects = [...(bodyFitContext?.jerseyObjects || []), ...(bodyFitContext?.footwearObjects || [])];
  if (!bodyFitContext?.bodyObjects.length || !garmentObjects.length) return null;
  const cellSize = Math.max(0.5, bodyFitContext.height * 0.018, bodyFitContext.torsoHalfWidth * 0.18);
  const maxDistance = bodyFitContext.height * 0.12;
  const maxSquared = maxDistance * maxDistance;
  const grid = new Map();
  const bucketAt = (cx, cy, cz) => grid.get(cx)?.get(cy)?.get(cz);
  let bodyVertices = 0;
  for (const object of bodyFitContext.bodyObjects) {
    const base = object.userData.bodyFitBasePositions;
    const mask = object.userData.bodyFitMask;
    if (!base || !mask) continue;
    for (let index = 0; index < mask.length; index += 1) {
      if (!mask[index]) continue;
      const offset = index * 3;
      const sample = { object, index, x: base[offset], y: base[offset + 1], z: base[offset + 2] };
      const cx = Math.floor(sample.x / cellSize), cy = Math.floor(sample.y / cellSize), cz = Math.floor(sample.z / cellSize);
      let plane = grid.get(cx);
      if (!plane) grid.set(cx, plane = new Map());
      let row = plane.get(cy);
      if (!row) plane.set(cy, row = new Map());
      const bucket = row.get(cz);
      if (bucket) bucket.push(sample); else row.set(cz, [sample]);
      bodyVertices += 1;
    }
  }
  let jerseyVertices = 0;
  let boundJerseyVertices = 0;
  const candidates = [];
  const nearest = [], nearestDistance = [];
  for (const object of garmentObjects) {
    const counted = object.userData.bodyFitRole === 'jersey';
    const base = object.userData.bodyFitBasePositions;
    const mask = object.userData.bodyFitMask;
    if (!base || !mask) continue;
    const bindings = new Array(mask.length).fill(null);
    for (let index = 0; index < mask.length; index += 1) {
      if (!mask[index]) continue;
      if (counted) jerseyVertices += 1;
      const offset = index * 3;
      const x = base[offset], y = base[offset + 1], z = base[offset + 2];
      const cx = Math.floor(x / cellSize), cy = Math.floor(y / cellSize), cz = Math.floor(z / cellSize);
      candidates.length = 0;
      for (let radius = 0; radius <= 5 && candidates.length < 8; radius += 1) {
        for (let dz = -radius; dz <= radius; dz += 1) {
          for (let dy = -radius; dy <= radius; dy += 1) {
            for (let dx = -radius; dx <= radius; dx += 1) {
              if (radius && Math.max(Math.abs(dx), Math.abs(dy), Math.abs(dz)) !== radius) continue;
              const bucket = bucketAt(cx + dx, cy + dy, cz + dz);
              if (bucket) for (const sample of bucket) candidates.push(sample);
            }
          }
        }
      }
      // The 4 nearest within maxDistance, ordered by distance then vertex index;
      // equal entries keep candidate order, matching a stable sort and slice(0, 4).
      // Ranked by squared distance (Math.hypot dominated this loop); the kept
      // samples get their exact distances below.
      nearest.length = 0; nearestDistance.length = 0;
      for (const sample of candidates) {
        const dx = x - sample.x, dy = y - sample.y, dz = z - sample.z;
        const squared = dx * dx + dy * dy + dz * dz;
        if (squared > maxSquared) continue;
        let at = nearest.length;
        while (at > 0 && (nearestDistance[at - 1] > squared
          || nearestDistance[at - 1] === squared && nearest[at - 1].index > sample.index)) at -= 1;
        if (at >= 4) continue;
        nearest.splice(at, 0, sample);
        nearestDistance.splice(at, 0, squared);
        if (nearest.length > 4) { nearest.pop(); nearestDistance.pop(); }
      }
      if (!nearest.length) continue;
      for (let slot = 0; slot < nearest.length; slot += 1) {
        nearestDistance[slot] = Math.hypot(x - nearest[slot].x, y - nearest[slot].y, z - nearest[slot].z);
      }
      const exact = nearest.filter((_, slot) => nearestDistance[slot] <= 0.000001);
      const selected = exact.length ? exact : nearest;
      const rawWeights = selected.map((_, slot) => exact.length ? 1 : 1 / Math.max(nearestDistance[slot] * nearestDistance[slot], 0.000001));
      const total = rawWeights.reduce((sum, value) => sum + value, 0);
      bindings[index] = selected.map((sample, slot) => ({
        object: sample.object,
        index: sample.index,
        weight: rawWeights[slot] / total,
      }));
      if (counted) boundJerseyVertices += 1;
    }
    object.userData.bodyFitBindings = bindings;
  }
  return {
    method: 'body-surface-follow',
    bodyVertices,
    jerseyVertices,
    boundJerseyVertices,
    coverage: jerseyVertices ? boundJerseyVertices / jerseyVertices : 0,
  };
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
    footwearObjects: bodyFitObjects('footwear'),
    centerX,
    centerY,
    minZ,
    maxZ,
    height,
    torsoHalfWidth,
    legCenter: Math.max(torsoHalfWidth * 0.38, percentile(legWidths, 0.52)),
    dimensionBaseline: { ...baseValues },
  };
  jerseyFitDiagnostics = prepareJerseySurfaceBindings();
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

function applyParametricBodyFit(object, signals, dimensionDeltas, dimensionChanges, response = 1) {
    const base = object.userData.bodyFitBasePositions;
    const mask = object.userData.bodyFitMask;
    const attribute = object.geometry?.getAttribute('position');
    if (!base || !mask || !attribute) return;
    const output = attribute.array;
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

function applyJerseyFitClearance(object, signals) {
  const base = object.userData.bodyFitBasePositions;
  const mask = object.userData.bodyFitMask;
  const attribute = object.geometry?.getAttribute('position');
  if (!base || !mask || !attribute) return;
  const output = attribute.array;
  for (let index = 0; index < mask.length; index += 1) {
    if (!mask[index]) continue;
    const offset = index * 3;
    const x = base[offset];
    const y = base[offset + 1];
    const z = base[offset + 2];
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
      const delta = signal * definition.amplitude * weight;
      if (definition.kind === 'arm') {
        armDelta += delta;
        const regionCenterZ = bodyFitContext.minZ
          + bodyFitContext.height * ((definition.low + definition.high) / 2);
        armZDelta += (z - regionCenterZ) * delta;
      } else if (definition.kind === 'leg') legDelta += delta;
      else coreDelta += delta;
    }
    coreDelta = THREE.MathUtils.clamp(coreDelta, -0.24, 0.24);
    armDelta = THREE.MathUtils.clamp(armDelta, -0.2, 0.2);
    legDelta = THREE.MathUtils.clamp(legDelta, -0.2, 0.2);
    output[offset] += (output[offset] - bodyFitContext.centerX) * coreDelta;
    output[offset + 1] += (output[offset + 1] - bodyFitContext.centerY)
      * (coreDelta + armDelta + legDelta);
    output[offset + 2] += armZDelta;
    if (legDelta) {
      const side = output[offset] < bodyFitContext.centerX ? -1 : 1;
      const legCenterX = bodyFitContext.centerX + side * bodyFitContext.legCenter;
      output[offset] += (output[offset] - legCenterX) * legDelta;
    }
  }
  attribute.needsUpdate = true;
  object.geometry.computeBoundingBox();
  object.geometry.computeBoundingSphere();
}

function applyNativeJerseyMorphs(object, weights) {
  const names = object.userData.bodyFitMorphNames;
  const deltas = object.userData.bodyFitMorphDeltas;
  const base = object.userData.bodyFitBasePositions;
  const attribute = object.geometry?.getAttribute('position');
  if (!names?.length || !base || !attribute || deltas?.length !== names.length * base.length) return null;
  const output = attribute.array;
  let applied = 0;
  for (let target = 0; target < names.length; target += 1) {
    const weight = weights.get(String(names[target]).toLowerCase()) || 0;
    if (!Number.isFinite(weight) || Math.abs(weight) <= 0.000001) continue;
    const start = target * base.length;
    for (let component = 0; component < base.length; component += 1) {
      output[component] += deltas[start + component] * weight;
    }
    applied += 1;
  }
  attribute.needsUpdate = true;
  object.geometry.computeBoundingBox();
  object.geometry.computeBoundingSphere();
  return { targets: names.length, applied };
}

function applyBoundJerseyFit(object) {
  const base = object.userData.bodyFitBasePositions;
  const mask = object.userData.bodyFitMask;
  const bindings = object.userData.bodyFitBindings;
  const attribute = object.geometry?.getAttribute('position');
  if (!base || !mask || !bindings || !attribute) return false;
  const output = attribute.array;
  for (let index = 0; index < mask.length; index += 1) {
    const offset = index * 3;
    output[offset] = base[offset];
    output[offset + 1] = base[offset + 1];
    output[offset + 2] = base[offset + 2];
    if (!mask[index] || !bindings[index]) continue;
    for (const binding of bindings[index]) {
      const bodyBase = binding.object.userData.bodyFitBasePositions;
      const bodyPosition = binding.object.geometry?.getAttribute('position')?.array;
      if (!bodyBase || !bodyPosition) continue;
      const bodyOffset = binding.index * 3;
      output[offset] += (bodyPosition[bodyOffset] - bodyBase[bodyOffset]) * binding.weight;
      output[offset + 1] += (bodyPosition[bodyOffset + 1] - bodyBase[bodyOffset + 1]) * binding.weight;
      output[offset + 2] += (bodyPosition[bodyOffset + 2] - bodyBase[bodyOffset + 2]) * binding.weight;
    }
  }
  attribute.needsUpdate = true;
  object.geometry.computeBoundingBox();
  object.geometry.computeBoundingSphere();
  return true;
}

function applyBodyFit(values = {}, drawAfter = true) {
  if (!bodyFitContext) return;
  currentBodyFitValues = { ...values };
  const signals = bodyFitSignals(values);
  const morphWeights = new Map(Object.entries(values).map(([key, value]) => [String(key).toLowerCase(), Number(value)]));
  const dimensionDeltas = bodyDimensionDeltas(values);
  const dimensionChanges = bodyDimensionChangeGroups(values);
  for (const object of bodyFitContext.bodyObjects) {
    // THIN/THICK are clothing-fit weights. Body dimensions come from the
    // skeleton translation values, while the regional fit changes clearance.
    applyParametricBodyFit(object, new Map(), dimensionDeltas, dimensionChanges, 1);
  }
  let nativeMorphMeshes = 0;
  let nativeMorphTargets = 0;
  let appliedMorphTargets = 0;
  let fallbackMeshes = 0;
  for (const object of bodyFitContext.footwearObjects) {
    if (!applyBoundJerseyFit(object)) applyParametricBodyFit(object, new Map(), dimensionDeltas, dimensionChanges, 1);
  }
  for (const object of bodyFitContext.jerseyObjects) {
    const bound = applyBoundJerseyFit(object);
    if (!bound) applyParametricBodyFit(object, new Map(), dimensionDeltas, dimensionChanges, 0.45);
    const native = applyNativeJerseyMorphs(object, morphWeights);
    if (native) {
      nativeMorphMeshes += 1;
      nativeMorphTargets += native.targets;
      appliedMorphTargets += native.applied;
    } else {
      fallbackMeshes += 1;
      if (!bound) applyParametricBodyFit(object, signals, dimensionDeltas, dimensionChanges, 0.45);
      else applyJerseyFitClearance(object, signals);
    }
  }
  if (jerseyFitDiagnostics) {
    Object.assign(jerseyFitDiagnostics, {
      method: nativeMorphMeshes
        ? (fallbackMeshes ? 'native-jersey-morphs-with-fallback' : 'native-jersey-morphs')
        : 'body-surface-follow-fallback',
      nativeMorphMeshes,
      nativeMorphTargets,
      appliedMorphTargets,
      fallbackMeshes,
    });
  }
  const chestCorrection = chestCorrectionEnabled ? applyChestCorrectionGeometry() : null;
  if (drawAfter) renderer.render(scene, camera);
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

function initialize(payload) {
  bodyFitContext = null; jerseyFitDiagnostics = null; chestCorrectionEnabled = false; lastChestCorrection = null;
  if (String(payload.player?.mode || '').startsWith('hair-catalog-')) return;
  for (let itemIndex = 0; itemIndex < (payload.meshes || []).length; itemIndex++) {
    const item = payload.meshes[itemIndex];
    const object = modelRoot.children[itemIndex];
    if (!object?.geometry) continue;
    const positions = object.geometry.getAttribute('position').array;
    const indices = object.geometry.index.array;
    const mask = new Uint8Array(positions.length / 3);
    const chest = new Uint8Array(mask.length), torso = new Uint8Array(mask.length);
    // Shoes and socks follow the body like the jersey but stay out of the jersey's
    // fit diagnostics, suggestions and chest correction.
    const role = previewGarmentKind(item) || 'body';
    if (role !== 'body') mask.fill(1);
    else for (const group of item.groups || []) {
      const target = materialTextureTarget(group.material);
      if (!['face', 'chest', 'torso', 'legs'].includes(target)) continue;
      for (let cursor = group.start; cursor < group.start + group.count; cursor++) {
        mask[indices[cursor]] = 1;
        if (target === 'chest') chest[indices[cursor]] = 1;
        if (target === 'torso') torso[indices[cursor]] = 1;
      }
    }
    if (!mask.some(Boolean)) continue;
    Object.assign(object.userData, {
      bodyFitRole: role, bodyFitMask: mask, bodyChestMask: chest, bodyTorsoMask: torso,
      bodyFitBasePositions: Float32Array.from(positions), bodyFitBones: item.bones || [],
      bodyFitSkinOffsets: typed(item.skinOffsets, Uint32Array), bodyFitSkinBones: typed(item.skinBones, Uint16Array),
      bodyFitSkinWeights: typed(item.skinWeights, Float32Array),
      bodyFitMorphNames: Array.isArray(item.fitMorphNames) ? item.fitMorphNames.map(String) : [],
      bodyFitMorphDeltas: typed(item.fitMorphDeltas, Float32Array),
    });
  }
  prepareBodyFitContext(payload.player?.bodyFit || {});
  // No draw here: this ran the load's first render, compiling every shader
  // synchronously before the preview could precompile them off the main thread.
  applyBodyFit(payload.player?.bodyFit || {}, false);
}
function jerseyBindingMetrics() {
  if (!bodyFitContext?.jerseyObjects.length) return null;
  let measured = 0;
  let totalClearance = 0;
  let minimumClearance = Infinity;
  let maximumClearance = 0;
  let maximumClearanceDrift = 0;
  for (const object of bodyFitContext.jerseyObjects) {
    const base = object.userData.bodyFitBasePositions;
    const current = object.geometry?.getAttribute('position')?.array;
    const bindings = object.userData.bodyFitBindings;
    if (!base || !current || !bindings) continue;
    for (let index = 0; index < bindings.length; index += 1) {
      if (!bindings[index]) continue;
      const offset = index * 3;
      const anchorBase = [0, 0, 0], anchorCurrent = [0, 0, 0];
      for (const binding of bindings[index]) {
        const bodyBase = binding.object.userData.bodyFitBasePositions;
        const bodyCurrent = binding.object.geometry?.getAttribute('position')?.array;
        if (!bodyBase || !bodyCurrent) continue;
        const bodyOffset = binding.index * 3;
        for (let axis = 0; axis < 3; axis += 1) {
          anchorBase[axis] += bodyBase[bodyOffset + axis] * binding.weight;
          anchorCurrent[axis] += bodyCurrent[bodyOffset + axis] * binding.weight;
        }
      }
      const baselineOffset = [0, 1, 2].map(axis => base[offset + axis] - anchorBase[axis]);
      const currentOffset = [0, 1, 2].map(axis => current[offset + axis] - anchorCurrent[axis]);
      const clearance = Math.hypot(...currentOffset);
      const drift = Math.hypot(...currentOffset.map((value, axis) => value - baselineOffset[axis]));
      measured += 1;
      totalClearance += clearance;
      minimumClearance = Math.min(minimumClearance, clearance);
      maximumClearance = Math.max(maximumClearance, clearance);
      maximumClearanceDrift = Math.max(maximumClearanceDrift, drift);
    }
  }
  return measured ? {
    measured,
    averageClearance: totalClearance / measured,
    minimumClearance,
    maximumClearance,
    maximumClearanceDrift,
  } : null;
}
function diagnostics() {
  return jerseyFitDiagnostics ? { ...jerseyFitDiagnostics, clearance: jerseyBindingMetrics() } : null;
}
return { initialize, setBodyFit: applyBodyFit, setChestCorrection, getChestCorrectionPatch, suggestBodyFit, diagnostics };
}
