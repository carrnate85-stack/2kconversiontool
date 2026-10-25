const assert = require('node:assert/strict');
const finishTest = require('./test-watchdog.cjs')('bridge lifecycle');
const vm = require('node:vm');
const path = require('node:path');
const esbuild = require('../build-tools/node_modules/esbuild');

const source = esbuild.buildSync({ entryPoints: [path.join(__dirname, '../web/tauri-bridge.ts')], bundle: true, write: false, platform: 'browser', format: 'iife', target: 'es2022' }).outputFiles[0].text;
const flush = () => new Promise(resolve => setTimeout(resolve, 10));
function deferred() { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; }
function setup() {
  const calls = [], listenerErrors = [];
  let event, id = 0;
  const window = { __TAURI__: {
    event: { listen: async (_name, callback) => { event = callback; } },
    core: { invoke: async (_name, { command, payload }) => {
      if (command === 'runtime-info') return { smoke: false };
      if (command === 'blender-snapshot-prepare') return { snapshotId: payload.handoffId };
      if (command === 'blender-snapshot-release') return true;
      if (command === 'cancel-command') { calls.push({ command, payload }); return true; }
      const task = deferred(); calls.push({ command, payload, ...task }); return task.promise;
    } }
  } };
  vm.runInNewContext(source, { window, crypto: { randomUUID: () => `job-${++id}` }, console: { error: (...args) => listenerErrors.push(args) }, structuredClone, setInterval, clearInterval, setTimeout });
  return { api: window.characterTool, calls, listenerErrors, emit: value => event({ payload: value }) };
}

(async () => {
  const first = setup(); await flush();
  let restarted;
  first.api.onJobComplete(() => { throw new Error('Broken consumer'); });
  first.api.onJobComplete(() => { restarted = first.api.startSwap({}); });
  const initial = await first.api.startSwap({}); await flush();
  assert.equal(await first.api.cancelSwap('not-the-current-job'), false);
  first.calls[0].resolve({ output: 'first.iff' }); await flush();
  assert.equal((await restarted).jobId, 'job-2', 'Completion subscribers must be able to start another swap');
  for (let attempt = 0; attempt < 100 && !first.calls.some(row => row.command === 'run-swap' && row.payload._cmtJobId === 'job-2'); attempt++) await flush();
  assert(first.calls.some(row => row.command === 'run-swap' && row.payload._cmtJobId === 'job-2'), 'Restarted conversion must reach native invocation before testing registration cancellation');
  assert.equal(first.listenerErrors.length, 1, 'Subscriber failures should be isolated');
  assert.equal(initial.jobId, 'job-1');
  let failure;
  first.api.onJobFailed(value => { failure = value; });
  const cancellation = first.api.cancelSwap('job-2'); await flush();
  assert.equal(first.calls.filter(row => row.command === 'cancel-command').length, 0, 'Cancellation must wait for native job registration');
  first.emit({ command: 'run-swap', type: 'job-started', jobId: 'job-1' }); await flush();
  assert.equal(first.calls.filter(row => row.command === 'cancel-command').length, 0, 'An old registration must not acknowledge a new job');
  first.emit({ command: 'run-swap', type: 'job-started', jobId: 'job-2' });
  assert.equal(await cancellation, true);
  assert.deepEqual(JSON.parse(JSON.stringify(first.calls.at(-1).payload)), { command: 'run-swap', jobId: 'job-2' });
  let progress = 0; first.api.onJobEvent(() => progress++);
  first.emit({ command: 'run-swap', jobId: 'job-1', message: 'stale' });
  first.emit({ command: 'run-swap', jobId: 'job-2', message: 'current' });
  assert.equal(progress, 1, 'Stale job progress must not appear on the current conversion');
  first.calls[1].reject(new Error('The conversion engine ended without a result')); await flush();
  assert.equal(failure.cancelled, true);
  assert.equal(failure.message, 'Conversion cancelled.');
  assert.equal(await first.api.cancelSwap('job-2'), false);

  const early = setup();
  const earlyJob = await early.api.startSwap({});
  let earlyFailure;
  early.api.onJobFailed(value => { earlyFailure = value; });
  await early.api.cancelSwap(earlyJob.jobId); await flush();
  assert.equal(early.calls.filter(row => row.command === 'run-swap').length, 0, 'Immediate cancellation must not start the conversion');
  assert.equal(earlyFailure.cancelled, true);

  const blender = setup(); await flush();
  const a = blender.api.openPreviewInBlender({ target: 'C:/test/A.iff' });
  const b = blender.api.openPreviewInBlender({ target: 'C:/test/B.iff' });
  await flush();
  assert.equal(blender.calls.length, 2);
  const firstA = blender.calls[0], firstB = blender.calls[1];
  blender.emit({ type: 'blender', state: 'opened', target: 'c:\\virtualized\\B.iff', requestedTarget: 'c:\\test\\B.iff', handoffId: firstB.payload.handoffId });
  assert.equal((await b).target, 'c:\\virtualized\\B.iff');
  blender.emit({ type: 'blender', state: 'error', target: 'c:\\test\\A.iff', message: 'Import failed', handoffId: firstA.payload.handoffId });
  await assert.rejects(a, /Import failed/);
  const retry = blender.api.openPreviewInBlender({ target: 'C:/test/A.iff' }); await flush();
  const secondA = blender.calls.at(-1);
  let blenderEvents = 0; blender.api.onBlenderEvent(() => blenderEvents++);
  blender.emit({ type: 'blender', state: 'closed', target: 'C:/test/A.iff', handoffId: firstA.payload.handoffId });
  firstA.reject(new Error('Late failure from old session')); await flush();
  assert.equal(blenderEvents, 0, 'Old events and RPC failures must not close a replacement handoff');
  await assert.rejects(blender.api.openPreviewInBlender({ target: 'C:/test/A.iff' }), /already open/);
  blender.emit({ type: 'blender', state: 'opened', target: 'C:/test/A.iff', handoffId: secondA.payload.handoffId }); await retry;
  await assert.rejects(blender.api.openPreviewInBlender({ target: 'c:\\test\\a.iff' }), /already open/);
  blender.emit({ type: 'blender', state: 'closed', target: 'c:\\test\\a.iff', handoffId: secondA.payload.handoffId });
  const silent = blender.api.openPreviewInBlender({ target: 'C:/test/A.iff' }); await flush();
  blender.calls.at(-1).resolve({});
  await assert.rejects(silent, /closed before opening/);
  console.log('Bridge lifecycle: completion restart, subscriber isolation, acknowledged cancellation, stale-event isolation and independent Blender handoffs passed.');
})().catch(error => { console.error(error); process.exitCode = 1; }).finally(finishTest);
