const assert = require('assert/strict');
const fs = require('fs');
const path = require('path');
const vm = require('vm');
const root = path.resolve(__dirname, '..');
function load(name, globals = {}) {
  const source = fs.readFileSync(path.join(root, 'worker', name), 'utf8')
    .replace(/^import .*;\r?\n/gm, '').replace(/export /g, '');
  const context = vm.createContext({ Float32Array, Uint32Array, ArrayBuffer, structuredClone,
    crypto: require('crypto').webcrypto, ...globals });
  vm.runInContext(source, context);
  return context;
}
(async () => {
  let reads = 0, decodes = 0, fileBytes = Buffer.from('one');
  const cacheContext = load('head-cache.js', {
    readFile: async () => { reads++; return fileBytes; },
    stat: async () => ({ size: 3 }), realpath: async p => p,
  });
  const cache = vm.runInContext('new HeadPreviewCache()', cacheContext);
  const decode = async () => { decodes++; return { meshes: [{ name: 'face', positions: new Float32Array([0, 1, 2]) }] }; };
  for (let i = 0; i < 8; i++) await cache.get('cmt-preview://donor-' + i, decode);
  const first = await cache.get('cmt-preview://donor-0', decode);
  first.value.meshes[0].positions[0] = 99;
  assert.equal((await cache.get('cmt-preview://donor-0', decode)).value.meshes[0].positions[0], 0);
  await cache.get('cmt-preview://donor-8', decode);
  assert.equal(cache.entries.size, 8);
  assert.equal(reads, 0, 'Virtual donor fingerprints must not read a complete IFF through the texture route');
  assert.equal(decodes, 9);
  assert.equal((await cache.get('cmt-preview://donor-1', decode)).cached, false);
  await cache.get('local.iff', decode);
  assert.equal((await cache.get('local.iff', decode)).cached, true);
  fileBytes = Buffer.from('two');
  assert.equal((await cache.get('local.iff', decode)).cached, false, 'Same-size edits must invalidate the head');
  assert(cache.bytes <= 24 * 1024 * 1024);
  const huge = () => ({ meshes: [{ positions: new Float32Array(7 * 1024 * 1024) }] });
  await cache.get('cmt-preview://oversize', huge);
  assert(!cache.entries.has('cmt-preview://oversize'));

  const frame = load('tangent-frame.js');
  const source = fs.readFileSync(path.resolve(root, '../character-mod-tool-webview2/Reference/src/core/preview.js'), 'utf8');
  const begin = source.indexOf('function projectTangentSpaceMeshes(preview, basePreview)');
  const end = source.indexOf('function projectTangentSpaceMeshesRigidReference', begin);
  const original = vm.createContext({ Float32Array });
  vm.runInContext(source.slice(begin, end), original);
  const head = key => ({ meshes: [{ geometryKey: key, positions: new Float32Array([0,0,0,1,0,0.2,0,1,0.3]),
    uvs: new Float32Array([0,0,1,0,0,1]), indices: new Uint32Array([0,1,2]),
    sourceVertexIds: new Uint32Array([7,8,9]), groups: [{ material:'face_shader',start:0,count:3 }] }] });
  const attachment = () => ({ meshes: [{ tangentAnchorIds: new Uint32Array([7,9,8]),
    tangentOffsets: new Float32Array([0.01,0.02,0.03,-0.05,0.04,0.03,0.1,-0.03,0.02]) }] });
  const expected = attachment(), actual = attachment();
  assert.equal(original.projectTangentSpaceMeshes(expected, head('a')), 3);
  assert.equal(frame.projectTangentSpaceMeshes(actual, head('a')), 3);
  assert.deepEqual(actual.meshes[0].positions, expected.meshes[0].positions);
  frame.projectTangentSpaceMeshes(attachment(), head('a'));
  assert.equal(frame.frameCacheStats().builds, 1);
  assert.equal(frame.frameCacheStats().hits, 1);
  for (let i = 0; i < 9; i++) frame.projectTangentSpaceMeshes(attachment(), head('donor-' + i));
  const stats = frame.frameCacheStats();
  assert.equal(stats.entries, 8); assert(stats.bytes <= stats.budget);
  assert.equal(frame.projectTangentSpaceMeshes(attachment(), { meshes:[] }), 0);
  console.log('Catalog caches passed: native fingerprints, LRU, clone isolation, edit invalidation, oversize bypass, exact tangent math, reusable frames.');
})().catch(error => { console.error(error); process.exitCode = 1; });
