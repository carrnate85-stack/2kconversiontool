const assert = require('node:assert/strict');
const vm = require('node:vm');
const path = require('node:path');
const finish = require('./test-watchdog.cjs')('binary geometry bridge');
const esbuild = require('../build-tools/node_modules/esbuild');
const source = esbuild.buildSync({ entryPoints: [path.join(__dirname, '../web/tauri-bridge.ts')],
  bundle: true, write: false, platform: 'browser', format: 'iife', target: 'es2022' }).outputFiles[0].text;
const tick = () => new Promise(resolve => setTimeout(resolve, 10));
function setup(capability) {
  const calls = [], workers = [];
  let sequence = 0;
  const window = { __TAURI__: { event: { listen: async () => {} }, core: {
    invoke(name, { command, payload }) {
      if (command === 'runtime-info') return Promise.resolve({ previewBinary: true, previewGeometryBinary: capability });
      if (command === 'native-preview-hair') return Promise.resolve({ native: true, resourceLease: String(++sequence) });
      if (command === 'cancel-preview' || command === 'release-preview-resources') return Promise.resolve(true);
      return new Promise((resolve, reject) => calls.push({ name, command, payload, resolve, reject }));
    },
  } } };
  class Worker {
    constructor() { workers.push(this); this.messages = []; }
    postMessage(message, transfers) { this.messages.push({ message, transfers }); }
    terminate() { this.terminated = true; }
    request(id, payload = {}) { return this.onmessage({ data: { kind: 'native', command: 'native-catalog-geometry',
      payload: { geometryPath: 'cmt-preview://hair', sourcePlayerPath: 'cmt-preview://head', ...payload }, id } }); }
    complete(result = {}) { this.onmessage({ data: { id: this.messages[0].message.id, result } }); }
  }
  vm.runInNewContext(source, { window, Worker, console, ArrayBuffer, structuredClone,
    setTimeout, setInterval, clearInterval, crypto: { randomUUID: () => 'geometry-test' } });
  return { api: window.characterTool, calls, workers };
}
async function start(env, request = {}) {
  await tick();
  const loading = env.api.previewNativeHair({ assetType: 'hair', assetId: 'fixture', ...request }).catch(String);
  await tick();
  return { loading, worker: env.workers.at(-1) };
}
(async () => {
  for (const capability of [undefined, false, true]) {
    const env = setup(capability), { loading, worker } = await start(env);
    const reply = worker.request(7); await tick();
    const call = env.calls[0];
    assert.equal(call.name, capability === true ? 'preview_binary' : 'engine_call');
    assert.equal(call.command, 'native-catalog-geometry');
    assert.equal(call.payload._cmtPreviewKind, 'catalog');
    const value = capability === true ? new ArrayBuffer(24) : { asset: {}, base: null };
    call.resolve(value); await reply;
    const delivered = worker.messages.at(-1);
    assert.equal(delivered.message.value, value);
    assert.equal(delivered.transfers.length, capability === true ? 1 : 0);
    if (capability === true) assert.equal(delivered.transfers[0], value);
    worker.complete(); await loading;
    await env.api.cancelNativeHairPreview();
  }

  const diagnostic = setup(true), first = await start(diagnostic, { nativeBinaryGeometry: false });
  assert.equal(first.worker.messages[0].message.payload.nativeBinaryGeometry, false);
  const jsonReply = first.worker.request(8, { nativeBinaryGeometry: false }); await tick();
  assert.equal(diagnostic.calls[0].name, 'engine_call');
  diagnostic.calls[0].resolve({ asset: {}, base: null }); await jsonReply;
  first.worker.complete(); await first.loading; await diagnostic.api.cancelNativeHairPreview();

  const scoped = setup(true), old = await start(scoped);
  const inFlight = old.worker.request(9); await tick();
  assert.equal(await Promise.race([scoped.api.cancelStagedPreview(), tick().then(() => 'blocked')]), true);
  assert(!old.worker.terminated, 'Unrelated cancellation terminated the catalog worker');
  let drained = false;
  const stopping = scoped.api.cancelNativeHairPreview().then(() => { drained = true; });
  await tick(); assert(!drained, 'Cancellation did not drain the raw native call');
  assert(old.worker.terminated);
  assert.match(await old.loading, /cancelled/);
  const count = old.worker.messages.length;
  scoped.calls[0].resolve(new ArrayBuffer(20)); await inFlight; await stopping;
  assert.equal(old.worker.messages.length, count, 'A late reply reached a retired worker');
  const next = await start(scoped);
  assert.notEqual(next.worker, old.worker);
  next.worker.complete(); await next.loading;
  await scoped.api.cancelNativeHairPreview();

  const failed = setup(true), last = await start(failed);
  const invalid = last.worker.request(10); await tick();
  failed.calls[0].resolve({ wrong: 'not a raw buffer' }); await invalid;
  assert.match(last.worker.messages.at(-1).message.error, /invalid binary preview response/);
  const rejection = last.worker.request(11); await tick();
  failed.calls[1].reject(Error('Controlled native failure')); await rejection;
  assert.match(last.worker.messages.at(-1).message.error, /Controlled native failure/);
  last.worker.complete(); await last.loading; await failed.api.cancelNativeHairPreview();
  console.log('Geometry bridge: capability routing, JSON override, transferred ownership, scoped cancellation, late replies and errors passed.');
})().catch(error => { console.error(error); process.exitCode = 1; }).finally(finish);
