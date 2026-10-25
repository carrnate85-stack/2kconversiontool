const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const esbuild = require('../build-tools/node_modules/esbuild');
const source = esbuild.transformSync(fs.readFileSync(path.join(__dirname, '../web/ui/app.ts'), 'utf8'),
  { loader: 'ts', target: 'es2022' }).code;
const start = source.includes('function nativeHairTargetKey(')
  ? source.indexOf('function nativeHairTargetKey(') : source.indexOf('async function refreshNativeHairTarget(');
const end = source.indexOf('function setPreviewStatus(', start);
assert(start >= 0 && end > start);
const baseline = process.argv.includes('--baseline');
const failures = [];
function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
function setup() {
  const elements = new Map();
  const requests = [], installs = [], logs = [];
  let pendingInstall = null;
  let config = 'FIRST';
  const state = {
    workingPlayer: { path: 'C:/players/first.iff' }, preview: { dirty: false },
    nativeHair: { assetType: 'hair', slots: [], loading: false, targetLoading: false,
      catalogLoaded: true, catalogSourceKey: '',
      targetRequestId: 0, targetContext: '', displayedPreviewKey: 'source' },
  };
  const element = id => {
    if (!elements.has(id)) elements.set(id, { value: '', disabled: false, textContent: '', className: '',
      replaceChildren(...options) { this.options = options; this.value = options[0]?.value || ''; },
      add(option) { this.options.push(option); if (this.options.length === 1) this.value = option.value; },
    });
    return elements.get(id);
  };
  const asset = { id: 'source-hair', key: 'source' };
  const context = vm.createContext({
    state, $: element, Option: function(label, value) { return { label, value }; },
    nativeHairSourceKey: () => '',
    selectedNativeHair: () => asset, basename: value => value.split('/').pop(), addLog: (...args) => logs.push(args),
    setNativeHairPreviewState() {}, renderWorkingPlayer() {}, renderSelectedNativeHair() {},
    window: { characterTool: {
      loadNativeHairTarget(payload) { const request = { payload, ...deferred() }; requests.push(request); return request.promise; },
      async installNativeHair(payload) {
        installs.push(payload);
        if (pendingInstall) return pendingInstall.promise;
        return { sourceKey: 'source', targetKey: payload.targetKey, outputs: [], backups: [] };
      },
    }, configsEditor: { active: () => config, async refresh() {} } },
  });
  vm.runInContext(source.slice(source.indexOf('function samePlayerPath('),source.indexOf('function currentPreviewBodyFit(')),context);
  vm.runInContext(source.slice(start, end), context);
  const reply = (index, key) => requests[index].resolve({ slots: [{ key, label: key }], targetPng: key });
  return { context, state, element, requests, installs, logs, reply, setConfig(value) { config = value; },
    deferInstall() { pendingInstall = deferred(); return pendingInstall; } };
}
async function check(label, run) {
  try { await run(); } catch (error) {
    if (!baseline) throw new Error(label + ': ' + error.message, { cause: error });
    failures.push({ label, error: error.message });
  }
}
(async () => {
  for (const change of ['player', 'config', 'category']) {
    await check('Out-of-order ' + change, async () => {
      const env = setup();
      const first = env.context.refreshNativeHairTarget();
      if (change === 'player') env.state.workingPlayer = { path: 'C:/players/second.iff' };
      if (change === 'config') env.setConfig('SECOND');
      if (change === 'category') env.state.nativeHair.assetType = 'facialhair';
      const second = env.context.refreshNativeHairTarget();
      env.reply(1, 'CURRENT'); await second;
      env.reply(0, 'OBSOLETE'); await first;
      assert.equal(env.element('nativeHairSlot').value, 'CURRENT', 'An obsolete response replaced the current slots');
      assert.equal(env.element('installNativeHair').disabled, false);
    });
  }
  await check('Obsolete error', async () => {
    const env = setup();
    const first = env.context.refreshNativeHairTarget();
    const second = env.context.refreshNativeHairTarget();
    env.reply(1, 'CURRENT'); await second;
    env.requests[0].reject(new Error('old failure')); await first;
    assert.equal(env.element('nativeHairSlot').disabled, false, 'An obsolete failure disabled the current picker');
    assert(!env.element('nativeHairStatus').textContent.includes('old failure'));
  });
  await check('Changed selection without replacement read', async () => {
    const env = setup();
    const first = env.context.refreshNativeHairTarget();
    env.setConfig('SECOND');
    env.reply(0, 'OBSOLETE'); await first;
    assert.equal(env.state.nativeHair.slots.length, 0, 'Slots from the prior config were published');
    env.element('nativeHairSlot').value = 'OBSOLETE';
    await env.context.installNativeHair();
    assert.equal(env.installs.length, 0, 'Install accepted a slot from a prior config');
  });
  await check('Reset while pending', async () => {
    const env = setup();
    const first = env.context.refreshNativeHairTarget();
    env.state.workingPlayer = null;
    await env.context.refreshNativeHairTarget();
    env.reply(0, 'OBSOLETE'); await first;
    assert.equal(env.state.nativeHair.slots.length, 0);
    assert.equal(env.element('nativeHairSlot').disabled, true);
    assert.equal(env.element('installNativeHair').disabled, true);
  });
  await check('Install waits for slots', async () => {
    const env = setup();
    const first = env.context.refreshNativeHairTarget();
    env.element('nativeHairSlot').value = 'FORGED';
    await env.context.installNativeHair();
    assert.equal(env.installs.length, 0, 'Install ran before target slots were loaded');
    env.reply(0, 'VALID'); await first;
    await env.context.installNativeHair();
    assert.equal(env.installs.length, 1);
    assert.equal(env.installs[0].config, 'FIRST');
    assert.equal(env.installs[0].targetKey, 'VALID');
  });
  await check('Unknown slot rejected and failure recoverable', async () => {
    const env = setup();
    const first = env.context.refreshNativeHairTarget();
    env.requests[0].reject(new Error('current failure')); await first;
    assert.equal(env.element('nativeHairSlot').disabled, true);
    assert.equal(env.element('installNativeHair').disabled, true);
    const second = env.context.refreshNativeHairTarget();
    env.reply(1, 'VALID'); await second;
    env.element('nativeHairSlot').value = 'FORGED';
    env.context.updateNativeHairInstallState();
    await env.context.installNativeHair();
    assert.equal(env.element('installNativeHair').disabled, true);
    assert.equal(env.installs.length, 0);
    env.element('nativeHairSlot').value = 'VALID';
    env.context.updateNativeHairInstallState();
    assert.equal(env.element('installNativeHair').disabled, false);
  });
  for (const outcome of ['success', 'failure']) await check('Install completion after selection changed: ' + outcome, async () => {
    const env = setup();
    const first = env.context.refreshNativeHairTarget();
    env.reply(0, 'VALID'); await first;
    const pending = env.deferInstall();
    const installing = env.context.installNativeHair();
    env.state.workingPlayer = { path: 'C:/players/second.iff' };
    env.setConfig('SECOND');
    env.element('nativeHairStatus').textContent = 'Current player status';
    if (outcome === 'success') pending.resolve({ sourceKey: 'source', targetKey: 'VALID', outputs: ['old-output.iff'], backups: [] });
    else pending.reject(new Error('Old install failure'));
    await installing;
    assert.equal(env.element('nativeHairStatus').textContent, 'Current player status');
    assert.equal(env.state.preview.dirty, false);
    assert.equal(env.state.nativeHair.loading, false);
    assert.equal(env.element('installNativeHair').disabled, true);
    assert.equal(env.logs.length, 1, 'Completed work or error should still be logged');
  });
  if (baseline) {
    assert(failures.length > 0, 'Expected the uncorrected implementation to reproduce a failure');
    console.log(JSON.stringify({ baseline: true, reproduced: failures }, null, 2));
  } else console.log('Hair target ownership: player/config/category ordering, stale errors, reset, install guards and retry passed.');
})().catch(error => { console.error(error); process.exitCode = 1; });
