const assert = require('node:assert/strict');
const finishTest = require('./test-watchdog.cjs')('preview fallback diagnostics');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const esbuild = require('../build-tools/node_modules/esbuild');
const root = path.resolve(__dirname, '..');
const baseline = process.argv.includes('--baseline');
const source = baseline ? fs.readFileSync(path.join(root, 'frontend/tauri-bridge.js'), 'utf8')
  : esbuild.buildSync({ entryPoints: [path.join(root, 'web/tauri-bridge.ts')], bundle: true,
    write: false, platform: 'browser', format: 'iife', target: 'es2022' }).outputFiles[0].text;
const tick = () => new Promise(resolve => setTimeout(resolve, 5));
const geometry = () => ({ meshes: [{ name: 'sentinel', positions: new Float32Array([1, 2, 3]) }],
  textures: [{ name: 'face', bytes: 'AQID', binding: { target: 'face', channel: 'color' }, range: { min: [0], max: [1] } }], stats: {}, skipped: [] });
function setup(options = {}) {
  let nativeError = options.nativeError || 'NBA2K27.exe unavailable', hold = false, release, workers = 0, fallbackCalls = 0;
  const warnings = ['Original optional dependency warning'];
  const resources = native => ({ native, filePath: 'player', fingerprint: 'same-geometry', warnings,
    options: { companionPaths: [] }, player: {}, activeItems: [], name: 'Fixture' });
  const window = { __TAURI__: { event: { listen: async () => {} }, core: { invoke: async (_, { command }) => {
    if (command === 'runtime-info') return {};
    if (command === 'native-preview-resources') { if (!options.nativeSuccess) throw Error(nativeError); return resources(true); }
    if (command === 'preview-resources') {
      fallbackCalls++;
      if (options.fallbackError) throw Error(options.fallbackError);
      if (hold) return new Promise(resolve => { release = () => resolve(resources(false)); });
      return resources(false);
    }
    if (command === 'cancel-preview') return true;
    throw Error('Unexpected fixture command: ' + command);
  } } } };
  class Worker {
    constructor() { workers++; }
    postMessage(message) { queueMicrotask(() => this.onmessage({ data: { id: message.id, result: geometry() } })); }
    terminate() {}
  }
  vm.runInNewContext(source, { window, Worker, console, structuredClone, ArrayBuffer, setInterval, clearInterval, setTimeout });
  return { api: window.characterTool, warnings, setReason(value) { nativeError = value; },
    hold() { hold = true; }, release() { hold = false; release(); }, counts: () => ({ workers, fallbackCalls }) };
}
const failures = [];
async function check(label, run, options) {
  try { await run(setup(options)); }
  catch (error) { if (!baseline) throw new Error(label + ': ' + error.message, { cause: error }); failures.push({ label, error: error.message }); }
}
(async () => {
  await check('Fallback reason accompanies original warnings without changing payloads', async env => {
    const value = await env.api.loadStagedPreview({ target: 'player' });
    assert(value.skipped.some(warning => warning.includes('NBA2K27.exe unavailable')));
    assert(value.skipped.includes(env.warnings[0])); assert.equal(env.warnings.length, 1);
    assert.equal(value.stats.nativeAssetReader, false);
    assert.deepEqual(Array.from(value.meshes[0].positions), [1, 2, 3]);
    assert.equal(value.textures[0].bytes, 'AQID'); assert.equal(value.textures[0].binding.target, 'face');
  });
  await check('Cached geometry gets the current preparation reason, not an old one', async env => {
    const first = await env.api.loadStagedPreview({ target: 'player' });
    env.setReason('Oodle decoder unavailable');
    const second = await env.api.loadStagedPreview({ target: 'player' });
    assert.equal(second.stats.previewCacheHit, true);
    assert(first.skipped.some(warning => warning.includes('NBA2K27.exe')));
    assert(second.skipped.some(warning => warning.includes('Oodle decoder')));
    assert(!second.skipped.some(warning => warning.includes('NBA2K27.exe')));
  });
  await check('Both preparation failures preserve both reasons', async env => {
    await assert.rejects(env.api.loadStagedPreview({ target: 'player' }), error =>
      /NBA2K27.exe unavailable/.test(error.message) && /Compatibility extractor unavailable/.test(error.message) && Boolean(error.cause));
  }, { fallbackError: 'Compatibility extractor unavailable' });
  await check('Diagnostic size stays bounded', async env => {
    const value = await env.api.loadStagedPreview({ target: 'player' });
    const prefix = 'Native resource fallback: ';
    const warnings = value.skipped.filter(warning => warning.startsWith(prefix));
    assert.equal(warnings.length, 1); assert(warnings[0].length <= prefix.length + 2000);
  }, { nativeError: 'missing decoder '.repeat(1000) });
  await check('Cancelled preparation still rejects and cannot warm the preview cache', async env => {
    env.hold(); const loading = env.api.loadStagedPreview({ target: 'player' }).catch(String); await tick();
    const stopping = env.api.cancelStagedPreview(); env.release(); await stopping;
    assert.match(await loading, /cancelled/); assert.equal(env.counts().workers, 0);
    const value = await env.api.loadStagedPreview({ target: 'player' });
    assert(!value.stats.previewCacheHit); assert.equal(env.counts().workers, 1);
  });
  await check('Successful native preparation has no fallback diagnostic', async env => {
    const value = await env.api.loadStagedPreview({ target: 'player' });
    assert.equal(value.stats.nativeAssetReader, true); assert.equal(env.counts().fallbackCalls, 0);
    assert.equal(value.skipped.length, 1); assert.equal(value.skipped[0], env.warnings[0]);
  }, { nativeSuccess: true });
  if (baseline) { assert(failures.length > 0); console.log(JSON.stringify({ baseline: true, reproduced: failures }, null, 2)); }
  else console.log('Preview fallback diagnostics: reasons, original warnings/payloads, cache freshness, bounds, cancellation and native success passed.');
})().catch(error => { console.error(error); process.exitCode = 1; }).finally(finishTest);
