const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const esbuild = require('../build-tools/node_modules/esbuild');
const finishTest = require('./test-watchdog.cjs')('staging ownership');
const source = esbuild.transformSync(fs.readFileSync(path.join(__dirname, '../web/ui/app.ts'), 'utf8'), { loader: 'ts', target: 'es2022' }).code;
const tick = () => new Promise(resolve => setTimeout(resolve, 5));
const baseline = process.argv.includes('--baseline'), failures = [];
function deferred() { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; }
function setup() {
  const inspections = [], configs = [], elements = new Map(), storage = new Map();
  const state = { workingRequestId: 0, workingGeneration: 0, workingPlayer: null, workingSource: null, workingBaseline: null,
    activeView: 'full-swap', startingJob: false, activeJob: '', selectionBusy: false,
    body: { requestId: 0, dirtyFields: new Set(), loading: false }, hair: { loading: false, requestId: 0 },
    facialHair: { loading: false }, nativeHair: { baseRequestId: 0, previewRequestId: 0 },
    headband: { requestId: 1, loading: true, configs: ['old'], styles: ['old'], sourceHeadbands: ['old'] },
    preview: { requestId: 0, editRequestId: 0, loading: true, loadedPath: 'old', dirty: false, bodyFitDirtyFields: new Set(), bodyFitInvalidInputs: new Map() } };
  let holdConfigs = false, heldBody = null, reset = null, nativeTargets = 0, clears = 0;
  const element = id => { if (!elements.has(id)) elements.set(id, { hidden: false, classList: { remove() {} }, replaceChildren() {} }); return elements.get(id); };
  const context = vm.createContext({ state, $: element, basename: value => value.split('/').pop(),
    clearBodyFitValues() {}, renderWorkingPlayer() {}, invalidateNativeHairTarget() {}, renderHeadbandControls() {},
    renderPreviewControls() {}, renderPreviewBodyFitControls() {}, resetPreviewDetails() {}, syncPreviewBlenderState() {}, renderPlayerEditControls() {},
    refreshBodyFit: () => heldBody?.promise || Promise.resolve(),
    refreshNativeHairTarget: async () => { nativeTargets++; }, refreshHeadbandOptions: async () => {}, loadStagedPreview: async () => {},
    updateReadiness() {}, addLog() {}, resetFullSwapOutputNames() {}, resetFacialHairSelection() {},
    resetPreviewState() { state.preview.requestId++; state.preview.loading = false; },
    resetBodyFitSession() { state.body.requestId++; state.body.targetPlayer = null; },
    renderCharacter() {}, renderHairControls() {}, setHairStatus() {}, setProgress() {}, setActiveView() {},
    Option: function() {}, localStorage: { setItem: (key, value) => storage.set(key, value), removeItem: key => storage.delete(key) },
    window: { confirm: () => true, characterTool: {
      inspectCharacter(filePath) { const request = { filePath, ...deferred() }; inspections.push(request); return request.promise; },
      resetSession: () => reset?.promise || Promise.resolve(), cancelStagedPreview: async () => true,
    }, configsEditor: { refresh() { const request = deferred(); configs.push(request); if (!holdConfigs) request.resolve(); return request.promise; }, reset() {} },
    modelPreview: { clear() { clears++; } } },
  });
  let start = source.indexOf('async function setWorkingPlayer('), end = source.indexOf('function formatBodyFitValue(', start);
  vm.runInContext(source.slice(start, end), context);
  start = source.indexOf('async function recoverFailedReset('); end = source.indexOf('async function selectFile(', start);
  vm.runInContext(source.slice(start, end), context);
  return { context, state, inspections, configs, storage, element,
    reply(index) { inspections[index].resolve({ path: inspections[index].filePath }); },
    holdConfigs() { holdConfigs = true; }, holdBody() { heldBody = deferred(); return heldBody; },
    holdReset() { reset = deferred(); return reset; }, counts: () => ({ nativeTargets, clears }),
  };
}
async function check(label, run) {
  try { await run(setup()); } catch (error) { if (!baseline) throw new Error(label + ': ' + error.message, { cause: error }); failures.push({ label, error: error.message }); }
}
(async () => {
  await check('Latest inspection owns the staged player', async env => {
    const older = env.context.setWorkingPlayer('A'), newer = env.context.setWorkingPlayer('B');
    env.reply(1); await newer; env.reply(0); await older;
    assert.equal(env.state.workingPlayer.path, 'B'); assert.equal(env.state.body.targetPlayer.path, 'B');
    assert.equal(env.storage.get('characterModWorkingPlayer'), 'B');
  });
  await check('Late configs cannot mix old source/body metadata into the new stage', async env => {
    env.holdConfigs();
    const older = env.context.setWorkingPlayer('A', { path: 'source-A' }); env.reply(0); await tick();
    const newer = env.context.setWorkingPlayer('B', { path: 'source-B' }); env.reply(1); await tick();
    env.configs[1].resolve(); await newer; env.configs[0].resolve(); await older;
    assert.equal(env.state.workingSource.path, 'source-B'); assert.equal(env.state.body.targetPlayer.path, 'B');
    assert.equal(env.storage.get('characterModWorkingPlayer'), 'B');
  });
  await check('New Swap prevents late inspection from resurrecting a staged player', async env => {
    const older = env.context.setWorkingPlayer('A'); await env.context.resetFullSwapSession();
    env.reply(0); await older;
    assert.equal(env.state.workingPlayer, null); assert(!env.storage.has('characterModWorkingPlayer'));
  });
  await check('Old inspection errors cannot fail a newer selection', async env => {
    const older = env.context.setWorkingPlayer('A').catch(String), newer = env.context.setWorkingPlayer('B');
    env.reply(1); await newer; env.inspections[0].reject(Error('obsolete inspection'));
    assert.equal(await older, false); assert.equal(env.state.workingPlayer.path, 'B');
  });
  await check('Current inspection errors preserve the existing player', async env => {
    env.state.workingPlayer = { path: 'existing' };
    const pending = env.context.setWorkingPlayer('A'); env.inspections[0].reject(Error('current inspection'));
    await assert.rejects(pending, /current inspection/); assert.equal(env.state.workingPlayer.path, 'existing');
  });
  await check('Player/source/body state is coherent while configs are loading', async env => {
    env.holdConfigs(); const loading = env.context.setWorkingPlayer('A', { path: 'source-A' }, { path: 'baseline-A' });
    env.reply(0); await tick();
    const snapshot = { source: env.state.workingSource?.path, body: env.state.body.targetPlayer?.path, baseline: env.state.workingBaseline?.path };
    env.configs[0].resolve(); await loading;
    assert.deepEqual(snapshot, { source: 'source-A', body: 'A', baseline: 'baseline-A' });
  });
  await check('A new stage invalidates old headband and preview reads immediately', async env => {
    env.holdConfigs(); const loading = env.context.setWorkingPlayer('A'); env.reply(0); await tick();
    const snapshot = { headband: env.state.headband.requestId, loading: env.state.headband.loading,
      configs: env.state.headband.configs.length, preview: env.state.preview.loading, cleared: env.counts().clears };
    env.configs[0].resolve(); await loading;
    assert(snapshot.headband > 1); assert.equal(snapshot.loading, false); assert.equal(snapshot.configs, 0);
    assert.equal(snapshot.preview, false); assert.equal(snapshot.cleared, 1);
  });
  await check('An old view refresh cannot start extra work for the new stage', async env => {
    env.state.activeView = 'body'; const body = env.holdBody();
    const older = env.context.setWorkingPlayer('A'); env.reply(0); await tick();
    env.state.activeView = 'hair'; const newer = env.context.setWorkingPlayer('B'); env.reply(1); await newer;
    body.resolve(); await older; assert.equal(env.counts().nativeTargets, 1);
  });
  await check('Reset invalidates an old configs continuation before cleanup completes', async env => {
    env.holdConfigs(); const older = env.context.setWorkingPlayer('A'); env.reply(0); await tick();
    const reset = env.holdReset(), resetting = env.context.resetFullSwapSession();
    env.configs[0].resolve(); const result = await older; reset.resolve(); await resetting;
    assert.equal(result, false); assert.equal(env.state.workingPlayer, null);
  });
  await check('Failed Reset releases retired edit busy flags and keeps the player', async env => {
    const staging = env.context.setWorkingPlayer('A'); env.reply(0); await staging;
    env.state.body.loading = true; env.state.body.operation = 'save';
    env.state.preview.bodyFitBusy = true; env.state.preview.chestCorrectionBusy = true;
    env.state.headband.loading = true; env.state.headband.applying = true;
    const reset = env.holdReset(), resetting = env.context.resetFullSwapSession(); reset.reject(Error('cleanup failed')); await resetting;
    assert.equal(env.state.workingPlayer.path, 'A'); assert.equal(env.state.body.loading, false); assert.equal(env.state.body.operation, '');
    assert.equal(env.state.preview.bodyFitBusy, false); assert.equal(env.state.preview.chestCorrectionBusy, false);
    assert.equal(env.state.headband.applying, false); assert.equal(env.state.headband.loading, false);
    assert.equal(env.state.selectionBusy, false); assert.equal(env.state.resetting, false);
  });
  await check('Reset blocks a new stage until cleanup settles', async env => {
    const reset = env.holdReset(), resetting = env.context.resetFullSwapSession();
    await assert.rejects(env.context.setWorkingPlayer('B'), /Wait for New Swap/);
    reset.resolve(); await resetting; assert.equal(env.state.resetting, false); assert.equal(env.state.workingPlayer, null);
  });
  await check('Recovery failure still releases Reset controls and permits retry', async env => {
    const staging = env.context.setWorkingPlayer('A'); env.reply(0); await staging;
    env.context.window.configsEditor.refresh = () => { throw Error('recovery failed'); };
    const reset = env.holdReset(), resetting = env.context.resetFullSwapSession(); reset.reject(Error('cleanup failed')); await resetting;
    assert.equal(env.state.resetting, false); assert.equal(env.state.selectionBusy, false); assert.equal(env.state.workingPlayer.path, 'A');
    const retry = env.holdReset(), retrying = env.context.resetFullSwapSession(); retry.resolve(); await retrying;
    assert.equal(env.state.workingPlayer, null); assert.equal(env.state.resetting, false);
  });
  if (baseline) { assert(failures.length > 0); console.log(JSON.stringify({ baseline: true, reproduced: failures }, null, 2)); }
  else console.log('Staging ownership: 12 latest-selection, reset/recovery, retry, metadata and dependent-view cases passed.');
})().catch(error => { console.error(error); process.exitCode = 1; }).finally(finishTest);
