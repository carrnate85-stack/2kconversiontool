const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { Rpc, root, currentExe } = require('./rpc-client.cjs');
(async () => {
  const data = fs.mkdtempSync(path.join(root, 'artifacts/cache-ownership-'));
  const target = path.resolve(root, '../character-mod-tool-rust/artifacts/conversion-output/png06827.iff');
  const rpc = new Rpc({ env: { CMT_TEST_DATA: data } });
  const report = { version: path.basename(currentExe()), completed: false, cases: [] };
  const digest = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
  const original = digest(target);
  try {
    await rpc.call('bootstrap');
    const resources = await rpc.call('native-preview-resources', { target });
    const archive = await rpc.call('read-archive', { path: resources.filePath });
    const before = await rpc.call('preview-cache-stats');
    assert(before.archiveCacheBytes > 0 && before.archives > 0);
    for (const [command, payload] of [
      ['templates-save', { hair: { png: '06827', hair: 'dreads' } }],
      ['cli-language-set', { language: 'en' }],
    ]) {
      await rpc.call(command, payload);
      const after = await rpc.call('preview-cache-stats');
      report.cases.push({ command, beforeBytes: before.archiveCacheBytes, afterBytes: after.archiveCacheBytes });
      assert.equal(after.archiveCacheBytes, before.archiveCacheBytes, command + ' discarded unrelated preview geometry');
      assert.equal(after.archives, before.archives);
      assert.deepEqual(await rpc.call('read-archive', { path: resources.filePath }), archive);
    }
    const player = path.join(data, 'player', path.basename(target));
    fs.mkdirSync(path.dirname(player));
    fs.copyFileSync(target, player);
    const privateResources = await rpc.call('native-preview-resources', { target: player });
    await rpc.call('read-archive', { path: privateResources.filePath });
    assert((await rpc.call('preview-cache-stats')).archives > 0);
    await rpc.call('body-fit-save', { target: player, changes: { chestTHICK: 1.5 } });
    assert.equal((await rpc.call('preview-cache-stats')).cacheBytes, 0, 'Player edit must discard old geometry');
    assert.equal((await rpc.call('body-fit', { target: player })).targetBodyFit.chestTHICK, 1.5);
    const editedResources = await rpc.call('native-preview-resources', { target: player });
    assert.notEqual(editedResources.fingerprint, privateResources.fingerprint);
    await rpc.call('read-archive', { path: editedResources.filePath });
    assert((await rpc.call('preview-cache-stats')).archiveCacheBytes > 0);
    report.playerEditInvalidates = true;
    await rpc.call('reset');
    const reset = await rpc.call('preview-cache-stats');
    assert.equal(reset.cacheBytes, 0);
    assert.equal(reset.archives, 0);
    report.resetClearsPayloads = true;
    assert.equal(digest(target), original);
    report.originalPlayerUnchanged = true;
    report.completed = true;
    console.log(JSON.stringify(report, null, 2));
  } finally {
    await rpc.close();
    fs.writeFileSync(path.join(root, 'artifacts/preview-cache-ownership-report.json'), JSON.stringify(report, null, 2));
    assert.equal(path.dirname(fs.realpathSync(data)), fs.realpathSync(path.join(root, 'artifacts')));
    fs.rmSync(data, { recursive: true });
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
