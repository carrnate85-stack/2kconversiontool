const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { Rpc, root, currentExe } = require('./rpc-client.cjs');

(async () => {
  const data = fs.mkdtempSync(path.join(root, 'artifacts/accessory-cache-'));
  const rpc = new Rpc({ env: { CMT_TEST_DATA: data } });
  const report = { version: path.basename(currentExe()), completed: false, cases: [] };
  const progress = () => rpc.events.filter(row => /Reading NBA 2K27 (?:hair|facialhair) manifest assets/.test(row.message || '')).length;
  try {
    const bootstrap = await rpc.call('bootstrap');
    for (const assetType of ['hair', 'facialhair']) {
      const start = performance.now();
      const first = await rpc.call('native-hair-catalog', { assetType });
      const firstMs = performance.now() - start;
      const before = progress();
      const cachedStart = performance.now();
      const cached = await rpc.call('native-hair-catalog', { assetType });
      const cachedMs = performance.now() - cachedStart;
      assert(first.assets.length > 100);
      assert.deepEqual(cached, first, 'Every catalog field and preview descriptor must be preserved');
      report.cases.push({ assetType, rows: first.assets.length, responseBytes: Buffer.byteLength(JSON.stringify(first)),
        firstMs: Math.round(firstMs), cachedMs: Math.round(cachedMs), workerReloads: progress() - before });
    }
    // Finish both baseline measurements before checking the new cache requirement.
    assert(report.cases.every(row => row.workerReloads === 0), 'Switching back to an unchanged accessory catalog launched another worker');

    const game = path.join(data, 'private-game');
    fs.mkdirSync(game);
    fs.writeFileSync(path.join(game, 'mod.exe'), 'Catalog-only fixture; never executed.');
    fs.writeFileSync(path.join(game, 'NBA2K27.exe'), 'Catalog-only fixture; never executed.');
    fs.mkdirSync(path.join(game, 'data/oodle'), { recursive: true });
    fs.writeFileSync(path.join(game, 'data/oodle/oo2core_9_win64.dll'), 'Catalog-only fixture; never loaded.');
    const manifest = path.join(game, 'manifest');
    const records = id => [
      'char/sig/png' + id + '.iff',
      'char/sig/png' + id + '_geo_hair_test.iff',
      'char/sig/png' + id + '_item_hair_test.iff',
      'char/sig/png' + id + '_config_test.iff',
      'char/sig/png' + id + '_geo_facialhair_beard_tangentspace.iff',
      'char/sig/png' + id + '_item_facialhair_beard.iff',
    ].map(name => name + ',0A,0,1\n').join('');
    fs.writeFileSync(manifest, records('29990'));
    await rpc.call('save-settings', { ...bootstrap.settings, game_root_2k27: game });
    const settingsPath = path.join(data, 'settings.json');
    const catalogs = async id => {
      for (const assetType of ['hair', 'facialhair']) {
        const value = await rpc.call('native-hair-catalog', { assetType });
        assert.equal(value.native, true, 'Fixture should exercise the native catalog path');
        assert.deepEqual(value.assets.map(asset => asset.sourcePng), [id]);
        const before = progress();
        assert.deepEqual(await rpc.call('native-hair-catalog', { assetType }), value);
        assert.equal(progress(), before);
      }
    };
    await catalogs('29990');
    fs.writeFileSync(manifest, records('29991'));
    await catalogs('29991');
    report.sameSizeManifestEdit = true;
    await rpc.call('reset');
    const beforeReset = progress();
    await catalogs('29991');
    assert.equal(progress(), beforeReset, 'New Swap should retain manifest-only catalog lists');
    report.resetRetainsLists = true;
    const alternate = path.join(data, 'alternate-game');
    fs.mkdirSync(alternate);
    fs.writeFileSync(path.join(alternate, 'mod.exe'), 'Catalog-only fixture; never executed.');
    fs.writeFileSync(path.join(alternate, 'NBA2K27.exe'), 'Catalog-only fixture; never executed.');
    fs.mkdirSync(path.join(alternate, 'data/oodle'), { recursive: true });
    fs.writeFileSync(path.join(alternate, 'data/oodle/oo2core_9_win64.dll'), 'Catalog-only fixture; never loaded.');
    fs.writeFileSync(path.join(alternate, 'manifest'), records('29992'));
    const settings = JSON.parse(fs.readFileSync(settingsPath, 'utf8'));
    fs.writeFileSync(settingsPath, JSON.stringify({ ...settings, game_root_2k27: alternate }));
    await catalogs('29992');
    report.externalSettingsEdit = true;
    fs.unlinkSync(path.join(alternate, 'manifest'));
    const fallback = await rpc.call('native-hair-catalog', { assetType: 'hair' });
    assert.notEqual(path.resolve(fallback.gameRoot).toLowerCase(), alternate.toLowerCase());
    fs.writeFileSync(path.join(alternate, 'manifest'), records('29992'));
    await catalogs('29992');
    report.preferredManifestRestoration = true;
    await assert.rejects(rpc.call('native-hair-catalog', { assetType: 'invalid' }), /Asset type must be hair or facialhair/);
    report.invalidTypeRejected = true;
    report.nativeFixtureCoverage = true;
    report.completed = true;
    console.log(JSON.stringify(report, null, 2));
  } finally {
    await rpc.close();
    fs.writeFileSync(path.join(root, 'artifacts/accessory-catalog-cache-report.json'), JSON.stringify(report, null, 2));
    assert.equal(path.dirname(fs.realpathSync(data)), fs.realpathSync(path.join(root, 'artifacts')));
    fs.rmSync(data, { recursive: true });
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
