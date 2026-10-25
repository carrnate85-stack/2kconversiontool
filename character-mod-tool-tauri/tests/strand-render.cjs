const assert = require('assert/strict');
const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');
const { chromium } = require('C:/Users/carrn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright');
const { PNG } = require('C:/Users/carrn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/pngjs');
const { root, currentExe } = require('./rpc-client.cjs');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
(async () => {
  const child = spawn(currentExe(), ['--ui-test'], { windowsHide: true, stdio: 'ignore' });
  const report = { cases: [], errors: [] };
  let browser;
  try {
    for (let i = 0; i < 100; i++) {
      try { browser = await chromium.connectOverCDP('http://127.0.0.1:9764'); break; }
      catch { await delay(200); }
    }
    assert(browser, 'Isolated test window unavailable');
    let page;
    for (let i = 0; i < 100; i++) {
      page = browser.contexts().flatMap(c => c.pages()).find(p => p.url().includes('tauri.localhost'));
      if (page) break;
      await delay(100);
    }
    assert(page);
    page.on('pageerror', error => report.errors.push(error.message));
    page.on('console', msg => { if (msg.type() === 'error') report.errors.push(msg.text()); });
    await page.waitForFunction(() => document.getElementById('engineStatus')?.textContent.includes('Engine '));
    for (const assetId of [
      'char/sig/png06572_geo_hair_curly_afro_tangentspace.iff',
      'char/sig/png00143_geo_hair_straight.iff',
      'char/sig/png00159_geo_hair_fade.iff',
      'char/sig/png06827_geo_facialhair_beard.iff',
      'char/sig/png06827_geo_facialhair_beard_tangentspace.iff',
    ]) {
      const result = await page.evaluate(async assetId => {
        const preview = await window.characterTool.previewNativeHair({ assetId,
          assetType: /facialhair/.test(assetId) ? 'facialhair' : 'hair', includeBase: true });
        const { createExplorerRenderer } = await import('./explorer/model-preview.js');
        const canvas = document.createElement('canvas');
        canvas.width = canvas.height = 640; document.body.append(canvas);
        const context = createExplorerRenderer({ canvas, picker: true });
        await context.api.load({ ...preview.asset, meshes: [...preview.base.meshes, ...preview.asset.meshes] });
        const uv = preview.asset.meshes[0].uvs;
        let low = Infinity, high = -Infinity;
        for (const value of uv) { low = Math.min(low, value); high = Math.max(high, value); }
        const capture = () => { context.renderer.render(context.scene, context.camera); return canvas.toDataURL(); };
        const visible = capture();
        const strands = [];
        context.modelRoot.traverse(object => {
          const materials = Array.isArray(object.material) ? object.material : [object.material];
          if (materials.some(m => m?.userData.strandSpecularResponse)) strands.push(object);
        });
        strands.forEach(object => { object.visible = false; });
        const hidden = capture();
        strands.forEach(object => { object.visible = true; });
        const camera = context.cameraState();
        context.setCameraState({ ...camera, yaw: camera.yaw + .6 });
        const rotated = capture();
        const materials = context.api.stats().materialTextures.filter(m => m.strandSpecularResponse);
        const uvSemantics = preview.asset.meshes.map(mesh => ({ texture: mesh.textureUvSemantic,
          attachment: mesh.attachmentUvSemantic || null, alternate: Boolean(mesh.attachmentUvs?.length) }));
        context.disposeRoot(); context.renderer.dispose(); canvas.remove();
        return { visible, hidden, rotated, materials, uvRange: [low, high], uvSemantics };
      }, assetId);
      const images = {};
      for (const key of ['visible', 'hidden', 'rotated']) images[key] = PNG.sync.read(Buffer.from(result[key].split(',')[1], 'base64'));
      const differences = (a, b) => {
        assert.equal(a.data.length, b.data.length);
        let count = 0;
        for (let i = 0; i < a.data.length; i += 4) {
          if (Math.abs(a.data[i] - b.data[i]) + Math.abs(a.data[i + 1] - b.data[i + 1]) + Math.abs(a.data[i + 2] - b.data[i + 2]) > 6) count++;
        }
        return count;
      };
      const visiblePixels = differences(images.visible, images.hidden);
      const rotatedPixels = differences(images.visible, images.rotated);
      assert(visiblePixels > 20, 'Strands invisible: ' + assetId);
      assert(rotatedPixels > 100, 'Rotation did not change the image');
      assert(result.materials.length, 'Strand response missing');
      assert(result.materials.every(m=>!m.transparent&&m.depthWrite&&['coverage','cutout'].includes(m.strandAlphaMode)), 'Strand coverage missing');
      assert(result.uvRange[1] - result.uvRange[0] > .01, 'Collapsed texture UVs');
      for (const semantic of result.uvSemantics) {
        assert(['TEXCOORD0', 'TEXCOORD1'].includes(semantic.texture), 'Native texture UV semantic missing');
        if (semantic.alternate) assert.equal(semantic.attachment, semantic.texture === 'TEXCOORD0' ? 'TEXCOORD1' : 'TEXCOORD0');
      }
      const file = path.basename(assetId, '.iff');
      fs.writeFileSync(path.join(root, 'artifacts/strand-' + file + '.png'), PNG.sync.write(images.visible));
      report.cases.push({ assetId, visiblePixels, rotatedPixels, materials: result.materials, uvRange: result.uvRange, uvSemantics: result.uvSemantics });
    }
    assert.deepEqual(report.errors, []);
    console.log(JSON.stringify({ cases: report.cases.map(c => ({ assetId: c.assetId, visiblePixels: c.visiblePixels, uvRange: c.uvRange })), errors: report.errors }, null, 2));
  } finally {
    fs.writeFileSync(path.join(root, 'artifacts/strand-render-report.json'), JSON.stringify(report, null, 2));
    await browser?.close(); child.kill();
    if (child.exitCode === null) await new Promise(resolve => child.once('exit', resolve));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
