const fs = require('fs');
const path = require('path');
const assert = require('node:assert/strict');
const { spawn, execFileSync } = require('child_process');
const { chromium } = require('C:/Users/carrn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright');
const root = path.resolve(__dirname, '..');
const delay = ms => new Promise(r => setTimeout(r, ms));
const exeIndex = process.argv.indexOf('--exe');
const exe = exeIndex >= 0 ? path.resolve(process.argv[exeIndex + 1]) : require('./rpc-client.cjs').currentExe();
const target = path.resolve(root, '../character-mod-tool-rust/artifacts/conversion-output/png06827.iff');
(async () => {
  const child = spawn(exe, ['--ui-test'], { windowsHide: true, stdio: 'ignore' });
  let browser, page, fixtureFolder; const report = {}, errors = [];
  try {
    for (let i = 0; i < 100; i++) { try { browser = await chromium.connectOverCDP('http://127.0.0.1:9764'); break; } catch { await delay(200); } }
    if (!browser) throw new Error('Owned Tauri test window unavailable');
    for (let i = 0; i < 100; i++) { page = browser.contexts().flatMap(c => c.pages()).find(p => p.url().includes('tauri.localhost')); if (page) break; await delay(100); }
    page.on('pageerror', e => errors.push(e.message));
    await page.waitForFunction(() => document.getElementById('engineStatus')?.textContent.includes('Engine '), null, { timeout: 30000 });
    report.parity = await page.evaluate(async target => {
      const invoke = (command, payload) => window.__TAURI__.core.invoke('engine_call', { command, payload });
      const decode = resources => new Promise((resolve, reject) => {
        const w = new Worker('preview-worker.bundle.js');
        w.onmessage = async ({ data }) => {
          if (data.kind === 'native') {
            try { w.postMessage({ kind: 'native-reply', id: data.id, value: await invoke(data.command, data.payload) }); }
            catch (error) { w.postMessage({ kind: 'native-reply', id: data.id, error: String(error) }); }
          } else { w.terminate(); data.error ? reject(new Error(JSON.stringify(data.error))) : resolve(data.result); }
        };
        w.onerror = e => { w.terminate(); reject(new Error(e.message)); };
        w.postMessage({ id: 1, type: 'preview', payload: { filePath: resources.filePath, options: { ...resources.options, preferHighestDetail: true } } });
      });
      const digest = async bytes => Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', bytes))).map(x => x.toString(16).padStart(2, '0')).join('');
      window.__parityDecode = decode;
      const summarize = async preview => ({
        meshes: await Promise.all(preview.meshes.map(async m => ({ name: m.name, source: m.sourceName, kind: m.sourceKind,
          positions: await digest(new Float32Array(m.positions).buffer), indices: await digest(new Uint32Array(m.indices).buffer),
          uvs: await digest(new Float32Array(m.uvs || []).buffer) }))),
        textures: await Promise.all(preview.textures.map(async t => ({ name: t.name, bytes: await digest(Uint8Array.from(atob(t.bytes), c => c.charCodeAt(0))), range: t.range, binding: t.binding }))),
        triangles: preview.stats.triangles, projected: preview.stats.facialHairProjectedVertices,
        skipped: preview.skipped,
      });
      let start = performance.now(); const native = await invoke('native-preview-resources', { target });
      const nativeResourcesMs = performance.now() - start;
      start = performance.now(); const n = await decode(native); const nativeDecodeMs = performance.now() - start;
      start = performance.now(); const legacy = await invoke('preview-resources', { target }); const legacyResourcesMs = performance.now() - start;
      start = performance.now(); const l = await decode(legacy); const legacyDecodeMs = performance.now() - start;
      return { nativeResourcesMs, nativeDecodeMs, legacyResourcesMs, legacyDecodeMs,
        bodyFixture: legacy.options.companionPaths.find(item => item.kind === 'body-color' && /(?:^|\/)chest_color_/.test(item.name)),
        bodyFixtures: legacy.options.companionPaths.filter(item => ['body-color', 'body-detail'].includes(item.kind)),
        accessoryFixtures: legacy.options.companionPaths.filter(item => item.kind === 'item'),
        native: await summarize(n), legacy: await summarize(l) };
    }, target);
    const a = report.parity.native, b = report.parity.legacy;
    if (JSON.stringify(a.meshes) !== JSON.stringify(b.meshes)) throw new Error('Native/legacy geometry differs: inspect native-preview-report.json');
    for (const legacy of b.textures) {
      const native = a.textures.find(t => t.name === legacy.name);
      if (JSON.stringify(native) !== JSON.stringify(legacy)) throw new Error('Native/legacy DDS or TXTR bindings differ for ' + legacy.name);
    }
    report.additionalNativeTextures = a.textures.filter(t => !b.textures.some(l => l.name === t.name)).map(t => t.name);
    if (a.projected !== b.projected || a.triangles !== b.triangles) throw new Error('Tangent attachment or triangle counts differ');
    await page.evaluate(async target => { await setWorkingPlayer(target); setActiveView('preview'); }, target);
    await page.waitForFunction(() => state.preview.loadedPath && !state.preview.loading, null, { timeout: 120000 });
    report.cache = await page.evaluate(async target => {
      const first = await window.characterTool.loadStagedPreview({ target });
      const ids = window.modelPreview.stats().geometryIds;
      const start = performance.now(); const second = await window.characterTool.loadStagedPreview({ target });
      const decodeMs = performance.now() - start;
      const loaded = await window.modelPreview.load(second);
      const after = window.modelPreview.stats().geometryIds;
      const subset = structuredClone(second);
      subset.meshes = subset.meshes.filter(mesh => !String(mesh.sourceName).includes('_geo_hair_'));
      subset.geometryKey += ':accessory-regression';
      const partial = await window.modelPreview.load(subset);
      const subsetIds = window.modelPreview.stats().geometryIds;
      const retained = subsetIds.every(id => after.includes(id));
      await window.modelPreview.load(second);
      return { native: first.stats.nativeAssetReader, hit: second.stats.previewCacheHit, firstSkipped: first.skipped, secondSkipped: second.skipped, decodeMs, loaded,
        before: ids, after, partial, retained, subsetIds, memory: await window.characterTool.previewCacheStats() };
    }, target);
    if (!report.cache.native || !report.cache.hit) throw new Error('Native reader or preview cache was not used');
    if (JSON.stringify(report.cache.before) !== JSON.stringify(report.cache.after)) throw new Error('Refresh rebuilt unchanged GPU geometry');
    if (!report.cache.retained || report.cache.partial.reusedGeometry !== report.cache.subsetIds.length) throw new Error('Accessory change rebuilt unchanged body geometry');
    if (report.cache.memory.cacheBytes > report.cache.memory.cacheBudget) throw new Error('Native cache exceeds its memory budget');
    const folder = fixtureFolder = fs.mkdtempSync(path.join(root, 'artifacts/staged-body-priority-'));
    const staged = path.join(folder, path.basename(target));
    fs.copyFileSync(target, staged);
    assert.equal(report.parity.bodyFixtures.length, 4, 'All three body colors and the shared skin detail must be covered');
    assert.equal(report.parity.accessoryFixtures.length, 2, 'Hair and beard texture items must be covered');
    const fixtures = [...report.parity.bodyFixtures, ...report.parity.accessoryFixtures];
    const digestFile = file => require('node:crypto').createHash('sha256').update(fs.readFileSync(file)).digest('hex');
    const originalHashes = new Map([target, ...fixtures.map(item => item.path)].map(file => [file, digestFile(file)]));
    const python = path.resolve(root, '../character-mod-tool-webview2/dist/Character-Mod-Tool-WebView2-0.1.31/App/Backend/python_runtime/python.exe');
    const expected = [];
    for (let index = 0; index < fixtures.length; index++) {
      const fixture = fixtures[index];
      fixture.stagedPath = path.join(folder, path.basename(fixture.name));
      const result = JSON.parse(execFileSync(python, ['-B', path.join(__dirname, 'body-map-fixture.py'), fixture.path, fixture.stagedPath, String(index + 1)], { windowsHide: true, encoding: 'utf8' }));
      expected.push(...result.textures);
    }
    const prepare = () => page.evaluate(target => window.__TAURI__.core.invoke('engine_call', {
      command: 'native-preview-resources', payload: { target }
    }), staged);
    const local = await prepare();
    const original = await page.evaluate(target => window.__TAURI__.core.invoke('engine_call', {
      command: 'native-preview-resources', payload: { target }
    }), target);
    assert.equal(local.player.stagedBodyTextures, 4);
    for (const fixture of fixtures) {
      const stagedMap = local.options.companionPaths.find(item => item.name === path.basename(fixture.name));
      const sharedMap = original.options.companionPaths.find(item => item.name === fixture.name);
      assert(stagedMap && stagedMap.path !== sharedMap?.path, 'Staged body map did not override ' + fixture.name);
      if (fixture.kind.startsWith('body-')) assert(!local.options.companionPaths.some(item => item.name === fixture.name), 'Manifest map was loaded beside its staged override');
    }
    const decoded = (fallback = false) => page.evaluate(async ({ target, fallback }) => {
      const preview = fallback
        ? await window.__parityDecode(await window.__TAURI__.core.invoke('engine_call', { command: 'preview-resources', payload: { target } }))
        : await window.characterTool.loadStagedPreview({ target });
      const textures = await Promise.all(preview.textures.map(async texture => ({
        name: texture.name, range: texture.range, binding: texture.binding,
        bytes: Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', Uint8Array.from(atob(texture.bytes), c => c.charCodeAt(0))))).map(x => x.toString(16).padStart(2, '0')).join(''),
      })));
      return { textures, native: preview.stats.nativeAssetReader };
    }, { target: staged, fallback });
    const actual = await decoded();
    const fallbackActual = await decoded(true);
    assert.equal(actual.native, true);
    report.stagedPriority = { completed: false, expected, actual: actual.textures, baseline: b.textures };
    const preparedDds = [];
    for (const fixture of fixtures) {
      const resource = local.options.companionPaths.find(item => item.name === path.basename(fixture.name));
      const archive = await page.evaluate(async path => window.__TAURI__.core.invoke('engine_call', { command: 'read-archive', payload: { path } }), resource.path);
      for (const member of archive.members.filter(item => /\.dds$/i.test(item.name))) {
        preparedDds.push({ name: member.name, bytes: require('node:crypto').createHash('sha256').update(Buffer.from(member.data, 'base64')).digest('hex') });
      }
    }
    for (const texture of expected) {
      assert.equal(preparedDds.find(item => item.name === texture.name)?.bytes, texture.sha256, 'Native archive replaced staged DDS with shared TLD: ' + texture.name);
      const baseline = b.textures.find(item => item.name === texture.name);
      if (!baseline) continue; // The inherited preview budget does not upload every skin-detail channel.
      const matching = actual.textures.filter(item => item.name === texture.name);
      assert.equal(matching.length, 1, 'Body texture should not be duplicated: ' + texture.name);
      assert.equal(matching[0].bytes, texture.sha256, 'Preview did not use the modified staged DDS: ' + texture.name);
      assert.deepEqual(matching[0], fallbackActual.textures.find(item => item.name === texture.name), 'Native staged texture differs from WPF fallback: ' + texture.name);
      assert.notEqual(matching[0].bytes, baseline.bytes);
      assert.deepEqual(matching[0].range, baseline.range, 'Staged TXTR range changed');
      assert.deepEqual(matching[0].binding, baseline.binding, 'Staged region/channel changed');
    }
    const legs = fixtures.find(item => /\/legs_color_/.test(item.name));
    assert(legs);
    fs.unlinkSync(legs.stagedPath);
    const partial = await prepare();
    assert.equal(partial.player.stagedBodyTextures, 3);
    assert(partial.options.companionPaths.some(item => item.name === legs.name), 'Missing staged legs must fall back to manifest');
    const afterRemoval = await decoded();
    for (const texture of expected) {
      const baseline = b.textures.find(item => item.name === texture.name);
      if (!baseline) continue;
      const expectedBytes = /legs_color/i.test(texture.name) ? baseline.bytes : texture.sha256;
      assert.equal(afterRemoval.textures.find(item => item.name === texture.name)?.bytes, expectedBytes, 'Fallback displaced an unrelated staged map');
    }
    const torso = fixtures.find(item => /\/torso_color_/.test(item.name));
    assert(torso);
    const changedTexture = JSON.parse(execFileSync(python, ['-B', path.join(__dirname, 'body-map-fixture.py'), torso.path, torso.stagedPath, '37'], { windowsHide: true, encoding: 'utf8' })).textures[0];
    const changed = new Date(Date.now() + 20000);
    fs.utimesSync(torso.stagedPath, changed, changed);
    const updated = await prepare();
    assert.notEqual(updated.fingerprint, partial.fingerprint, 'Changed staged map reused stale preview fingerprint');
    const afterEdit = await decoded();
    assert.equal(afterEdit.textures.find(item => item.name === changedTexture.name)?.bytes, changedTexture.sha256);
    for (const [file, hash] of originalHashes) assert.equal(digestFile(file), hash, 'Original fixture/source was modified');
    report.stagedPriority = { maps: fixtures.map(item => item.name), decodedDds: expected.length, uploadedDdsChecked: expected.filter(item => b.textures.some(texture => texture.name === item.name)).length, stagedFirst: true, fallbackMatches: true, txtrRangeAndBindingsPreserved: true, singleRegionFallback: true, changesInvalidate: true, originalsUnchanged: true };
    await page.screenshot({ path: path.join(root, 'artifacts/native-preview.png') });
    await page.evaluate(() => window.characterTool.resetSession());
    report.afterReset = await page.evaluate(() => window.characterTool.previewCacheStats());
    if (report.afterReset.cacheBytes !== 0 || report.afterReset.archives !== 0) throw new Error('New Swap retained preview payloads');
    report.errors = errors;
    if (errors.length) throw new Error(errors.join('\n'));
    console.log(JSON.stringify({ parity: true, nativeResourcesMs: report.parity.nativeResourcesMs, nativeDecodeMs: report.parity.nativeDecodeMs, legacyResourcesMs: report.parity.legacyResourcesMs, legacyDecodeMs: report.parity.legacyDecodeMs, cache: report.cache }, null, 2));
  } finally {
    fs.writeFileSync(path.join(root, 'artifacts/native-preview-report.json'), JSON.stringify({ ...report, errors }, null, 2));
    if (page) await page.evaluate(() => window.__TAURI__.core.invoke('engine_call', { command: 'close-app', payload: {} })).catch(() => {});
    if (browser) await browser.close().catch(() => {});
    await delay(500); if (child.exitCode === null) child.kill();
    if (fixtureFolder) {
      assert.equal(path.dirname(fs.realpathSync(fixtureFolder)), fs.realpathSync(path.join(root, 'artifacts')));
      for (const entry of fs.readdirSync(fixtureFolder)) {
        const file = path.join(fixtureFolder, entry);
        assert(fs.lstatSync(file).isFile() && !fs.lstatSync(file).isSymbolicLink());
        fs.unlinkSync(file);
      }
      fs.rmdirSync(fixtureFolder);
    }
  }
})().catch(e => { console.error(e); process.exitCode = 1; });
