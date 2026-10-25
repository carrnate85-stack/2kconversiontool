const assert = require('node:assert/strict');
const finishTest = require('./test-watchdog.cjs')('preview barriers');
const path = require('node:path');
const vm = require('node:vm');
const esbuild = require('../build-tools/node_modules/esbuild');
const source = esbuild.buildSync({ entryPoints: [path.join(__dirname, '../web/tauri-bridge.ts')],
  bundle: true, write: false, platform: 'browser', format: 'iife', target: 'es2022' }).outputFiles[0].text;
const flush = () => new Promise(resolve => setTimeout(resolve, 10));
const baseline = process.argv.includes('--baseline');
const failures = [];
const request = { assetType: 'hair', assetId: 'fixture' };
const resources = native => ({ native, fingerprint: 'fixture-revision', filePath: 'C:/test/player.iff',
  options: { companionPaths: [] }, warnings: [], player: {}, name: 'Fixture', activeItems: [] });
const preview = () => ({ meshes: [], textures: [], stats: {}, skipped: [] });
const catalogPreview = () => ({ asset: preview(), base: null, stats: {} });
function setup(options = {}) {
  const calls = [], workers = [], holds = [];
  const heldKinds = new Set();
  let closed = false, holdFallback = false, holdPreparation = false;
  const window = { __TAURI__: { event: { listen: async () => {} }, core: {
    invoke: async (transport, { command, payload }) => {
      if (command === 'runtime-info') return { smoke: false, previewBinary: Boolean(options.binary) };
      calls.push({ transport, command, payload });
      if (closed) throw new Error('Owned cleanup');
      if (command === 'cancel-preview') {
        if (!heldKinds.has(payload.kind)) return true;
        return new Promise(resolve => holds.push({ kind: payload.kind, resolve }));
      }
      if (command === 'reset' || command === 'blender-snapshot-release') return true;
      if (command === 'blender-snapshot-prepare') return { ...resources(Boolean(options.native)), snapshotId: 'barrier-lease' };
      if (command === 'native-preview-hair') {
        if (holdPreparation) return new Promise((resolve, reject) => holds.push({ kind: 'preparation', resolve, reject }));
        return { native: Boolean(options.native) };
      }
      if (command === 'native-preview-resources') return resources(Boolean(options.native));
      if (command === 'preview-resources' || command === 'native-hair-preview') {
        if (holdFallback) return new Promise((resolve, reject) => holds.push({ kind: 'fallback', resolve, reject }));
        return command === 'preview-resources' ? resources(false) : { native: false };
      }
      if (command === 'read-archive' || command === 'read-file') return {};
      throw new Error('Unexpected fixture command: ' + command);
    },
  } } };
  class Worker {
    constructor() { workers.push(this); this.messages = []; }
    postMessage(message) { this.messages.push(message); if (message.type) this.message = message; }
    terminate() { this.terminated = true; }
    finish(result = preview()) { this.onmessage({ data: { id: this.message.id, result } }); }
    fail(message = 'Expected decode failure') { this.onmessage({ data: { id: this.message.id, error: message } }); }
  }
  vm.runInNewContext(source, { window, Worker, console, ArrayBuffer, structuredClone, setInterval, clearInterval,
    setTimeout, crypto: { randomUUID: () => 'barrier-test-handoff' } });
  const release = (kind, value = true) => {
    heldKinds.delete(kind);
    if (kind === 'preparation') holdPreparation = false;
    if (kind === 'fallback') holdFallback = false;
    for (const hold of holds.filter(hold => hold.kind === kind)) hold.resolve(value);
  };
  return { api: window.characterTool, calls, workers, hold: kind => heldKinds.add(kind), release,
    holdFallback() { holdFallback = true; },
    holdPreparation() { holdPreparation = true; },
    async warm() {
      const loading = window.characterTool.previewNativeHair(request);
      await flush(); workers.at(-1).finish(catalogPreview()); await loading;
    },
    async close() {
      closed = true;
      for (const hold of holds) ['fallback', 'preparation'].includes(hold.kind) ? hold.reject(new Error('Owned cleanup')) : hold.resolve(true);
      for (const worker of workers) if (!worker.terminated) worker.onerror({ message: 'Owned cleanup' });
      await flush();
    },
  };
}
async function check(label, run, options) {
  const env = setup(options);
  try { await flush(); await run(env); } catch (error) {
    if (!baseline) throw new Error(label + ': ' + error.message, { cause: error });
    failures.push({ label, error: error.message });
  } finally { await env.close(); }
}
(async () => {
  await check('Cached catalog waits for its own cancellation', async env => {
    await env.warm();
    env.hold('catalog');
    const stopping = env.api.cancelNativeHairPreview();
    await flush();
    const loading = env.api.previewNativeHair(request).catch(String);
    await flush();
    assert.equal(env.workers.length, 1, 'Cached selection restarted before cancellation drained');
    env.release('catalog'); await stopping; await flush();
    env.workers.at(-1).finish(catalogPreview());
    assert.equal(typeof await loading, 'object');
    assert.equal(env.calls.filter(call => call.command === 'native-preview-hair').length, 1, 'Unchanged descriptors should remain reusable');
  });
  await check('Fit-base waits and can be cancelled before starting', async env => {
    env.hold('catalog');
    const stopping = env.api.cancelNativeHairPreview();
    await flush();
    const fitting = env.api.loadNativeHairFitBase({ target: 'C:/test/player.iff' }).catch(String);
    await flush();
    assert.equal(env.workers.length, 0);
    const again = env.api.cancelNativeHairPreview();
    env.release('catalog'); await Promise.all([stopping, again]);
    assert.match(await fitting, /cancelled/);
    assert.equal(env.workers.length, 0, 'A cancelled waiting decode was restarted');
  });
  await check('Reset requests both stops before waiting and cleans only after both drain', async env => {
    env.hold('staged'); env.hold('catalog');
    const resetting = env.api.resetSession();
    resetting.catch(() => {});
    await flush();
    const stops = env.calls.filter(call => call.command === 'cancel-preview').map(call => call.payload.kind).sort();
    assert.deepEqual(stops, ['catalog', 'staged']);
    assert(!env.calls.some(call => call.command === 'reset'));
    env.release('staged'); await flush();
    assert(!env.calls.some(call => call.command === 'reset'), 'Reset cleaned while catalog cancellation was pending');
    env.release('catalog'); await resetting;
    assert(env.calls.some(call => call.command === 'reset'));
  });
  for (const held of ['catalog', 'staged']) await check('Other cancellation is not queued behind ' + held, async env => {
    env.hold(held);
    const stopping = held === 'catalog' ? env.api.cancelNativeHairPreview() : env.api.cancelStagedPreview();
    await flush();
    const other = held === 'catalog' ? env.api.cancelStagedPreview() : env.api.cancelNativeHairPreview();
    assert.equal(await Promise.race([other, flush().then(() => 'blocked')]), true);
    env.release(held); await stopping;
  });
  for (const [held, kind] of [['catalog', 'staged'], ['catalog', 'independent'], ['staged', 'catalog'], ['staged', 'independent']])
    await check(kind + ' preparation ignores a ' + held + ' stop barrier', async env => {
    env.hold(held);
    const stopping = held === 'catalog' ? env.api.cancelNativeHairPreview() : env.api.cancelStagedPreview();
    await flush();
    const task = kind === 'staged' ? env.api.loadStagedPreview({ target: 'C:/test/player.iff' }).catch(String)
      : kind === 'catalog' ? env.api.previewNativeHair(request).catch(String)
      : env.api.openPreviewInBlender({ target: 'C:/test/player.iff', includeTextures: true }).catch(String);
    await flush();
    assert(env.calls.some(call => kind === 'catalog' ? call.command === 'native-preview-hair'
      : call.command === (kind === 'independent' ? 'blender-snapshot-prepare' : 'native-preview-resources') && call.payload._cmtPreviewKind === kind));
    env.workers.at(-1).fail('Owned decode cleanup'); await task;
    env.release(held); await stopping;
  });
  for (const binary of [false, true]) await check('Terminated worker cannot issue late ' + (binary ? 'binary' : 'JSON') + ' reads', async env => {
    const fitting = env.api.loadNativeHairFitBase({ target: 'C:/test/player.iff' }).catch(String);
    await flush();
    const old = env.workers[0];
    await env.api.cancelNativeHairPreview(); await fitting;
    const before = env.calls.length;
    await old.onmessage({ data: { kind: 'native', command: 'read-archive', payload: { path: 'cmt-preview://fixture.iff' }, id: 99 } });
    assert.equal(env.calls.length, before, 'A terminated worker submitted a new native read');
  }, { binary });
  await check('Terminated worker error cannot cancel its replacement', async env => {
    const first = env.api.loadNativeHairFitBase({ target: 'C:/test/first.iff' }).catch(String);
    await flush();
    const old = env.workers[0];
    await env.api.cancelNativeHairPreview(); await first;
    const next = env.api.loadNativeHairFitBase({ target: 'C:/test/next.iff' }).catch(String);
    await flush();
    const current = env.workers.at(-1);
    old.onerror({ message: 'Late old worker failure' });
    assert(!current.terminated, 'An old worker error terminated the replacement decoder');
    current.finish(); assert.equal(typeof await next, 'object');
  });
  await check('Staged cancellation during fallback cannot restart decoding', async env => {
    env.holdFallback();
    const loading = env.api.loadStagedPreview({ target: 'C:/test/player.iff' }).catch(String);
    await flush(); env.workers[0].fail(); await flush();
    const stopping = env.api.cancelStagedPreview();
    await flush();
    env.release('fallback', resources(false)); await stopping; await flush();
    assert.equal(env.workers.length, 1, 'Cancelled fallback restarted the staged decoder');
    assert.match(await loading, /cancelled/);
  }, { native: true });
  await check('Reset during catalog fallback cannot cache expired resources', async env => {
    env.holdFallback();
    const loading = env.api.previewNativeHair(request).catch(String);
    await flush(); env.workers[0].fail(); await flush();
    const resetting = env.api.resetSession();
    await flush();
    env.release('fallback', { native: false }); await resetting; await flush();
    assert.equal(env.workers.length, 1, 'Reset fallback restarted a decoder');
    assert.match(await loading, /cancelled/);
    const count = env.calls.filter(call => call.command === 'native-preview-hair').length;
    const next = env.api.previewNativeHair(request);
    await flush();
    assert.equal(env.calls.filter(call => call.command === 'native-preview-hair').length, count + 1);
    env.workers.at(-1).finish(catalogPreview()); await next;
  }, { native: true });
  await check('Reset prevents a just-completed staged result from repopulating its cache', async env => {
    const loading = env.api.loadStagedPreview({ target: 'C:/test/player.iff' }).catch(String);
    await flush();
    env.workers[0].finish();
    const resetting = env.api.resetSession();
    await resetting; assert.match(await loading, /cancelled/);
    const next = env.api.loadStagedPreview({ target: 'C:/test/player.iff' });
    await flush();
    assert.equal(env.workers.length, 2, 'A cancelled result was reused after Reset');
    env.workers.at(-1).finish(); await next;
  });
  await check('Reset during initial catalog preparation cannot restore an expired descriptor', async env => {
    env.holdPreparation();
    const loading = env.api.previewNativeHair(request).catch(String);
    await flush();
    const resetting = env.api.resetSession();
    await flush();
    env.release('preparation', { native: false }); await resetting;
    assert.match(await loading, /cancelled/);
    const before = env.calls.filter(call => call.command === 'native-preview-hair').length;
    const next = env.api.previewNativeHair(request);
    await flush();
    assert.equal(env.calls.filter(call => call.command === 'native-preview-hair').length, before + 1);
    env.workers.at(-1).finish(catalogPreview()); await next;
  });
  if (baseline) {
    assert(failures.length > 0);
    console.log(JSON.stringify({ baseline: true, reproduced: failures }, null, 2));
  } else console.log('Preview barriers: scoped draining, cached/fit-base waits, stale worker isolation, fallback cancellation and reset cache ownership passed.');
})().catch(error => { console.error(error); process.exitCode = 1; }).finally(finishTest);
