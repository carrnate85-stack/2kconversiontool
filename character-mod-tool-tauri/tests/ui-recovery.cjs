const assert = require('assert/strict');
const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');
const { chromium } = require('C:/Users/carrn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright');
const { root } = require('./rpc-client.cjs');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
(async () => {
  const version = JSON.parse(fs.readFileSync(path.join(root, 'dist/BUILD.json'), 'utf8').replace(/^\uFEFF/, '')).version;
  const child = spawn(path.join(root, 'dist/Character-Mod-Tool-Tauri-Preview-' + version + '.exe'), ['--ui-test'], { windowsHide: true, stdio: 'ignore' });
  const report = {}, errors = []; let browser, page;
  try {
    for (let i = 0; i < 100; i++) { try { browser = await chromium.connectOverCDP('http://127.0.0.1:9764'); break; } catch { await delay(200); } }
    assert(browser);
    for (let i = 0; i < 100; i++) { page = browser.contexts().flatMap(c => c.pages()).find(p => p.url().includes('tauri.localhost')); if (page) break; await delay(100); }
    assert(page);
    page.on('pageerror', e => errors.push(e.message));
    await page.waitForFunction(() => document.getElementById('engineStatus')?.textContent.includes('Engine '));
    await page.locator('[data-theme-choice="light"]').click();
    assert.equal(await page.evaluate(() => document.documentElement.dataset.theme), 'light');
    await page.locator('[data-theme-choice="dark"]').click();
    await page.locator('#hairTab').click();
    await page.waitForFunction(() => state.nativeHair.assets.length && !state.nativeHair.loading, null, { timeout: 90000 });
    await page.locator('.native-hair-favorite').first().click();
    await page.locator('#nativeHairFavoritesOnly').click();
    assert.equal(await page.locator('.native-hair-favorite').count(), 1);
    await page.reload();
    await page.waitForFunction(() => document.getElementById('engineStatus')?.textContent.includes('Engine '));
    assert.equal(await page.evaluate(() => document.documentElement.dataset.theme), 'dark');
    assert.equal(await page.evaluate(() => state.nativeHair.favorites.hair.size), 1);
    report.favoritesAndTheme = 'filter and persistence passed';
    const target = path.join(root, 'artifacts/ui-conversion-output/png06827.iff');
    report.cancellation = await page.evaluate(async target => {
      const api = window.characterTool;
      setActiveView('full-swap');
      await setWorkingPlayer(target);
      while (state.selectionBusy || state.hair.loading || state.facialHair.loading) await new Promise(resolve => setTimeout(resolve, 20));
      const preview = api.loadStagedPreview({ target }).then(() => 'completed', () => 'cancelled');
      await api.cancelStagedPreview();
      const outcome = await preview;
      await resetFullSwapSession();
      return { outcome, workingPlayer: state.workingPlayer, source: state.source, target: state.target,
        bodyFields: Object.keys(state.body.targetBodyFit).length, hidden: document.getElementById('runSwap').hidden,
        cache: await api.previewCacheStats() };
    }, target);
    assert.equal(report.cancellation.outcome, 'cancelled');
    assert.equal(report.cancellation.workingPlayer, null);
    assert.equal(report.cancellation.source, null);
    assert.equal(report.cancellation.target, null);
    assert.equal(report.cancellation.bodyFields, 0);
    assert.equal(report.cancellation.hidden, false);
    report.failureRecovery = await page.evaluate(async ({ target, outputDir }) => {
      const api = window.characterTool;
      state.source = await api.inspectCharacter(target); state.source.gameLabel = 'NBA 2K23';
      state.target = await api.inspectCharacter(target); state.target.gameLabel = 'NBA 2K27';
      await new Promise(async (resolve, reject) => {
        const off = api.onJobFailed(() => { off(); resolve(null); });
        try { const job = await api.startSwap({ source: target + '.missing', target, sourceGame: 'NBA 2K23', targetGame: 'NBA 2K27', outputDir, shrinkwrapBody: true, hairEnabled: false, facialHairEnabled: false }); state.activeJob = job.jobId; }
        catch (error) { off(); reject(error); }
      });
      await new Promise(resolve => setTimeout(resolve, 100));
      await resetFullSwapSession();
      await setWorkingPlayer(target);
      const preview = await api.loadStagedPreview({ target });
      return { activeJob: state.activeJob, buttonVisible: !document.getElementById('runSwap').hidden,
        meshes: preview.meshes.length, textures: preview.textures.length };
    }, { target, outputDir: path.join(root, 'artifacts/expected-failure') });
    assert.equal(report.failureRecovery.activeJob, '');
    assert(report.failureRecovery.buttonVisible);
    assert(report.failureRecovery.meshes && report.failureRecovery.textures);
    assert.deepEqual(errors, []);
    console.log(JSON.stringify(report, null, 2));
  } finally {
    fs.writeFileSync(path.join(root, 'artifacts/ui-recovery-report.json'), JSON.stringify({ ...report, errors }, null, 2));
    if (page) await page.evaluate(() => window.__TAURI__.core.invoke('engine_call', { command: 'close-app', payload: {} })).catch(() => {});
    if (browser) await browser.close().catch(() => {});
    await delay(500); if (child.exitCode === null) child.kill();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
