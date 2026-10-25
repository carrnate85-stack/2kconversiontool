const assert = require('node:assert/strict');
const finishTest = require('./test-watchdog.cjs')('preview scopes');
const vm = require('node:vm');
const path = require('node:path');
const esbuild = require('../build-tools/node_modules/esbuild');
const source = esbuild.buildSync({ entryPoints: [path.join(__dirname, '../web/tauri-bridge.ts')], bundle: true,
  write: false, platform: 'browser', format: 'iife', target: 'es2022' }).outputFiles[0].text;
const flush = () => new Promise(resolve => setTimeout(resolve, 10));
function setup() {
  const calls = [], workers = [];
  let closed = false;
  const window = { __TAURI__: { event: { listen: async () => {} }, core: {
    invoke: async (_name, { command, payload }) => {
      if (command === 'runtime-info') return { smoke: false };
      if (command === 'cancel-preview' || command === 'blender-snapshot-release') return true;
      if (command === 'reset') { calls.push({ command, payload }); return true; }
      if (closed) throw new Error('Owned test cleanup');
      return new Promise((resolve, reject) => calls.push({ command, payload, resolve, reject }));
    }
  } } };
  class Worker {
    constructor() { workers.push(this); }
    postMessage(message) { this.message = message; }
    terminate() { this.terminated = true; }
  }
  vm.runInNewContext(source, { window, Worker, console, structuredClone, setInterval, clearInterval,
    setTimeout, crypto: { randomUUID: () => 'scope-test-handoff' } });
  return { api: window.characterTool, calls, workers, close: () => { closed = true; } };
}
(async () => {
  const staged = setup(); await flush();
  const loading = staged.api.loadStagedPreview({ target: 'C:/test/player.iff' }).catch(error => String(error));
  await flush();
  assert.equal(staged.calls[0].payload._cmtPreviewKind, 'staged');
  assert.equal(await Promise.race([staged.api.cancelNativeHairPreview(), flush().then(() => 'blocked')]), true,
    'Catalog cancellation must not wait for staged preparation');
  let finished = false;
  const cancellation = staged.api.cancelStagedPreview().then(() => { finished = true; });
  await flush(); assert.equal(finished, false, 'Own preparation must still drain before cancellation finishes');
  staged.calls[0].resolve({ native: true });
  await cancellation; assert.match(await loading, /cancelled/);

  const catalog = setup(); await flush();
  const fitting = catalog.api.loadNativeHairFitBase({ target: 'C:/test/player.iff' }).catch(error => String(error));
  await flush();
  const worker = catalog.workers[0];
  assert.equal(worker.message.latestOnly, false, 'Fit-base reads are not disposable catalog selections');
  worker.onmessage({ data: { kind: 'native', command: 'read-archive', payload: { path: 'cmt-preview://test.iff' }, id: 9 } });
  await flush();
  assert.equal(catalog.calls[0].payload._cmtPreviewKind, 'catalog');
  assert.equal(await Promise.race([catalog.api.cancelStagedPreview(), flush().then(() => 'blocked')]), true,
    'Staged cancellation must not wait for catalog archive reads');
  catalog.calls[0].resolve({});
  await catalog.api.cancelNativeHairPreview();
  assert.match(await fitting, /cancelled/);

  const selections = setup(); await flush();
  const selected = selections.api.previewNativeHair({ assetType: 'hair', assetId: 'fixture' }).catch(error => String(error));
  await flush();
  selections.calls[0].resolve({ native: true });
  await flush();
  assert.equal(selections.workers[0].message.latestOnly, true, 'Only catalog selections opt into the latest-only queue');
  await selections.api.cancelNativeHairPreview();
  assert.match(await selected, /cancelled/);

  const independent = setup(); await flush();
  const handoff = independent.api.openPreviewInBlender({ target: 'C:/test/player.iff', includeTextures: true }).catch(error => String(error));
  await flush();
  assert.equal(independent.calls[0].payload._cmtPreviewKind, 'independent');
  independent.calls[0].resolve({ native: false, snapshotId: 'scope-lease', filePath: 'C:/test/player.iff', options: {} });
  await flush();
  const referenceWorker = independent.workers[0];
  assert.equal(referenceWorker.message.latestOnly, false, 'Blender reference work must not be coalesced');
  await independent.api.cancelStagedPreview(); await independent.api.cancelNativeHairPreview();
  assert(!referenceWorker.terminated, 'Catalog cancellation terminated independent Blender preparation');
  const resetting = independent.api.resetSession();
  await flush();
  assert(!independent.calls.some(call => call.command === 'reset'), 'Reset must wait for private Blender preparation');
  independent.close();
  referenceWorker.onmessage({ data: { id: referenceWorker.message.id, error: 'Owned test cleanup' } });
  assert.match(await handoff, /Owned test cleanup/);
  assert.equal(await resetting, true);
  assert(independent.calls.some(call => call.command === 'reset'));
  console.log('Preview scopes: independent cancellation waits, latest-only opt-in, archive tags, Blender preparation isolation and reset ordering passed.');
})().catch(error => { console.error(error); process.exitCode = 1; }).finally(finishTest);
