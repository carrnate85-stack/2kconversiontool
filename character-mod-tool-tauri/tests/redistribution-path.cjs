const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { Rpc, root } = require('./rpc-client.cjs');
(async () => {
  const data = fs.mkdtempSync(path.join(root, 'artifacts/redistribution-'));
  const windows = process.env.WINDIR || 'C:/Windows';
  const rpc = new Rpc({ env: { CMT_TEST_DATA: data, PATH: `${windows}/System32;${windows}`, PYTHONHOME: '', PYTHONPATH: '',
    CHARACTER_MOD_NODE: '', CMT_NODE: '', NODE_OPTIONS: '' } });
  const report = { checkedUtc: new Date().toISOString(), systemPathOnly: true, cleanMachineVerified: false };
  try {
    const initial = await rpc.call('bootstrap');
    assert(fs.existsSync(initial.settings.head_swap_tool));
    assert(fs.existsSync(initial.settings.mesh_data_transfer));
    const source = await rpc.call('extract', { gameLabel: 'NBA 2K23', png: '4100' });
    const target = await rpc.call('extract', { gameLabel: 'NBA 2K27', png: '06827' });
    const request = { source: source.mainPath, target: target.mainPath, sourceGame: 'NBA 2K23', targetGame: 'NBA 2K27', outputDir: path.join(data, 'outputs'), shrinkwrapBody: true, hairEnabled: false };
    const beard = await rpc.call('facial-hair-options', request);
    assert(beard.candidates.length && beard.slots.length);
    Object.assign(request, { facialHairEnabled: true, facialHairSource: beard.candidates[0].path, facialHairTargetKey: beard.slots[0].key });
    const result = await rpc.call('run-swap', request, 600000);
    assert.equal(result.validation.errors, 0);
    const resources = await rpc.call('native-preview-resources', { target: result.output });
    assert(resources.options.companionPaths.length);
    const fallback = await rpc.call('preview-resources', { target: result.output });
    assert(fallback.options.companionPaths.length);
    const health = await rpc.call('health');
    assert.equal(health.counts.error, 0, JSON.stringify(health.counts));
    report.fullSwap = { errors: result.validation.errors, warnings: result.validation.warnings, archives: result.validation.archives };
    report.nativeCompanions = resources.options.companionPaths.length;
    report.fallbackCompanions = fallback.options.companionPaths.length;
    report.health = health.counts;
    report.remaining = ['A separate clean Windows machine is still required to verify missing WebView2/.NET Framework prerequisites.'];
    console.log(JSON.stringify(report, null, 2));
  } finally {
    await rpc.close();
    fs.writeFileSync(path.join(root, 'artifacts/redistribution-path-report.json'), JSON.stringify(report, null, 2));
    assert.equal(path.dirname(fs.realpathSync(data)), fs.realpathSync(path.join(root, 'artifacts')));
    fs.rmSync(data, { recursive: true });
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
