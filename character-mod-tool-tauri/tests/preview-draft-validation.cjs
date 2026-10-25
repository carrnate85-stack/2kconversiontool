const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const esbuild = require('../build-tools/node_modules/esbuild');
const finishTest = require('./test-watchdog.cjs')('preview draft validation');
const source = esbuild.transformSync(fs.readFileSync(path.join(__dirname, '../web/ui/app.ts'), 'utf8'), { loader: 'ts', target: 'es2022' }).code;
const baseline = process.argv.includes('--baseline'), failures = [];
function setup({ previous = {}, draft = {}, fit = { chestTHIN: .5, chestTHICK: .25, 'Spine2.tx': 0 }, otherTarget = false, loadedPath = 'A', draftTarget, cancelRender = false } = {}) {
  const state = { resetting: false, workingPlayer: { path: 'A', name: 'A' }, body: { targetPlayer: { path: otherTarget ? 'B' : 'A' }, targetDraft: draft },
    preview: { loadedPath, bodyFitTarget: draftTarget, requestId: 0, dirty: true, bodyFitBusy: false, chestCorrectionBusy: false, bodyFitDraft: previous, bodyFitDirtyFields: new Set(), bodyFitInvalidInputs: new Map(), jersey: false } };
  const elements = new Map(), rendered = [], rendererInputs = [], live = []; let saves = 0;
  const element = id => { if (!elements.has(id)) elements.set(id, { hidden: false, textContent: '', checked: false, classList: { toggle() {} }, querySelectorAll: () => [], querySelector() { return this; } }); return elements.get(id); };
  const payload = { player: { bodyFit: { ...fit } }, meshes: [{}], stats: { modelsDecoded: 1, companions: 0 }, skipped: [], activeItems: [] };
  const context = vm.createContext({ state, $: element, samePlayerPath: (a,b) => a === b, bodyFitValuesEqual: (a,b) => a === b,
    requestAnimationFrame: callback => callback(), setPreviewStatus() {}, renderWorkingPlayer() {}, renderPreviewControls() {},
    renderPreviewBodyFitControls() {}, resetPreviewDetails() {}, setPreviewBodyFitStatus() {}, renderPreviewWarnings() {}, addLog() {}, updateBodyFitActions() {},
    currentPreviewBodyFit: () => ({ ...state.preview.bodyFit, ...state.preview.bodyFitDraft }), syncBodyFitDraftFromPreview() {},
    previewBodyFitAreas: () => [], previewBodyDimensionGroups: () => [],
    window: { configsEditor: { active: () => 'BASE' }, characterTool: { loadStagedPreview: async () => payload, saveBodyFit: async () => { saves++; return {}; } },
      modelPreview: { load: async preview => { rendererInputs.push(structuredClone(preview.player.bodyFit)); rendered.push(structuredClone(preview.player.bodyFit)); return cancelRender ? { cancelled: true } : { decodedTextures: 0 }; },
        setBodyFit: values => { live.push(structuredClone(values)); if(rendered.length)rendered[rendered.length-1]=structuredClone(values); },
        resize() {}, setJerseyVisible() {}, setChestCorrection() {} } },
  });
  const first = source.indexOf('function previewBodyFitDraft(');
  if (first >= 0) vm.runInContext(source.slice(first, source.indexOf('async function loadStagedPreview(', first)), context);
  const start = source.indexOf('async function loadStagedPreview(');
  vm.runInContext(source.slice(start, source.indexOf('function selectedSourceHeadband(', start)), context);
  return { state, context, rendered, rendererInputs, payload, live, element, saves: () => saves,
    useInputs() {
      vm.runInContext(source.slice(source.indexOf('function bodyFitEditBusy('), source.indexOf('function updateBodyFitActions(')), context);
      for (const [first, last] of [['function updatePreviewBodyFitActions(', 'function setPreviewBodyFitOpen('], ['function setPreviewFitValue(', 'function renderPreviewBodyFitControls('], ['function revertPreviewBodyFit(', 'function calculatePreviewBodyFit('], ['async function applyPreviewBodyFit(', 'function resetPreviewDetails(']]) {
        const start = source.indexOf(first); vm.runInContext(source.slice(start, source.indexOf(last, start)), context);
      }
    },
  };
}
async function check(label, data, verify) {
  const e = setup(data);
  try { await e.context.loadStagedPreview(true); assert.equal(e.rendered.length, 1); e.live.length=0; await verify(e); }
  catch (error) { if (!baseline) throw new Error(label + ': ' + error.message, { cause: error }); failures.push({ label, error: error.message }); }
}
(async () => {
  await check('Finite negative and over-one values remain supported', { draft: { chestTHIN: -1, chestTHICK: 2, 'Spine2.tx': -17 } }, e => {
    assert.deepEqual(e.rendered[0], { chestTHIN: -1, chestTHICK: 2, 'Spine2.tx': -17 }); assert.equal(e.state.preview.bodyFitDirtyFields.size, 3);
  });
  await check('Unfinished body inputs stay out of render data', { draft: { chestTHIN: '-', chestTHICK: '', 'Spine2.tx': '1e' } }, e => {
    assert.deepEqual(e.rendered[0], { chestTHIN: .5, chestTHICK: .25, 'Spine2.tx': 0 });
    assert.equal(e.state.body.targetDraft.chestTHIN, '-'); assert.equal(e.state.preview.bodyFitDirtyFields.size, 0);
  });
  await check('Nonfinite draft numbers cannot reach geometry', { draft: { chestTHIN: NaN, chestTHICK: Infinity, 'Spine2.tx': -Infinity } }, e => {
    assert(Object.values(e.rendered[0]).every(Number.isFinite)); assert.equal(e.state.preview.bodyFitDirtyFields.size, 0);
  });
  await check('JavaScript numbers outside the game float range are excluded', { draft: { chestTHIN: Number.MAX_VALUE, chestTHICK: -Number.MAX_VALUE } }, e => {
    assert(Object.values(e.rendered[0]).every(value => Number.isFinite(Math.fround(value)))); assert.equal(e.state.preview.bodyFitDirtyFields.size, 0);
  });
  await check('Non-number draft types are never coerced to zero', { draft: { chestTHIN: null, chestTHICK: false, 'Spine2.tx': [] } }, e => {
    assert.deepEqual(e.rendered[0], { chestTHIN: .5, chestTHICK: .25, 'Spine2.tx': 0 });
  });
  await check('Unknown and prototype-shaped fields do not enter render state', { draft: JSON.parse('{"removed":2,"constructor":3,"__proto__":4}') }, e => {
    assert.deepEqual(Object.keys(e.rendered[0]).sort(), ['Spine2.tx','chestTHICK','chestTHIN']); assert.equal(e.state.preview.bodyFitDirtyFields.size, 0);
  });
  await check('Previous drafts also use the current schema and float validation', { previous: { removed: 3, chestTHIN: '-', chestTHICK: 1.5 } }, e => {
    assert.deepEqual(e.rendered[0], { chestTHIN: .5, chestTHICK: 1.5, 'Spine2.tx': 0 }); assert.equal(e.state.preview.bodyFitDirtyFields.size, 1);
  });
  await check('Another body target cannot override the staged preview', { previous: { chestTHIN: -.5 }, draft: { chestTHIN: 2 }, otherTarget: true }, e => {
    assert.equal(e.rendered[0].chestTHIN, -.5);
  });
  await check('Saved baseline and unapplied input objects stay unchanged', { previous: { chestTHIN: -1 }, draft: { chestTHICK: 'unfinished' } }, e => {
    assert.equal(e.state.preview.bodyFit.chestTHIN, .5); assert.equal(e.state.body.targetDraft.chestTHICK, 'unfinished');
    assert.equal(e.rendered[0].chestTHIN, -1); assert.equal(e.rendered[0].chestTHICK, .25);
  });
  await check('Empty dimension input keeps the last valid value and blocks saving', { previous: { 'Spine2.tx': 2 } }, async e => {
    e.useInputs(); assert.equal(e.context.setPreviewFitValue('Spine2.tx', ''), false);
    assert.equal(e.state.preview.bodyFitDraft['Spine2.tx'], 2); assert.equal(e.live.length, 0);
    assert(e.element('previewApplyFit').disabled); assert(e.state.preview.bodyFitInvalidInputs.has('Spine2.tx'));
    await e.context.applyPreviewBodyFit(); assert.equal(e.saves(), 0);
  });
  await check('Overflow input is rejected and a corrected negative value recovers', {}, e => {
    e.useInputs(); assert.equal(e.context.setPreviewFitValue('Spine2.tx', '1e309'), false); assert.equal(e.live.length, 0);
    assert.equal(e.context.setPreviewFitValue('Spine2.tx', '-17'), true);
    assert.equal(e.live[0]['Spine2.tx'], -17); assert.equal(e.state.preview.bodyFitInvalidInputs.size, 0); assert.equal(e.element('previewApplyFit').disabled, false);
  });
  await check('Revert clears incomplete input as well as numeric drafts', { previous: { 'Spine2.tx': 2 } }, e => {
    e.useInputs(); e.context.setPreviewFitValue('Spine2.tx', ''); e.context.revertPreviewBodyFit();
    assert.equal(e.state.preview.bodyFitInvalidInputs.size, 0); assert.equal(e.state.preview.bodyFitDirtyFields.size, 0);
    assert.equal(e.live.at(-1)['Spine2.tx'], 0); assert(e.element('previewApplyFit').disabled);
  });
  await check('Removed dimension controls cannot add obsolete fields', {}, e => {
    e.useInputs(); assert.equal(e.context.setPreviewFitValue('removed', 2), false);
    assert.equal(e.live.length, 0); assert.equal(e.state.preview.bodyFitInvalidInputs.size, 0);
  });
  await check('Reload keeps known incomplete input without injecting it into geometry', { previous: { 'Spine2.tx': 2 } }, async e => {
    e.useInputs(); e.context.setPreviewFitValue('Spine2.tx', ''); e.state.preview.bodyFitInvalidInputs.set('removed', '?');
    await e.context.loadStagedPreview(true);
    assert.equal(e.state.preview.bodyFitInvalidInputs.size, 1); assert(e.state.preview.bodyFitInvalidInputs.has('Spine2.tx'));
    assert.equal(e.rendered.at(-1)['Spine2.tx'], 2); assert(Object.values(e.rendered.at(-1)).every(Number.isFinite));
  });
  await check('Input editability follows current load, chest and Reset state', {}, e => {
    e.useInputs(); const input = { disabled: true }; e.element('previewBodyFitControls').querySelectorAll = () => [input];
    e.state.preview.loading = true; e.context.updatePreviewBodyFitActions(); assert(input.disabled);
    e.state.preview.loading = false; e.context.updatePreviewBodyFitActions(); assert.equal(input.disabled, false);
    e.state.preview.chestCorrectionBusy = true; e.context.updatePreviewBodyFitActions(); assert(input.disabled);
    e.state.preview.chestCorrectionBusy = false; e.context.updatePreviewBodyFitActions(); assert.equal(input.disabled, false);
    e.state.resetting = true; e.context.updatePreviewBodyFitActions(); assert(input.disabled);
    e.state.resetting = false; e.context.updatePreviewBodyFitActions(); assert.equal(input.disabled, false);
  });
  await check('Cancelled GPU load preserves explicitly owned same-player drafts', {loadedPath:'',draftTarget:'A',previous:{chestTHIN:-1},otherTarget:true},e=>{
    assert.equal(e.rendered[0].chestTHIN,-1);assert.equal(e.state.preview.bodyFitTarget,'A');
  });
  await check('An explicit other-player draft owner cannot leak through a matching loaded path', {loadedPath:'A',draftTarget:'B',previous:{chestTHIN:-1}},e=>{
    assert.equal(e.rendered[0].chestTHIN,.5);
  });
  await check('A cancelled renderer result cannot mark the preview current', {cancelRender:true},e=>{
    assert(e.state.preview.dirty);assert.equal(e.state.preview.loadedPath,'');assert.equal(e.state.preview.loading,false);
  });
  if (baseline) { assert(failures.length); console.log(JSON.stringify({ baseline: true, reproduced: failures }, null, 2)); }
  else console.log('Preview draft validation: 18 reload/schema, game-float, incomplete input, save/revert, render ownership and preservation cases passed.');
})().catch(error => { console.error(error); process.exitCode = 1; }).finally(finishTest);
