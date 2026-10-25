const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const { Rpc, root } = require('./rpc-client.cjs');
(async () => {
  const directory = fs.mkdtempSync(path.join(root, 'artifacts/catalog-invalidation-'));
  const data = path.join(directory, 'app');
  const game = path.join(directory, 'game');
  const alternate = path.join(directory, 'alternate-game');
  const report = { version: JSON.parse(fs.readFileSync(path.join(root, 'dist/BUILD.json'), 'utf8').replace(/^\uFEFF/, '')).version, completed: false };
  const row = id => 'char/sig/png' + id + '.iff,0A,0,1\n';
  for (const folder of [game, alternate]) {
    fs.mkdirSync(folder, { recursive: true });
    fs.writeFileSync(path.join(folder, 'mod.exe'), 'Catalog-only fixture; never executed.');
  }
  const manifest = path.join(game, 'manifest');
  fs.writeFileSync(manifest, row('29990'));
  fs.writeFileSync(path.join(alternate, 'manifest'), row('29999'));
  const rpc = new Rpc({ env: { CMT_TEST_DATA: data } });
  try {
    await rpc.call('bootstrap');
    await rpc.call('save-settings', { game_root_2k27: game });
    const catalog = () => rpc.call('catalog', { gameLabel: 'NBA 2K27' });
    const ids = value => value.rows.map(row => row.png);
    assert.deepEqual(ids(await catalog()), ['29990']);
    const events = rpc.events.length;
    assert.deepEqual(ids(await catalog()), ['29990']);
    assert.equal(rpc.events.length, events, 'Unchanged catalog should reuse the fast cache');
    report.unchangedCacheHit = true;
    fs.writeFileSync(manifest, row('29990') + row('29991'));
    const refreshed = await catalog();
    const production = path.resolve(root, '../character-mod-tool-webview2/dist/Character-Mod-Tool-WebView2-0.1.31/App/Backend');
    report.originalReader = JSON.parse(execFileSync(path.join(production, 'python_runtime/python.exe'), ['-B', '-c',
      'import sys,json; sys.path.insert(0,sys.argv[1]); from character_manifest import build_character_manifest_catalog; print(json.dumps([r["png"] for r in build_character_manifest_catalog(sys.argv[2],{},"NBA 2K27")]))',
      production, manifest], { windowsHide: true, encoding: 'utf8' }));
    report.afterManifestEdit = ids(refreshed);
    assert.deepEqual(report.originalReader, ['29990', '29991']);
    assert.deepEqual(report.afterManifestEdit, report.originalReader, 'Catalog cache served stale rows after manifest changed');
    fs.writeFileSync(manifest, row('29990') + row('29992'));
    assert.deepEqual(ids(await catalog()), ['29990', '29992'], 'Same-size manifest replacement must invalidate');
    report.sameSizeReplacement = true;
    const settingsPath = path.join(data, 'settings.json');
    const settings = JSON.parse(fs.readFileSync(settingsPath, 'utf8').replace(/^\uFEFF/, ''));
    fs.writeFileSync(settingsPath, JSON.stringify({ ...settings, game_root_2k27: alternate }));
    const changedRoot = await catalog();
    assert.equal(path.resolve(changedRoot.gameRoot).toLowerCase(), path.resolve(alternate).toLowerCase());
    assert.deepEqual(ids(changedRoot), ['29999']);
    report.externalSettingsEdit = true;
    fs.unlinkSync(path.join(alternate, 'manifest'));
    const fallback = await catalog();
    assert.notEqual(path.resolve(fallback.gameRoot).toLowerCase(), path.resolve(alternate).toLowerCase(), 'Removed manifest must not retain old catalog');
    report.removedManifestRechecksDiscovery = true;
    fs.writeFileSync(path.join(alternate, 'manifest'), row('29999'));
    const restored = await catalog();
    report.restoredRoot = restored.gameRoot;
    assert.equal(path.resolve(restored.gameRoot).toLowerCase(), path.resolve(alternate).toLowerCase(), 'Restored preferred manifest must replace a fallback-root cache');
    assert.deepEqual(ids(restored), ['29999']);
    report.restoredPreferredManifest = true;
    fs.unlinkSync(path.join(alternate, 'mod.exe'));
    const missingExtractor = await catalog();
    assert.notEqual(path.resolve(missingExtractor.gameRoot).toLowerCase(), path.resolve(alternate).toLowerCase());
    fs.writeFileSync(path.join(alternate, 'mod.exe'), 'Catalog-only fixture; never executed.');
    assert.equal(path.resolve((await catalog()).gameRoot).toLowerCase(), path.resolve(alternate).toLowerCase());
    report.extractorPresenceRechecksDiscovery = true;
    report.completed = true;
    console.log(JSON.stringify(report, null, 2));
  } finally {
    await rpc.close();
    fs.writeFileSync(path.join(root, 'artifacts/catalog-invalidation-report.json'), JSON.stringify(report, null, 2));
    assert.equal(path.dirname(fs.realpathSync(directory)), fs.realpathSync(path.join(root, 'artifacts')));
    fs.rmSync(directory, { recursive: true });
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
