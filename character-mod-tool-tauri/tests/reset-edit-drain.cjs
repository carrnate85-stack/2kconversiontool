const assert = require('node:assert/strict');
const path = require('node:path');
const vm = require('node:vm');
const esbuild = require('../build-tools/node_modules/esbuild');
const finishTest = require('./test-watchdog.cjs')('reset edit drain');
const source = esbuild.buildSync({ entryPoints: [path.join(__dirname, '../web/tauri-bridge.ts')], bundle: true, write: false, platform: 'browser', format: 'iife', target: 'es2022' }).outputFiles[0].text;
const tick = () => new Promise(resolve => setTimeout(resolve, 10));
function setup() {
  const calls = []; let failCancel = true;
  const window = { __TAURI__: { event: { listen: async () => {} }, core: { invoke: async (_name, { command }) => {
    if (command === 'runtime-info') return { smoke: false };
    if (command === 'cancel-preview') { if (failCancel) throw Error('cleanup failed'); return true; }
    if (command === 'reset') return true;
    return new Promise((resolve, reject) => calls.push({ command, resolve, reject }));
  } } } };
  vm.runInNewContext(source, { window, console, structuredClone, setTimeout, setInterval, clearInterval, crypto: { randomUUID: () => 'fixture' } });
  return { api: window.characterTool, calls, retry() { failCancel = false; } };
}
(async () => {
  for (const method of ['saveBodyFit', 'applyChestFix', 'applyPreviewChestCorrection', 'applyHeadband', 'installNativeHair', 'changeConfig']) {
    const e = setup(); const writing = e.api[method]({ target: 'A' }).catch(error => String(error)); await tick();
    let settled = false; const resetting = e.api.resetSession().catch(error => String(error)).then(value => { settled = true; return value; });
    await tick(); const early = settled;
    e.calls[0].resolve({}); await writing; assert.match(await resetting, /cleanup failed/);
    assert.equal(early, false, method + ' was not drained after failed cancellation');
    e.retry(); assert.equal(await e.api.resetSession(), true);
  }
  const e = setup(); const writing = e.api.saveBodyFit({ target: 'A' }).catch(String); await tick();
  const resetting = e.api.resetSession().catch(String); await tick(); e.calls[0].reject(Error('write failed'));
  assert.match(await writing, /write failed/); assert.match(await resetting, /cleanup failed/);
  console.log('Reset edit drain: six mutation types, failed writes, original cleanup errors and retry passed.');
})().catch(error => { console.error(error); process.exitCode = 1; }).finally(finishTest);
