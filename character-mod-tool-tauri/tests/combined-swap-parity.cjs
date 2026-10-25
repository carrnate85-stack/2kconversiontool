const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');
const { Rpc, root } = require('./rpc-client.cjs');
const { WpfRpc } = require('./wpf-rpc.cjs');
const python = path.resolve(root, '../character-mod-tool-webview2/dist/Character-Mod-Tool-WebView2-0.1.31/App/Backend/python_runtime/python.exe');
const snapshot = folder => Object.fromEntries(fs.readdirSync(folder).filter(name => /\.iff$/i.test(name))
  .map(name => [name, crypto.createHash('sha256').update(fs.readFileSync(path.join(folder, name))).digest('hex')]));
const bounded = async (promise, ms) => {
  let timer;
  try { return await Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('Combined audit deadline exceeded')), ms); })]); }
  finally { clearTimeout(timer); }
};
(async () => {
  const folder = fs.mkdtempSync(path.join(root, 'artifacts/combined-swap-parity-'));
  const rpc = new Rpc({ env: { CMT_TEST_DATA: path.join(folder, 'data') } });
  const report = { checkedUtc: new Date().toISOString(), completed: false };
  let old;
  try {
    const { settings } = await rpc.call('bootstrap');
    const source = await rpc.call('extract', { gameLabel: 'NBA 2K23', png: '4109' });
    const target = await rpc.call('extract', { gameLabel: 'NBA 2K27', png: '06827' });
    const sourceBefore = snapshot(path.dirname(source.mainPath)), targetBefore = snapshot(path.dirname(target.mainPath));
    const sourceCopy = path.join(folder, 'source');
    fs.cpSync(path.dirname(source.mainPath), sourceCopy, { recursive: true });
    const request = { source: source.mainPath, target: target.mainPath, sourceGame: 'NBA 2K23',
      targetGame: 'NBA 2K27', renameId: '28001', shrinkwrapBody: true, hairEnabled: true, facialHairEnabled: true };
    const hair = await rpc.call('hair-options', request), beard = await rpc.call('facial-hair-options', request);
    assert.equal(hair.candidates.length, 1); assert.equal(beard.candidates.length, 1);
    Object.assign(request, { hairSource: hair.candidates[0].path, hairTargetKey: hair.slots[0].key,
      facialHairSource: beard.candidates[0].path, facialHairTargetKey: beard.slots[0].key });
    const baseline = path.join(folder, 'wpf'), migrated = path.join(folder, 'tauri');
    fs.mkdirSync(baseline); fs.mkdirSync(migrated);
    old = new WpfRpc(path.join(folder, 'wpf-data'), settings);
    const { jobId } = await old.call('swap:start', { ...request, outputDir: baseline });
    const completed = await bounded(old.waitForPush(['job:complete', 'job:failed'], row => row.jobId === jobId), 180000);
    assert.equal(completed.channel, 'job:complete', JSON.stringify(completed.value));
    assert.equal(completed.value.validation.errors, 0);
    await old.close();
    const result = await rpc.call('run-swap', { ...request, outputDir: migrated }, 180000);
    assert.equal(result.validation.errors, 0);
    assert.equal(path.basename(result.output), 'png28001.iff');
    const compared = spawnSync(python, ['-B', path.join(__dirname, 'wpf-output-compare.py'), baseline, migrated],
      { encoding: 'utf8', windowsHide: true, timeout: 30000 });
    assert.equal(compared.status, 0, compared.stderr);
    const comparison = JSON.parse(compared.stdout);
    report.originalWpfComparison = comparison;
    const hairArchive = 'png28001_geo_' + request.hairTargetKey + '.iff';
    assert.equal(comparison.missing.length + comparison.extra.length, 0, JSON.stringify(comparison));
    assert(comparison.different.every(item => item.archive === hairArchive), JSON.stringify(comparison));
    const policyBaseline = path.join(folder, 'highest-policy-wpf');
    fs.cpSync(baseline, policyBaseline, { recursive: true });
    const proof = spawnSync(python, ['-B', path.join(__dirname, 'highest-hair-policy.py'), path.dirname(path.dirname(python)),
      path.join(sourceCopy, path.basename(request.hairSource)), path.join(migrated, hairArchive), path.join(baseline, hairArchive),
      path.join(policyBaseline, hairArchive), path.join(folder, 'highest-hair-proof.json')], { encoding: 'utf8', windowsHide: true, timeout: 30000 });
    assert.equal(proof.status, 0, proof.stderr);
    report.highestHairProof = JSON.parse(proof.stdout);
    const policyCompared = spawnSync(python, ['-B', path.join(__dirname, 'wpf-output-compare.py'), policyBaseline, migrated],
      { encoding: 'utf8', windowsHide: true, timeout: 30000 });
    assert.equal(policyCompared.status, 0, policyCompared.stderr);
    const policyComparison = JSON.parse(policyCompared.stdout);
    assert.equal(policyComparison.missing.length + policyComparison.extra.length + policyComparison.different.length, 0, JSON.stringify(policyComparison));
    const preview = await rpc.call('native-preview-resources', { target: result.output });
    assert(preview.activeItems.some(item => /^hair_/i.test(item)));
    assert(preview.activeItems.some(item => /^facialhair_/i.test(item)));
    assert(preview.options.companionPaths.filter(item => item.kind === 'geometry').every(item => /^png28001_/i.test(item.name)));
    assert.deepEqual(snapshot(path.dirname(source.mainPath)), sourceBefore);
    assert.deepEqual(snapshot(path.dirname(target.mainPath)), targetBefore);
    Object.assign(report, { completed: true, sourceGame: 'NBA 2K23', source: '4109', target: '06827',
      renameId: '28001', hair: true, facialHair: true, ...comparison, activeItems: preview.activeItems,
      exactOriginalWpfMatch: comparison.different.length === 0, matchesRequestedHighestPolicy: true,
      unrelatedArchivesMatch: true, policyComparison, sourceCompanionsUnchanged: true, targetCompanionsUnchanged: true, output: result.output });
    console.log(JSON.stringify(report));
  } finally {
    await old?.close(); await rpc.close();
    if (old) fs.writeFileSync(path.join(folder, 'wpf.log'), old.log);
    fs.writeFileSync(path.join(root, 'artifacts/combined-swap-parity-report.json'), JSON.stringify(report, null, 2));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
