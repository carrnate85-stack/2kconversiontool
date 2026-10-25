const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const esbuild = require('../build-tools/node_modules/esbuild');
const source = esbuild.transformSync(fs.readFileSync(path.join(__dirname, '../web/ui/app.ts'), 'utf8'),
  { loader: 'ts', target: 'es2022' }).code;
const begin = source.indexOf('function nativeHairCached(');
const end = source.indexOf('let nativeHairTable', begin);
assert(begin >= 0 && end > begin);
const baseline = process.argv.includes('--baseline');
function setup() {
  let finish, fail, rendered = 'A', loading = false, message = '', calls = 0, loads = 0;
  const asset = id => ({ id, sourcePng: id });
  const preview = id => ({ asset: { id }, base: { id } });
  const state = { nativeHair: { assetType: 'hair', activePreviewKey: '', displayedPreviewKey: 'hair:A',
    previewRequestId: 0, previewCache: new Map([['hair:A', preview('A').asset]]),
    headCache: new Map([['A', preview('A').base]]), baseLoadedPath: 'A', basePreview: preview('A').base } };
  const camera = { yaw: 0, pitch: 0, distance: 1, fittedDistance: 1 };
  const context = vm.createContext({
    state, NATIVE_HAIR_ASSET_CACHE_LIMIT: 12, NATIVE_HAIR_HEAD_CACHE_LIMIT: 4,
    nativeHairSourceKey: () => '',
    setNativeHairPreviewState(value = '', busy = false) { message = value; loading = busy; }, saveNativeHairCamera() {},
    window: { characterTool: { previewNativeHair() {
      calls++;
      return new Promise((resolve, reject) => { finish = resolve; fail = reject; });
    } }, hairBasePreview: {
      async load() {}, cameraState: () => camera, setJerseyVisible() {}, setEyelashesVisible() {},
      setCameraState() {}, focusHead() {}, fitCamera() {},
    }, hairAssetPreview: {
      clear() { rendered = ''; }, async load(value) { loads++; rendered = value.id; }, setJerseyVisible() {},
      setOcclusionBase() {}, setEyelashesVisible() {}, bounds() {}, setCameraState() {},
    } },
  });
  vm.runInContext(source.slice(begin, end), context);
  return { context, state, asset, finish: id => finish(preview(id)), fail: () => fail(new Error('Obsolete failure')),
    status: () => ({ rendered, loading, message, calls, loads }) };
}
(async () => {
  for (const outcome of ['success', 'failure']) {
    const env = setup();
    const pending = env.context.previewNativeHairAsset(env.asset('B'));
    assert.equal(env.status().calls, 1);
    await env.context.previewNativeHairAsset(env.asset('A'));
    if (outcome === 'success') env.finish('B'); else env.fail();
    await pending;
    if (baseline) {
      assert(outcome === 'success' ? env.status().rendered === 'B' : env.status().message === 'Obsolete failure');
      continue;
    }
    assert.equal(env.status().rendered, 'A', 'An obsolete load replaced the selected cached asset');
    assert.equal(env.status().loading, false);
    assert.equal(env.status().message, '');
    assert.equal(env.state.nativeHair.displayedPreviewKey, 'hair:A');
    assert.equal(env.state.nativeHair.activePreviewKey, '');
  }
  const pending = setup();
  const first = pending.context.previewNativeHairAsset(pending.asset('B'));
  await pending.context.previewNativeHairAsset(pending.asset('B'));
  assert.equal(pending.status().calls, 1, 'Identical active selections must not duplicate work');
  pending.finish('B'); await first;
  await pending.context.previewNativeHairAsset(pending.asset('B'));
  assert.equal(pending.status().loads, 1, 'An unchanged idle selection must not reload the renderer');
  console.log(baseline ? 'Baseline reproduces A-B-A stale render and stale error.'
    : 'Catalog UI latest intent: A-B-A cached restoration, stale success/error suppression and active/idle deduplication passed.');
})().catch(error => { console.error(error); process.exitCode = 1; });
