const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawn } = require('node:child_process');
const { chromium } = require('C:/Users/carrn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright');
const { root, currentExe } = require('./rpc-client.cjs');
const baseline = process.argv.includes('--baseline');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const digest = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
(async () => {
  const original = path.resolve(root, '../character-mod-tool-rust/artifacts/conversion-output/png06827.iff');
  const originalHash = digest(original), artifacts = path.join(root, 'artifacts');
  const folder = fs.mkdtempSync(path.join(artifacts, 'edit-cache-'));
  const target = path.join(folder, 'png06827.iff'); fs.copyFileSync(original, target);
  const exe = currentExe(), child = spawn(exe, ['--ui-test'], { windowsHide: true, stdio: 'ignore' });
  const report = { version: path.basename(exe), executableSha256: digest(exe), baseline, completed: false, errors: [] };
  let browser, page;
  try {
    for (let i = 0; i < 100; i++) { try { browser = await chromium.connectOverCDP('http://127.0.0.1:9764'); break; } catch { await delay(200); } }
    assert(browser);
    for (let i = 0; i < 100; i++) { page = browser.contexts().flatMap(c => c.pages()).find(p => p.url().includes('tauri.localhost')); if (page) break; await delay(100); }
    assert(page); page.on('pageerror', error => report.errors.push(error.message));
    await page.waitForFunction(() => document.getElementById('startupLoading')?.hidden);
    report.result = await page.evaluate(async target => {
      const api = window.characterTool;
      const invoke = (command, payload) => window.__TAURI__.core.invoke('engine_call', { command, payload });
      const requests = [
        { assetType: 'hair', assetId: 'char/sig/png06827_geo_hair_dreads.iff', includeBase: true },
        { assetType: 'facialhair', assetId: 'char/sig/png06827_geo_facialhair_beard_tangentspace.iff', includeBase: true },
      ];
      const hash = async bytes => Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', bytes)), n => n.toString(16).padStart(2, '0')).join('');
      const summarize = async value => ({
        meshes: await Promise.all(value.asset.meshes.map(async mesh => {
          const arrays = {};
          for (const [key, data] of Object.entries(mesh)) if (ArrayBuffer.isView(data)) arrays[key] = await hash(new Uint8Array(data.buffer, data.byteOffset, data.byteLength));
          return { name: mesh.name, groups: mesh.groups, arrays };
        })),
        textures: await hash(new TextEncoder().encode(JSON.stringify(value.asset.textures))),
      });
      const expected = [];
      for (const request of requests) {
        const first = await api.previewNativeHair(request);
        if (!first.stats.nativeGeometry) throw Error('Initial native preview failed');
        expected.push(await summarize(first));
      }
      const resources = await invoke('native-preview-resources', { target });
      await invoke('read-archive', { path: resources.filePath });
      const manifestPath = resources.options.companionPaths.find(row => row.kind === 'body-color')?.path;
      if (!manifestPath) throw Error('Mixed local/manifest fixture has no body map');
      const body = await api.loadBodyFit({ target }), key = 'hips.tx', value = body.targetBodyFit[key] + 0.25;
      const before = await api.previewCacheStats();
      const saved = await api.saveBodyFit({ target, changes: { [key]: value } });
      const after = await api.previewCacheStats();
      let oldLocalExpired = false, manifestRetained = true;
      try { await invoke('read-archive', { path: resources.filePath }); } catch { oldLocalExpired = true; }
      try { await invoke('file-stat', { path: manifestPath }); } catch { manifestRetained = false; }
      await invoke('release-preview-resources', { lease: resources.resourceLease });
      const refreshed = await api.loadStagedPreview({ target });
      const selections = [];
      for (let i = 0; i < requests.length; i++) {
        const preview = await api.previewNativeHair(requests[i]);
        selections.push({ asset: requests[i].assetId, nativeGeometry: preview.stats.nativeGeometry,
          headCacheHit: preview.stats.headCacheHit, outputMatches: JSON.stringify(await summarize(preview)) === JSON.stringify(expected[i]) });
      }
      let failedWriteRejected = false;
      try { await api.saveBodyFit({ target: target + '.missing.iff', changes: { [key]: value } }); } catch { failedWriteRejected = true; }
      const afterFailure = await api.previewNativeHair(requests[0]);
      await api.resetSession();
      const reset = await api.previewCacheStats();
      return { before, after, oldLocalExpired, manifestRetained, value,
        savedValue: saved.targetBodyFit[key], loadedValue: refreshed.player.bodyFit[key], selections,
        failedWriteRejected, nativeAfterFailedWrite: afterFailure.stats.nativeGeometry, reset };
    }, target);
    const r = report.result;
    assert(r.oldLocalExpired, 'The previous local handle must not expose stale bytes');
    assert.equal(r.savedValue, r.value); assert.equal(r.loadedValue, r.value);
    assert(r.failedWriteRejected); assert.equal(r.reset.registeredResources, 0); assert.equal(r.reset.resourceLeases, 0);
    const lost = !r.manifestRetained || r.selections.some(row => !row.nativeGeometry || !row.headCacheHit);
    if (baseline) assert(lost, 'Baseline did not reproduce manifest cache invalidation');
    else {
      assert(!lost, 'A local player edit invalidated warmed manifest resources or donor heads');
      assert(r.selections.every(row => row.outputMatches), 'Catalog output changed across a local edit');
      assert(r.nativeAfterFailedWrite, 'Failed edit discarded working catalog descriptors');
    }
    assert.equal(digest(original), originalHash); assert.deepEqual(report.errors, []);
    report.completed = true;
    console.log(JSON.stringify(report, null, 2));
  } finally {
    if (page) await page.evaluate(() => window.__TAURI__.core.invoke('engine_call', { command: 'close-app', payload: {} })).catch(() => {});
    await browser?.close().catch(() => {});
    if (child.exitCode === null) await Promise.race([new Promise(resolve => child.once('exit', resolve)), delay(5000)]);
    if (child.exitCode === null) { child.kill(); await new Promise(resolve => child.once('exit', resolve)); }
    fs.writeFileSync(path.join(artifacts, baseline ? 'player-edit-cache-baseline.json' : 'player-edit-cache-report.json'), JSON.stringify(report, null, 2));
    assert.equal(fs.realpathSync(path.dirname(folder)), fs.realpathSync(artifacts));
    for (const name of fs.readdirSync(folder)) assert(!fs.lstatSync(path.join(folder, name)).isSymbolicLink());
    fs.rmSync(folder, { recursive: true, maxRetries: 10, retryDelay: 200 });
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
