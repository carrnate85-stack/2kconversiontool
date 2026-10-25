const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawn } = require('node:child_process');
const { chromium } = require('C:/Users/carrn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright');
const { PNG } = require('C:/Users/carrn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/pngjs');
const { root, currentExe } = require('./rpc-client.cjs');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const hash = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const changedPixels = (before, after) => {
  const a = PNG.sync.read(before), b = PNG.sync.read(after);
  assert.equal(a.width, b.width); assert.equal(a.height, b.height);
  let changed = 0;
  for (let i = 0; i < a.data.length; i += 4) {
    if (Math.max(...[0, 1, 2].map(channel => Math.abs(a.data[i + channel] - b.data[i + channel]))) > 8) changed++;
  }
  return changed;
};
(async () => {
  const inputs = JSON.parse(fs.readFileSync(path.join(root, 'artifacts/accessory-conversion-parity-report.json')));
  assert(inputs.completed && inputs.cases.length === 3);
  const snapshots = Object.fromEntries(inputs.cases.map(item => [item.output, hash(item.output)]));
  const child = spawn(currentExe(), ['--ui-test'], { windowsHide: true, stdio: 'ignore',
    env: { ...process.env, CMT_TEST_DATA: path.join(root, 'artifacts/accessory-preview-data') } });
  const report = { checkedUtc: new Date().toISOString(), completed: false, cases: [], errors: [] };
  let browser, page;
  try {
    for (let i = 0; i < 100; i++) {
      try { browser = await chromium.connectOverCDP('http://127.0.0.1:9764'); break; } catch { await delay(200); }
    }
    assert(browser, 'Owned accessory-preview window did not start');
    for (let i = 0; i < 100; i++) {
      page = browser.contexts().flatMap(context => context.pages()).find(page => page.url().includes('tauri.localhost'));
      if (page) break; await delay(100);
    }
    assert(page);
    page.on('pageerror', error => report.errors.push(error.message));
    page.on('console', message => {
      if (message.type() === 'error' && /shader|webgl/i.test(message.text())) report.errors.push(message.text());
    });
    await page.waitForFunction(() => document.getElementById('engineStatus')?.textContent.includes('Engine '));
    await page.setViewportSize({ width: 1364, height: 861 });
    for (const item of inputs.cases) {
      await page.evaluate(async target => { await setWorkingPlayer(target); setActiveView('preview'); }, item.output);
      await page.waitForFunction(target => state.preview.loadedPath === target && !state.preview.loading, item.output, { timeout: 120000 });
      const summary = await page.evaluate(async () => {
        const preview = await window.characterTool.loadStagedPreview({ target: state.workingPlayer.path });
        const beards = preview.meshes.filter(mesh => /_geo_facialhair_/i.test(mesh.sourceName));
        if (!beards.length || !beards.every(mesh => mesh.positions.length && Array.from(mesh.positions).every(Number.isFinite))) throw new Error('Missing or invalid beard geometry');
        window.modelPreview.focusHead();
        const camera = window.modelPreview.cameraState();
        window.modelPreview.setCameraState({ ...camera, yaw: 0, pitch: 0 });
        window.accessoryTestPreview = preview;
        window.accessoryTestCamera = window.modelPreview.cameraState();
        return { beardMeshes: beards.map(mesh => ({ source: mesh.sourceName, vertices: mesh.positions.length / 3 })),
          projected: preview.stats.facialHairProjectedVertices,
          materials: window.modelPreview.stats().materialTextures.filter(material => /facialhair|beard/i.test(material.name)),
          skipped: preview.skipped };
      });
      report.current = { mode: item.mode, ...summary };
      assert(summary.materials.some(material => material.map && material.lightmap && material.anisotropyMap), 'Beard color/lightmap/tangent maps are not all bound');
      if (item.mode === 'yes') {
        assert(summary.projected > 0, 'Tangent-only beard must be reconstructed');
        assert(summary.beardMeshes.every(mesh => /_tangentspace\.iff$/i.test(mesh.source)));
      }
      if (item.mode === 'no') assert(summary.beardMeshes.every(mesh => !/_tangentspace\.iff$/i.test(mesh.source)));
      await delay(200);
      const canvas = page.locator('#previewCanvas');
      const before = await canvas.screenshot({ path: path.join(root, 'artifacts/accessory-preview-' + item.mode + '.png') });
      await page.evaluate(async () => {
        const preview = structuredClone(window.accessoryTestPreview);
        preview.meshes = preview.meshes.filter(mesh => !/_geo_facialhair_/i.test(mesh.sourceName));
        preview.geometryKey += ':beard-visibility-check';
        await window.modelPreview.load(preview);
        window.modelPreview.setCameraState(window.accessoryTestCamera);
      });
      await delay(200);
      const visiblePixels = changedPixels(before, await canvas.screenshot());
      assert(visiblePixels > 20, 'Beard geometry exists but does not visibly render');
      await page.evaluate(async () => { await window.modelPreview.load(window.accessoryTestPreview); window.modelPreview.setCameraState(window.accessoryTestCamera); });
      const first = await canvas.screenshot();
      const box = await canvas.boundingBox();
      await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
      await page.mouse.down(); await page.mouse.move(box.x + box.width / 2 + 60, box.y + box.height / 2, { steps: 8 }); await page.mouse.up();
      await delay(200);
      assert(!first.equals(await canvas.screenshot()), 'Converted beard preview must rotate');
      await page.setViewportSize({ width: 900, height: 650 });
      await page.screenshot({ path: path.join(root, 'artifacts/accessory-preview-' + item.mode + '-compact.png') });
      await page.setViewportSize({ width: 1364, height: 861 });
      report.cases.push({ source: item.sourcePng, target: item.targetPng, mode: item.mode, ...summary, visiblePixels, rotationChanged: true });
      console.log(item.mode + ': ' + visiblePixels + ' visible beard pixels; maps and rotation passed');
    }
    for (const [file, before] of Object.entries(snapshots)) assert.equal(hash(file), before);
    assert.deepEqual(report.errors, []);
    report.outputsUnchanged = true; report.completed = true;
    delete report.current;
  } finally {
    fs.writeFileSync(path.join(root, 'artifacts/accessory-preview-report.json'), JSON.stringify(report, null, 2));
    if (page) await page.evaluate(() => window.__TAURI__.core.invoke('engine_call', { command: 'close-app', payload: {} })).catch(() => {});
    await browser?.close().catch(() => {});
    if (child.exitCode === null) await Promise.race([new Promise(resolve => child.once('exit', resolve)), delay(3000)]);
    if (child.exitCode === null) child.kill();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
