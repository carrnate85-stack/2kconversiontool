const assert = require('node:assert/strict');
const path = require('node:path');
const vm = require('node:vm');
const finish = require('./test-watchdog.cjs')('player preview freshness');
const esbuild = require('../build-tools/node_modules/esbuild');
const baseline = process.argv.includes('--baseline'), failures = [];
const source = esbuild.buildSync({ entryPoints: [path.join(__dirname, '../web/tauri-bridge.ts')],
  bundle: true, write: false, platform: 'browser', format: 'iife', target: 'es2022' }).outputFiles[0].text;
const tick = () => new Promise(resolve => setTimeout(resolve, 10));
const methods = ['saveBodyFit', 'applyChestFix', 'applyPreviewChestCorrection', 'applyHeadband', 'installNativeHair', 'changeConfig'];
const key = target => String(target).replaceAll('/', '\\').toLowerCase();
function setup() {
  const versions = new Map(), edits = [], workers = [], holds = [], releases = [];
  let holdNext = false, holdRelease = false, sameFingerprint = false, decodes = 0, preparations = 0, event;
  const window = { __TAURI__: { event: { listen: async (_, handler) => { event = handler; } }, core: {
    invoke: async (_, { command, payload }) => {
      if (command === 'runtime-info') return {};
      if (command === 'blender-snapshot-prepare') return { snapshotId: payload.handoffId };
      if (command === 'blender-snapshot-release') return true;
      if (command === 'native-preview-resources') {
        preparations++;
        return { filePath: payload.target, resourceLease: 'fixture', fingerprint: sameFingerprint ? 'same' : 'unchanged:' + key(payload.target), options: { companionPaths: [] },
          player: {}, activeItems: [], warnings: [] };
      }
      if (['body-fit-save','body-chest-fix','preview-chest-fix','headband-apply','native-hair-install','configs-change'].includes(command)) {
        return new Promise((resolve, reject) => edits.push({ command, target: payload.target, reject,
          resolve() { const target = key(payload.target); versions.set(target, (versions.get(target) || 0) + 1); resolve({}); } }));
      }
      if (command === 'release-preview-resources' && holdRelease) return new Promise(resolve => releases.push(resolve));
      if (['cancel-preview','release-preview-resources'].includes(command)) return true;
      if (command === 'blender-open') {
        queueMicrotask(() => event({ payload: { type: 'blender', state: 'opened', target: payload.target, handoffId: payload.handoffId } }));
        return true;
      }
      if (command === 'run-swap') {
        const target = key(payload.output); versions.set(target, (versions.get(target) || 0) + 1);
        return { output: payload.output };
      }
      throw Error('Unexpected command: ' + command);
    },
  } } };
  class Worker {
    constructor() { workers.push(this); }
    postMessage(message) {
      this.message = message; decodes++;
      const complete = () => this.onmessage({ data: { id: message.id,
        result: { meshes: [], textures: [], stats: {}, skipped: [], marker: versions.get(key(message.payload.filePath)) || 0 } } });
      if (holdNext) { holdNext = false; holds.push(complete); }
      else queueMicrotask(complete);
    }
    terminate() { this.terminated = true; }
  }
  vm.runInNewContext(source, { window, Worker, console, ArrayBuffer, structuredClone,
    setTimeout, setInterval, clearInterval, crypto: { randomUUID: () => 'freshness-handoff' } });
  return { api: window.characterTool, edits, holds, workers, releases, get decodes() { return decodes; },
    get preparations() { return preparations; }, hold() { holdNext = true; },
    holdRelease() { holdRelease = true; }, collide() { sameFingerprint = true; },
    event: row => event({ payload: row }), advance(target) { versions.set(key(target),(versions.get(key(target)) || 0)+1); } };
}
async function check(label, run) {
  try { await run(); } catch(error) { if (!baseline) throw new Error(label + ': ' + error.message, { cause: error }); failures.push({label,error:error.message}); }
}
(async () => {
  for (const method of methods) await check(method + ' invalidates same-player cached output', async () => {
    const env = setup(); await env.api.loadStagedPreview({ target: 'C:/Player/A.iff' });
    const edit = env.api[method]({ target: 'c:\\player\\a.iff' }); await tick(); env.edits[0].resolve(); await edit;
    const result = await env.api.loadStagedPreview({ target: 'C:/Player/A.iff' });
    assert.equal(result.marker, 1); assert.equal(env.decodes, 2);
  });
  await check('Unrelated player caches survive a committed edit', async () => {
    const env = setup(); await env.api.loadStagedPreview({ target: 'A' }); await env.api.loadStagedPreview({ target: 'B' });
    const edit = env.api.saveBodyFit({ target: 'A' }); await tick(); env.edits[0].resolve(); await edit;
    const result = await env.api.loadStagedPreview({ target: 'B' });
    assert.equal(result.stats.previewCacheHit, true); assert.equal(env.decodes, 2);
  });
  await check('Failed writes preserve the existing cached preview', async () => {
    const env = setup(); await env.api.loadStagedPreview({ target: 'A' });
    const edit = env.api.saveBodyFit({ target: 'A' }).catch(String); await tick(); env.edits[0].reject(Error('Controlled failure')); await edit;
    const result = await env.api.loadStagedPreview({ target: 'A' });
    assert.equal(result.stats.previewCacheHit, true); assert.equal(env.decodes, 1);
  });
  await check('In-flight same-player decode cannot publish after an edit', async () => {
    const env = setup(); env.hold();
    const loading = env.api.loadStagedPreview({ target: 'A' }).catch(String); await tick();
    const edit = env.api.saveBodyFit({ target: 'A' }); await tick(); env.edits[0].resolve(); await edit;
    env.holds[0](); assert.match(String(await loading), /cancelled/);
    const result = await env.api.loadStagedPreview({ target: 'A' }); assert.equal(result.marker, 1);
  });
  await check('An unrelated edit cannot reject the current player read', async () => {
    const env = setup(); env.hold(); const loading = env.api.loadStagedPreview({ target: 'A' }); await tick();
    const edit = env.api.saveBodyFit({ target: 'B' }); await tick(); env.edits[0].resolve(); await edit;
    env.holds[0](); assert.equal((await loading).marker, 0);
  });
  await check('A new preview waits for its player write to settle', async () => {
    const env = setup(), edit = env.api.saveBodyFit({ target: 'A' }); await tick();
    const loading = env.api.loadStagedPreview({ target: 'A' }); await tick();
    assert.equal(env.preparations, 0); env.edits[0].resolve(); await edit;
    assert.equal((await loading).marker, 1);
  });
  await check('An unrelated write does not impose a bridge read barrier', async () => {
    const env = setup(), edit = env.api.saveBodyFit({ target: 'B' }); await tick();
    assert.equal((await env.api.loadStagedPreview({ target: 'A' })).marker, 0);
    env.edits[0].resolve(); await edit;
  });
  await check('Only the matching Blender return invalidates its player', async () => {
    const env = setup(); await env.api.loadStagedPreview({ target: 'A' }); await env.api.loadStagedPreview({ target: 'B' });
    await env.api.openPreviewInBlender({ target: 'A' });
    env.event({ type:'blender',state:'complete',target:'A',handoffId:'retired-handoff' });
    assert.equal((await env.api.loadStagedPreview({target:'A'})).stats.previewCacheHit,true);
    env.advance('A'); env.event({ type:'blender',state:'complete',target:'A',handoffId:'freshness-handoff' });
    assert.equal((await env.api.loadStagedPreview({target:'B'})).stats.previewCacheHit,true);
    assert.equal((await env.api.loadStagedPreview({target:'A'})).marker,1);
  });
  await check('An overwritten Full Swap output retires its old preview', async () => {
    const env = setup(); await env.api.loadStagedPreview({target:'A'});
    const complete = new Promise(resolve => env.api.onJobComplete(resolve));
    await env.api.startSwap({output:'A'}); await complete;
    assert.equal((await env.api.loadStagedPreview({target:'A'})).marker,1);
  });
  await check('A same-player edit during resource release retires the pending result', async () => {
    const env = setup(); env.holdRelease();
    const loading = env.api.loadStagedPreview({target:'A'}).catch(String); await tick();
    assert.equal(env.releases.length,1);
    const edit = env.api.saveBodyFit({target:'A'}); await tick(); env.edits[0].resolve(); await edit;
    env.releases[0](true); assert.match(String(await loading), /cancelled/);
  });
  await check('Even identical fingerprints cannot reuse another player result', async () => {
    const env = setup(); env.collide(); env.advance('A');
    assert.equal((await env.api.loadStagedPreview({target:'A'})).marker,1);
    assert.equal((await env.api.loadStagedPreview({target:'B'})).marker,0);
    assert.equal(env.decodes,2);
  });
  if (baseline) { assert(failures.length); console.log(JSON.stringify({baseline:true,failures}, null, 2)); }
  else console.log('Player preview freshness: six mutations, same-player invalidation, unrelated/failed cache retention and read/write barriers passed.');
})().catch(error => { console.error(error); process.exitCode=1; }).finally(finish);
