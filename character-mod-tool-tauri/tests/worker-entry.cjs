const assert = require('node:assert/strict');
const path = require('node:path');
const vm = require('node:vm');
const esbuild = require('../build-tools/node_modules/esbuild');
const options = {
  entryPoints: [path.join(__dirname, '../worker/index.js')], bundle: true, write: false,
  format: 'iife', platform: 'browser', target: 'es2022',
  plugins: [{ name: 'fixtures', setup(build) {
    build.onResolve({ filter: /^(cmt-preview|\.\/catalog\.js)$/ }, args => ({ path: args.path, namespace: 'fixtures' }));
    build.onLoad({ filter: /.*/, namespace: 'fixtures' }, args => ({ contents: args.path === 'cmt-preview'
      ? 'export const createIffPreview = globalThis.decodePreview; export const projectTangentSpaceMeshes = globalThis.project;'
      : 'export const catalog = globalThis.decodeCatalog;' }));
  } }],
};
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
(async () => {
  const bundle = (await esbuild.build(options)).outputFiles[0].text;
  let callback, active = 0, maxActive = 0, projected = 0;
  const replies = [];
  const context = vm.createContext({ ArrayBuffer, Object, Set, Float32Array, Promise, Error,
    addEventListener(_name, handler) { callback = handler; },
    postMessage(value, buffers) { replies.push(structuredClone(value, { transfer: Array.from(buffers) })); },
    async decodePreview(file) {
      active++; maxActive = Math.max(maxActive, active);
      try {
        await delay(10);
        if (file === 'missing') throw new Error('Missing fixture');
        return { meshes: [
          { sourceKind: 'main', geometryKey: 'head-v1', positions: new Float32Array([0, 0, 0]) },
          { sourceName: 'png1_geo_facialhair_beard_tangentspace.iff', geometryKey: 'beard-v1',
            tangentOffsets: new Float32Array([1, 2, 3]), tangentSpaceAttachment: true },
        ], stats: {}, skipped: [] };
      } finally { active--; }
    },
    project({ meshes }) { projected += meshes.length; return meshes.length; },
    async decodeCatalog() { assert.equal(active, 0); return { meshes: [], stats: { catalog: true } }; },
  });
  vm.runInContext(bundle, context);
  for (const [id, type, payload] of [
    [1, 'preview', { filePath: 'valid', options: { projectTangent: 'staged' } }],
    [2, 'preview', { filePath: 'missing', options: {} }],
    [3, 'catalog', {}],
    [4, 'unknown', {}],
    [5, 'preview', { filePath: 'valid', options: {} }],
  ]) callback({ data: { id, type, payload } });
  for (let i = 0; i < 100 && replies.length < 5; i++) await delay(10);
  assert.equal(replies.length, 5);
  assert.equal(maxActive, 1, 'Worker operations must remain serial');
  assert.equal(projected, 1);
  assert.equal(replies[0].result.meshes[1].geometryKey, 'beard-v1:head:head-v1');
  assert.equal(replies[0].result.stats.facialHairProjectedVertices, 1);
  assert.equal(replies[1].error.message, 'Missing fixture');
  assert.equal(replies[2].result.stats.catalog, true);
  assert.equal(replies[3].error.message, 'Unknown preview operation.');
  assert.equal(replies[4].result.meshes[1].geometryKey, 'beard-v1');
  assert.equal(replies[4].result.stats.workerTransport.mode, 'transfer');
  console.log('Owned worker entry: serial decode, failure recovery, staged geometry identity and operation guards passed.');
})().catch(error => { console.error(error); process.exitCode = 1; });
