const assert = require('node:assert/strict');
const path = require('node:path');

const root = path.resolve(__dirname, '..');
const preview = require(path.resolve(root, '../character-mod-tool-webview2/Reference/src/core/preview.js'));

function packedRun(skip, rows, bits) {
  const header = Buffer.alloc(4);
  header.writeUInt16LE(skip, 0);
  header.writeUInt16LE(rows.length, 2);
  const width = bits.reduce((sum, value) => sum + value, 0);
  const payload = Buffer.alloc(Math.ceil(rows.length * width / 8));
  let bitOffset = 0;
  for (const row of rows) for (let axis = 0; axis < 3; axis += 1) {
    const count = bits[axis];
    if (!count) continue;
    const encoded = row[axis] < 0 ? (2 ** count) + row[axis] : row[axis];
    for (let bit = 0; bit < count; bit += 1) {
      payload[bitOffset >>> 3] |= ((encoded >>> bit) & 1) << (bitOffset & 7);
      bitOffset += 1;
    }
  }
  return Buffer.concat([header, payload]);
}

(async () => {
  const descriptor = { Bits: [4, 3, 0], Scale: 10 };
  const bytes = Buffer.concat([
    packedRun(1, [[3, -2, 0], [-4, 1, 0]], descriptor.Bits),
    packedRun(1, [[7, -3, 0]], descriptor.Bits),
  ]);
  const decoded = preview.decodeSparseMorph(bytes, descriptor, 5, new Uint32Array([1, 3, 4]));
  const expected = [0.3, 0, -0.2, 0, 0, 0, 0.7, 0, -0.3];
  expected.forEach((value, index) => assert(Math.abs(decoded[index] - value) < 0.000001,
    `Sparse morph component ${index} differs: ${decoded[index]} versus ${value}`));
  assert.throws(() => preview.decodeSparseMorph(bytes.subarray(0, -1), descriptor, 5, [1]));
  assert.throws(() => preview.decodeSparseMorph(bytes, descriptor, 4, [1]));
  assert.throws(() => preview.decodeSparseMorph(bytes, { ...descriptor, Bits: [32, 3, 0] }, 5, [1]));
  assert.throws(() => preview.decodeSparseMorph(bytes, { ...descriptor, Scale: 0 }, 5, [1]));
  assert.throws(() => preview.decodeSparseMorph(bytes, descriptor, 5, [5]));

  const jersey = path.join(root, 'artifacts/blender-test-jersey.iff');
  const real = await preview.createIffPreview(jersey, { skipTextures: true, preferHighestDetail: true });
  const native = real.meshes.find(mesh => mesh.fitMorphNames?.includes('chestTHICK'));
  assert(native, 'The real 2K27 jersey did not expose chestTHICK');
  assert(native.fitMorphNames.includes('chestTHIN'));
  assert.equal(native.fitMorphDeltas.length, native.fitMorphNames.length * native.positions.length);
  assert(native.fitMorphDeltas.some(value => Math.abs(value) > 0.000001), 'Real jersey morphs decoded to zero');
  const staged = await preview.createIffPreview(jersey, { skipTextures: true, preferHighestDetail: true,
    companionPaths: [{ path: jersey, name: path.basename(jersey), kind: 'preview-jersey' }] });
  const stagedJersey = staged.meshes.find(mesh => mesh.sourceKind === 'preview-jersey'
    && mesh.fitMorphNames?.includes('chestTHICK'));
  assert(stagedJersey, 'The staged-player companion path lost native jersey morphs');
  console.log(JSON.stringify({
    checks: ['sparse runs', 'signed little-endian components', 'zero-bit components', 'source remap',
      'axis conversion', 'malformed input rejection', 'real 2K27 static jersey morphs',
      'staged-player companion transport'],
    realTargets: native.fitMorphNames.length,
    vertices: native.positions.length / 3,
  }, null, 2));
})().catch(error => { console.error(error); process.exitCode = 1; });
