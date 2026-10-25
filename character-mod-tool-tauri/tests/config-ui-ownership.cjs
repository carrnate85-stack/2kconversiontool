const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const esbuild = require('../build-tools/node_modules/esbuild');
const finishTest = require('./test-watchdog.cjs')('config UI ownership');
const source = esbuild.transformSync(fs.readFileSync(path.join(__dirname, '../web/ui/configs-ui.ts'), 'utf8'), { loader: 'ts', target: 'es2022' }).code;
const baseline = process.argv.includes('--baseline'), failures = [];
const tick = () => new Promise(resolve => setTimeout(resolve, 5));
function deferred() { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; }
const result = name => ({ configs: [{ name, items: [], texturePresent: true }], defaultConfig: name, activeConfig: name, items: [] });
function setup() {
  class Element {
    constructor(tag = 'div') { this.tagName = tag; this.children = []; this.queries = new Map(); this.events = new Map(); this.value = ''; this.disabled = false; this.open = false; }
    append(...children) { this.children.push(...children); }
    replaceChildren(...children) { this.children = children; }
    add(child) { this.children.push(child); }
    querySelector(key) { if (!this.queries.has(key)) this.queries.set(key, new Element(key)); return this.queries.get(key); }
    querySelectorAll() { return []; }
    addEventListener(name, callback) { this.events.set(name, callback); }
    closest() { return this.querySelector('parent'); }
    showModal() { this.open = true; } close() { this.open = false; } select() {}
    click() { if (!this.disabled) return this.onclick ? this.onclick() : this.events.get('click')?.({ currentTarget: this }); }
  }
  const elements = new Map(), reads = [], writes = [], dialogs = [];
  const element = id => { if (!elements.has(id)) elements.set(id, new Element()); return elements.get(id); };
  const state = { workingPlayer: { path: 'A', name: 'A' }, preview: { requestId: 0, dirty: false, loading: false, blenderBusy: false } };
  const window = { characterTool: {
    listConfigs(payload) { const request = { payload, ...deferred() }; reads.push(request); return request.promise; },
    changeConfig(payload) { const request = { payload, ...deferred() }; writes.push(request); return request.promise; },
  } };
  const context = vm.createContext({ state, window, $: element, invalidateNativeHairTarget() {},
    loadStagedPreview: async () => {}, setActiveView() {}, setNativeHairType: async () => {},
    document: { body: new Element(), createElement(tag) { const node = new Element(tag); if (tag === 'dialog') dialogs.push(node); return node; }, createTextNode(text) { return { textContent: text }; } },
    Option: function(name, value) { return { textContent: name, value }; },
  });
  const app = esbuild.transformSync(fs.readFileSync(path.join(__dirname,'../web/ui/app.ts'),'utf8'),{loader:'ts',target:'es2022'}).code;
  vm.runInContext(app.slice(app.indexOf('function samePlayerPath('),app.indexOf('function currentPreviewBodyFit(')),context);
  vm.runInContext(source, context);
  return { state, element, reads, writes, dialog: dialogs[0], editor: window.configsEditor,
    async load(player, name) { state.workingPlayer = { path: player, name: player }; const loading = window.configsEditor.refresh(); await tick(); reads.at(-1).resolve(result(name)); await loading; },
    async close() { for (const request of reads) request.resolve(result('cleanup')); for (const request of writes) request.resolve(result('cleanup')); await tick(); },
  };
}
async function check(label, run) {
  const env = setup();
  try { await run(env); } catch (error) { if (!baseline) throw new Error(label + ': ' + error.message, { cause: error }); failures.push({ label, error: error.message }); }
  finally { await env.close(); }
}
(async () => {
  await check('Repeated config readers share one current request', async env => {
    const first = env.editor.refresh(), second = env.editor.refresh(); await tick();
    const count = env.reads.length; for (const request of env.reads) request.resolve(result('BASE')); await Promise.all([first, second]);
    assert.equal(count, 1);
  });
  await check('New player hides old rows and disables edits before its list arrives', async env => {
    await env.load('A', 'BASE-A'); env.state.workingPlayer = { path: 'B', name: 'B' };
    const loading = env.editor.refresh(); await tick();
    const snapshot = { rows: env.element('configsList').children.length, disabled: env.element('newConfig').disabled };
    env.reads.at(-1).resolve(result('BASE-B')); await loading;
    assert.deepEqual(snapshot, { rows: 0, disabled: true });
  });
  await check('An active list read blocks writes using stale editor controls', async env => {
    await env.load('A', 'BASE'); const loading = env.editor.refresh(); await tick();
    const saving = env.element('saveConfig').click(); const count = env.writes.length;
    for (const request of env.writes) request.resolve(result('BASE'));
    env.reads.at(-1).resolve(result('BASE')); await Promise.all([loading, saving]); assert.equal(count, 0);
  });
  await check('An old player error cannot overwrite the new player status', async env => {
    await env.load('A', 'BASE-A'); const saving = env.element('saveConfig').click();
    await env.load('B', 'BASE-B'); env.writes[0].reject(Error('obsolete write failure')); await saving;
    assert.equal(env.editor.active(), 'BASE-B'); assert.equal(env.element('configsStatus').textContent, 'Editing BASE-B');
  });
  await check('Old completion cannot clear a new player mutation busy state', async env => {
    await env.load('A', 'BASE-A'); const older = env.element('saveConfig').click();
    await env.load('B', 'BASE-B'); const newer = env.element('saveConfig').click();
    const accepted = env.writes.length;
    env.writes[0].resolve(result('OLD-A')); await older;
    const disabled = env.element('saveConfig').disabled;
    if (env.writes[1]) env.writes[1].resolve(result('CURRENT-B')); await newer;
    assert.equal(accepted, 2); assert.equal(disabled, true); assert.equal(env.editor.active(), 'CURRENT-B');
  });
  await check('Refresh waits for same-player mutation before sharing a fresh read', async env => {
    await env.load('A', 'BASE'); const saving = env.element('saveConfig').click();
    const refreshing = env.editor.refresh(); await tick(); const before = env.reads.length;
    env.writes[0].resolve(result('RENAMED')); await saving; await tick();
    env.reads.at(-1).resolve(result(before === 1 ? 'RENAMED' : 'BASE')); await refreshing;
    assert.equal(before, 1); assert.equal(env.editor.active(), 'RENAMED');
  });
  await check('Reset rejects an old write even when the same player is staged again', async env => {
    await env.load('A', 'BASE'); const saving = env.element('saveConfig').click();
    env.state.workingPlayer = null; env.editor.reset(); await env.load('A', 'NEW-SESSION');
    env.writes[0].resolve(result('OLD-SESSION')); await saving;
    assert.equal(env.editor.active(), 'NEW-SESSION');
  });
  await check('Old errors cannot replace a newer loading state', async env => {
    await env.load('A', 'BASE'); const saving = env.element('saveConfig').click();
    env.state.workingPlayer = { path: 'B' }; const loading = env.editor.refresh(); await tick();
    env.writes[0].reject(Error('old failure')); await saving;
    const status = env.element('configsStatus').textContent;
    env.reads.at(-1).resolve(result('BASE-B')); await loading;
    assert.equal(status, 'Loading configurations...');
  });
  await check('Player changes close an old config dialog', async env => {
    await env.load('A', 'BASE'); env.element('newConfig').click();
    assert.equal(env.dialog.open, true); await env.load('B', 'BASE-B'); assert.equal(env.dialog.open, false);
  });
  await check('Late dialog submission cannot enable or close a new busy dialog', async env => {
    await env.load('A', 'BASE-A'); env.element('newConfig').click();
    env.dialog.querySelector('input').value = 'NEW-A'; env.dialog.querySelector('select').value = 'BASE-A';
    const old = env.dialog.querySelector('form').onsubmit({ preventDefault() {} });
    await env.load('B', 'BASE-B'); env.element('newConfig').click();
    env.dialog.querySelector('input').value = 'NEW-B'; env.dialog.querySelector('select').value = 'BASE-B';
    const current = env.dialog.querySelector('form').onsubmit({ preventDefault() {} });
    const accepted = env.writes.length;
    env.writes[0].resolve(result('NEW-A')); await old;
    const busy = env.dialog.querySelector('button[type="submit"]').disabled;
    if (env.writes[1]) env.writes[1].resolve(result('NEW-B')); await current;
    assert.equal(accepted, 2); assert.equal(busy, true); assert.equal(env.editor.active(), 'NEW-B');
  });
  await check('Staging guard tracks edited controls and releases after save', async env => {
    await env.load('A', 'BASE');
    assert.equal(env.editor.stagingBlockReason(), '');
    env.element('configHair').value = 'new-hair';
    assert.match(env.editor.stagingBlockReason(), /Save configuration/);
    const saving = env.element('saveConfig').click();
    assert.match(env.editor.stagingBlockReason(), /operation/);
    env.writes.at(-1).resolve(result('BASE')); await saving;
    assert.equal(env.editor.stagingBlockReason(), '');
  });
  if (baseline) { assert(failures.length > 0); console.log(JSON.stringify({ baseline: true, reproduced: failures }, null, 2)); }
  else console.log('Config UI ownership: 11 shared-read, mutation, reset, loading, staging guard and dialog cases passed.');
})().catch(error => { console.error(error); process.exitCode = 1; }).finally(finishTest);
