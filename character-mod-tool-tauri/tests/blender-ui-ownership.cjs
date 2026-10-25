const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const esbuild = require('../build-tools/node_modules/esbuild');
const finishTest = require('./test-watchdog.cjs')('Blender UI ownership');
const source = esbuild.transformSync(fs.readFileSync(path.join(__dirname, '../web/ui/app.ts'), 'utf8'), { loader: 'ts', target: 'es2022' }).code;
const baseline = process.argv.includes('--baseline'), failures = [];
function deferred() { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; }
function setup() {
  const requests = [], statuses = [], logs = [], elements = new Map(); let handler, reloads = 0, heldReload;
  const state = { workingPlayer: { path: 'C:/A.iff', name: 'A' }, workingGeneration: 1, preview: { loadedPath: 'C:/A.iff', loading: false, blenderBusy: false, blenderSessionId: '', jersey: false, dirty: false } };
  const element = id => { if (!elements.has(id)) elements.set(id, { textContent: '', disabled: false }); return elements.get(id); };
  const context = vm.createContext({ state, $: element, samePlayerPath: (a,b) => String(a || '').replaceAll('/', '\\').toLowerCase() === String(b || '').replaceAll('/', '\\').toLowerCase(),
    updateStagePlayerControls() {}, updatePreviewBodyFitActions() {}, addLog: (...args) => logs.push(args), setPreviewStatus: (...args) => statuses.push(args),
    loadStagedPreview: () => { reloads++; return heldReload?.promise || Promise.resolve(); },
    window: { configsEditor: { active: () => 'BASE', renderPreviewSelector() {} }, characterTool: {
      openPreviewInBlender(payload) { const request = { payload, ...deferred() }; requests.push(request); return request.promise; },
      onBlenderEvent(callback) { handler = callback; },
    } },
  });
  vm.runInContext(source.slice(source.indexOf('function invalidatePlayerRender('),source.indexOf('function currentPreviewBodyFit(')),context);
  let start = source.indexOf('const previewBlenderSessions');
  if (start < 0) start = source.indexOf('function renderPreviewControls(');
  vm.runInContext(source.slice(start, source.indexOf('async function applyPreviewChestCorrection(', start)), context);
  start = source.indexOf('window.characterTool.onBlenderEvent(');
  vm.runInContext(source.slice(start, source.indexOf('\n}', start)), context);
  return { context, state, requests, statuses, logs, element,
    stage(path) { state.workingPlayer = path ? { path, name: path } : null; state.workingGeneration++; state.preview.loadedPath = path || ''; if (context.syncPreviewBlenderState) context.syncPreviewBlenderState(); },
    event(event) { return handler({ target: 'C:/A.iff', sessionId: 'A-session', handoffId: 'A-handoff', message: 'Blender event', ...event }); },
    reply(index, result = {}) { requests[index].resolve({ sessionId: index ? 'B-session' : 'A-session', handoffId: index ? 'B-handoff' : 'A-handoff', ...result }); },
    holdReload() { heldReload = deferred(); return heldReload; }, count: () => reloads,
  };
}
async function check(label, run) { try { await run(setup()); } catch (error) { if (!baseline) throw new Error(label + ': ' + error.message, { cause: error }); failures.push({ label, error: error.message }); } }
(async () => {
  await check('Switching players releases the visible busy flag', async e => {
    const task = e.context.openPreviewInBlender(); e.stage('C:/B.iff'); const busy = e.state.preview.blenderBusy;
    e.reply(0); await task; assert.equal(busy, false); assert.equal(e.state.preview.blenderSessionId, '');
    assert.equal(e.element('previewBlenderStatus').textContent, 'Ready');
  });
  await check('Two players can open independent handoffs', async e => {
    const first = e.context.openPreviewInBlender(); e.stage('C:/B.iff'); const second = e.context.openPreviewInBlender();
    const count = e.requests.length; e.reply(0); if (e.requests[1]) e.reply(1); await Promise.all([first, second]);
    assert.equal(count, 2); assert.equal(e.state.preview.blenderSessionId, 'B-session');
  });
  await check('Returning to a player restores its handoff', async e => {
    const task = e.context.openPreviewInBlender(); e.reply(0); await task; e.stage('C:/B.iff'); e.stage('c:\\a.iff');
    assert.equal(e.state.preview.blenderBusy, true); assert.equal(e.state.preview.blenderSessionId, 'A-session');
  });
  await check('Old opening error cannot clear another player busy state', async e => {
    const first = e.context.openPreviewInBlender(); e.stage('C:/B.iff'); const second = e.context.openPreviewInBlender();
    e.requests[0].reject(Error('A failed')); await first; const busy = e.state.preview.blenderBusy, title = e.element('previewBlenderStatus').textContent;
    if (e.requests[1]) e.reply(1); await second;
    assert.equal(busy, true); assert.equal(title, 'Opening Blender 5.1');
  });
  await check('Inactive completion is recorded without reloading the current player', async e => {
    const task = e.context.openPreviewInBlender(); e.reply(0); await task; e.stage('C:/B.iff');
    await e.event({ state: 'complete' }); assert.equal(e.count(), 0); e.stage('C:/A.iff'); assert.equal(e.state.preview.blenderBusy, false);
  });
  await check('Inactive returning status is restored on selection', async e => {
    const task = e.context.openPreviewInBlender(); e.reply(0); await task; e.stage('C:/B.iff');
    await e.event({ state: 'returning' }); e.stage('C:/A.iff');
    assert.equal(e.element('previewBlenderStatus').textContent, 'Applying Blender changes');
  });
  await check('Terminal event before open resolution cannot resurrect busy state', async e => {
    const task = e.context.openPreviewInBlender(); await e.event({ state: 'opened' }); await e.event({ state: 'complete' });
    e.reply(0); await task; assert.equal(e.state.preview.blenderBusy, false); assert.equal(e.state.preview.blenderSessionId, '');
  });
  await check('Returning event before open resolution cannot regress to editing', async e => {
    const task = e.context.openPreviewInBlender(); await e.event({ state: 'opened' }); await e.event({ state: 'returning' });
    e.reply(0); await task; assert.equal(e.element('previewBlenderStatus').textContent, 'Applying Blender changes');
  });
  await check('Old handoff cannot close a newer handoff on the same path', async e => {
    const first = e.context.openPreviewInBlender(); e.reply(0); await first; await e.event({ state: 'closed' });
    const second = e.context.openPreviewInBlender(); await e.event({ state: 'opened', handoffId: 'B-handoff', sessionId: 'B-session' }); e.reply(1); await second;
    await e.event({ state: 'closed' }); assert.equal(e.state.preview.blenderBusy, true); assert.equal(e.state.preview.blenderSessionId, 'B-session');
  });
  await check('Reset view does not lose a backend-owned live handoff', async e => {
    const task = e.context.openPreviewInBlender(); e.reply(0); await task; e.stage(null); e.stage('C:/A.iff');
    assert.equal(e.state.preview.blenderBusy, true); assert.equal(e.state.preview.blenderSessionId, 'A-session');
  });
  if (baseline) { assert(failures.length); console.log(JSON.stringify({ baseline: true, reproduced: failures }, null, 2)); }
  else console.log('Blender UI ownership: 10 target, opening, event ordering, session and reset cases passed.');
})().catch(error => { console.error(error); process.exitCode = 1; }).finally(finishTest);
