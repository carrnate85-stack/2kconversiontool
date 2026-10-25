const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { spawn } = require('node:child_process');
const { chromium } = require('C:/Users/carrn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright');
const { PNG } = require('C:/Users/carrn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/pngjs');
const { root, currentExe } = require('./rpc-client.cjs');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const baseline = process.argv.includes('--baseline');
function coloredPixels(bytes) {
  const png = PNG.sync.read(bytes);
  let colored = 0;
  for (let i = 0; i < png.data.length; i += 4) {
    if (Math.max(...png.data.subarray(i, i + 3)) - Math.min(...png.data.subarray(i, i + 3)) > 20
        && png.data[i] > 40) colored++;
  }
  return colored;
}
(async () => {
  const data = fs.mkdtempSync(path.join(root, 'artifacts/preview-layout-data-'));
  const child = spawn(currentExe(), ['--ui-test'], { windowsHide: true, stdio: 'ignore',
    env: { ...process.env, CMT_TEST_DATA: data } });
  const report = { version: path.basename(currentExe()), baseline, completed: false, cases: [], errors: [] };
  let browser, page;
  try {
    for (let i = 0; i < 100; i++) {
      try { browser = await chromium.connectOverCDP('http://127.0.0.1:9764'); break; } catch { await delay(200); }
    }
    assert(browser, 'Owned Tauri test window unavailable');
    for (let i = 0; i < 100; i++) {
      page = browser.contexts().flatMap(context => context.pages()).find(page => page.url().includes('tauri.localhost'));
      if (page) break; await delay(100);
    }
    assert(page);
    page.on('pageerror', error => report.errors.push(error.message));
    await page.waitForFunction(() => document.getElementById('startupLoading')?.hidden, null, { timeout: 30000 });
    const target = path.resolve(root, '../character-mod-tool-rust/artifacts/conversion-output/png06827.iff');
    await page.evaluate(async target => { await setWorkingPlayer(target); setActiveView('preview'); }, target);
    await page.waitForFunction(() => state.preview.loadedPath && !state.preview.loading, null, { timeout: 120000 });
    const initialCamera = await page.evaluate(() => window.modelPreview.cameraState());
    for (const [width, height] of [[1364, 861], [1024, 768], [900, 650], [900, 620], [720, 620]]) {
      await page.setViewportSize({ width, height });
      await delay(200);
      const metrics = await page.evaluate(() => {
        const box = node => {
          const rect = node.getBoundingClientRect();
          return { x: rect.x, y: rect.y, width: rect.width, height: rect.height, right: rect.right, bottom: rect.bottom };
        };
        const visible = node => node.getClientRects().length && getComputedStyle(node).visibility !== 'hidden';
        const view = document.getElementById('previewView');
        const layout = view.querySelector('.player-preview-layout');
        const canvas = document.getElementById('previewCanvas');
        const toolbar = view.querySelector('.player-preview-tools');
        const viewport = view.querySelector('.player-preview-viewport');
        const rect = box(layout);
        const controls = [...toolbar.querySelectorAll('button,input,select')].filter(visible).map(node => ({
          name: node.id || node.getAttribute('aria-label') || node.dataset.viewerSetting || node.title,
          ...box(node),
        }));
        return { width: innerWidth, height: innerHeight, canvas: box(canvas), viewport: box(viewport), layout: rect,
          toolbar: box(toolbar), controls,
          status: box(document.getElementById('previewStatus')),
          internalOverflow: layout.scrollHeight - layout.clientHeight,
          horizontalOverflow: document.documentElement.scrollWidth - innerWidth,
          scrollTop: layout.scrollTop,
          configSelectWidth: box(document.getElementById('previewConfigSelect')).width,
          clippedControls: controls.filter(control => control.y < rect.y - 1 || control.bottom > rect.bottom + 1
            || control.x < rect.x - 1 || control.right > rect.right + 1).map(control => control.name),
        };
      });
      const bytes = await page.locator('#previewCanvas').screenshot();
      const colored = coloredPixels(bytes);
      metrics.coloredPixels = colored;
      report.cases.push(metrics);
      await page.screenshot({ path: path.join(root, 'artifacts/preview-layout-' + (baseline ? 'baseline-' : '') + width + '-' + height + '.png') });
      if (!baseline) {
        assert(metrics.internalOverflow <= 1, 'Preview has an internal scrollbar at ' + width + 'x' + height);
        assert(metrics.horizontalOverflow <= 1, 'Preview overflows window horizontally');
        assert.deepEqual(metrics.clippedControls, [], 'Preview toolbar is clipped');
        assert(metrics.canvas.y >= metrics.toolbar.bottom - 1, 'Toolbar overlaps canvas');
        assert(metrics.canvas.bottom <= metrics.layout.bottom + 1, 'Canvas is clipped');
        assert(metrics.status.bottom <= height + 1, 'Status is off-screen');
        assert(metrics.canvas.height >= 220, 'Preview leaves too little rendering space');
        assert(metrics.configSelectWidth >= 120, 'Configuration selector collapsed');
        assert(colored > 1000, 'Preview is blank');
        assert.deepEqual(await page.evaluate(() => window.modelPreview.cameraState()), initialCamera,
          'Resizing must not reset the user camera');
        const toggle = page.locator('[data-preview-inspector-toggle]');
        const userCamera = await page.evaluate(() => {
          const current = window.modelPreview.cameraState();
          window.modelPreview.setCameraState({ ...current, yaw: current.yaw + 0.17,
            pitch: current.pitch - 0.05, distance: current.distance * 0.93 });
          return window.modelPreview.cameraState();
        });
        assert.equal(await toggle.locator('svg').count(), 1, 'Inspector icon missing');
        const inspector = page.locator('#previewRenderSummary');
        const initiallyOpen = await inspector.isVisible();
        assert.equal(initiallyOpen, width > 1100, 'Unexpected initial summary state');
        if (!initiallyOpen) await toggle.click();
        assert(await inspector.isVisible(), 'Summary cannot be opened');
        assert.equal(await toggle.getAttribute('aria-expanded'), 'true');
        const summary = await inspector.boundingBox();
        assert(summary.y >= metrics.layout.y - 1 && summary.y + summary.height <= metrics.layout.bottom + 1,
          'Summary is clipped');
        await page.getByRole('button', { name: 'Close render summary', exact: true }).click();
        assert(!await inspector.isVisible(), 'Summary cannot be closed');
        assert.equal(await toggle.getAttribute('aria-expanded'), 'false');
        assert.deepEqual(await page.evaluate(() => window.modelPreview.cameraState()), userCamera,
          'Summary toggling must not reset camera or reload the player');
        // Restore automatic sizing before testing the next window size.
        await page.reload();
        await page.waitForFunction(() => document.getElementById('startupLoading')?.hidden);
        await page.evaluate(async target => { await setWorkingPlayer(target); setActiveView('preview'); }, target);
        await page.waitForFunction(() => state.preview.loadedPath && !state.preview.loading, null, { timeout: 120000 });
      }
    }
    if (!baseline) {
      const originalTheme = await page.evaluate(() => document.documentElement.dataset.theme);
      report.fitCases = [];
      for (const [width, height] of [[1364, 861], [900, 620], [720, 620]]) {
      await page.setViewportSize({ width, height });
      for (const theme of ['light', 'dark']) {
        await page.evaluate(theme => window.characterModAppearance.set(theme), theme);
        await page.locator('#previewBodyFitToggle').click();
        const fit = await page.evaluate(() => {
          const panel = document.getElementById('previewBodyFitPanel');
          const box = panel.getBoundingClientRect();
          const close = document.getElementById('previewCloseBodyFit').getBoundingClientRect();
          const calculate = document.getElementById('previewCalculateFit').getBoundingClientRect();
          const apply = document.getElementById('previewApplyFit').getBoundingClientRect();
          const canvas = document.getElementById('previewCanvas').getBoundingClientRect();
          return { visible: !panel.hidden, x: box.x, y: box.y, right: box.right, bottom: box.bottom,
            controls: document.getElementById('previewBodyFitControls').clientHeight,
            actionsBottom: apply.bottom, headingAlignment: Math.abs(close.y - calculate.y),
            canvas: { right: canvas.right, width: canvas.width, height: canvas.height } };
        });
        assert(fit.visible && fit.x >= 0 && fit.y >= 0 && fit.right <= width && fit.bottom <= height);
        assert(fit.controls >= 100 && fit.actionsBottom <= fit.bottom && fit.headingAlignment <= 1);
        assert(fit.canvas.right <= fit.x && fit.canvas.width >= 300 && fit.canvas.height >= 200,
          'Fit editor must leave a visible unobstructed player canvas');
        const pixels = coloredPixels(await page.locator('#previewCanvas').screenshot());
        assert(pixels > 500, 'Fit editor player canvas is blank');
        report.fitCases.push({ width, height, theme, ...fit, coloredPixels: pixels });
        await page.screenshot({ path: path.join(root, 'artifacts/preview-layout-fit-' + theme + '-' + width + '-' + height + '.png') });
        await page.locator('#previewCloseBodyFit').click();
        assert(!await page.locator('#previewBodyFitPanel').isVisible());
      }
      }
      await page.evaluate(theme => window.characterModAppearance.set(theme), originalTheme);
      const previousRequest = await page.evaluate(() => state.preview.requestId);
      await page.getByRole('button', { name: 'Refresh render', exact: true }).click();
      await page.waitForFunction(previous => state.preview.requestId > previous && !state.preview.loading, previousRequest, { timeout: 120000 });
      assert.equal(await page.evaluate(() => state.preview.loadedPath), target);
      await page.getByRole('button', { name: 'Expand preview', exact: true }).click();
      await page.waitForFunction(() => document.fullscreenElement || document.querySelector('.viewer-expanded'));
      report.expanded = await page.evaluate(() => {
        const box = document.getElementById('previewCanvas').getBoundingClientRect();
        return { mode: document.fullscreenElement ? 'fullscreen-api' : 'local-fallback', width: innerWidth, height: innerHeight,
          canvas: { x: box.x, y: box.y, right: box.right, bottom: box.bottom, width: box.width, height: box.height } };
      });
      assert(report.expanded.canvas.right <= report.expanded.width + 1
        && report.expanded.canvas.bottom <= report.expanded.height + 1);
      assert(coloredPixels(await page.locator('#previewCanvas').screenshot()) > 1000);
      await page.keyboard.press('Escape');
      await page.waitForFunction(() => !document.fullscreenElement && !document.querySelector('.viewer-expanded'));
      report.summaryAndFitControls = true;
    }
    if (baseline) assert(report.cases.some(row => row.internalOverflow > 100), 'Baseline did not reproduce clipped preview');
    report.completed = true;
    assert.deepEqual(report.errors, []);
    console.log(JSON.stringify({ ...report, cases: report.cases.map(({ controls, ...row }) => row) }, null, 2));
  } finally {
    fs.writeFileSync(path.join(root, 'artifacts', baseline ? 'preview-layout-0.2.23-baseline.json' : 'preview-layout-report.json'), JSON.stringify(report, null, 2));
    if (page) await page.evaluate(() => window.__TAURI__.core.invoke('engine_call', { command: 'close-app', payload: {} })).catch(() => {});
    if (browser) await browser.close().catch(() => {});
    await delay(500);
    if (child.exitCode === null) child.kill();
    if (child.exitCode === null) await new Promise(resolve => child.once('exit', resolve));
    assert.equal(path.dirname(fs.realpathSync(data)), fs.realpathSync(path.join(root, 'artifacts')));
    fs.rmSync(data, { recursive: true, maxRetries: 10, retryDelay: 100 });
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
