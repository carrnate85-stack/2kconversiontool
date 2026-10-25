const assert = require('node:assert/strict');
const finishTest = require('./test-watchdog.cjs')('source freshness');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const esbuild = require('../build-tools/node_modules/esbuild');
const root = path.resolve(__dirname, '..');
const source = esbuild.buildSync({ entryPoints: [path.join(root, 'web/tauri-bridge.ts')], bundle: true,
  write: false, platform: 'browser', format: 'iife', target: 'es2022' }).outputFiles[0].text;
const ui = esbuild.transformSync(fs.readFileSync(path.join(root, 'web/ui/app.ts'), 'utf8'), { loader: 'ts', target: 'es2022' }).code;
const tick = () => new Promise(resolve => setTimeout(resolve, 10));
const baseline = process.argv.includes('--baseline'), failures = [];
const preview = () => ({ meshes: [], textures: [], stats: {}, skipped: [] });
function bridge() {
  const calls = [], workers = [], lists = [];
  let holdLists = false, failSave = false;
  const window = { __TAURI__: { event: { listen: async () => {} }, core: { invoke: async (_, { command, payload }) => {
    calls.push(command);
    if (command === 'runtime-info') return {};
    if (command === 'native-hair-catalog') {
      if (holdLists) return new Promise(resolve => lists.push(resolve));
      return { assetType: payload.assetType, assets: [], sourceRevision: 'first', gameRoot: 'A' };
    }
    if (command === 'save-settings') { if (failSave) throw Error('save failed'); return { settings: payload }; }
    if (command === 'health-repair' || command === 'health' || command === 'bootstrap') return {};
    if (command === 'native-preview-hair') return { native: false };
    if (command === 'native-preview-resources') return { filePath: 'player', fingerprint: 'same', options: { companionPaths: [] }, player: {}, warnings: [], activeItems: [] };
    throw Error('Unexpected command: ' + command);
  } } } };
  class Worker {
    constructor() { workers.push(this); }
    postMessage(message) { this.message = message; }
    terminate() { this.terminated = true; }
    finish(result) { this.onmessage({ data: { id: this.message.id, result } }); }
  }
  vm.runInNewContext(source, { window, Worker, console, structuredClone, ArrayBuffer, setInterval, clearInterval, setTimeout });
  return { api: window.characterTool, calls, workers, lists,
    hold() { holdLists = true; }, fail() { failSave = true; },
    async warm() { const p = window.characterTool.previewNativeHair({ assetType: 'hair', assetId: 'same' }); await tick(); workers.at(-1).finish({ asset: preview(), base: preview() }); await p; },
  };
}
function frontend() {
  const requests = [], previews = [], elements = new Map();
  const hair = { assetType: 'hair', assets: [], selectedId: '', loading: false, catalogLoaded: false, catalogRequest: null,
    catalogSourceKey: '', catalogRevision: '', previewRequestId: 0, activePreviewKey: '', displayedPreviewKey: '',
    previewCache: new Map(), headCache: new Map(), baseLoadedPath: '', basePreview: null };
  const state = { bootstrap: { settings: { game_root_2k27: 'A' } }, nativeHair: hair };
  const context = vm.createContext({ state, $: id => { if (!elements.has(id)) elements.set(id, {}); return elements.get(id); },
    NATIVE_HAIR_ASSET_CACHE_LIMIT: 12, NATIVE_HAIR_HEAD_CACHE_LIMIT: 4,
    setNativeHairCatalogLoading() {}, setNativeHairPreviewState() {}, updateNativeHairInstallState() {},
    invalidateNativeHairTarget() {}, renderNativeHairRows() {}, renderSelectedNativeHair() {}, addLog() {},
    selectedNativeHair: () => null,
    window: { characterTool: {
      loadNativeHairCatalog: () => new Promise((resolve, reject) => requests.push({ resolve, reject })),
      previewNativeHair: () => new Promise(resolve => previews.push(resolve)),
    }, hairBasePreview: { clear() {} }, hairAssetPreview: { clear() {} } },
  });
  const helpers = ui.indexOf('function nativeHairSourceKey(');
  const start = helpers < 0 ? ui.indexOf('async function loadNativeHairCatalog(') : helpers;
  vm.runInContext(ui.slice(start, ui.indexOf('function nativeHairTargetKey(', start)), context);
  const previewStart = ui.indexOf('function nativeHairCached(');
  vm.runInContext(ui.slice(previewStart, ui.indexOf('let nativeHairTable', previewStart)), context);
  return { context, state, hair, requests, previews, reply(index, revision) { requests[index].resolve({ assets: [{ id: 'same' }], sourceRevision: revision }); } };
}
async function check(label, run) {
  try { await run(); } catch (error) { if (!baseline) throw new Error(label + ': ' + error.message, { cause: error }); failures.push({ label, error: error.message }); }
}
(async () => {
  await check('Bridge preserves authoritative source identity', async () => {
    const env = bridge(), result = await env.api.loadNativeHairCatalog({ assetType: 'hair' });
    assert.equal(result.sourceRevision, 'first'); assert.equal(result.gameRoot, 'A');
  });
  for (const action of ['saveSettings', 'repairHealth']) await check(action + ' cancels old decoding and releases descriptors', async () => {
    const env = bridge(); await env.warm();
    const old = env.api.previewNativeHair({ assetType: 'hair', assetId: 'same' }).catch(String); await tick();
    const worker = env.workers.at(-1);
    await env.api[action]({});
    worker.finish({ asset: preview(), base: preview() });
    assert.match(String(await old), /cancelled/);
    await env.warm(); assert.equal(env.calls.filter(x => x === 'native-preview-hair').length, 2);
  });
  await check('Failed settings write preserves reusable descriptors', async () => {
    const env = bridge(); await env.warm(); env.fail();
    await assert.rejects(env.api.saveSettings({}), /save failed/); await env.warm();
    assert.equal(env.calls.filter(x => x === 'native-preview-hair').length, 1);
  });
  await check('A settings commit rejects older catalog replies', async () => {
    const env = bridge(); env.hold();
    const loading = env.api.loadNativeHairCatalog({ assetType: 'hair' }).catch(String); await tick();
    await env.api.saveSettings({}); env.lists[0]({ assets: [], sourceRevision: 'old' });
    assert.match(String(await loading), /cancelled/);
  });
  await check('Changed source invalidates descriptors; unchanged category retains them', async () => {
    const env = bridge(); await env.api.loadNativeHairCatalog({ assetType: 'hair' }); await env.warm();
    await env.api.loadNativeHairCatalog({ assetType: 'facialhair' }); await env.warm();
    assert.equal(env.calls.filter(x => x === 'native-preview-hair').length, 1);
    env.hold(); const list = env.api.loadNativeHairCatalog({ assetType: 'hair' }); await tick();
    env.lists[0]({ assets: [], sourceRevision: 'changed' }); await list; await env.warm();
    assert.equal(env.calls.filter(x => x === 'native-preview-hair').length, 2);
  });
  await check('A late list cannot move the accepted revision backwards', async () => {
    const env = bridge(); env.hold();
    const older = env.api.loadNativeHairCatalog({ assetType: 'hair' }).catch(String);
    const newer = env.api.loadNativeHairCatalog({ assetType: 'facialhair' }); await tick();
    env.lists[1]({ assets: [], sourceRevision: 'new' }); await newer;
    env.lists[0]({ assets: [], sourceRevision: 'old' });
    assert.match(String(await older), /cancelled/);
  });
  await check('Parallel categories with the same revision remain valid', async () => {
    const env = bridge(); env.hold();
    const older = env.api.loadNativeHairCatalog({ assetType: 'hair' });
    const newer = env.api.loadNativeHairCatalog({ assetType: 'facialhair' }); await tick();
    env.lists[1]({ assets: [], sourceRevision: 'same' }); await newer;
    env.lists[0]({ assets: [], sourceRevision: 'same' });
    assert.equal((await older).sourceRevision, 'same');
  });
  await check('Settings commits also cancel and clear staged previews', async () => {
    const env = bridge();
    const first = env.api.loadStagedPreview({ target: 'player' }); await tick();
    env.workers.at(-1).finish(preview()); await first;
    await env.api.saveSettings({});
    const second = env.api.loadStagedPreview({ target: 'player' }); await tick();
    assert.equal(env.workers.length, 2, 'Settings reused a staged preview from the previous source');
    const worker = env.workers.at(-1), stale = second.catch(String);
    await env.api.saveSettings({}); worker.finish(preview());
    assert.match(await stale, /cancelled/);
  });
  await check('UI shares pending reads only within the same source', async () => {
    const env = frontend(), first = env.context.loadNativeHairCatalog();
    env.state.bootstrap.settings.game_root_2k27 = 'B';
    const second = env.context.loadNativeHairCatalog();
    assert.equal(env.requests.length, 2);
    env.reply(1, 'B'); await second; env.reply(0, 'A'); await first;
    assert.equal(env.hair.catalogRevision, 'B');
  });
  await check('UI retains warm caches for unchanged sources and clears changed ones', async () => {
    const env = frontend(), first = env.context.loadNativeHairCatalog(); env.reply(0, 'first'); await first;
    env.hair.selectedId = 'same'; env.hair.displayedPreviewKey = 'hair:same';
    env.hair.previewCache.set('hair:same', {}); env.hair.headCache.set('head', {});
    const again = env.context.loadNativeHairCatalog(true); env.reply(1, 'first'); await again;
    assert.equal(env.hair.previewCache.size, 1); assert.equal(env.hair.displayedPreviewKey, 'hair:same');
    const changed = env.context.loadNativeHairCatalog(true); env.reply(2, 'new'); await changed;
    assert.equal(env.hair.previewCache.size, 0); assert.equal(env.hair.headCache.size, 0);
    assert.equal(env.hair.displayedPreviewKey, ''); assert.equal(env.hair.selectedId, 'same');
  });
  await check('Old source preview cannot refill UI caches', async () => {
    const env = frontend(), first = env.context.loadNativeHairCatalog(); env.reply(0, 'first'); await first;
    const loading = env.context.previewNativeHairAsset({ id: 'same', sourcePng: 'head' });
    env.state.bootstrap.settings.game_root_2k27 = 'B';
    const changed = env.context.loadNativeHairCatalog(true); env.reply(1, 'new'); await changed;
    env.previews[0]({ asset: preview(), base: preview() }); await loading;
    assert.equal(env.hair.previewCache.size, 0); assert.equal(env.hair.headCache.size, 0);
  });
  if (baseline) { assert(failures.length > 0); console.log(JSON.stringify({ baseline: true, reproduced: failures }, null, 2)); }
  else console.log('Source freshness: 12 bridge/UI ownership and cache-invalidation cases passed.');
})().catch(error => { console.error(error); process.exitCode = 1; }).finally(finishTest);
