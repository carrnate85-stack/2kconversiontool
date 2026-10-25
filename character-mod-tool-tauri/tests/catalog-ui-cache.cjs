const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const esbuild = require('../build-tools/node_modules/esbuild');
const file = path.resolve(__dirname, '../web/ui/app.ts');
const source = esbuild.transformSync(fs.readFileSync(file, 'utf8'), { loader: 'ts', target: 'es2022' }).code;
const begin = source.indexOf('function nativeHairSourceKey(');
const end = source.indexOf('async function refreshNativeHairTarget(', begin);
assert(begin >= 0 && end > begin);
const elements = new Map();
const element = id => {
  if (!elements.has(id)) elements.set(id, {});
  return elements.get(id);
};
let calls = 0, revision = 'first';
const state = { nativeHair: { loading: false, catalogLoaded: false, assetType: 'hair',
  catalogSourceKey: '', catalogRevision: '', previewCache: new Map(), headCache: new Map(),
  assets: [], selectedId: '', catalogCache: { hair: null, facialhair: null } } };
const context = vm.createContext({
  state, $: element, setNativeHairCatalogLoading() {}, updateNativeHairInstallState() {},
  renderNativeHairRows() {}, renderSelectedNativeHair() {}, addLog() {},
  setNativeHairPreviewState() {}, invalidateNativeHairTarget() {}, selectedNativeHair: () => null,
  window: { characterTool: { async loadNativeHairCatalog({ assetType }) {
    calls++;
    return { assets: [{ id: assetType + '-' + revision }] };
  } } },
});
vm.runInContext(source.slice(begin, end), context);
(async () => {
  await context.loadNativeHairCatalog();
  assert.equal(state.nativeHair.assets[0].id, 'hair-first');
  state.nativeHair.selectedId = 'hair-first';
  await context.loadNativeHairCatalog(true);
  assert.equal(state.nativeHair.selectedId, 'hair-first', 'Rechecking an unchanged list must preserve selection');
  for (const assetType of ['facialhair', 'hair']) {
    revision = 'updated';
    state.nativeHair.assetType = assetType;
    state.nativeHair.catalogLoaded = false;
    await context.loadNativeHairCatalog();
    assert.equal(state.nativeHair.assets[0].id, assetType + '-updated', 'Frontend array bypassed authoritative catalog refresh');
  }
  assert.equal(calls, 4);
  assert.equal(state.nativeHair.loading, false);
  console.log('Catalog UI: backend ownership, category refresh and selected-row preservation passed.');
})().catch(error => { console.error(error); process.exitCode = 1; });
