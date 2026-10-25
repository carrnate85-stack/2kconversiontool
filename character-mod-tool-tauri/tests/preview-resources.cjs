const assert = require('node:assert/strict');
const finishTest = require('./test-watchdog.cjs')('preview resource ownership');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const esbuild = require('../build-tools/node_modules/esbuild');
const root = path.resolve(__dirname, '..');
const baseline = process.argv.includes('--baseline');
const source = baseline ? fs.readFileSync(path.join(root, 'frontend/tauri-bridge.js'), 'utf8')
  : esbuild.buildSync({ entryPoints: [path.join(root, 'web/tauri-bridge.ts')], bundle: true,
    write: false, platform: 'browser', format: 'iife', target: 'es2022' }).outputFiles[0].text;
const tick = () => new Promise(resolve => setTimeout(resolve, 5));
const preview = () => ({ meshes: [], textures: [], stats: {}, skipped: [] });
function setup() {
  const live = new Map(), released = [], duplicate = [], workers = [], holds = [], preparations = [];
  let next = 1, holdNext = false, holdPreparation = false, failNative = false, events;
  const resource = kind => { const lease = String(next++); live.set(lease, kind); return lease; };
  const resources = native => ({ native, resourceLease: native ? resource('staged') : undefined,
    filePath: native ? 'native-player' : 'fallback-player', fingerprint: native ? 'same-player' : undefined,
    options: { companionPaths: [] }, player: {}, warnings: [], name: 'Fixture', activeItems: [] });
  const window = { __TAURI__: { event: { listen: async (_, handler) => { events = handler; } }, core: {
    invoke: async (_, { command, payload }) => {
      if (command === 'runtime-info') return {};
      if (command === 'blender-snapshot-prepare') { const value = resources(true); return { ...value, snapshotId: value.resourceLease }; }
      if (command === 'blender-snapshot-fallback') return { ...resources(false), snapshotId: payload.snapshotId };
      if (command === 'blender-snapshot-release') { if (!live.delete(payload.snapshotId)) duplicate.push(payload.snapshotId); return true; }
      if (command === 'native-preview-hair') {
        const value = { native: true, resourceLease: resource('catalog:' + payload.assetId) };
        if (holdPreparation) return new Promise(resolve => preparations.push(() => resolve(value)));
        return value;
      }
      if (command === 'native-preview-resources') return resources(true);
      if (command === 'preview-resources') return resources(false);
      if (command === 'native-hair-preview') return { native: false };
      if (command === 'release-preview-resources') {
        if (!live.delete(payload.lease)) duplicate.push(payload.lease);
        released.push(payload.lease); return true;
      }
      if (command === 'save-settings') return { settings: payload };
      if (command === 'health-repair' || command === 'health' || command === 'bootstrap') return {};
      if (command === 'reset' || command === 'cancel-preview') return true;
      if (command === 'blender-open') {
        queueMicrotask(() => events({ payload: { type: 'blender', state: 'opened', target: payload.target, handoffId: payload.handoffId } }));
        return true;
      }
      throw Error('Unexpected fixture command: ' + command);
    },
  } } };
  class Worker {
    constructor() { workers.push(this); }
    postMessage(message) {
      if (!message.type) return;
      const finish = () => {
        const native = message.type === 'catalog' ? message.payload.native : message.payload.filePath === 'native-player';
        this.onmessage({ data: { id: message.id, error: failNative && native ? 'Expected native failure' : undefined,
          result: message.type === 'catalog' ? { asset: preview(), base: preview() } : preview() } });
      };
      if (holdNext) { holdNext = false; holds.push(finish); }
      else queueMicrotask(finish);
    }
    terminate() { this.terminated = true; }
  }
  vm.runInNewContext(source, { window, Worker, console, structuredClone, ArrayBuffer, setInterval, clearInterval,
    setTimeout, crypto: { randomUUID: () => 'resource-test-handoff' } });
  return { api: window.characterTool, live, released, duplicate, holds, preparations,
    hold() { holdNext = true; }, fail() { failNative = true; },
    holdPreparation() { holdPreparation = true; },
    async catalog(id) { return window.characterTool.previewNativeHair({ assetType: 'hair', assetId: String(id) }); },
  };
}
const failures = [];
async function check(label, run) {
  const env = setup();
  try { await tick(); await run(env); await tick(); assert.deepEqual(env.duplicate, []); }
  catch (error) { if (!baseline) throw new Error(label + ': ' + error.message, { cause: error }); failures.push({ label, error: error.message }); }
}
(async () => {
  await check('Staged resource owners end after decode and cache hits', async env => {
    await env.api.loadStagedPreview({ target: 'player' }); assert.equal(env.live.size, 0);
    const cached = await env.api.loadStagedPreview({ target: 'player' });
    assert.equal(cached.stats.previewCacheHit, true); assert.equal(env.live.size, 0); assert.equal(env.released.length, 2);
  });
  await check('Staged fallback releases its native owner', async env => {
    env.fail(); await env.api.loadStagedPreview({ target: 'player' }); assert.equal(env.live.size, 0);
  });
  await check('Cancelled staged decode releases its owner', async env => {
    env.hold(); const loading = env.api.loadStagedPreview({ target: 'player' }).catch(String); await tick();
    assert.equal(env.live.size, 1); await env.api.cancelStagedPreview();
    assert.match(await loading, /cancelled/); assert.equal(env.live.size, 0);
  });
  await check('Catalog LRU stays bounded during 250 distinct selections', async env => {
    for (let i = 0; i < 250; i++) { await env.catalog(i); await tick(); assert(env.live.size <= 48); }
    assert.equal(env.live.size, 48); await env.api.resetSession(); await tick(); assert.equal(env.live.size, 0);
  });
  await check('An evicted active decoder retains its owner until settlement', async env => {
    env.hold(); const old = env.catalog('active').catch(String); await tick();
    const active = [...env.live.keys()][0];
    for (let i = 0; i < 60; i++) await env.catalog(i);
    await tick(); assert(env.live.has(active)); assert.equal(env.live.size, 49);
    env.holds[0](); assert.match(await old, /cancelled/); await tick(); assert(!env.live.has(active)); assert.equal(env.live.size, 48);
  });
  await check('LRU hits retain the recently used descriptor', async env => {
    for (let i = 0; i < 48; i++) await env.catalog(i);
    await env.catalog(0); await env.catalog(48); await tick();
    assert([...env.live.values()].includes('catalog:0')); assert(![...env.live.values()].includes('catalog:1'));
  });
  await check('Obsolete completed preparations release uncached owners', async env => {
    env.holdPreparation(); const loading = env.catalog('old').catch(String); await tick();
    await env.api.saveSettings({}); env.preparations[0](); assert.match(await loading, /cancelled/); assert.equal(env.live.size, 0);
  });
  await check('Catalog fallback retires the original native descriptor', async env => {
    env.fail(); await env.catalog('fallback'); await tick(); assert.equal(env.live.size, 0);
  });
  await check('Settings cleanup waits for an active borrower to settle', async env => {
    env.hold(); const old = env.catalog('active').catch(String); await tick();
    await env.api.saveSettings({}); assert.match(await old, /cancelled/); await tick(); assert.equal(env.live.size, 0);
  });
  await check('Independent Blender preparation is pinned through catalog eviction', async env => {
    env.hold(); const opening = env.api.openPreviewInBlender({ target: 'blender-player', includeTextures: true }); await tick();
    const blender = [...env.live.keys()][0];
    for (let i = 0; i < 60; i++) await env.catalog(i);
    await tick(); assert(env.live.has(blender));
    env.holds[0](); await opening; assert(!env.live.has(blender)); assert.equal(env.live.size, 48);
  });
  if (baseline) { assert(failures.length > 0); console.log(JSON.stringify({ baseline: true, reproduced: failures }, null, 2)); }
  else console.log('Preview resources: 10 staged, catalog, LRU, fallback, cancellation and independent Blender ownership cases passed.');
})().catch(error => { console.error(error); process.exitCode = 1; }).finally(finishTest);
