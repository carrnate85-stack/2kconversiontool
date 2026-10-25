const fs = require('fs');
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const path = require('path');
const { spawn } = require('child_process');
const { chromium } = require('C:/Users/carrn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright');
const { PNG } = require('C:/Users/carrn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/pngjs');
const root = path.resolve(__dirname, '..');
const exe = require('./rpc-client.cjs').currentExe();
const target = path.resolve(root, '../character-mod-tool-rust/artifacts/conversion-output/png06827.iff');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
function pixels(bytes) {
  const png = PNG.sync.read(bytes);
  let varied = 0;
  for (let i = 0; i < png.data.length; i += 4) if (Math.max(...png.data.subarray(i, i + 3)) - Math.min(...png.data.subarray(i, i + 3)) > 20 && png.data[i] > 40) varied++;
  return { width: png.width, height: png.height, coloredPixels: varied };
}
(async () => {
  const process = spawn(exe, ['--ui-test'], { windowsHide: true, stdio: 'ignore' });
  const errors = [], report = {};
  let browser, page;
  try {
    for (let i = 0; i < 100; i++) {
      try { browser = await chromium.connectOverCDP('http://127.0.0.1:9764'); break; } catch { await delay(200); }
    }
    if (!browser) throw new Error('Tauri test WebView did not start.');
    for (let i = 0; i < 100; i++) { page = browser.contexts().flatMap(context => context.pages()).find(page => page.url().includes('tauri.localhost')); if (page) break; await delay(100); }
    if (!page) throw new Error('No owned Tauri frontend found: ' + browser.contexts().flatMap(context => context.pages()).map(page => page.url()).join(', '));
    page.on('pageerror', error => errors.push(error.message));
    await page.waitForFunction(() => document.getElementById('engineStatus')?.textContent.includes('Engine '), null, { timeout: 30000 });
    await page.waitForFunction(() => document.getElementById('startupLoading')?.hidden, null, { timeout: 30000 });
    report.startup = await page.locator('#engineStatus').innerText();
    report.tabs = await page.locator('.nav-item').allTextContents();
    if (report.tabs.length !== 8) throw new Error('Production tabs missing.');
    await page.screenshot({ path: path.join(root, 'artifacts/tauri-full-swap.png') });
    await page.evaluate(async target => { await setWorkingPlayer(target); }, target);
    await page.locator('#previewTab').click();
    await page.waitForFunction(() => state.preview.loadedPath && !state.preview.loading, null, { timeout: 180000 });
    report.preview = await page.evaluate(() => ({ models: document.getElementById('previewMeshCount').textContent, textures: document.getElementById('previewTextureCount').textContent, jersey: state.preview.jerseyAvailable, warnings: document.getElementById('previewWarnings')?.textContent }));
    const hashPlayer = () => crypto.createHash('sha256').update(fs.readFileSync(target)).digest('hex');
    const playerHash = hashPlayer(), jerseyBeforeEdit = await page.evaluate(() => state.preview.jersey);
    await page.locator('#previewBodyFitToggle').click();
    const dimension = page.locator('#previewBodyFitControls input[type="number"]').first();
    assert.equal(await dimension.isDisabled(), false, 'Body dimension remained disabled after real preview loading');
    const dimensionOriginal = await dimension.inputValue();
    await dimension.fill(String(Number(dimensionOriginal) - 1));
    const validBounds = await page.evaluate(() => window.modelPreview.bounds());
    await dimension.fill('');
    const incomplete = await page.evaluate(() => ({ bounds: window.modelPreview.bounds(), invalid: state.preview.bodyFitInvalidInputs.size, blocked: document.getElementById('previewApplyFit').disabled }));
    assert.equal(incomplete.invalid, 1); assert(incomplete.blocked); assert.deepEqual(incomplete.bounds, validBounds);
    assert([...incomplete.bounds.min, ...incomplete.bounds.max].every(Number.isFinite));
    assert.equal(await dimension.getAttribute('aria-invalid'), 'true');
    await page.locator('#previewBodyFitPanel').screenshot({ path: path.join(root, 'artifacts/body-fit-invalid-input.png') });
    await dimension.fill(String(Number(dimensionOriginal) + 1));
    assert.equal(await page.locator('#previewApplyFit').isDisabled(), false);
    await page.locator('#previewRevertFit').click();
    assert.equal(await page.locator('#previewBodyFitControls input[type="number"]').first().inputValue(), dimensionOriginal);
    await page.locator('#previewCloseBodyFit').click();
    if (!jerseyBeforeEdit && await page.locator('#previewJersey').isChecked()) await page.locator('#previewJersey').uncheck();
    assert.equal(hashPlayer(), playerHash, 'Preview-only edit changed the player file');
    report.bodyInput = { enabledAfterLoading: true, incompletePreservesFiniteGeometry: true, correctedUnblocksApply: true, revertRestoresValue: true, sourceFileUnchanged: true };
    const handoff = await page.evaluate(async () => {
      const preview = await window.characterTool.loadStagedPreview({ target: state.workingPlayer.path });
      const ordinary = new Set(preview.meshes.filter(mesh => mesh.sourceKind === 'geometry' && !String(mesh.sourceName).includes('_tangentspace.iff')).map(mesh => String(mesh.sourceName).toLowerCase()));
      const meshes = preview.meshes.filter(mesh => mesh.sourceKind === 'geometry' && (!String(mesh.sourceName).includes('_tangentspace.iff') || !ordinary.has(String(mesh.sourceName).toLowerCase().replace('_tangentspace.iff', '.iff'))))
        .map(mesh => ({ name: mesh.name, kind: /facialhair|beard/i.test(mesh.sourceName) ? 'facialhair' : 'hair', sourceName: mesh.sourceName, positions: Array.from(mesh.positions), uvs: Array.from(mesh.uvs || []), indices: Array.from(mesh.indices), groups: mesh.groups || [] }));
      const textures = preview.textures.filter(texture => texture.bytes).map(texture => ({ name: texture.name, bytes: texture.bytes, binding: texture.binding || null, assignments: texture.assignments || [], inputColorSpace: texture.inputColorSpace || 'none', range: texture.range || null }));
      return { transport: preview.stats.nativeTransport, summary: preview.meshes.map(mesh => ({ name: mesh.name, source: mesh.sourceName, kind: mesh.sourceKind, vertices: mesh.positions.length / 3 })), bundle: { meshes, textures, eye: { irisColor: preview.player.irisColor, irisSize: preview.player.irisSize } } };
    });
    report.meshes = handoff.summary;
    report.previewTransport = handoff.transport;
    if (!(handoff.transport?.binaryPackets > 0)) throw new Error('Packaged preview did not use binary transport.');
    fs.writeFileSync(path.join(root, 'artifacts/blender-test-bundle.json'), JSON.stringify(handoff.bundle));
    const blenderSetup = await page.evaluate(async target => ({ settings: (await window.characterTool.bootstrap()).settings, resources: await window.__TAURI__.core.invoke('engine_call', { command: 'preview-resources', payload: { target, includeJersey: true } }) }), target);
    const jersey = blenderSetup.resources.options.companionPaths.find(row => row.kind === 'preview-jersey' && /jersey/i.test(row.name));
    fs.copyFileSync(jersey.path, path.join(root, 'artifacts/blender-test-jersey.iff'));
    fs.writeFileSync(path.join(root, 'artifacts/blender-test-setup.json'), JSON.stringify({ settings: blenderSetup.settings, target }));
    console.log(JSON.stringify(report.preview));
    const canvas = page.locator('#previewCanvas');
    const first = await canvas.screenshot({ path: path.join(root, 'artifacts/tauri-preview-canvas.png') });
    report.pixels = pixels(first);
    if (report.pixels.coloredPixels < 500) throw new Error('Player preview canvas is blank.');
    const box = await canvas.boundingBox();
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.down(); await page.mouse.move(box.x + box.width / 2 + 95, box.y + box.height / 2, { steps: 12 }); await page.mouse.up();
    await delay(500);
    const second = await canvas.screenshot();
    report.rotationChanged = !first.equals(second);
    if (!report.rotationChanged) throw new Error('Preview does not rotate.');
    if (report.preview.jersey) await page.locator('#previewJersey').check();
    await page.screenshot({ path: path.join(root, 'artifacts/tauri-preview-jersey.png') });
    await page.setViewportSize({ width: 900, height: 650 });
    await delay(500);
    report.compactPixels = pixels(await canvas.screenshot());
    await page.screenshot({ path: path.join(root, 'artifacts/tauri-preview-compact.png') });
    if (report.compactPixels.coloredPixels < 500) throw new Error('Compact preview is blank.');
    await page.setViewportSize({ width: 1364, height: 861 });
    await page.locator('#bodyTab').click();
    await page.waitForFunction(() => !state.body.loading, null, { timeout: 30000 });
    report.bodyValues = await page.evaluate(() => Object.keys(state.body.targetBodyFit).length);
    if (report.bodyValues < 20) throw new Error('Body fit values missing.');
    await page.locator('#configsTab').click();
    await page.waitForFunction(() => document.getElementById('configsList')?.children.length > 0);
    report.configs = await page.locator('#configsList').innerText();
    await page.locator('#hairTab').click();
    await page.waitForFunction(() => !state.nativeHair.loading && state.nativeHair.assets.length > 0, null, { timeout: 90000 });
    report.hairCatalog = await page.evaluate(() => state.nativeHair.assets.length);
    const beard = await page.evaluate(async () => {
      const result = await window.characterTool.loadNativeHairCatalog({ assetType: 'facialhair' });
      const asset = result.assets.find(x => x.sourcePng === '06827') || result.assets[0];
      const preview = await window.characterTool.previewNativeHair({ assetType: 'facialhair', assetId: asset.id, includeBase: true });
      return { count: result.assets.length, models: preview.asset.meshes.length, headModels: preview.base.meshes.length, textures: preview.asset.textures.length, projected: preview.stats };
    });
    report.beard = beard;
    if (!beard.models || !beard.headModels) throw new Error('Beard or original head preview missing.');
    if (!beard.textures) throw new Error('Beard catalog textures missing.');
    if (!(beard.projected.nativeTransport?.binaryPackets > 0)) throw new Error('Catalog preview did not use binary transport.');
    report.swapBridge = await page.evaluate(async outputDir => {
      const api = window.characterTool;
      const source = await api.extractPlayer({ gameLabel: 'NBA 2K23', png: '4100' });
      const target = await api.extractPlayer({ gameLabel: 'NBA 2K27', png: '06827' });
      const request = { source: source.mainPath, target: target.mainPath, sourceGame: 'NBA 2K23', targetGame: 'NBA 2K27', outputDir, shrinkwrapBody: true, hairEnabled: false, facialHairEnabled: true };
      const options = await api.loadFacialHairOptions(request);
      request.facialHairSource = options.candidates[0].path;
      request.facialHairTargetKey = options.slots[0].key;
      let progress = 0;
      const offProgress = api.onJobEvent(() => progress++);
      try {
        return await new Promise(async (resolve, reject) => {
          let jobId;
          const offComplete = api.onJobComplete(event => { offComplete(); offFailed(); resolve({ jobId, completedId: event.jobId, progress, validation: event.result.validation, output: event.result.output }); });
          const offFailed = api.onJobFailed(event => { offComplete(); offFailed(); reject(new Error(event.error)); });
          try {
            state.source = await api.inspectCharacter(request.source); state.source.gameLabel = 'NBA 2K23';
            state.target = await api.inspectCharacter(request.target); state.target.gameLabel = 'NBA 2K27';
            jobId = (await api.startSwap(request)).jobId; state.activeJob = jobId;
          } catch (error) { offComplete(); offFailed(); reject(error); }
        });
      } finally { offProgress(); }
    }, path.join(root, 'artifacts/ui-conversion-output'));
    if (report.swapBridge.validation.errors || !report.swapBridge.progress || report.swapBridge.jobId !== report.swapBridge.completedId) throw new Error('Full swap event bridge failed.');
    assert.equal(report.swapBridge.validation.warnings, 0, 'Full Swap retained an invalid distance reference');
    assert.deepEqual(report.swapBridge.validation.highestDetailReferences.map(row => row.item.toLowerCase()).sort(), ['facialhair_beard', 'hair_dreads']);
    await page.waitForFunction(output => state.workingPlayer?.path?.toLowerCase() === output.toLowerCase(), report.swapBridge.output, { timeout: 30000 });
    report.stagedAfterSwap = await page.evaluate(() => ({ path: state.workingPlayer.path, source: state.workingSource?.path, activeJob: state.activeJob }));
    report.errors = errors;
    if (errors.length) throw new Error(errors.join('\n'));
    console.log(JSON.stringify(report, null, 2));
    fs.writeFileSync(path.join(root, 'artifacts/tauri-ui-report.json'), JSON.stringify(report, null, 2));
  } finally {
    if (page) report.lastState = await page.evaluate(() => ({ preview: document.getElementById('previewStatus')?.textContent, empty: document.getElementById('previewEmpty')?.textContent, log: document.getElementById('activityLog')?.textContent, loading: state.preview.loading })).catch(() => null);
    fs.writeFileSync(path.join(root, 'artifacts/tauri-ui-report.json'), JSON.stringify({ ...report, errors }, null, 2));
    if (page) await page.evaluate(() => window.__TAURI__.core.invoke('engine_call', { command: 'close-app', payload: {} })).catch(() => {});
    if (browser) await browser.close().catch(() => {});
    await delay(500);
    if (process.exitCode === null) process.kill();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
