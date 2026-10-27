import assert from 'node:assert/strict';
import { skinBoundaryResponse, skinLuminanceResponse } from '../web/renderer/skin-response.mjs';
assert.deepEqual(skinBoundaryResponse([0.3, 0.25, 0.45], [0.2, 0.125, 0.1]), [1.4999999999999998, 2, 4.5]);
const reference = [0.3, 0.25, 0.45], source = [0.2, 0.125, 0.1];
const gain = skinBoundaryResponse(reference, source);
for (let channel = 0; channel < 3; channel++) assert(Math.abs(source[channel] * gain[channel] - reference[channel]) < 1e-10);
assert.deepEqual(skinBoundaryResponse([0.25, 0.25, 0.25], [0.25, 0.25, 0.25]), [1, 1, 1]);
assert.deepEqual(skinBoundaryResponse([0.2, 0.3, 0.4], [0.2, 0.3, 0.4], [2, 3, 4]), [2, 3, 4]);
assert.equal(skinBoundaryResponse([0.2, 0.2, 1], [0.2, 0.2, 0.01]), null, 'Out-of-bounds channel rejects the whole vector');
assert.equal(skinBoundaryResponse([0.01, 0.3, 0.4], [1, 0.3, 0.4]), null);
assert.deepEqual(skinBoundaryResponse([0.005, 0.005, 0.005], [0, 0, 0]), [1, 1, 1]);
for (const invalid of [null, [], [1, 2], [1, 2, 3, 4], [NaN, 1, 1], [Infinity, 1, 1], [-1, 1, 1], ['1', 1, 1]]) {
  assert.equal(skinBoundaryResponse(invalid, source), null);
  assert.equal(skinBoundaryResponse(reference, invalid), null);
  assert.equal(skinBoundaryResponse(reference, source, invalid), null);
}
assert.deepEqual(reference, [0.3, 0.25, 0.45]); assert.deepEqual(source, [0.2, 0.125, 0.1]);
// A rejected per-channel response still gets a hue-neutral luminance gain.
const rejected = skinLuminanceResponse([0.2, 0.2, 1], [0.2, 0.2, 0.01]);
assert.ok(rejected && rejected.every(value => value === rejected[0]), 'Luminance fallback is one gain for every channel');
assert.ok(Math.abs(rejected[0] - (0.2 * 0.9278 + 0.0722) / (0.2 * 0.9278 + 0.01 * 0.0722)) < 1e-9);
assert.deepEqual(skinLuminanceResponse([0.25, 0.25, 0.25], [0.25, 0.25, 0.25]), [1, 1, 1]);
assert.deepEqual(skinLuminanceResponse([0.2, 0.2, 0.2], [0.2, 0.2, 0.2], [2, 2, 2]), [2, 2, 2]);
assert.equal(skinLuminanceResponse([0.9, 0.9, 0.9], [0.01, 0.01, 0.01]), null, 'Out-of-bounds luminance gain is rejected');
for (const invalid of [null, [1, 2], [NaN, 1, 1], [-1, 1, 1]]) assert.equal(skinLuminanceResponse(invalid, source), null);
console.log('Skin boundary response: hue preservation, unclipped blue gain, inherited response, whole-vector bounds, finite validation and source immutability passed.');
