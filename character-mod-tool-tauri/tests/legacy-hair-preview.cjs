const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { spawn } = require('node:child_process');
const { chromium } = require('C:/Users/carrn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright');
const { PNG } = require('C:/Users/carrn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/pngjs');
const { root, currentExe } = require('./rpc-client.cjs');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
(async () => {
  const combined = process.argv.includes('--combined');
  const prefix = combined ? 'combined-swap-preview-' : 'legacy-hair-preview-';
  const inputs = JSON.parse(fs.readFileSync(path.join(root, 'artifacts/' + (combined ? 'combined-swap' : 'legacy-hair') + '-parity-report.json')));
  assert(inputs.completed);
  const cases = combined ? [{ game: 'NBA 2K23', output: inputs.output }] : inputs.cases;
  const child = spawn(currentExe(), ['--ui-test'], { windowsHide: true, stdio: 'ignore',
    env: { ...process.env, CMT_TEST_DATA: path.join(root, 'artifacts/accessory-preview-data') } });
  const report = { completed: false, cases: [], errors: [] };
  let browser, page;
  try {
    for (let i = 0; i < 100; i++) {
      try { browser = await chromium.connectOverCDP('http://127.0.0.1:9764'); break; } catch { await delay(200); }
    }
    assert(browser);
    for (let i = 0; i < 100; i++) {
      page = browser.contexts().flatMap(context => context.pages()).find(page => page.url().includes('tauri.localhost'));
      if (page) break; await delay(100);
    }
    assert(page); page.on('pageerror', error => report.errors.push(error.message));
    await page.waitForFunction(() => document.getElementById('engineStatus')?.textContent.includes('Engine '));
    for (const item of cases) {
      await page.evaluate(async target => { await setWorkingPlayer(target); setActiveView('preview'); }, item.output);
      await page.waitForFunction(target => state.preview.loadedPath === target && !state.preview.loading, item.output, { timeout: 120000 });
      const summary = await page.evaluate(async () => {
        const preview = await window.characterTool.loadStagedPreview({ target: state.workingPlayer.path });
        const hair = preview.meshes.filter(mesh => /_geo_hair_/i.test(mesh.sourceName) && !mesh.tangentSpaceAttachment);
        if (!hair.length || !hair.every(mesh => mesh.positions.length && Array.from(mesh.positions).every(Number.isFinite))) throw new Error('Missing converted hair');
        window.modelPreview.focusHead();
        const camera = window.modelPreview.cameraState();
        window.modelPreview.setCameraState({ ...camera, distance: camera.distance * .65 });
        window.legacyHairTest = { preview, camera: window.modelPreview.cameraState() };
        return { meshes: hair.map(mesh => ({ source: mesh.sourceName, vertices: mesh.positions.length / 3 })),
          beardMeshes: preview.meshes.filter(mesh => /_geo_facialhair_/i.test(mesh.sourceName)).map(mesh => ({ source: mesh.sourceName, vertices: mesh.positions.length / 3 })),
          beardMaterials: window.modelPreview.stats().materialTextures.filter(material => /facialhair|beard/i.test(material.name)),
          materials: window.modelPreview.stats().materialTextures.filter(material => /hair/i.test(material.name) && !/facialhair|beard/i.test(material.name)) };
      });
      assert(summary.materials.some(material => material.map && material.lightmap && material.anisotropyMap));
      if (combined) {
        assert.equal(summary.meshes.reduce((sum, mesh) => sum + mesh.vertices, 0), inputs.highestHairProof.vertices);
        assert(summary.meshes.every(mesh => mesh.source.startsWith('png28001_')));
        assert(summary.beardMeshes.length && summary.beardMeshes.every(mesh => mesh.source.startsWith('png28001_')));
        assert(summary.beardMaterials.some(material => material.map && material.lightmap && material.anisotropyMap));
      }
      await delay(200);
      const canvas = page.locator('#previewCanvas');
      const before = PNG.sync.read(await canvas.screenshot({ path: path.join(root, 'artifacts/' + prefix + item.game.replace(/ /g, '-') + '.png') }));
      await page.evaluate(async () => {
        const preview = structuredClone(window.legacyHairTest.preview);
        preview.meshes = preview.meshes.filter(mesh => !/_geo_hair_/i.test(mesh.sourceName));
        preview.geometryKey += ':visibility-check';
        await window.modelPreview.load(preview); window.modelPreview.setCameraState(window.legacyHairTest.camera);
      });
      await delay(200);
      const after = PNG.sync.read(await canvas.screenshot());
      let visiblePixels = 0;
      for (let i = 0; i < before.data.length; i += 4) {
        if (Math.max(...[0, 1, 2].map(channel => Math.abs(before.data[i + channel] - after.data[i + channel]))) > 8) visiblePixels++;
      }
      assert(visiblePixels > 50, 'Converted hair is not visibly rendered');
      report.cases.push({ game: item.game, ...summary, visiblePixels });
      console.log(item.game + ': ' + visiblePixels + ' visible converted hair pixels');
    }
    assert.deepEqual(report.errors, []); report.completed = true;
  } finally {
    fs.writeFileSync(path.join(root, 'artifacts/' + prefix + 'report.json'), JSON.stringify(report, null, 2));
    if (page) await page.evaluate(() => window.__TAURI__.core.invoke('engine_call', { command: 'close-app', payload: {} })).catch(() => {});
    await browser?.close().catch(() => {});
    if (child.exitCode === null) await Promise.race([new Promise(resolve => child.once('exit', resolve)), delay(3000)]);
    if (child.exitCode === null) child.kill();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
