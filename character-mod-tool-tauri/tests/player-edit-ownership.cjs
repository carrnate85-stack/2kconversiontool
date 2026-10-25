const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const esbuild = require('../build-tools/node_modules/esbuild');
const finishTest = require('./test-watchdog.cjs')('player edit ownership');
const source = esbuild.transformSync(fs.readFileSync(path.join(__dirname, '../web/ui/app.ts'), 'utf8'), { loader: 'ts', target: 'es2022' }).code;
const baseline = process.argv.includes('--baseline'), failures = [];
const tick = () => new Promise(resolve => setTimeout(resolve, 5));
function deferred() { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; }
function setup() {
  const calls = [], elements = new Map(), logs = [], statuses = [];
  const state = { workingRequestId: 1, workingGeneration: 1, workingPlayer: { path: 'A', name: 'Player A' }, workingSource: null,
    body: { targetPlayer: { path: 'A' }, requestId: 1, loading: false, operation: '', targetBodyFit: { chest: 1 }, targetDraft: { chest: 2 }, dirtyFields: new Set(['chest']), invalidFields: new Set() },
    preview: { requestId: 1, editRequestId: 0, loadedPath: 'A', loading: false, dirty: false, bodyFitBusy: false, chestCorrectionBusy: false, jerseyAvailable: true,
      bodyFit: { chest: 1 }, bodyFitDraft: { chest: 2 }, bodyFitDirtyFields: new Set(['chest']), bodyFitInvalidInputs: new Map(), bodyFitOpen: true },
    headband: { requestId: 1, loading: false, applying: false, sourceHeadbands: [], configs: ['BASE'], styles: ['plain'] } };
  const element = id => { if (!elements.has(id)) elements.set(id, { textContent: '', className: '', disabled: false, hidden: false, value: 'plain', checked: false, replaceChildren() {}, classList: { toggle() {} } }); return elements.get(id); };
  let holdConfigs = false, holdPreview = false, holdHeadbands = false, bodyReads = 0, synced = 0;
  function api(method, request) { const call = { method, request, ...deferred() }; calls.push(call); return call.promise; }
  const context = vm.createContext({ state, $: element, basename: x => x, samePlayerPath: (a,b) => a === b,
    addLog: (...args) => logs.push(args), renderBodyFitRows() {}, updateBodyFitActions() {},
    syncLoadedPreviewBodyFit() { synced++; }, renderPreviewBodyFitControls() {}, renderPreviewControls() {}, updatePreviewBodyFitActions() {},
    bodyFitValuesEqual: (a,b) => a === b, syncPreviewBodyFitFromBody() {},
    currentPreviewBodyFit: () => ({ ...state.preview.bodyFit, ...state.preview.bodyFitDraft }),
    setPreviewBodyFitOpen: value => { state.preview.bodyFitOpen = value; },
    setPreviewBodyFitStatus: (...args) => statuses.push(args), setPreviewStatus: (...args) => statuses.push(args),
    selectedSourceHeadband: () => ({ path: 'source' }), renderHeadbandControls() {},
    refreshBodyFit: async () => { bodyReads++; state.body.requestId++; },
    loadStagedPreview: () => holdPreview ? api('reload', {}) : Promise.resolve(),
    refreshHeadbandOptions: () => { state.headband.requestId++; return holdHeadbands ? api('headbands', {}) : Promise.resolve(); },
    window: { characterTool: {
      saveBodyFit: request => api('save', request), applyChestFix: request => api('chest', request),
      loadBodyFit: request => api('bodyRead', request),
      applyPreviewChestCorrection: request => api('previewChest', request), applyHeadband: request => api('headband', request),
    }, configsEditor: { refresh: () => holdConfigs ? api('configs', {}) : Promise.resolve() } },
  });
  vm.runInContext(source.slice(source.indexOf('function bodyFitEditBusy('), source.indexOf('function updateBodyFitActions(')), context);
  vm.runInContext(source.slice(source.indexOf('function invalidatePlayerRender('), source.indexOf('function currentPreviewBodyFit(')), context);
  for (const [first, last] of [
    ['async function saveBodyFit(', 'function selectedNativeHair('],
    ['async function applyPreviewBodyFit(', 'function resetPreviewDetails('],
    ['async function applyPreviewChestCorrection(', 'function renderPreviewWarnings('],
    ['async function applyHeadband(', 'async function runSwap('],
  ]) { const start = source.indexOf(first); vm.runInContext(source.slice(start, source.indexOf(last, start)), context); }
  return { state, context, calls, logs, statuses, element,
    switchPlayer(path = 'B') {
      state.workingRequestId++; state.workingGeneration++; state.workingPlayer = { path, name: 'Player ' + path };
      state.body.targetPlayer = { path }; state.body.requestId++; state.body.loading = false; state.body.operation = '';
      state.body.targetBodyFit = { chest: 10 }; state.body.targetDraft = { chest: 11 }; state.body.dirtyFields = new Set(['chest']);
      state.preview.requestId++; state.preview.editRequestId++; state.preview.loadedPath = path;
      state.preview.bodyFitBusy = false; state.preview.chestCorrectionBusy = false;
      state.preview.bodyFit = { chest: 10 }; state.preview.bodyFitDraft = { chest: 11 }; state.preview.bodyFitDirtyFields = new Set(['chest']);
      state.headband.requestId++; state.headband.loading = false; state.headband.applying = false;
      element('bodyFitStatus').textContent = 'B body status'; element('headbandStatus').textContent = 'B headband status';
    },
    reply(index, result = { targetBodyFit: { chest: 2 }, changed: ['chest'], target: 'A', headband: 'band', style: 'plain', corrected: 3 }) { calls[index].resolve(result); },
    holdConfigs() { holdConfigs = true; }, holdPreview() { holdPreview = true; }, holdHeadbands() { holdHeadbands = true; },
    useActualBodyRefresh() { const start = source.indexOf('async function refreshBodyFit('); vm.runInContext(source.slice(start, source.indexOf('async function saveBodyFit(', start)), context); },
    counts: () => ({ bodyReads, synced }),
  };
}
async function check(label, run) { try { await run(setup()); } catch (error) { if (!baseline) throw new Error(label + ': ' + error.message, { cause: error }); failures.push({ label, error: error.message }); } }
(async () => {
  await check('Old body success cannot overwrite a new player', async e => {
    const task = e.context.saveBodyFit(); e.switchPlayer(); e.reply(0); await task;
    assert.equal(e.calls[0].request.target, 'A'); assert.equal(e.state.body.targetBodyFit.chest, 10);
    assert.equal(e.state.body.targetDraft.chest, 11); assert.equal(e.element('bodyFitStatus').textContent, 'B body status');
  });
  await check('Old body errors cannot replace current status', async e => {
    const task = e.context.saveBodyFit(); e.switchPlayer(); e.calls[0].reject(Error('old save')); await task;
    assert.equal(e.element('bodyFitStatus').textContent, 'B body status'); assert.equal(e.logs.length, 0);
  });
  await check('Old body finally cannot clear a newer save', async e => {
    const first = e.context.saveBodyFit(); e.switchPlayer(); const second = e.context.saveBodyFit();
    e.reply(0); await first; const busy = e.state.body.loading, label = e.element('saveBodyFit').textContent;
    e.reply(1); await second; assert.equal(busy, true); assert.equal(label, 'Saving...');
  });
  await check('Same-path restaging retires body ownership', async e => {
    const task = e.context.saveBodyFit(); e.switchPlayer('A'); e.reply(0); await task; assert.equal(e.state.body.targetBodyFit.chest, 10);
  });
  await check('Old body chest fix cannot refresh another player', async e => {
    e.state.body.dirtyFields.clear(); const task = e.context.applyChestFix(); e.switchPlayer(); e.reply(0); await task;
    assert.equal(e.counts().bodyReads, 0); assert.equal(e.logs.length, 0);
  });
  await check('Body chest fix holds loading and prevents another write', async e => {
    e.state.body.dirtyFields.clear(); const task = e.context.applyChestFix(), busy = e.state.body.loading;
    e.state.body.dirtyFields.add('chest'); const second = e.context.saveBodyFit(); e.reply(0); await task;
    if (e.calls[1]) e.reply(1); await second; assert.equal(busy, true); assert.equal(e.calls.length, 1);
  });
  await check('Current body save retains save/sync behavior', async e => {
    const task = e.context.saveBodyFit(); e.reply(0); await task;
    assert.equal(e.state.body.targetBodyFit.chest, 2); assert.equal(e.state.body.dirtyFields.size, 0);
    assert.equal(e.state.body.loading, false); assert.equal(e.counts().synced, 1);
  });
  await check('Old preview body save cannot overwrite another player', async e => {
    const task = e.context.applyPreviewBodyFit(); e.switchPlayer(); e.reply(0); await task;
    assert.equal(e.state.preview.bodyFit.chest, 10); assert.equal(e.state.body.targetBodyFit.chest, 10);
    assert.equal(e.state.preview.bodyFitOpen, true); assert.equal(e.statuses.length, 1);
  });
  await check('Old preview body errors cannot replace current status', async e => {
    const task = e.context.applyPreviewBodyFit(); e.switchPlayer(); e.calls[0].reject(Error('old preview save')); await task;
    assert.equal(e.statuses.length, 1); assert.equal(e.logs.length, 0);
  });
  await check('Preview reload retires old drafts but releases busy', async e => {
    const task = e.context.applyPreviewBodyFit(); e.state.preview.requestId++;
    e.state.preview.bodyFit = { chest: 7 }; e.state.preview.bodyFitDraft = { chest: 8 };
    e.reply(0); await task; assert.equal(e.state.preview.bodyFit.chest, 7);
    assert.equal(e.state.preview.bodyFitDraft.chest, 8); assert.equal(e.state.preview.bodyFitBusy, false);
  });
  await check('Current preview save still minimizes the panel', async e => {
    const task = e.context.applyPreviewBodyFit(); e.reply(0); await task;
    assert.equal(e.state.preview.bodyFit.chest, 2); assert.equal(e.state.preview.bodyFitOpen, false); assert.equal(e.state.preview.bodyFitBusy, false);
  });
  await check('Old preview chest fix cannot reload another player', async e => {
    const task = e.context.applyPreviewChestCorrection(); e.switchPlayer(); e.reply(0); await task;
    assert.equal(e.statuses.length, 1); assert.equal(e.logs.length, 0); assert.equal(e.state.preview.dirty, false);
  });
  await check('Old preview chest error cannot replace current status', async e => {
    const task = e.context.applyPreviewChestCorrection(); e.switchPlayer(); e.calls[0].reject(Error('old chest')); await task;
    assert.equal(e.statuses.length, 1); assert.equal(e.logs.length, 0);
  });
  await check('Switch during chest reload suppresses old success', async e => {
    e.holdPreview(); const task = e.context.applyPreviewChestCorrection(); e.reply(0); await tick();
    e.switchPlayer(); e.reply(1); await task; assert.equal(e.statuses.length, 1); assert.equal(e.logs.length, 0);
  });
  await check('Old headband success cannot refresh new configs', async e => {
    e.holdConfigs(); const task = e.context.applyHeadband('add'); e.switchPlayer(); e.reply(0); await tick();
    if (e.calls[1]) e.reply(1); await task;
    assert.equal(e.calls.length, 1); assert.equal(e.element('headbandStatus').textContent, 'B headband status');
    assert.equal(e.state.preview.dirty, false); assert.equal(e.logs.length, 0);
  });
  await check('Old headband failure cannot replace current status', async e => {
    const task = e.context.applyHeadband('add'); e.switchPlayer(); e.calls[0].reject(Error('old headband')); await task;
    assert.equal(e.element('headbandStatus').textContent, 'B headband status'); assert.equal(e.logs.length, 0);
  });
  await check('Switch during configs refresh retires headband continuation', async e => {
    e.holdConfigs(); const task = e.context.applyHeadband('add'); e.reply(0); await tick();
    e.switchPlayer(); const before = e.state.headband.requestId; e.reply(1); await task;
    assert.equal(e.state.headband.requestId, before); assert.equal(e.element('headbandStatus').textContent, 'B headband status');
  });
  await check('New headband read owns status over an old final refresh', async e => {
    e.holdHeadbands(); const task = e.context.applyHeadband('add'); e.reply(0); await tick();
    e.state.headband.requestId++; e.element('headbandStatus').textContent = 'New read'; e.reply(1); await task;
    assert.equal(e.element('headbandStatus').textContent, 'New read');
  });
  await check('Failed player selection leaves current body save valid', async e => {
    const task = e.context.saveBodyFit(); e.state.workingRequestId++; e.reply(0); await task;
    assert.equal(e.state.body.targetBodyFit.chest, 2); assert.equal(e.state.body.loading, false);
  });
  await check('Failed player selection leaves current preview save valid', async e => {
    const task = e.context.applyPreviewBodyFit(); e.state.workingRequestId++; e.reply(0); await task;
    assert.equal(e.state.preview.bodyFit.chest, 2); assert.equal(e.state.preview.bodyFitBusy, false);
  });
  await check('Reset generation retires writes before the player is cleared', async e => {
    const task = e.context.applyPreviewBodyFit(); e.state.workingGeneration++; e.reply(0); await task;
    assert.equal(e.state.preview.bodyFit.chest, 1); assert.equal(e.logs.length, 0);
  });
  await check('Recovery reread retains unsaved numeric body-fit values', async e => {
    e.useActualBodyRefresh(); const task = e.context.refreshBodyFit({ preserveDraft: true });
    e.reply(0, { targetBodyFit: { chest: 1 }, targetEntry: 'fixture' }); await task;
    assert.equal(e.state.body.targetBodyFit.chest, 1); assert.equal(e.state.body.targetDraft.chest, 2);
    assert(e.state.body.dirtyFields.has('chest')); assert.equal(e.state.body.loading, false);
  });
  await check('Recovery reread retains invalid text for correction', async e => {
    e.useActualBodyRefresh(); e.state.body.targetDraft.chest = 'unfinished'; e.state.body.invalidFields.add('chest');
    const task = e.context.refreshBodyFit({ preserveDraft: true }); e.reply(0, { targetBodyFit: { chest: 1 }, targetEntry: 'fixture' }); await task;
    assert.equal(e.state.body.targetDraft.chest, 'unfinished'); assert(e.state.body.invalidFields.has('chest'));
  });
  await check('Recovery reread removes edits already saved to disk', async e => {
    e.useActualBodyRefresh(); const task = e.context.refreshBodyFit({ preserveDraft: true }); e.reply(0); await task;
    assert.equal(e.state.body.targetBodyFit.chest, 2); assert.equal(e.state.body.dirtyFields.size, 0);
    assert.equal(Object.keys(e.state.body.targetDraft).length, 0);
  });
  await check('Ordinary Refresh retains its explicit discard behavior', async e => {
    e.useActualBodyRefresh(); const task = e.context.refreshBodyFit(); e.reply(0, { targetBodyFit: { chest: 1 }, targetEntry: 'fixture' }); await task;
    assert.equal(e.state.body.dirtyFields.size, 0); assert.equal(Object.keys(e.state.body.targetDraft).length, 0);
  });
  await check('Reset excludes new body, chest and headband writes', async e => {
    e.state.resetting = true;
    await e.context.saveBodyFit(); await e.context.applyPreviewBodyFit(); await e.context.applyPreviewChestCorrection();
    e.state.body.dirtyFields.clear(); await e.context.applyChestFix(); await e.context.applyHeadband('add');
    assert.equal(e.calls.length, 0);
  });
  await check('Body save excludes matching Preview writes and reads', async e => {
    e.useActualBodyRefresh(); const task = e.context.saveBodyFit();
    await e.context.applyPreviewBodyFit(); await e.context.applyPreviewChestCorrection(); await e.context.refreshBodyFit();
    assert.equal(e.calls.length, 1); e.reply(0); await task;
    assert.equal(e.context.bodyFitEditBusy('A'), false);
  });
  await check('Preview save excludes matching Body writes and reads', async e => {
    e.useActualBodyRefresh(); const task = e.context.applyPreviewBodyFit();
    await e.context.saveBodyFit(); await e.context.refreshBodyFit();
    e.state.body.dirtyFields.clear(); await e.context.applyChestFix();
    assert.equal(e.calls.length, 1); e.reply(0); await task;
    assert.equal(e.context.bodyFitEditBusy('A'), false);
  });
  await check('Shared busy checks are target-scoped and distinguish reads', async e => {
    e.state.body.loading = true;
    assert.equal(e.context.bodyFitEditBusy('A'), true);
    assert.equal(e.context.bodyFitEditBusy('A', false), false);
    assert.equal(e.context.bodyFitEditBusy('B'), false);
    e.state.body.operation = 'save'; assert.equal(e.context.bodyFitEditBusy('A', false), true);
    e.state.body.targetPlayer.path = 'B'; e.state.preview.bodyFitBusy = true;
    assert.equal(e.context.bodyFitEditBusy('A', false), true);
    assert.equal(e.context.bodyFitEditBusy('C'), false);
  });
  if (baseline) { assert(failures.length); console.log(JSON.stringify({ baseline: true, reproduced: failures }, null, 2)); }
  else console.log('Player edit ownership: 29 body-fit, shared-lock, recovery/drafts, chest correction, headband and continuation cases passed.');
})().catch(error => { console.error(error); process.exitCode = 1; }).finally(finishTest);
