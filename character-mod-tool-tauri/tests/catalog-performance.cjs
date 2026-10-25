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
  let browser, page;
  try {
    for (let i = 0; i < 100; i++) { try { browser = await chromium.connectOverCDP('http://127.0.0.1:9764'); break; } catch { await delay(200); } }
    assert(browser);
    for (let i = 0; i < 100; i++) { page = browser.contexts().flatMap(c => c.pages()).find(p => p.url().includes('tauri.localhost')); if (page) break; await delay(100); }
    assert(page);
    page.on('pageerror', error => report.errors.push(error.message));
    await page.waitForFunction(() => document.getElementById('engineStatus')?.textContent.includes('Engine '));
    report.cases = await page.evaluate(async () => {
      const rows = [];
      const cases = [
        ['hair', 'char/sig/png06827_geo_hair_dreads.iff'],
        ['hair', 'char/sig/png06827_geo_hair_dreads_tangentspace.iff'],
        ['facialhair', 'char/sig/png06827_geo_facialhair_beard.iff'],
        ['facialhair', 'char/sig/png06827_geo_facialhair_beard_tangentspace.iff'],
        ['hair', 'char/sig/png06572_geo_hair_curly_afro_tangentspace.iff'],
      ];
      for (const assetType of ['hair', 'facialhair']) {
        const catalog = await window.characterTool.loadNativeHairCatalog({ assetType });
        const donorIds = new Set(['06827', '06572']);
        for (const asset of catalog.assets) {
          if (donorIds.has(asset.sourcePng)) continue;
          donorIds.add(asset.sourcePng);
          cases.push([assetType, asset.id]);
          if (donorIds.size >= 6) break;
        }
      }
      const compare = (actual, expected, label, tolerance = 0) => {
        if (actual.length !== expected.length) throw new Error(label + ' length mismatch ' + actual.length + '/' + expected.length);
        let max = 0, squared = 0;
        for (let i = 0; i < actual.length; i++) { const difference = Math.abs(actual[i] - expected[i]); max = Math.max(max, difference); squared += difference * difference; }
        if (max > tolerance) throw new Error(label + ' differs by ' + max);
        return { max, rms: Math.sqrt(squared / Math.max(1, actual.length)) };
      };
      for (const [assetType, assetId] of cases) {
        const start = performance.now();
        const native = await window.characterTool.previewNativeHair({ assetType, assetId, includeBase: true });
        const nativeMs = performance.now() - start;
        if (!native.stats.nativeGeometry) throw new Error('Unexpected native fallback: ' + assetId + ' ' + native.asset.skipped.join('; '));
        const legacyStart = performance.now();
        const legacy = await window.characterTool.previewNativeHair({ assetType, assetId, includeBase: true, nativeGeometry: false });
        const legacyMs = performance.now() - legacyStart;
        if (legacy.stats.nativeGeometry) throw new Error('Fallback test did not use original geometry decoder');
        const checks = {};
        for (const part of ['asset', 'base']) {
          if (native[part].meshes.length !== legacy[part].meshes.length) throw new Error(assetId + ' ' + part + ' model count mismatch');
          checks[part] = native[part].meshes.map((mesh, i) => {
            const before = legacy[part].meshes[i];
            if (JSON.stringify(mesh.groups) !== JSON.stringify(before.groups)) throw new Error(assetId + ' ' + part + ' groups mismatch');
            return { vertices: mesh.positions.length / 3, positions: compare(mesh.positions, before.positions, part + ' positions', 0.0001),
              uv: compare(mesh.uvs, before.uvs, assetId + ' ' + part + ' UV ' + JSON.stringify({ native: Array.from(mesh.uvs).slice(0, 10), legacy: Array.from(before.uvs).slice(0, 10) }), 0.000001), indices: compare(mesh.indices, before.indices, part + ' indices'),
              sourceIds: compare(mesh.sourceVertexIds, before.sourceVertexIds, part + ' source identities') };
          });
        }
        if (JSON.stringify(native.asset.textures) !== JSON.stringify(legacy.asset.textures)) throw new Error(assetId + ' texture metadata/payload mismatch');
        rows.push({ assetType, assetId, nativeMs, legacyMs, nativeStats: native.stats, legacyStats: legacy.stats, checks });
      }
      return rows;
    });
    report.revisit = await page.evaluate(async () => {
      const request = { assetType: 'facialhair', assetId: 'char/sig/png06827_geo_facialhair_beard_tangentspace.iff', includeBase: false };
      const first = await window.characterTool.previewNativeHair(request);
      const next = await window.characterTool.previewNativeHair(request);
      return { first: first.stats, next: next.stats, baseOmitted: next.base === null, cache: await window.characterTool.previewCacheStats() };
    });
    assert(report.revisit.next.headCacheHit && report.revisit.next.tangentFrameCacheHit);
    assert(report.revisit.baseOmitted);
    assert(report.revisit.cache.heads <= 8 && report.revisit.cache.headCacheBytes <= report.revisit.cache.headCacheBudget);
    await page.locator('#hairTab').click();
    await page.waitForFunction(() => state.nativeHair.assets.length && !state.nativeHair.loading, null, { timeout: 90000 });
    await page.evaluate(async () => {
      state.nativeHair.assetType = 'hair';
      const asset = state.nativeHair.assets.find(a => a.id === 'char/sig/png06827_geo_hair_dreads_tangentspace.iff');
      if (!asset) throw new Error('Preview asset absent from catalog');
      await previewNativeHairAsset(asset);
    });
    await page.waitForFunction(() => document.getElementById('nativeHairPreviewLoading').hidden);
    const canvas = page.locator('#nativeHairBaseCanvas');
    const first = await page.locator('.native-hair-preview-shell').screenshot({ path: path.join(root, 'artifacts/catalog-performance.png') });
    const image = PNG.sync.read(first);
    await page.evaluate(() => window.hairAssetPreview.setVisible(false));
    await delay(100);
    const hidden = PNG.sync.read(await page.locator('.native-hair-preview-shell').screenshot());
    await page.evaluate(() => window.hairAssetPreview.setVisible(true));
    let hairPixels = 0;
    for (let i = image.width * 48 * 4; i < image.data.length; i += 4) {
      if (Math.max(...[0, 1, 2].map(channel => Math.abs(image.data[i + channel] - hidden.data[i + channel]))) > 12) hairPixels++;
    }
    assert(hairPixels > 250, 'Hair overlay must change actual canvas pixels');
    const box = await canvas.boundingBox();
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.down(); await page.mouse.move(box.x + box.width / 2 + 80, box.y + box.height / 2, { steps: 10 }); await page.mouse.up();
    await delay(300);
    assert(!first.equals(await page.locator('.native-hair-preview-shell').screenshot()));
    report.render = { hairPixels, rotationChanged: true };
    await page.evaluate(async () => { await window.characterTool.resetSession(); });
    report.reset = await page.evaluate(() => window.characterTool.previewCacheStats());
    assert.equal(report.reset.headCacheBytes, 0);
    assert.equal(report.reset.cacheBytes, 0);
    assert.deepEqual(report.errors, []);
    console.log(JSON.stringify({ cases: report.cases.map(row => ({ asset: row.assetId, nativeMs: row.nativeMs, legacyMs: row.legacyMs, nativeGeometryMs: row.nativeStats.nativeGeometryMs })), revisit: report.revisit, render: report.render, errors: report.errors }, null, 2));
  } finally {
    fs.writeFileSync(path.join(root, 'artifacts/catalog-performance-report.json'), JSON.stringify(report, null, 2));
    await browser?.close(); child.kill();
    if (child.exitCode === null) await new Promise(resolve => child.once('exit', resolve));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
