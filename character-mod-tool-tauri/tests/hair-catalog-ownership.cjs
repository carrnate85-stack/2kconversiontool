const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const esbuild = require('../build-tools/node_modules/esbuild');
const source = esbuild.transformSync(fs.readFileSync(path.join(__dirname, '../web/ui/app.ts'), 'utf8'),
  { loader: 'ts', target: 'es2022' }).code;
const begin = source.indexOf('function nativeHairSourceKey(');
const end = source.indexOf('function nativeHairTargetKey(', begin);
assert(begin >= 0 && end > begin);
const baseline = process.argv.includes('--baseline');
const failures = [];
function setup() {
  const requests = [], elements = new Map();
  const state = { nativeHair: { assetType: 'hair', loading: false, catalogLoaded: false,
    catalogSourceKey: '', catalogRevision: '', previewCache: new Map(), headCache: new Map(),
    catalogRequest: null, assets: [], selectedId: '' } };
  const element = id => { if (!elements.has(id)) elements.set(id, {}); return elements.get(id); };
  const context = vm.createContext({
    state, $: element, setNativeHairCatalogLoading() {}, updateNativeHairInstallState() {},
    renderNativeHairRows() {}, renderSelectedNativeHair() {}, addLog() {},
    setNativeHairPreviewState() {}, invalidateNativeHairTarget() {}, selectedNativeHair: () => null,
    window: { characterTool: { loadNativeHairCatalog(payload) {
      let resolve, reject;
      const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
      requests.push({ payload, resolve, reject }); return promise;
    } } },
  });
  vm.runInContext(source.slice(begin, end), context);
  const reply = (index, name) => requests[index].resolve({ assets: [{ id: name }] });
  return { context, state, requests, reply, element };
}
async function check(label, run) {
  try { await run(); } catch (error) {
    if (!baseline) throw new Error(label + ': ' + error.message, { cause: error });
    failures.push({ label, error: error.message });
  }
}
(async () => {
  await check('Shared pending category read', async () => {
    const env = setup();
    const first = env.context.loadNativeHairCatalog();
    let secondFinished = false;
    const second = env.context.loadNativeHairCatalog().then(() => { secondFinished = true; });
    await Promise.resolve();
    assert.equal(secondFinished, false, 'A repeated caller returned before the shared catalog was ready');
    assert.equal(env.requests.length, 1);
    env.reply(0, 'hair'); await Promise.all([first, second]);
    assert.equal(env.state.nativeHair.assets[0].id, 'hair');
  });
  for (const stale of ['success', 'failure']) await check('Category change with stale ' + stale, async () => {
    const env = setup();
    const first = env.context.loadNativeHairCatalog();
    env.state.nativeHair.assetType = 'facialhair';
    env.state.nativeHair.catalogLoaded = false;
    const second = env.context.loadNativeHairCatalog();
    assert.equal(env.requests.length, 2, 'The requested new category did not start loading');
    env.reply(1, 'beard'); await second;
    if (stale === 'success') env.reply(0, 'old-hair');
    else env.requests[0].reject(new Error('old failure'));
    await first;
    assert.equal(env.state.nativeHair.assets[0].id, 'beard');
    assert.equal(env.state.nativeHair.catalogLoaded, true);
    assert.equal(env.state.nativeHair.loading, false);
    assert.notEqual(env.element('nativeHairStatus').textContent, 'old failure');
  });
  await check('Old completion cannot clear current loading state', async () => {
    const env = setup();
    const first = env.context.loadNativeHairCatalog();
    env.state.nativeHair.assetType = 'facialhair';
    const second = env.context.loadNativeHairCatalog();
    assert.equal(env.requests.length, 2);
    env.reply(0, 'old-hair'); await first;
    assert.equal(env.state.nativeHair.loading, true);
    assert.equal(env.state.nativeHair.assets.length, 0);
    env.reply(1, 'beard'); await second;
    assert.equal(env.state.nativeHair.loading, false);
  });
  await check('Current failure and explicit retry', async () => {
    const env = setup();
    const first = env.context.loadNativeHairCatalog();
    env.requests[0].reject(new Error('current failure')); await first;
    assert.equal(env.state.nativeHair.catalogLoaded, false);
    assert.equal(env.state.nativeHair.loading, false);
    const second = env.context.loadNativeHairCatalog();
    env.reply(1, 'recovered'); await second;
    assert.equal(env.state.nativeHair.assets[0].id, 'recovered');
    const refresh = env.context.loadNativeHairCatalog(true);
    env.reply(2, 'updated'); await refresh;
    assert.equal(env.state.nativeHair.assets[0].id, 'updated');
  });
  await check('Install owns busy state', async () => {
    const env = setup();
    env.state.nativeHair.loading = true;
    await env.context.loadNativeHairCatalog(true);
    assert.equal(env.requests.length, 0);
    assert.equal(env.state.nativeHair.loading, true);
  });
  if (baseline) {
    assert(failures.length > 0);
    console.log(JSON.stringify({ baseline: true, reproduced: failures }, null, 2));
  } else console.log('Hair catalog ownership: shared reads, category changes, stale errors, busy state and retry passed.');
})().catch(error => { console.error(error); process.exitCode = 1; });
