const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { Rpc, root, currentExe } = require('./rpc-client.cjs');
(async () => {
  const folder = fs.mkdtempSync(path.join(root, 'artifacts/resource-rpc-'));
  const game = path.join(folder, 'game');
  const exe = process.env.CMT_TEST_RESOURCE_EXE || currentExe();
  const report = { executableSha256: crypto.createHash('sha256').update(fs.readFileSync(exe)).digest('hex'), geometryDecoded: false, completed: false };
  fs.mkdirSync(game);
  const manifest = [];
  for (let i = 0; i < 600; i++) {
    const png = String(30000 + i);
    for (const suffix of ['', '_geo_hair_test', '_item_hair_test', '_geo_facialhair_test', '_item_facialhair_test']) {
      manifest.push('char/sig/png' + png + suffix + '.iff,0A,0,1');
    }
  }
  manifest.push('char/sig/png40000_geo_hair_test.iff,0A,0,1');
  fs.writeFileSync(path.join(game, 'manifest'), manifest.join('\n') + '\n');
  fs.writeFileSync(path.join(game, 'mod.exe'), 'Resource registration fixture; never executed.');
  fs.writeFileSync(path.join(game, 'NBA2K27.exe'), 'Folder validation fixture; never executed.');
  fs.mkdirSync(path.join(game, 'data/oodle'), { recursive: true });
  fs.writeFileSync(path.join(game, 'data/oodle/oo2core_9_win64.dll'), 'Folder validation fixture; never loaded.');
  const rpc = new Rpc({ exe, env: { CMT_TEST_DATA: folder } });
  const prepare = (index, assetType = 'hair') => rpc.call('native-preview-hair', { assetType,
    assetId: `char/sig/png${30000 + index}_geo_${assetType}_test.iff` });
  const release = value => rpc.call('release-preview-resources', { lease: value.resourceLease });
  try {
    await rpc.call('bootstrap');
    await rpc.call('save-settings', { game_root_2k27: game });
    await assert.rejects(rpc.call('native-preview-hair', { assetType: 'hair', assetId: 'char/sig/png40000_geo_hair_test.iff' }));
    let stats = await rpc.call('preview-cache-stats');
    assert.equal(stats.registeredResources, 0); assert.equal(stats.resourceLeases, 0);
    report.failedPreparationRolledBack = true;
    let maximum = 0;
    for (let i = 0; i < 600; i++) {
      const value = await prepare(i, i % 2 ? 'facialhair' : 'hair');
      assert.equal(typeof value.resourceLease, 'string');
      stats = await rpc.call('preview-cache-stats');
      maximum = Math.max(maximum, stats.registeredResources);
      assert.equal(stats.registeredResources, 3); assert.equal(stats.resourceLeases, 1);
      await release(value);
    }
    stats = await rpc.call('preview-cache-stats');
    assert.equal(stats.registeredResources, 0); assert.equal(stats.resourceLeases, 0);
    report.sequentialSelections = 600; report.maximumRegisteredResources = maximum;
    const first = await prepare(0), second = await prepare(0);
    assert.equal(first.sourcePlayerPath, second.sourcePlayerPath);
    await release(first); await release(first);
    await rpc.call('file-stat', { path: second.sourcePlayerPath });
    await release(second);
    await assert.rejects(rpc.call('file-stat', { path: second.sourcePlayerPath }), /expired/);
    report.sharedReadersPreserved = true;
    const owners = [];
    for (let i = 0; i < 128; i++) owners.push(await prepare(0));
    await assert.rejects(prepare(0), /live preview resource owners/);
    for (const value of owners) await release(value);
    report.ownerBoundAndRecovery = true;
    const old = await prepare(0);
    await rpc.call('reset');
    const fresh = await prepare(0);
    assert.notEqual(old.resourceLease, fresh.resourceLease);
    await release(old);
    await rpc.call('file-stat', { path: fresh.sourcePlayerPath });
    await release(fresh);
    stats = await rpc.call('preview-cache-stats');
    assert.equal(stats.registeredResources, 0); assert.equal(stats.resourceLeases, 0);
    report.staleResetReleaseIgnored = true;
    report.finalStats = stats; report.completed = true;
    console.log(JSON.stringify(report, null, 2));
  } finally {
    await rpc.close();
    fs.writeFileSync(path.join(root, 'artifacts/preview-resource-rpc-report.json'), JSON.stringify(report, null, 2));
    assert.equal(path.dirname(fs.realpathSync(folder)), fs.realpathSync(path.join(root, 'artifacts')));
    assert(!fs.lstatSync(folder).isSymbolicLink());
    fs.rmSync(folder, { recursive: true });
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
