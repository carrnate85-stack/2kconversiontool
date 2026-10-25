const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { spawn, spawnSync } = require('node:child_process');
const { chromium } = require('C:/Users/carrn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright');
const { root, currentExe } = require('./rpc-client.cjs');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
(async () => {
  const child = spawn(currentExe(), ['--ui-test'], { windowsHide: true, stdio: 'ignore',
    env: { ...process.env, CMT_TEST_DATA: path.join(root, 'artifacts/accessory-preview-data') } });
  let browser, page;
  const report = { checkedUtc: new Date().toISOString(), completed: false };
  try {
    for (let i = 0; i < 100; i++) {
      try { browser = await chromium.connectOverCDP('http://127.0.0.1:9764'); break; } catch { await delay(200); }
    }
    assert(browser);
    for (let i = 0; i < 100; i++) {
      page = browser.contexts().flatMap(context => context.pages()).find(page => page.url().includes('tauri.localhost'));
      if (page) break; await delay(100);
    }
    assert(page);
    await page.waitForFunction(() => document.getElementById('engineStatus')?.textContent.includes('Engine '));
    const original = await page.evaluate(async () => {
      const player = await window.characterTool.extractPlayer({ gameLabel: 'NBA 2K27', png: '06572' });
      return player.mainPath;
    });
    const fixture = fs.mkdtempSync(path.join(root, 'artifacts/tangent-only-'));
    const prepared = spawnSync(path.resolve(root, '../character-mod-tool-webview2/dist/Character-Mod-Tool-WebView2-0.1.31/App/Backend/python_runtime/python.exe'),
      ['-B', path.join(__dirname, 'tangent-only-fixture.py'), original, fixture], { encoding: 'utf8', windowsHide: true });
    assert.equal(prepared.status, 0, prepared.stderr);
    const target = prepared.stdout.trim();
    await page.evaluate(async target => { await setWorkingPlayer(target); setActiveView('preview'); }, target);
    await page.waitForFunction(target => state.preview.loadedPath === target && !state.preview.loading, target, { timeout: 120000 });
    await page.locator('#previewConfigSelect').selectOption('CHEER');
    await page.waitForFunction(() => document.getElementById('previewConfig')?.textContent === 'CHEER' && !state.preview.loading, null, { timeout: 120000 });
    report.projection = await page.evaluate(async target => {
      const api = window.characterTool;
      const staged = await api.loadStagedPreview({ target, config: 'CHEER' });
      const catalog = await api.previewNativeHair({ assetType: 'hair',
        assetId: 'char/sig/png06572_geo_hair_curly_afro_tangentspace.iff', includeBase: true });
      const mesh = staged.meshes.find(mesh => /_geo_hair_curly_afro_tangentspace\.iff$/i.test(mesh.sourceName));
      if (!mesh) throw new Error('Main preview has no tangent-only afro');
      const expected = catalog.asset.meshes[0];
      const lookup = new Map();
      expected.sourceVertexIds.forEach((id, index) => lookup.set(id, index));
      let squared = 0, max = 0, matched = 0;
      mesh.sourceVertexIds.forEach((id, index) => {
        const other = lookup.get(id);
        if (other === undefined) return;
        for (let axis = 0; axis < 3; axis++) {
          const difference = Math.abs(mesh.positions[index * 3 + axis] - expected.positions[other * 3 + axis]);
          squared += difference * difference; max = Math.max(max, difference);
        }
        matched++;
      });
      window.modelPreview.focusHead();
      return { projected: Boolean(mesh.tangentProjected), vertices: mesh.positions.length / 3,
        matched, rms: Math.sqrt(squared / Math.max(1, matched * 3)), max, skipped: staged.skipped,
        attachment: mesh.tangentSpaceAttachment, offsets: mesh.tangentOffsets?.length, anchors: mesh.tangentAnchorIds?.length,
        sources: staged.meshes.map(mesh => ({ source: mesh.sourceName, attachment: mesh.tangentSpaceAttachment })), stats: staged.stats };
    }, target);
    console.log(JSON.stringify(report.projection));
    await page.screenshot({ path: path.join(root, 'artifacts/staged-tangent-hair.png') });
    assert(report.projection.projected, 'Main preview did not reconstruct tangent-only hair');
    assert.equal(report.projection.matched, report.projection.vertices);
    assert(report.projection.rms < .001, 'Main and catalog neutral tangent reconstruction differ');
    report.completed = true; report.privateTangentOnlyFixture = true;
  } finally {
    fs.writeFileSync(path.join(root, 'artifacts/staged-tangent-hair-report.json'), JSON.stringify(report, null, 2));
    if (page) await page.evaluate(() => window.__TAURI__.core.invoke('engine_call', { command: 'close-app', payload: {} })).catch(() => {});
    await browser?.close().catch(() => {});
    if (child.exitCode === null) await Promise.race([new Promise(resolve => child.once('exit', resolve)), delay(3000)]);
    if (child.exitCode === null) child.kill();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
