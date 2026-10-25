const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { spawn } = require('node:child_process');
const { chromium } = require('C:/Users/carrn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright');
const { PNG } = require('C:/Users/carrn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/pngjs');
const { root, currentExe } = require('./rpc-client.cjs');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const probe = process.argv.includes('--probe');
function differences(a, b) {
  const first = PNG.sync.read(Buffer.from(a.split(',')[1], 'base64')), second = PNG.sync.read(Buffer.from(b.split(',')[1], 'base64'));
  assert.equal(first.data.length, second.data.length); let count = 0;
  for (let i = 0; i < first.data.length; i += 4) if (Math.abs(first.data[i] - second.data[i]) + Math.abs(first.data[i+1] - second.data[i+1]) + Math.abs(first.data[i+2] - second.data[i+2]) > 6) count++;
  return count;
}
(async () => {
  const child = spawn(currentExe(), ['--ui-test'], { windowsHide: true, stdio: 'ignore' });
  const report = { version: path.basename(currentExe()), completed: false, probe, cases: [], errors: [] };
  let browser, page;
  try {
    for (let i = 0; i < 100; i++) { try { browser = await chromium.connectOverCDP('http://127.0.0.1:9764'); break; } catch { await delay(200); } }
    assert(browser);
    for (let i = 0; i < 100; i++) { page = browser.contexts().flatMap(c => c.pages()).find(p => p.url().includes('tauri.localhost')); if (page) break; await delay(100); }
    assert(page); page.on('pageerror', error => report.errors.push(error.message));
    await page.waitForFunction(() => document.getElementById('startupLoading')?.hidden);
    const stock = await page.evaluate(async () => (await window.characterTool.extractPlayer({ gameLabel: 'NBA 2K27', png: '06827' })).mainPath);
    const cases = [['stock', stock], ['prior-converted', path.resolve(root, '../character-mod-tool-rust/artifacts/conversion-output/png06827.iff')], ['fresh-converted', path.join(root, 'artifacts/ui-conversion-output/png06827.iff')]];
    for (const [label, target] of cases) {
      assert(fs.existsSync(target));
      await page.evaluate(async target => { state.activeView = 'full-swap'; await setWorkingPlayer(target); await setActiveView('preview'); }, target);
      await page.waitForFunction(() => state.preview.loadedPath && !state.preview.loading, null, { timeout: 180000 });
      const info = await page.evaluate(async () => {
        const viewer = window.modelPreview; viewer.focusHead();
        const camera = viewer.cameraState(), canvas = document.getElementById('previewCanvas');
        const payload = viewer.stats().payload;
        const meshes = payload.meshes.filter(m => m.sourceKind === 'geometry').map(m => {
          const min = [Infinity,Infinity,Infinity], max = [-Infinity,-Infinity,-Infinity];
          for (let i = 0; i < m.positions.length; i++) { const axis = i % 3; min[axis] = Math.min(min[axis],m.positions[i]); max[axis] = Math.max(max[axis],m.positions[i]); }
          return { source: m.sourceName, groups: m.groups, bounds: { min, max } };
        });
        const captures = [];
        for (const yaw of [0, Math.PI / 2, Math.PI]) {
          viewer.setCameraState({ ...camera, yaw, pitch: 0 });
          viewer.setMaterialTargetVisible('hair', true); viewer.setMaterialTargetVisible('facialhair', true);
          const visible = canvas.toDataURL();
          viewer.setMaterialTargetVisible('hair', false); const noHair = canvas.toDataURL();
          viewer.setMaterialTargetVisible('hair', true); viewer.setMaterialTargetVisible('facialhair', false); const noBeard = canvas.toDataURL();
          viewer.setMaterialTargetVisible('facialhair', true);
          captures.push({ yaw, visible, noHair, noBeard });
        }
        return { meshes, materials: viewer.stats().materialTextures, captures };
      });
      const angles = info.captures.map(c => ({ yaw: c.yaw, hairPixels: differences(c.visible,c.noHair), beardPixels: differences(c.visible,c.noBeard) }));
      fs.writeFileSync(path.join(root, 'artifacts/main-accessories-' + label + '.png'), Buffer.from(info.captures[1].visible.split(',')[1], 'base64'));
      report.cases.push({ label, target, meshes: info.meshes, materials: info.materials, angles });
      if (!probe) {
        assert(angles.some(a => a.hairPixels > 20), label + ' hair has no visible contribution');
        assert(angles.some(a => a.beardPixels > 20), label + ' beard has no visible contribution');
      }
    }
    assert.deepEqual(report.errors, []); report.completed = true;
    console.log(JSON.stringify({ ...report, cases: report.cases.map(c => ({ label:c.label, angles:c.angles, materials:c.materials.filter(m => /hair|beard/.test(m.name)) })) }, null, 2));
  } finally {
    if (page) await page.evaluate(() => window.__TAURI__.core.invoke('engine_call', { command: 'close-app', payload: {} })).catch(() => {});
    await browser?.close().catch(() => {});
    if (child.exitCode === null) await Promise.race([new Promise(resolve => child.once('exit', resolve)), delay(5000)]);
    if (child.exitCode === null) { child.kill(); await new Promise(resolve => child.once('exit', resolve)); }
    fs.writeFileSync(path.join(root, 'artifacts/main-accessory-visibility-report.json'), JSON.stringify(report, null, 2));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
