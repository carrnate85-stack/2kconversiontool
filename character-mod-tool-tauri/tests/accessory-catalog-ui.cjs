const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { spawn } = require('node:child_process');
const { chromium } = require('C:/Users/carrn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright');
const { root, currentExe } = require('./rpc-client.cjs');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
(async () => {
  const data = fs.mkdtempSync(path.join(root, 'artifacts/catalog-ui-'));
  const records = id => ['.iff', '_geo_hair_test.iff', '_geo_facialhair_beard_tangentspace.iff']
    .map(suffix => 'char/sig/png' + id + suffix + ',0A,0,1\n').join('');
  const game = path.join(data, 'game');
  const alternate = path.join(data, 'alternate');
  for (const [folder, id] of [[game, '29990'], [alternate, '29993']]) {
    fs.mkdirSync(folder, { recursive: true });
    fs.writeFileSync(path.join(folder, 'mod.exe'), 'Catalog-only fixture; never executed.');
    fs.writeFileSync(path.join(folder, 'manifest'), records(id));
  }
  const child = spawn(currentExe(), ['--ui-test'], { windowsHide: true, stdio: 'ignore',
    env: { ...process.env, CMT_TEST_DATA: data } });
  const report = { version: path.basename(currentExe()), completed: false, errors: [] };
  let browser, page;
  try {
    for (let i = 0; i < 100; i++) { try { browser = await chromium.connectOverCDP('http://127.0.0.1:9764'); break; } catch { await delay(200); } }
    assert(browser);
    for (let i = 0; i < 100; i++) { page = browser.contexts().flatMap(c => c.pages()).find(p => p.url().includes('tauri.localhost')); if (page) break; await delay(100); }
    assert(page);
    page.on('pageerror', error => report.errors.push(error.message));
    await page.waitForFunction(() => document.getElementById('engineStatus')?.textContent.includes('Engine '));
    const configure = folder => page.evaluate(async folder => {
      fillSettings({ ...state.bootstrap.settings, game_root_2k27: folder });
      await saveSettings();
      if (state.bootstrap.settings.game_root_2k27 !== folder) throw new Error('Settings did not save');
    }, folder);
    const ids = () => page.evaluate(() => state.nativeHair.assets.map(asset => asset.sourcePng));
    await configure(game);
    await page.evaluate(() => loadNativeHairCatalog());
    assert.deepEqual(await ids(), ['29990']);
    const initialRevision = await page.evaluate(() => state.nativeHair.catalogRevision);
    assert.match(initialRevision, /^[0-9a-f]{64}$/);
    report.warmSourceCache = await page.evaluate(async () => {
      const value = { marker: 'warm' }, revision = state.nativeHair.catalogRevision;
      state.nativeHair.previewCache.set('fixture', value);
      state.nativeHair.headCache.set('fixture', value);
      await loadNativeHairCatalog(true);
      return state.nativeHair.catalogRevision === revision
        && state.nativeHair.previewCache.get('fixture') === value
        && state.nativeHair.headCache.get('fixture') === value;
    });
    assert(report.warmSourceCache);
    const nativeCatalog = () => page.evaluate(() => window.__TAURI__.core.invoke('engine_call', {
      command: 'native-hair-catalog', payload: { assetType: 'hair' },
    }));
    assert.equal((await nativeCatalog()).native, false, 'Incomplete fixture should use the compatibility catalog');
    fs.writeFileSync(path.join(game, 'NBA2K27.exe'), 'Presence-only fixture; never executed.');
    fs.mkdirSync(path.join(game, 'data/oodle'), { recursive: true });
    fs.writeFileSync(path.join(game, 'data/oodle/oo2core_9_win64.dll'), 'Presence-only fixture; never loaded.');
    await page.evaluate(() => loadNativeHairCatalog(true));
    const prerequisitesPresent = await nativeCatalog();
    assert.equal(prerequisitesPresent.native, true);
    assert.notEqual(prerequisitesPresent.sourceRevision, initialRevision);
    fs.unlinkSync(path.join(game, 'NBA2K27.exe'));
    await page.evaluate(() => loadNativeHairCatalog(true));
    const prerequisiteRemoved = await nativeCatalog();
    assert.equal(prerequisiteRemoved.native, false);
    assert.notEqual(prerequisiteRemoved.sourceRevision, prerequisitesPresent.sourceRevision);
    fs.writeFileSync(path.join(game, 'NBA2K27.exe'), 'Restored presence-only fixture; never executed.');
    await page.evaluate(() => loadNativeHairCatalog(true));
    const restored = await nativeCatalog();
    assert.equal(restored.native, true);
    fs.writeFileSync(path.join(game, 'data/oodle/oo2core_9_win64.dll'), 'Changed presence-only decoder fixture; never loaded.');
    await page.evaluate(() => loadNativeHairCatalog(true));
    assert.notEqual((await nativeCatalog()).sourceRevision, restored.sourceRevision);
    assert.deepEqual(await ids(), ['29990']);
    report.nativePrerequisiteRefresh = { fallbackToNative: true, removedExeUsesFallback: true,
      restoredExeUsesNative: true, changedDecoderRefreshesRevision: true };
    report.selectionPreserved = await page.evaluate(async () => {
      state.nativeHair.selectedId = state.nativeHair.assets[0].id;
      const selected = state.nativeHair.selectedId;
      await loadNativeHairCatalog(true);
      return state.nativeHair.selectedId === selected;
    });
    assert(report.selectionPreserved);
    await page.evaluate(() => setNativeHairType('facialhair'));
    fs.writeFileSync(path.join(game, 'manifest'), records('29991'));
    await page.evaluate(() => setNativeHairType('hair'));
    assert.deepEqual(await ids(), ['29991']);
    assert.notEqual(await page.evaluate(() => state.nativeHair.catalogRevision), initialRevision);
    assert.equal(await page.evaluate(() => state.nativeHair.previewCache.size + state.nativeHair.headCache.size), 0);
    report.categoryChangeRechecksManifest = true;
    fs.writeFileSync(path.join(game, 'manifest'), records('29992'));
    await page.evaluate(() => ensureNativeHairReady());
    assert.deepEqual(await ids(), ['29992']);
    report.tabEntryRechecksManifest = true;
    await configure(alternate);
    assert.equal(await page.evaluate(() => state.nativeHair.catalogLoaded), false);
    await page.evaluate(() => loadNativeHairCatalog());
    assert.deepEqual(await ids(), ['29993']);
    report.settingsChangeRefreshesSameCategory = true;
    report.catalogOwnership = await page.evaluate(async () => {
      const api = window.characterTool;
      const original = api.loadNativeHairCatalog;
      const requests = [];
      api.loadNativeHairCatalog = payload => new Promise((resolve, reject) => requests.push({ payload, resolve, reject }));
      try {
        state.nativeHair.catalogLoaded = false;
        const first = loadNativeHairCatalog();
        let repeatedFinished = false;
        const repeated = loadNativeHairCatalog().then(() => { repeatedFinished = true; });
        await Promise.resolve();
        if (repeatedFinished || requests.length !== 1) throw new Error('Catalog callers did not share the pending read');
        state.nativeHair.assetType = 'facialhair';
        state.nativeHair.catalogLoaded = false;
        const second = loadNativeHairCatalog();
        if (requests.length !== 2) throw new Error('New category was blocked by the older catalog read');
        requests[1].resolve({ assets: [{ id: 'current-beard' }] }); await second;
        requests[0].reject(new Error('Obsolete catalog failure')); await Promise.all([first, repeated]);
        if (state.nativeHair.assets[0]?.id !== 'current-beard' || state.nativeHair.loading) throw new Error('Obsolete catalog failure changed the current list');
        return { sharedPendingRead: true, latestCategoryWins: true, obsoleteErrorIgnored: true };
      } finally {
        api.loadNativeHairCatalog = original;
        state.nativeHair.assetType = 'hair';
        state.nativeHair.catalogLoaded = false;
        await loadNativeHairCatalog();
      }
    });
    report.targetOwnership = await page.evaluate(async () => {
      const api = window.characterTool;
      const original = api.loadNativeHairTarget, originalInstall = api.installNativeHair;
      const originalActive = window.configsEditor.active;
      const player = state.workingPlayer;
      const requests = [];
      let config = 'FIRST', installs = 0;
      api.loadNativeHairTarget = payload => new Promise((resolve, reject) => requests.push({ payload, resolve, reject }));
      api.installNativeHair = async () => { installs++; throw new Error('Unexpected stale install'); };
      window.configsEditor.active = () => config;
      try {
        state.workingPlayer = { path: 'C:/isolated-picker-test/player.iff' };
        state.nativeHair.selectedId = state.nativeHair.assets[0].id;
        const first = refreshNativeHairTarget();
        if (!document.getElementById('nativeHairSlot').disabled || !document.getElementById('installNativeHair').disabled) throw new Error('Target controls were enabled before slots loaded');
        config = 'SECOND';
        const second = refreshNativeHairTarget();
        requests[1].resolve({ targetPng: '29993', slots: [{ key: '@config', label: 'SECOND' }] }); await second;
        requests[0].resolve({ targetPng: '29990', slots: [{ key: '@config', label: 'FIRST' }] }); await first;
        if (document.getElementById('nativeHairSlot').selectedOptions[0]?.textContent !== 'SECOND') throw new Error('Old config replaced the current target label');
        if (document.getElementById('installNativeHair').disabled) throw new Error('Current target did not become installable');
        config = 'THIRD';
        updateNativeHairInstallState();
        await installNativeHair();
        if (!document.getElementById('installNativeHair').disabled || installs) throw new Error('Install accepted stale config slots');
        const third = refreshNativeHairTarget();
        state.workingPlayer = null;
        await refreshNativeHairTarget();
        requests[2].resolve({ targetPng: '29993', slots: [{ key: '@config', label: 'THIRD' }] }); await third;
        if (state.nativeHair.slots.length || !document.getElementById('nativeHairSlot').disabled || !document.getElementById('installNativeHair').disabled) throw new Error('Reset admitted an obsolete target response');
        return { pendingControlsDisabled: true, latestConfigWins: true, staleInstallRejected: true, resetIgnoresOldSlots: true };
      } finally {
        api.loadNativeHairTarget = original;
        api.installNativeHair = originalInstall;
        window.configsEditor.active = originalActive;
        state.workingPlayer = player;
        state.nativeHair.selectedId = '';
        invalidateNativeHairTarget();
      }
    });
    await configure(game);
    await page.evaluate(() => loadNativeHairCatalog());
    const source = await page.evaluate(() => ({ id: state.nativeHair.assets[0].id, revision: state.nativeHair.catalogRevision }));
    await page.evaluate(() => {
      state.nativeHair.previewCache.set('same-id', { marker: 'old-root' });
      state.nativeHair.headCache.set('same-head', { marker: 'old-root' });
      state.nativeHair.displayedPreviewKey = 'same-id';
    });
    fs.writeFileSync(path.join(alternate, 'manifest'), records('29992'));
    await configure(alternate);
    report.sameAssetDifferentRoot = await page.evaluate(async source => {
      await loadNativeHairCatalog();
      return state.nativeHair.assets[0].id === source.id
        && state.nativeHair.catalogRevision !== source.revision
        && state.nativeHair.previewCache.size === 0 && state.nativeHair.headCache.size === 0
        && state.nativeHair.displayedPreviewKey === '';
    }, source);
    assert(report.sameAssetDifferentRoot);
    report.settingsDuringPendingList = await page.evaluate(async () => {
      const api = window.characterTool, original = api.loadNativeHairCatalog;
      const requests = [];
      api.loadNativeHairCatalog = () => new Promise(resolve => requests.push(resolve));
      try {
        state.nativeHair.catalogLoaded = false;
        const first = loadNativeHairCatalog();
        await saveSettings();
        const second = loadNativeHairCatalog();
        if (requests.length !== 2) throw new Error('Settings commit reused a pre-commit catalog request');
        requests[1]({ assets: [{ id: 'current-source' }], sourceRevision: 'current' }); await second;
        requests[0]({ assets: [{ id: 'obsolete-source' }], sourceRevision: 'obsolete' }); await first;
        return state.nativeHair.assets[0].id === 'current-source' && state.nativeHair.catalogRevision === 'current';
      } finally { api.loadNativeHairCatalog = original; }
    });
    assert(report.settingsDuringPendingList);
    assert.deepEqual(report.errors, []);
    report.completed = true;
    console.log(JSON.stringify(report, null, 2));
  } finally {
    if (page) await page.evaluate(() => window.__TAURI__.core.invoke('engine_call', { command: 'close-app', payload: {} })).catch(() => {});
    await browser?.close().catch(() => {});
    if (child.exitCode === null) await Promise.race([new Promise(resolve => child.once('exit', resolve)), delay(5000)]);
    if (child.exitCode === null) { child.kill(); await new Promise(resolve => child.once('exit', resolve)); }
    fs.writeFileSync(path.join(root, 'artifacts/accessory-catalog-ui-report.json'), JSON.stringify(report, null, 2));
    assert.equal(path.dirname(fs.realpathSync(data)), fs.realpathSync(path.join(root, 'artifacts')));
    fs.rmSync(data, { recursive: true });
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
