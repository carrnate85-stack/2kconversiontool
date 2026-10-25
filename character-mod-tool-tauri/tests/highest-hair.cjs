const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const { spawn } = require('child_process');
const { chromium } = require('C:/Users/carrn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright');
const root = path.resolve(__dirname, '..');
const original = 'C:/Users/carrn/AppData/Local/CharacterModToolTauri/outputs';
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const snapshot = () => Object.fromEntries(fs.readdirSync(original).filter(n => /^png6720.*\.iff$/i.test(n)).map(n => [n, crypto.createHash('sha256').update(fs.readFileSync(path.join(original, n))).digest('hex')]));
(async () => {
  const before = snapshot(), report = {}, errors = [];
  const child = spawn(require('./rpc-client.cjs').currentExe(), ['--ui-test'], { windowsHide: true, stdio: 'ignore' });
  let browser, page;
  try {
    for (let i = 0; i < 100; i++) { try { browser = await chromium.connectOverCDP('http://127.0.0.1:9764'); break; } catch { await delay(200); } }
    if (!browser) throw new Error('Owned test app did not start');
    for (let i = 0; i < 100; i++) { page = browser.contexts().flatMap(c => c.pages()).find(p => p.url().includes('tauri.localhost')); if (page) break; await delay(100); }
    if (!page) throw new Error('Owned test page unavailable');
    page.on('pageerror', error => errors.push(error.message));
    page.on('console', row => { if (row.type() === 'error' && /shader|WebGL/i.test(row.text())) errors.push(row.text()); });
    await page.waitForFunction(() => document.getElementById('engineStatus')?.textContent.includes('Engine '));
    report.result = await page.evaluate(async outputDir => {
      const api = window.characterTool;
      const source = await api.extractPlayer({ gameLabel: 'NBA 2K23', png: '6720' });
      const target = await api.extractPlayer({ gameLabel: 'NBA 2K27', png: '06827' });
      const request = { source: source.mainPath, target: target.mainPath, sourceGame: 'NBA 2K23', targetGame: 'NBA 2K27', outputDir, renameId: '6720', shrinkwrapBody: true, hairEnabled: true, facialHairEnabled: false };
      const options = await api.loadHairOptions(request);
      const candidate = options.candidates.find(row => /spiral_dreads/i.test(row.path));
      const slot = options.slots.find(row => /dreads/i.test(row.key)) || options.slots[0];
      if (!candidate || !slot) throw new Error('Test hair source/target slot unavailable: ' + JSON.stringify(options));
      request.hairSource = candidate.path;
      request.hairTargetKey = slot.key;
      return await new Promise(async (resolve, reject) => {
        let jobId;
        const offComplete = api.onJobComplete(event => { offComplete(); offFailed(); resolve({ ...event.result, jobId, completedId: event.jobId }); });
        const offFailed = api.onJobFailed(event => { offComplete(); offFailed(); reject(new Error(event.error)); });
        try {
          state.source = await api.inspectCharacter(request.source); state.source.gameLabel = request.sourceGame;
          state.target = await api.inspectCharacter(request.target); state.target.gameLabel = request.targetGame;
          jobId = (await api.startSwap(request)).jobId; state.activeJob = jobId;
        } catch (error) { offComplete(); offFailed(); reject(error); }
      });
    }, path.join(root, 'artifacts/highest-hair-output'));
    if (report.result.validation.errors) throw new Error('Corrected output failed validation');
    await page.waitForFunction(output => state.workingPlayer?.path?.toLowerCase() === output.toLowerCase(), report.result.output, { timeout: 30000 });
    report.staged = await page.evaluate(() => state.workingPlayer.path);
    await page.evaluate(() => setActiveView('preview'));
    await page.waitForFunction(() => state.preview.loadedPath && !state.preview.loading, null, { timeout: 120000 });
    report.preview = await page.evaluate(() => {
      window.modelPreview.focusHead();
      const camera = window.modelPreview.cameraState();
      window.modelPreview.setCameraState({ ...camera, yaw: Math.PI / 2, pitch: 0 });
      const stats = window.modelPreview.stats();
      return { renderer: stats.renderer, profile: stats.profile, materials: stats.materialTextures };
    });
    await page.screenshot({ path: path.join(root, 'artifacts/highest-hair-preview.png') });
    if (!report.preview.materials.some(m => /hair/i.test(m.name) && m.lightmap)) throw new Error('Hair lightmap not applied');
    if (errors.length) throw new Error(errors.join('\n'));
    if (JSON.stringify(before) !== JSON.stringify(snapshot())) throw new Error('User export changed');
    report.originalUnchanged = true;
    console.log(JSON.stringify(report, null, 2));
  } finally {
    if (page) report.runtime = await page.evaluate(() => window.__TAURI__.core.invoke('engine_call', { command: 'runtime-info', payload: {} })).catch(() => null);
    fs.writeFileSync(path.join(root, 'artifacts/highest-hair-report.json'), JSON.stringify({ ...report, errors }, null, 2));
    if (page) await page.evaluate(() => window.__TAURI__.core.invoke('engine_call', { command: 'close-app', payload: {} })).catch(() => {});
    if (browser) await browser.close().catch(() => {});
    await delay(500); if (child.exitCode === null) child.kill();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
