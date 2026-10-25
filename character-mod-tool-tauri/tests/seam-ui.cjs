const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const esbuild = require('../build-tools/node_modules/esbuild');
const { chromium } = require('C:/Users/carrn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright');
const finish = require('./test-watchdog.cjs')('seam UI and bridge');
const root = path.resolve(__dirname, '..');
const renderer = path.resolve(root, '../character-mod-tool-webview2/Reference/src/renderer');
const app = esbuild.transformSync(fs.readFileSync(path.join(root, 'web/ui/app.ts'), 'utf8'),
  { loader: 'ts', target: 'es2022' }).code.replace(/bootstrap\(\);\s*$/, '');
const bridge = esbuild.buildSync({ entryPoints: [path.join(root, 'web/tauri-bridge.ts')], bundle: true,
  write: false, platform: 'browser', format: 'iife', target: 'es2022' }).outputFiles[0].text;
const tick = () => new Promise(resolve => setTimeout(resolve, 10));

async function uiChecks() {
  const browser = await chromium.launch({ channel: 'chrome', headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.route('**/*', route => route.abort());
    const html = fs.readFileSync(path.join(renderer, 'index.html'), 'utf8')
      .replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi, '')
      .replace(/<meta[^>]*http-equiv="Content-Security-Policy"[^>]*>/i, '')
      .replace(/<link\b[^>]*>/gi, '');
    await page.setContent(html);
    for (const file of ['styles.css', 'blue-theme.css', 'configs.css']) {
      await page.addStyleTag({ content: fs.readFileSync(path.join(renderer, file), 'utf8') });
    }
    await page.addStyleTag({ content: fs.readFileSync(path.join(root, 'web/ui/preview-layout.css'), 'utf8') });
    await page.addScriptTag({ content: fs.readFileSync(path.join(renderer, 'lucide.min.js'), 'utf8') });
    await page.addScriptTag({ content: app });
    const checks = await page.evaluate(async () => {
      const require = (value, message) => { if (!value) throw Error(message); };
      const pending = [], logs = [], reloads = [], results = [];
      let reloadFailure = false;
      window.characterTool = {
        closePreviewSeams: request => new Promise((resolve, reject) => pending.push({ request, resolve, reject })),
        cancelStagedPreview: async () => {},
      };
      addLog = (message, kind) => logs.push({ message, kind });
      loadStagedPreview = async () => {
        state.preview.requestId++;
        reloads.push(state.workingPlayer.path);
        state.preview.dirty = reloadFailure;
        state.preview.loadedPath = reloadFailure ? '' : state.workingPlayer.path;
        if (reloadFailure) setPreviewStatus('Mock render failure', 'error');
      };
      setupPreviewSeamAction();
      const button = document.getElementById('previewCloseSeams');
      require(button.disabled, 'No-player state enabled the seam action');
      require(!button.querySelector('svg, i'), 'Close Seams should be a text-only row button');
      require(button.previousElementSibling && button.nextElementSibling.id === 'previewApplyChestCorrection', 'Existing toolbar ordering changed');
      const stage = path => {
        state.workingGeneration++;
        state.workingPlayer = { path, name: path };
        state.body.targetPlayer = state.workingPlayer;
        state.preview.editRequestId++;
        state.preview.requestId++;
        state.preview.closeSeamsBusy = false;
        state.preview.loadedPath = path;
        state.preview.bodyFitTarget = path;
        state.preview.dirty = false;
        renderPreviewControls();
      };
      stage('C:/fixture/A.iff');
      require(!button.disabled && !pending.length, 'Staging should enable but never run seam closing');
      const blocked = async (label, set, clear) => {
        set(); renderPreviewControls();
        require(button.disabled, label + ' did not disable the button');
        const count = pending.length; await closePreviewSeams();
        require(pending.length === count, label + ' reached the bridge');
        clear(); renderPreviewControls(); require(!button.disabled, label + ' did not release');
      };
      await blocked('Body draft', () => state.body.dirtyFields.add('chestTHIN'), () => state.body.dirtyFields.clear());
      await blocked('Invalid body input', () => state.body.invalidFields.add('chestTHIN'), () => state.body.invalidFields.clear());
      await blocked('Preview draft', () => state.preview.bodyFitDirtyFields.add('chestTHIN'), () => state.preview.bodyFitDirtyFields.clear());
      await blocked('Invalid preview input', () => state.preview.bodyFitInvalidInputs.set('chestTHIN', '-'), () => state.preview.bodyFitInvalidInputs.clear());
      for (const field of ['loading', 'bodyFitBusy', 'chestCorrectionBusy']) {
        await blocked(field, () => state.preview[field] = true, () => state.preview[field] = false);
      }
      for (const field of ['resetting', 'startingJob', 'selectionBusy']) {
        await blocked(field, () => state[field] = true, () => state[field] = false);
      }
      await blocked('Conversion', () => state.activeJob = 'job', () => state.activeJob = '');
      await blocked('Body save', () => { state.body.loading = true; state.body.operation = 'save'; },
        () => { state.body.loading = false; state.body.operation = ''; });
      await blocked('Headband edit', () => state.headband.applying = true, () => state.headband.applying = false);
      await blocked('Hair edit', () => state.nativeHair.loading = true, () => state.nativeHair.loading = false);
      const key = previewBlenderTarget(state.workingPlayer.path);
      await blocked('Blender handoff', () => previewBlenderSessions.set(key, {}), () => previewBlenderSessions.delete(key));
      previewBlenderSessions.set(previewBlenderTarget('C:/fixture/B.iff'), {});
      renderPreviewControls(); require(!button.disabled, 'Unrelated Blender target blocked seam closing');
      previewBlenderSessions.clear();
      results.push('staging never auto-runs; no player, drafts, busy operations and target-specific Blender guards');
      const result = { output: state.workingPlayer.path, applied: true, correctedVertices: 12, pairs: 6,
        maxGapBefore: 0.025, maxGapAfter: 0, backup: 'fixture.backup', warnings: ['Fixture warning'] };
      const applied = closePreviewSeams();
      require(button.disabled && button.getAttribute('aria-busy') === 'true' && button.textContent.includes('Closing...'), 'Busy toolbar state missing');
      require(document.getElementById('openPreviewInBlender').disabled && document.getElementById('refreshPreview').disabled
        && document.getElementById('previewApplyChestCorrection').disabled, 'Conflicting preview controls remained enabled');
      require(bodyFitEditBusy(state.workingPlayer.path), 'Body-fit edits did not see seam mutation');
      await closePreviewSeams(); require(pending.length === 1, 'Duplicate seam edit started');
      require(Object.keys(pending[0].request).join() === 'target', 'Request included unsaved draft/config values');
      pending[0].resolve(result); await applied;
      require(reloads.length === 1 && !button.disabled && !state.preview.closeSeamsBusy, 'Success did not refresh/release');
      require(logs.some(row => row.message.includes('12 vertices')) && logs.some(row => row.message.includes('fixture.backup'))
        && logs.some(row => row.kind === 'warning'), 'Result diagnostics were lost');
      const noop = closePreviewSeams(); pending.at(-1).resolve({ ...result, applied: false, backup: undefined }); await noop;
      require(logs.some(row => row.message.includes('no changes needed') && row.kind === 'info'), 'No-op was reported as an edit');
      const noBorders = closePreviewSeams();
      pending.at(-1).resolve({ ...result, applied: false, pairs: 0, correctedVertices: 0, backup: undefined, warnings: ['No safe borders found'] });
      await noBorders;
      require(document.getElementById('previewStatus').textContent.includes('no safe seam pairs found')
        && document.getElementById('previewStatus').textContent.includes('No safe borders found'), 'Zero safe pairs hid engine warnings');
      results.push('request shape, busy controls, duplicate exclusion, refresh, diagnostics and distinct no-op');
      const failed = closePreviewSeams(); pending.at(-1).reject(Error('Controlled seam failure')); await failed;
      require(!button.disabled && document.getElementById('previewStatus').textContent === 'Controlled seam failure', 'Failure did not release/report');
      reloadFailure = true;
      const badPreview = closePreviewSeams(); pending.at(-1).resolve(result); await badPreview;
      require(document.getElementById('previewStatus').textContent === 'Mock render failure', 'Edit success masked preview failure');
      reloadFailure = false; stage('C:/fixture/A.iff');
      const old = closePreviewSeams(), oldRequest = pending.at(-1);
      stage('C:/fixture/B.iff');
      const current = closePreviewSeams(), currentRequest = pending.at(-1), before = reloads.length;
      oldRequest.resolve(result); await old;
      require(state.preview.closeSeamsBusy && reloads.length === before, 'Obsolete completion changed newer edit state');
      currentRequest.resolve({ ...result, output: state.workingPlayer.path }); await current;
      require(!state.preview.closeSeamsBusy && reloads.at(-1) === 'C:/fixture/B.iff', 'Current edit lost ownership');
      const retiring = closePreviewSeams(), retireRequest = pending.at(-1);
      resetPreviewState(); state.workingPlayer = null; renderPreviewControls();
      retireRequest.reject(Error('Retired edit')); await retiring;
      require(!state.preview.closeSeamsBusy && button.disabled && !logs.some(row => row.message.includes('Retired edit')), 'Reset admitted obsolete failure');
      results.push('error recovery, render failure visibility, newer stage/edit ownership and reset retirement');
      stage('C:/fixture/A.iff');
      document.getElementById('startupLoading').hidden = true;
      for (const view of document.querySelectorAll('.view')) view.hidden = view.id !== 'previewView';
      document.getElementById('previewView').hidden = false;
      return results;
    });
    for (const width of [1280, 900]) {
      await page.setViewportSize({ width, height: 900 });
      const bounds = await page.locator('#previewCloseSeams').evaluate(button => {
        const b = button.getBoundingClientRect(), label = button.querySelector('span').getBoundingClientRect();
        const peer = document.getElementById('previewApplyChestCorrection').getBoundingClientRect();
        return { width: b.width, height: b.height, peerHeight: peer.height, left: b.left, right: b.right,
          labelFits: label.right <= b.right && label.left >= b.left };
      });
      assert(bounds.width > 0 && bounds.height === bounds.peerHeight && bounds.labelFits, JSON.stringify(bounds));
      assert(bounds.left >= 0 && bounds.right <= width, 'Seam button escaped viewport');
    }
    await page.locator('#previewCloseSeams').screenshot({
      path: path.join(root, 'artifacts', 'close-seams-button-0.2.53.png'),
    });
    assert.deepEqual(errors, []);
    return checks;
  } finally { await browser.close(); }
}

