const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');
const { spawn } = require('node:child_process');
const { chromium } = require('C:/Users/carrn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright');
const { root, currentExe } = require('./rpc-client.cjs');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const digest = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const version = JSON.parse(fs.readFileSync(path.join(root, 'dist/BUILD.json'), 'utf8').replace(/^\uFEFF/, '')).version;

(async () => {
  const target = path.resolve(root, '../character-mod-tool-rust/artifacts/conversion-output/png06827.iff');
  const data = fs.mkdtempSync(path.join(root, 'artifacts/jersey-bodyfit-real-'));
  const beforeHash = digest(target);
  const child = spawn(currentExe(), ['--ui-test'], { windowsHide: true, stdio: 'ignore', env: { ...process.env, CMT_TEST_DATA: data } });
  let browser, page;
  try {
    for (let index = 0; index < 120; index += 1) {
      try { browser = await chromium.connectOverCDP('http://127.0.0.1:9764'); break; } catch { await delay(250); }
    }
    assert(browser, 'Tauri test WebView did not start');
    for (let index = 0; index < 120; index += 1) {
      page = browser.contexts().flatMap(context => context.pages()).find(candidate => candidate.url().includes('tauri.localhost'));
      if (page) break;
      await delay(100);
    }
    assert(page, 'No owned Tauri frontend found');
    const errors = []; page.on('pageerror', error => errors.push(error.message));
    await page.waitForFunction(() => document.getElementById('startupLoading')?.hidden, null, { timeout: 60000 });
    await page.evaluate(async ({ target, data }) => {
      const boot = await window.characterTool.bootstrap();
      await window.characterTool.saveSettings({ ...boot.settings,
        game_root_2k27: 'C:/Program Files (x86)/Steam/steamapps/common/NBA 2K27',
        output_dir: data + '/outputs' });
      await setWorkingPlayer(target);
      setActiveView('preview');
      await loadStagedPreview(true);
    }, { target, data });
    await page.waitForFunction(() => state.preview.loadedPath && !state.preview.loading, null, { timeout: 180000 });
    await page.locator('#previewJersey').check();
    await delay(250);
    const baseline = await page.evaluate(() => ({
      diagnostics: window.modelPreview.stats().bodyFitJersey,
      bounds: window.modelPreview.bounds(),
      fit: { ...state.preview.bodyFit },
    }));
    assert(baseline.diagnostics, 'Real preview omitted body-fitted jersey diagnostics');
    assert(baseline.diagnostics.coverage >= 0.98, JSON.stringify(baseline.diagnostics));
    assert.match(baseline.diagnostics.method, /^native-jersey-morphs/, JSON.stringify(baseline.diagnostics));
    assert(baseline.diagnostics.nativeMorphTargets > 0, JSON.stringify(baseline.diagnostics));
    const key = ['chestTHICK', 'trapTHICK', 'shoulderTHICK'].find(name => Object.hasOwn(baseline.fit, name));
    assert(key, 'Real player did not expose a jersey-covered THICK parameter');
    await page.evaluate(({ key, value }) => setPreviewFitValue(key, value),
      { key, value: Math.min(2, Number(baseline.fit[key] || 0) + 0.75) });
    await delay(250);
    const changed = await page.evaluate(() => ({
      diagnostics: window.modelPreview.stats().bodyFitJersey,
      bounds: window.modelPreview.bounds(),
    }));
    assert(changed.diagnostics.clearance.maximumClearanceDrift > 0.0001, JSON.stringify(changed.diagnostics));
    assert.notDeepEqual(changed.bounds, baseline.bounds, 'Real clothing-fit parameter did not move the jersey');
    assert.equal(digest(target), beforeHash, 'Preview-only jersey fit changed the saved player');
    assert.deepEqual(errors, []);
    const canvas = page.locator('#previewCanvas');
    await canvas.screenshot({ path: path.join(root, `artifacts/jersey-bodyfit-real-${version}.png`) });
    const report = { version: path.basename(currentExe()), target: path.basename(target), parameter: key,
      sourceUnchanged: true, baseline: baseline.diagnostics, changed: changed.diagnostics };
    fs.writeFileSync(path.join(root, `artifacts/jersey-bodyfit-real-${version}.json`), JSON.stringify(report, null, 2));
    console.log(JSON.stringify(report, null, 2));
  } finally {
    if (page) await page.evaluate(() => window.__TAURI__.core.invoke('engine_call', { command: 'close-app', payload: {} })).catch(() => {});
    await browser?.close().catch(() => {});
    if (child.exitCode === null) await Promise.race([new Promise(resolve => child.once('exit', resolve)), delay(5000)]);
    if (child.exitCode === null) { child.kill(); await new Promise(resolve => child.once('exit', resolve)); }
    fs.rmSync(data, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 });
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
