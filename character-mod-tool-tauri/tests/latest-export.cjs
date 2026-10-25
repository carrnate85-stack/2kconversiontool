const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const { spawn } = require('child_process');
const { chromium } = require('C:/Users/carrn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright');
const root = path.resolve(__dirname, '..');
const target = process.env.CMT_TEST_TARGET || 'C:/Users/carrn/AppData/Local/CharacterModToolTauri/outputs/png6720.iff';
const delay = ms => new Promise(r => setTimeout(r, ms));
const snapshot = () => Object.fromEntries(fs.readdirSync(path.dirname(target)).filter(n => n.startsWith(path.basename(target, '.iff')) && n.endsWith('.iff')).map(n => [n, crypto.createHash('sha256').update(fs.readFileSync(path.join(path.dirname(target), n))).digest('hex')]));
(async () => {
  const before = snapshot(), report = {};
  const child = spawn(require('./rpc-client.cjs').currentExe(), ['--ui-test'], { windowsHide: true, stdio: 'ignore' });
  let browser, page;
  try {
    for (let i = 0; i < 100; i++) { try { browser = await chromium.connectOverCDP('http://127.0.0.1:9764'); break; } catch { await delay(200); } }
    if (!browser) throw new Error('Owned test window unavailable');
    for (let i = 0; i < 100; i++) { page = browser.contexts().flatMap(c => c.pages()).find(p => p.url().includes('tauri.localhost')); if (page) break; await delay(100); }
    if (!page) throw new Error('Owned test page unavailable');
    await page.waitForFunction(() => document.getElementById('engineStatus')?.textContent.includes('Engine '));
    await page.evaluate(async target => { await setWorkingPlayer(target); setActiveView('preview'); }, target);
    await page.waitForFunction(() => state.preview.loadedPath && !state.preview.loading, null, { timeout: 120000 });
    report.asset = await page.evaluate(async target => {
      const preview = await window.characterTool.loadStagedPreview({ target });
      const range = values => ({ min: Math.min(...values), max: Math.max(...values), finite: values.every(Number.isFinite) });
      const invoke = (command, payload) => window.__TAURI__.core.invoke('engine_call', { command, payload });
      const resources = await invoke('native-preview-resources', { target });
      const scenes = [];
      for (const companion of resources.options.companionPaths.filter(x => /hair/i.test(x.name))) {
        const archive = await invoke('read-archive', { path: companion.path });
        scenes.push({ name: companion.name, metadata: archive.members.filter(x => /\.(scne|txtr)$/i.test(x.name)).map(x => ({ name: x.name, value: JSON.parse(atob(x.data)) })) });
      }
      window.testPreview = preview;
      return { name: preview.name, native: preview.stats.nativeAssetReader, items: preview.activeItems,
        hair: preview.meshes.filter(m => /hair/i.test(m.sourceName)).map(m => ({ name: m.name, source: m.sourceName, vertices: m.positions.length / 3, groups: m.groups, uv: range(Array.from(m.uvs)), bounds: range(Array.from(m.positions)) })),
        textures: preview.textures.filter(t => /hair/i.test(t.name)).map(t => ({ name: t.name, source: t.source, range: t.range, binding: t.binding, assignments: t.assignments, inputColorSpace: t.inputColorSpace })),
        scenes };
    }, target);
    await page.waitForFunction(() => state.preview.loadedPath && !state.preview.loading, null, { timeout: 120000 });
    report.materials = await page.evaluate(() => {
      window.modelPreview.focusHead();
      const camera = window.modelPreview.cameraState();
      window.modelPreview.setCameraState({ ...camera, yaw: Math.PI / 2, pitch: 0 });
      return window.modelPreview.stats().materialTextures.filter(m => /hair/i.test(m.name));
    });
    await page.screenshot({ path: path.join(root, 'artifacts/latest-export-after.png') });
    report.maps = await page.evaluate(async () => {
      const { createExplorerRenderer } = await import('./explorer/model-preview.js');
      const canvas = document.createElement('canvas'); canvas.width = canvas.height = 256;
      const context = createExplorerRenderer({ canvas });
      try {
        await context.api.load(window.testPreview, () => {}, { deferRender: true });
        const result = [];
        for (const target of ['hair', 'facialhair']) {
          const texture = context.boundTextureMatching(target, 'color', /./);
          if (!texture) continue;
          const decoded = context.decodedTexturePixels(texture);
          const image = new Uint8ClampedArray(decoded.pixels);
          let transparent = 0, opaque = 0;
          for (let i = 3; i < image.length; i += 4) { if (image[i] < 32) transparent++; if (image[i] > 223) opaque++; }
          const imageCanvas = document.createElement('canvas'); imageCanvas.width = imageCanvas.height = decoded.size;
          imageCanvas.getContext('2d').putImageData(new ImageData(image, decoded.size, decoded.size), 0, 0);
          result.push({ target, width: decoded.size, height: decoded.size, transparent, opaque, png: imageCanvas.toDataURL('image/png') });
        }
        return result;
      } finally { context.disposeRoot(); context.renderer.dispose(); }
    });
    for (const map of report.maps) { fs.writeFileSync(path.join(root, 'artifacts/latest-' + map.target + '-color.png'), Buffer.from(map.png.split(',')[1], 'base64')); delete map.png; }
    report.source = await page.evaluate(async target => {
      const source = await window.characterTool.extractPlayer({ gameLabel: 'NBA 2K23', png: '6720', role: 'source' });
      const hair = await window.characterTool.loadHairOptions({ source: source.mainPath, target, sourceGame: 'NBA 2K23', targetGame: 'NBA 2K27' });
      return { source, hair };
    }, target);
    const sourceFolder = path.dirname(report.source.source.mainPath);
    const savedSource = path.join(root, 'artifacts/latest-source-2k23');
    fs.mkdirSync(savedSource, { recursive: true });
    for (const name of fs.readdirSync(sourceFolder).filter(n => n.endsWith('.iff'))) fs.copyFileSync(path.join(sourceFolder, name), path.join(savedSource, name));
    report.source.savedFolder = savedSource;
    if (JSON.stringify(before) !== JSON.stringify(snapshot())) throw new Error('Investigation changed exported files');
    report.exportsUnchanged = true;
    console.log(JSON.stringify({ ...report, asset: { ...report.asset, scenes: undefined } }, null, 2));
  } finally {
    fs.writeFileSync(path.join(root, 'artifacts/latest-export-report.json'), JSON.stringify(report, null, 2));
    if (page) await page.evaluate(() => window.__TAURI__.core.invoke('engine_call', { command: 'close-app', payload: {} })).catch(() => {});
    if (browser) await browser.close().catch(() => {});
    await delay(500); if (child.exitCode === null) child.kill();
  }
})().catch(e => { console.error(e); process.exitCode = 1; });
