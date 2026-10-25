const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const esbuild = require('../build-tools/node_modules/esbuild');
const finish = require('./test-watchdog.cjs')('body-fit reload synchronization');
const source = esbuild.transformSync(fs.readFileSync(path.join(__dirname, '../web/ui/app.ts'), 'utf8'), {
  loader: 'ts', target: 'es2022',
}).code;
const fit = { chestTHIN: .5, 'Spine2.tx': 0, 'Spine2.ty': 0 };
const tick = () => new Promise(resolve => setImmediate(resolve));

function setup({ otherTarget = false, previewOwner = '' } = {}) {
  const state = {
    resetting: false, workingGeneration: 1, workingPlayer: { path: 'A', name: 'A' },
    body: { targetPlayer: { path: otherTarget ? 'B' : 'A' }, sourceBodyFit: { ...fit }, targetBodyFit: { ...fit },
      targetDraft: {}, dirtyFields: new Set(), invalidFields: new Set(), loading: false, operation: '', requestId: 0 },
    preview: { loadedPath: '', bodyFitTarget: previewOwner, bodyFit: {}, bodyFitDraft: {},
      bodyFitDirtyFields: new Set(), bodyFitInvalidInputs: new Map(), loading: false, dirty: true,
      requestId: 0, editRequestId: 0, bodyFitBusy: false, chestCorrectionBusy: false, bodyFitOpen: true },
  };
  const elements = new Map(), inputs = [], visuals = [], writes = [];
  const payload = { player: { bodyFit: { ...fit } }, meshes: [{}], stats: { companions: 0 }, skipped: [] };
  let reads = 0;
  const element = id => {
    if (!elements.has(id)) elements.set(id, {
      value: '', textContent: '', disabled: false, hidden: false, checked: false, classList: { toggle() {}, add() {} },
      querySelectorAll: () => [], querySelector() { return this; },
      listeners: new Map(), addEventListener(event, listener) { this.listeners.set(event, listener); },
    });
    return elements.get(id);
  };
  const context = vm.createContext({
    state, $: element, queueMicrotask, requestAnimationFrame: fn => fn(), basename: value => value,
    renderBodyFitRows() {}, renderWorkingPlayer() {}, resetPreviewDetails() {}, renderPreviewWarnings() {},
    setPreviewStatus() {}, addLog() {}, updateStagePlayerControls() {},
    renderPreviewBodyFitControls() { context.updatePreviewBodyFitActions(); },
    window: {
      configsEditor: { active: () => 'BASE', renderPreviewSelector() {} },
      characterTool: {
        loadStagedPreview: async () => { reads++; return payload; },
        saveBodyFit: request => new Promise((resolve, reject) => writes.push({ request, resolve, reject })),
      },
      modelPreview: {
        load: async value => {
          inputs.push(structuredClone(value.player.bodyFit));
          visuals.push(structuredClone(value.player.bodyFit));
          return {};
        },
        setBodyFit: value => visuals.push(structuredClone(value)),
        resize() {}, setJerseyVisible() {}, setChestCorrection() {},
      },
    },
  });
  for (const [first, last] of [
    ['function formatBodyFitValue(', 'function renderBodyFitRows('],
    ['function samePlayerPath(', 'function renderPreviewBodyFitControls('],
    ['function revertPreviewBodyFit(', 'function calculatePreviewBodyFit('],
    ['async function applyPreviewBodyFit(', 'function resetPreviewDetails('],
    ['async function saveBodyFit(', 'async function applyChestFix('],
    ['function renderPreviewControls(', 'function setPreviewBlenderStatus('],
    ['function previewBodyFitDraft(', 'function selectedSourceHeadband('],
  ]) {
    const start = source.indexOf(first), end = source.indexOf(last, start);
    assert(start >= 0 && end > start, first);
    vm.runInContext(source.slice(start, end), context);
  }
  const handlerStart = source.indexOf('$("refreshPreview").addEventListener("click",');
  const handlerEnd = source.indexOf('$("openPreviewInBlender").addEventListener(', handlerStart);
  assert(handlerStart >= 0 && handlerEnd > handlerStart, 'Refresh click handler');
  vm.runInContext(source.slice(handlerStart, handlerEnd), context);
  return {
    state, context, payload, inputs, visuals, writes, element, reads: () => reads,
    load: () => context.loadStagedPreview(true),
    editBody(key, value) {
      context.handleBodyFitInput(key, { value: String(value), classList: { toggle() {} } },
        { classList: { add() {} } }, {});
    },
    clickRefresh: () => element('refreshPreview').listeners.get('click')(),
    reply(index = 0) {
      const write = writes[index];
      write.resolve({ targetBodyFit: { ...fit, ...write.request.changes }, changed: Object.keys(write.request.changes) });
    },
  };
}

let cases = 0;
async function check(label, run) {
  await run();
  cases++;
  console.log('PASS ' + label);
}

