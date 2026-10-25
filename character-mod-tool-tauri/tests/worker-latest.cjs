const assert = require('node:assert/strict');
const path = require('node:path');
const vm = require('node:vm');
const esbuild = require('../build-tools/node_modules/esbuild');
const baseline = process.argv.includes('--baseline');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const options = {
  entryPoints: [path.join(__dirname, '../worker/index.js')], bundle: true, write: false,
  format: 'iife', platform: 'browser', target: 'es2022',
  plugins: [{ name: 'fixtures', setup(build) {
    build.onResolve({ filter: /^(cmt-preview|\.\/catalog\.js)$/ }, args => ({ path: args.path, namespace: 'fixtures' }));
    build.onLoad({ filter: /.*/, namespace: 'fixtures' }, args => ({ contents: args.path === 'cmt-preview'
      ? 'export const createIffPreview = globalThis.decodePreview; export const projectTangentSpaceMeshes = () => 0;'
      : 'export const catalog = globalThis.decodeCatalog;' }));
  } }],
};
function setup(bundle) {
  let callback, release, active = 0, maxActive = 0;
  const gate = new Promise(resolve => { release = resolve; });
  const decoded = [], replies = [];
  const decode = async key => {
    active++; maxActive = Math.max(maxActive, active); decoded.push(key);
    try {
      if (key === 'active' || key === 'active-failure') await gate;
      if (key === 'active-failure') throw new Error('Expected active failure');
      return { key, meshes: [{ positions: new Float32Array([1, 2, 3]) }], stats: {}, skipped: [] };
    } finally { active--; }
  };
  const context = vm.createContext({
    ArrayBuffer, Object, Set, Float32Array, Promise, Error,
    addEventListener(_name, handler) { callback = handler; },
    postMessage(value, buffers) { replies.push(structuredClone(value, { transfer: Array.from(buffers) })); },
    decodePreview: decode, decodeCatalog: payload => decode(payload.key),
  });
  vm.runInContext(bundle, context);
  const send = (id, type, key, latestOnly = false) =>
    callback({ data: { id, type, latestOnly, payload: type === 'preview' ? { filePath: key, options: {} } : { key } } });
  return { send, release, decoded, replies, maxActive: () => maxActive,
    async wait(count) {
      for (let i = 0; i < 100 && replies.length < count; i++) await delay(5);
      assert.equal(replies.length, count, 'Every accepted/replaced request must settle');
    } };
}
(async () => {
  const bundle = (await esbuild.build(options)).outputFiles[0].text;
  const burst = setup(bundle);
  burst.send(1, 'catalog', 'active', true);
  await delay(0);
  assert.deepEqual(burst.decoded, ['active']);
  for (let id = 2; id <= 12; id++) burst.send(id, 'catalog', 'selection-' + id, true);
  burst.release();
  await burst.wait(12);
  if (baseline) {
    assert.equal(burst.decoded.length, 12);
    console.log(JSON.stringify({ baseline: true, requests: 12, decoded: burst.decoded.length, discardedOnlyAfterDecode: true }));
    return;
  }
  assert.deepEqual(burst.decoded, ['active', 'selection-12']);
  assert.equal(burst.maxActive(), 1);
  assert.equal(burst.replies.filter(row => row.error?.name === 'AbortError').length, 10);
  assert.equal(burst.replies.find(row => row.id === 12).result.key, 'selection-12');
  burst.send(13, 'catalog', 'next-selection', true);
  await burst.wait(13);
  assert.equal(burst.replies.find(row => row.id === 13).result.key, 'next-selection');

  const mixed = setup(bundle);
  mixed.send(1, 'catalog', 'active', true);
  await delay(0);
  mixed.send(2, 'preview', 'independent-preview', true);
  mixed.send(3, 'catalog', 'obsolete-selection', true);
  mixed.send(4, 'preview', 'second-preview');
  mixed.send(5, 'catalog', 'ordinary-catalog');
  mixed.send(6, 'catalog', 'current-selection', true);
  mixed.send(7, 'unknown', 'invalid', true);
  mixed.release();
  await mixed.wait(7);
  assert.deepEqual(mixed.decoded, ['active', 'independent-preview', 'second-preview', 'ordinary-catalog', 'current-selection']);
  assert.equal(mixed.maxActive(), 1);
  assert.equal(mixed.replies.find(row => row.id === 3).error.name, 'AbortError');
  assert.equal(mixed.replies.find(row => row.id === 7).error.message, 'Unknown preview operation.');
  assert.equal(mixed.replies.find(row => row.id === 6).result.stats.workerTransport.mode, 'transfer');

  const recovery = setup(bundle);
  recovery.send(1, 'catalog', 'active-failure', true);
  await delay(0);
  recovery.send(2, 'catalog', 'superseded', true);
  recovery.send(3, 'catalog', 'current', true);
  recovery.release();
  await recovery.wait(3);
  assert.equal(recovery.replies.find(row => row.id === 1).error.message, 'Expected active failure');
  assert.equal(recovery.replies.find(row => row.id === 3).result.key, 'current');
  assert.equal(recovery.maxActive(), 1);
  console.log('Latest worker selections: active work retained, obsolete queued catalogs settled/skipped, independent FIFO, transfer and error recovery passed.');
})().catch(error => { console.error(error); process.exitCode = 1; });
