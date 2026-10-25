const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');
const { Rpc, root } = require('./rpc-client.cjs');
const { WpfRpc } = require('./wpf-rpc.cjs');
const production = path.resolve(root, '../character-mod-tool-webview2/dist/Character-Mod-Tool-WebView2-0.1.31/App/Backend');
const hash = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
(async () => {
  const folder = fs.mkdtempSync(path.join(root, 'artifacts/accessory-conversion-'));
  const rpc = new Rpc({ env: { CMT_TEST_DATA: path.join(folder, 'data') } });
  const report = { checkedUtc: new Date().toISOString(), completed: false, cases: [] };
  try {
    const { settings } = await rpc.call('bootstrap');
    const cases = [
      { sourcePng: '4109', targetPng: '06827', mode: 'both', hairCount: 1 },
      { sourcePng: '2200', targetPng: '00143', mode: 'yes', hairCount: 0 },
      { sourcePng: '4109', targetPng: '06827', mode: 'no', hairCount: 1, regularOnlyFixture: true }
    ];
    for (const item of cases) {
      const directory = path.join(folder, item.sourcePng + '-' + item.targetPng + '-' + item.mode);
      fs.mkdirSync(directory);
      const source = await rpc.call('extract', { gameLabel: 'NBA 2K23', png: item.sourcePng });
      const originalTarget = await rpc.call('extract', { gameLabel: 'NBA 2K27', png: item.targetPng });
      const sourceHash = hash(source.mainPath), targetHash = hash(originalTarget.mainPath);
      const targetFolder = path.join(directory, 'target');
      fs.mkdirSync(targetFolder);
      for (const name of fs.readdirSync(path.dirname(originalTarget.mainPath)).filter(name => /\.iff$/i.test(name))) {
        if (item.regularOnlyFixture && /_geo_facialhair_.*_tangentspace\.iff$/i.test(name)) continue;
        fs.copyFileSync(path.join(path.dirname(originalTarget.mainPath), name), path.join(targetFolder, name));
      }
      const target = path.join(targetFolder, path.basename(originalTarget.mainPath));
      const request = { source: source.mainPath, target, sourceGame: 'NBA 2K23', targetGame: 'NBA 2K27',
        shrinkwrapBody: true, hairEnabled: false, facialHairEnabled: true };
      const old = new WpfRpc(path.join(directory, 'wpf-data'), settings);
      try {
        const hair = await rpc.call('hair-options', request);
        const oldHair = await old.call('hair:options', request);
        assert.deepEqual(hair, oldHair);
        assert.equal(hair.candidates.length, item.hairCount);
        const beard = await rpc.call('facial-hair-options', request);
        const oldBeard = await old.call('facial-hair-options', request);
        assert.deepEqual(beard, oldBeard);
        assert.equal(beard.candidates.length, 1);
        const slot = beard.slots.find(row => row.geometryMode === item.mode);
        assert(slot, JSON.stringify(beard));
        Object.assign(request, { facialHairSource: beard.candidates[0].path, facialHairTargetKey: slot.key });
        const baseline = path.join(directory, 'wpf'), migrated = path.join(directory, 'tauri');
        fs.mkdirSync(baseline); fs.mkdirSync(migrated);
        const { jobId } = await old.call('swap:start', { ...request, outputDir: baseline });
        const original = await old.waitForPush(['job:complete', 'job:failed'], row => row.jobId === jobId);
        assert.equal(original.channel, 'job:complete', JSON.stringify(original.value));
        assert.equal(original.value.validation.errors, 0);
        const converted = await rpc.call('run-swap', { ...request, outputDir: migrated }, 600000);
        assert.equal(converted.validation.errors, 0);
        const result = spawnSync(path.join(production, 'python_runtime/python.exe'),
          ['-B', path.join(__dirname, 'wpf-output-compare.py'), baseline, migrated], { windowsHide: true, encoding: 'utf8' });
        assert.equal(result.status, 0, result.stderr);
        const compared = JSON.parse(result.stdout);
        assert.equal(compared.missing.length + compared.extra.length + compared.different.length, 0, JSON.stringify(compared));
        const preview = await rpc.call('native-preview-resources', { target: converted.output });
        assert(preview.activeItems.some(key => /facialhair/i.test(key)));
        assert(preview.options.companionPaths.some(row => /_geo_facialhair_/i.test(row.name)));
        assert.equal(hash(source.mainPath), sourceHash);
        assert.equal(hash(originalTarget.mainPath), targetHash);
        report.cases.push({ ...item, ...compared, sourceUnchanged: true, targetUnchanged: true, output: converted.output });
        fs.writeFileSync(path.join(root, 'artifacts/accessory-conversion-parity-report.json'), JSON.stringify(report, null, 2));
        console.log(item.sourcePng + ' -> ' + item.targetPng + ' ' + item.mode + ': ' + compared.archives + ' archives, ' + compared.entries + ' entries match');
      } finally {
        await old.close();
        fs.writeFileSync(path.join(directory, 'wpf.log'), old.log);
      }
    }
    report.completed = true;
  } finally {
    await rpc.close();
    fs.writeFileSync(path.join(root, 'artifacts/accessory-conversion-parity-report.json'), JSON.stringify(report, null, 2));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