(async () => {
  for (const raw of ['', '1e100']) await check('Body invalid before first load: ' + JSON.stringify(raw), async () => {
    const e = setup();
    e.editBody('Spine2.tx', raw);
    assert(e.state.body.invalidFields.has('Spine2.tx'));
    assert.equal(e.state.preview.bodyFitInvalidInputs.size, 0);
    await e.load();
    assert.equal(e.state.preview.bodyFitInvalidInputs.get('Spine2.tx'), raw);
    e.context.setPreviewFitValue('Spine2.ty', 3);
    assert(e.element('previewApplyFit').disabled);
    await e.context.applyPreviewBodyFit();
    assert.equal(e.writes.length, 0);
    assert.equal(e.state.body.targetDraft['Spine2.tx'], raw);
    assert.deepEqual(e.inputs[0], fit);
    assert.deepEqual(e.payload.player.bodyFit, fit);
    e.context.setPreviewFitValue('Spine2.tx', -1);
    assert.equal(e.state.body.invalidFields.size, 0);
    assert.equal(e.state.preview.bodyFitInvalidInputs.size, 0);
    assert.equal(e.element('previewApplyFit').disabled, false);
    const task = e.context.applyPreviewBodyFit();
    e.reply(); await task;
    assert.equal(e.state.body.targetBodyFit['Spine2.tx'], -1);
    assert.equal(e.state.preview.bodyFitOpen, false);
  });
  for (const fromBody of [false, true]) await check((fromBody ? 'Body' : 'Preview') + ' finite -> invalid -> reload', async () => {
    const e = setup(); await e.load();
    const edit = value => fromBody ? e.editBody('Spine2.tx', value) : e.context.setPreviewFitValue('Spine2.tx', value);
    edit(2); edit('');
    assert.equal(e.state.body.targetDraft['Spine2.tx'], '');
    assert.equal(e.state.preview.bodyFitDraft['Spine2.tx'], 2);
    await e.load();
    assert.equal(e.state.preview.bodyFitDraft['Spine2.tx'], 2);
    assert.equal(e.visuals.at(-1)['Spine2.tx'], 2);
    assert.equal(e.state.preview.bodyFitInvalidInputs.get('Spine2.tx'), '');
    assert(e.state.body.invalidFields.has('Spine2.tx'));
    assert(e.element('previewApplyFit').disabled);
    assert.deepEqual(e.inputs.at(-1), fit);
    assert.deepEqual(e.payload.player.bodyFit, fit);
    e.context.revertPreviewBodyFit();
    assert.equal(e.state.body.invalidFields.size, 0);
    assert.equal(e.state.preview.bodyFitInvalidInputs.size, 0);
    assert.deepEqual(e.visuals.at(-1), fit);
  });
  await check('Other Body target cannot import invalid input', async () => {
    const e = setup({ otherTarget: true }); e.editBody('Spine2.tx', ''); await e.load();
    assert.equal(e.state.preview.bodyFitInvalidInputs.size, 0);
    assert(e.state.body.invalidFields.has('Spine2.tx'));
    assert.deepEqual(e.visuals.at(-1), fit);
  });
  await check('Other Preview owner cannot retain invalid input', async () => {
    const e = setup({ previewOwner: 'B' });
    e.state.preview.bodyFitInvalidInputs.set('Spine2.tx', ''); await e.load();
    assert.equal(e.state.preview.bodyFitInvalidInputs.size, 0);
  });
  await check('Reload removes obsolete invalid fields', async () => {
    const e = setup(); await e.load(); e.editBody('Spine2.tx', '');
    delete e.payload.player.bodyFit['Spine2.tx']; await e.load();
    assert.equal(e.state.preview.bodyFitInvalidInputs.size, 0);
    assert(!Object.hasOwn(e.state.preview.bodyFitDraft, 'Spine2.tx'));
  });
  for (const fail of [false, true]) await check('Refresh click during pending Apply: ' + (fail ? 'failure' : 'success'), async () => {
    const e = setup(); await e.load(); e.context.setPreviewFitValue('Spine2.tx', 2);
    const id = e.state.preview.requestId, reads = e.reads(), task = e.context.applyPreviewBodyFit();
    assert(e.state.preview.bodyFitBusy);
    assert(e.element('refreshPreview').disabled);
    // Dispatch even while disabled to exercise the queued-click guard itself.
    e.clickRefresh(); await tick();
    assert.equal(e.state.preview.requestId, id);
    assert.equal(e.reads(), reads);
    assert.equal(e.writes.length, 1);
    if (fail) e.writes[0].reject(new Error('fixture save failure')); else e.reply();
    await task;
    assert.equal(e.state.preview.bodyFitBusy, false);
    assert.equal(e.element('refreshPreview').disabled, false);
    assert.equal(e.state.preview.bodyFitOpen, fail);
    assert.equal(e.state.preview.bodyFitDirtyFields.has('Spine2.tx'), fail);
    assert.equal(e.state.body.targetBodyFit['Spine2.tx'], fail ? 0 : 2);
    assert.equal(e.state.preview.bodyFit['Spine2.tx'], fail ? 0 : 2);
    if (fail) assert.equal(e.state.body.targetDraft['Spine2.tx'], 2);
    e.clickRefresh(); await tick();
    assert.equal(e.reads(), reads + 1);
  });
  await check('Refresh click respects the shared Body write lock', async () => {
    const e = setup(); await e.load(); e.editBody('Spine2.tx', 2);
    const task = e.context.saveBodyFit(), reads = e.reads();
    e.clickRefresh(); await tick(); assert.equal(e.reads(), reads);
    e.reply(); await task;
    assert.equal(e.state.body.loading, false);
    assert.equal(e.state.preview.bodyFit['Spine2.tx'], 2);
  });
  await check('Other-target Body lock does not block Refresh', async () => {
    const e = setup({ otherTarget: true }); await e.load();
    e.state.body.loading = true; e.state.body.operation = 'save';
    const reads = e.reads(); e.clickRefresh(); await tick();
    assert.equal(e.reads(), reads + 1);
  });
  console.log(`Body-fit reload synchronization: ${cases} cases passed.`);
})().catch(error => { console.error(error); process.exitCode = 1; }).finally(finish);