async function bridgeChecks() {
  const calls = [], edits = [];
  let listener, id = 0, decodes = 0, failCancel = false;
  const window = { __TAURI__: {
    event: { listen: async (_, callback) => { listener = callback; } },
    core: { invoke: async (_, { command, payload }) => {
      calls.push({ command, payload });
      if (command === 'runtime-info') return {};
      if (command === 'preview-close-seams') return new Promise((resolve, reject) => edits.push({ resolve, reject }));
      if (command === 'cancel-preview' && failCancel) throw Error('Controlled cleanup failure');
      if (['cancel-preview', 'reset', 'release-preview-resources', 'blender-snapshot-release'].includes(command)) return true;
      if (command === 'blender-snapshot-prepare') return { snapshotId: payload.handoffId };
      if (command === 'blender-open') return new Promise(() => {});
      if (command === 'native-preview-resources') return { filePath: payload.target, fingerprint: 'same-fingerprint',
        options: { companionPaths: [] }, player: {}, warnings: [], activeItems: [] };
      throw Error('Unexpected command ' + command);
    } },
  } };
  class Worker {
    postMessage(message) {
      decodes++;
      queueMicrotask(() => this.onmessage({ data: { id: message.id, result: { meshes: [], textures: [], stats: {}, skipped: [] } } }));
    }
    terminate() {}
  }
  vm.runInNewContext(bridge, { window, Worker, console, structuredClone, setTimeout, setInterval, clearInterval,
    crypto: { randomUUID: () => 'handoff-' + ++id } });
  const api = window.characterTool, target = 'C:/fixture/A.iff';
  await api.loadStagedPreview({ target });
  const edit = api.closePreviewSeams({ target }); await tick();
  let loaded = false;
  const loading = api.loadStagedPreview({ target }).then(() => { loaded = true; });
  await tick(); assert.equal(loaded, false, 'Preview did not wait for seam edit');
  edits.at(-1).resolve({ applied: true }); await edit; await loading;
  assert.equal(decodes, 2, 'Successful seam edit retained stale cached geometry');
  failCancel = true;
  const draining = api.closePreviewSeams({ target }); await tick();
  let resetFinished = false;
  const resetting = api.resetSession().catch(String).then(value => { resetFinished = true; return value; });
  await tick(); assert.equal(resetFinished, false, 'Failed reset did not drain seam edits');
  edits.at(-1).resolve({}); await draining;
  assert.match(await resetting, /Controlled cleanup failure/);
  failCancel = false;
  await api.resetSession();
  const opening = api.openPreviewInBlender({ target }); await tick();
  await assert.rejects(api.closePreviewSeams({ target: 'c:\\fixture\\a.iff' }), /Blender round trip/);
  const handoff = calls.find(row => row.command === 'blender-open').payload.handoffId;
  listener({ payload: { type: 'blender', state: 'opened', target, handoffId: handoff } }); await opening;
  await assert.rejects(api.closePreviewSeams({ target }), /Blender round trip/);
  const other = api.closePreviewSeams({ target: 'C:/fixture/B.iff' }); await tick(); edits.at(-1).resolve({}); await other;
  listener({ payload: { type: 'blender', state: 'closed', target, handoffId: handoff } });
  const reopened = api.closePreviewSeams({ target }); await tick(); edits.at(-1).resolve({}); await reopened;
  await assert.rejects(api.closePreviewSeams({ target: '' }), /Stage a player/);
  return 'bridge cache invalidation, edit/read barriers, failed-reset draining and preparing/open Blender target exclusion';
}

(async () => {
  console.log(JSON.stringify({ headless: true, gameFilesTouched: false, geometryMocked: true,
    checks: [...await uiChecks(), await bridgeChecks()] }, null, 2));
})().catch(error => { console.error(error); process.exitCode = 1; }).finally(finish);
