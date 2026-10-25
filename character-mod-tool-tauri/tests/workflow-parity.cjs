const assert = require('assert/strict');
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const { Rpc, root } = require('./rpc-client.cjs');
const folder = path.join(root, 'artifacts/workflow-parity');
const source = path.join(root, 'artifacts/highest-hair-output');
const digest = p => crypto.createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const snapshot = () => Object.fromEntries(fs.readdirSync(source).filter(n => /^png6720.*\.iff$/i.test(n)).map(n => [n, digest(path.join(source, n))]));
(async () => {
  if (fs.existsSync(folder)) {
    assert(!fs.lstatSync(folder).isSymbolicLink());
    assert.equal(path.dirname(fs.realpathSync(folder)), fs.realpathSync(path.join(root, 'artifacts')));
    fs.rmSync(folder, { recursive: true });
  }
  fs.mkdirSync(folder, { recursive: true });
  for (const name of fs.readdirSync(source).filter(n => /^png6720.*\.iff$/i.test(n))) fs.copyFileSync(path.join(source, name), path.join(folder, name));
  const before = snapshot(), report = {}, rpc = new Rpc();
  const target = path.join(folder, 'png6720.iff');
  try {
    report.bootstrap = await rpc.call('bootstrap');
    report.configs = await rpc.call('configs-list', { target });
    const original = report.configs.defaultConfig;
    let result = await rpc.call('configs-change', { target, config: original, action: 'create', name: 'PARITY_TEST' });
    assert(result.configs.some(c => c.name === 'PARITY_TEST'));
    assert(fs.existsSync(path.join(folder, 'png6720_config_parity_test.iff')));
    const originalConfig = result.configs.find(c => c.name === original);
    result = await rpc.call('configs-change', { target, config: 'PARITY_TEST', action: 'save', items: [] });
    assert.deepEqual(result.configs.find(c => c.name === original), originalConfig);
    result = await rpc.call('configs-change', { target, config: 'PARITY_TEST', action: 'rename', name: 'RENAMED_TEST' });
    assert(result.configs.some(c => c.name === 'RENAMED_TEST'));
    result = await rpc.call('configs-change', { target, config: 'RENAMED_TEST', action: 'default' });
    assert.equal(result.defaultConfig, 'RENAMED_TEST');
    const fit = await rpc.call('body-fit', { target });
    const changes = { chestTHIN: -0.25, chestTHICK: 1.75 };
    const dimension = Object.keys(fit.targetBodyFit).find(k => /spine2.*x/i.test(k));
    if (dimension) changes[dimension] = -3.5;
    await rpc.call('body-fit-save', { target, changes });
    const fitted = await rpc.call('body-fit', { target });
    for (const [key, value] of Object.entries(changes)) assert.equal(fitted.targetBodyFit[key], value);
    for (const [key, value] of Object.entries(fit.targetBodyFit)) if (!(key in changes)) assert.equal(fitted.targetBodyFit[key], value);
    report.bodyFit = { fields: Object.keys(fitted.targetBodyFit).length, changes, unmatchedPreserved: true };
    for (const assetType of ['hair', 'facialhair']) {
      const catalog = await rpc.call('native-hair-catalog', { assetType });
      const asset = catalog.assets.find(a => a.sourcePng === '06827' && !a.tangentspace) || catalog.assets.find(a => a.sourcePng === '06827');
      assert(asset, 'Expected native donor not found');
      result = await rpc.call('native-hair-install', { target, config: 'RENAMED_TEST', configScoped: true, assetType, assetId: asset.id, copyItem: true });
      assert(result.targetKey.startsWith(assetType + '_renamed_test_'));
      assert(result.outputs.some(p => p.includes('_item_')));
      assert.deepEqual(result.configs.find(c => c.name === original), originalConfig);
      report[assetType] = { targetKey: result.targetKey, files: result.outputs.length, originalConfigPreserved: true };
    }
    const options = await rpc.call('headband-options', { target });
    assert(!options.styles.includes('Stock NBA 2K26 Headband'));
    report.headband = await rpc.call('headband-apply', { target, config: 'RENAMED_TEST', configScoped: true, mode: 'add', style: 'Stock NBA 2K27 Headband', color: '#2244CC', addLogo: false });
    const final = await rpc.call('configs-list', { target });
    assert.deepEqual(final.configs.find(c => c.name === original), originalConfig);
    assert(final.configs.find(c => c.name === 'RENAMED_TEST').items.includes('headband'));
    const preview = await rpc.call('native-preview-resources', { target, config: 'RENAMED_TEST' });
    assert(preview.activeItems.includes('headband'));
    assert(preview.options.companionPaths.some(c => c.name.endsWith('_geo_headband.iff')));
    report.health = await rpc.call('health');
    assert.equal(report.health.groups.find(g => g.id === 'runtime').checks[0].status, 'pass');
    assert.equal(report.health.groups.find(g => g.id === 'application').checks.find(c => c.id === 'roster-cli').status, 'pass');
    await rpc.call('reset');
    assert(fs.existsSync(target));
    assert.deepEqual(before, snapshot());
    report.originalsUnchanged = true;
    console.log(JSON.stringify({ bodyFit: report.bodyFit, hair: report.hair, facialhair: report.facialhair, headband: report.headband.assetName, health: report.health.counts, originalsUnchanged: true }, null, 2));
  } finally {
    fs.writeFileSync(path.join(root, 'artifacts/workflow-parity-report.json'), JSON.stringify(report, null, 2));
    await rpc.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
