const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');
const { Rpc, root } = require('./rpc-client.cjs');
const { WpfRpc } = require('./wpf-rpc.cjs');
const python = path.resolve(root, '../character-mod-tool-webview2/dist/Character-Mod-Tool-WebView2-0.1.31/App/Backend/python_runtime/python.exe');
const hash = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
(async () => {
  const folder = fs.mkdtempSync(path.join(root, 'artifacts/legacy-hair-parity-'));
  const rpc = new Rpc({ env: { CMT_TEST_DATA: path.join(folder, 'data') } });
  const report = { checkedUtc: new Date().toISOString(), completed: false, cases: [] };
  try {
    const { settings } = await rpc.call('bootstrap');
    for (const game of ['NBA 2K18', 'NBA 2K19']) {
      if (process.env.CMT_PARITY_STOP_AT && Date.now() + 300000 > Date.parse(process.env.CMT_PARITY_STOP_AT)) {
        report.stoppedAtWorkLimit = true; break;
      }
      const directory = path.join(folder, game.replace(/ /g, '-'));
      fs.mkdirSync(directory);
      const source = await rpc.call('extract', { gameLabel: game, png: '0005' });
      const target = await rpc.call('extract', { gameLabel: 'NBA 2K27', png: '06827' });
      const sourceHash = hash(source.mainPath), targetHash = hash(target.mainPath);
      const request = { source: source.mainPath, target: target.mainPath, sourceGame: game,
        targetGame: 'NBA 2K27', shrinkwrapBody: true, hairEnabled: true, facialHairEnabled: false };
      const options = await rpc.call('hair-options', request);
      assert.equal(options.candidates.length, 1); assert(options.slots.length);
      Object.assign(request, { hairSource: options.candidates[0].path, hairTargetKey: options.slots[0].key });
      const baseline = path.join(directory, 'wpf'), migrated = path.join(directory, 'tauri');
      fs.mkdirSync(baseline); fs.mkdirSync(migrated);
      const old = new WpfRpc(path.join(directory, 'wpf-data'), settings);
      try {
        assert.deepEqual(await old.call('hair:options', request), options);
        const { jobId } = await old.call('swap:start', { ...request, outputDir: baseline });
        const completed = await old.waitForPush(['job:complete', 'job:failed'], row => row.jobId === jobId);
        assert.equal(completed.channel, 'job:complete', JSON.stringify(completed.value));
        assert.equal(completed.value.validation.errors, 0);
        const result = await rpc.call('run-swap', { ...request, outputDir: migrated }, 360000);
        assert.equal(result.validation.errors, 0);
        const compared = spawnSync(python, ['-B', path.join(__dirname, 'wpf-output-compare.py'), baseline, migrated],
          { encoding: 'utf8', windowsHide: true });
        assert.equal(compared.status, 0, compared.stderr);
        const comparison = JSON.parse(compared.stdout);
        assert.equal(comparison.missing.length + comparison.extra.length + comparison.different.length, 0, JSON.stringify(comparison));
        assert.equal(hash(source.mainPath), sourceHash); assert.equal(hash(target.mainPath), targetHash);
        report.cases.push({ game, source: '0005', target: '06827', includeHair: true, ...comparison,
          sourceUnchanged: true, targetUnchanged: true, output: result.output });
        console.log(game + ' source hair: ' + comparison.archives + ' archives, ' + comparison.entries + ' entries match');
      } finally { await old.close(); fs.writeFileSync(path.join(directory, 'wpf.log'), old.log); }
      fs.writeFileSync(path.join(root, 'artifacts/legacy-hair-parity-report.json'), JSON.stringify(report, null, 2));
    }
    report.completed = report.cases.length === 2;
  } finally {
    await rpc.close();
    fs.writeFileSync(path.join(root, 'artifacts/legacy-hair-parity-report.json'), JSON.stringify(report, null, 2));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
