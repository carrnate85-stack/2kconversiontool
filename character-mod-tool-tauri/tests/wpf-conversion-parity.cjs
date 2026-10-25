const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { Rpc, root } = require('./rpc-client.cjs');
const { WpfRpc } = require('./wpf-rpc.cjs');
const production = path.resolve(root, '../character-mod-tool-webview2/dist/Character-Mod-Tool-WebView2-0.1.31/App/Backend');
const python = path.join(production, 'python_runtime/python.exe');
(async () => {
  const folder = fs.mkdtempSync(path.join(root, 'artifacts/wpf-conversion-'));
  const rpc = new Rpc({ env: { CMT_TEST_DATA: path.join(folder, 'tauri-data') } });
  const report = { baseline: 'WPF 0.1.31 complete backend', checkedUtc: new Date().toISOString(), cases: [] };
  try {
    const { settings } = await rpc.call('bootstrap');
    const cases = [['NBA 2K23', '4100', true], ['NBA 2K18', '0926', false], ['NBA 2K19', '0926', false],
      ['NBA 2K20', '4090', false, path.resolve(root, '../research/nba2k20_native_export_4090/png4090.iff')],
      ['NBA 2K26', '1533', false, 'C:/Users/carrn/Desktop/PCBasket26/PCB26/mods/char/sig/png1533.iff']];
    for (const [game, png, includeBeard, file] of cases) {
      console.log('Comparing WPF and Tauri outputs: ' + game + ' ' + png);
      const source = file ? { mainPath: file } : await rpc.call('extract', { gameLabel: game, png });
      const target = await rpc.call('extract', { gameLabel: 'NBA 2K27', png: '06827' });
      const caseRoot = path.join(folder, game.replace(/ /g, '-')); fs.mkdirSync(caseRoot);
      if (file) {
        const copied = path.join(caseRoot, 'source'); fs.mkdirSync(copied);
        const stem = path.basename(file, '.iff'), faceStem = stem.replace(/^png/i, 'face');
        for (const name of fs.readdirSync(path.dirname(file))) {
          if ([stem, faceStem].some(prefix => name.toLowerCase() === prefix.toLowerCase() + '.iff' || name.toLowerCase().startsWith(prefix.toLowerCase() + '_')) && /\.iff$/i.test(name)) fs.copyFileSync(path.join(path.dirname(file), name), path.join(copied, name));
        }
        source.mainPath = path.join(copied, path.basename(file));
      }
      const baseline = path.join(caseRoot, 'wpf'), migrated = path.join(caseRoot, 'tauri'), privateData = path.join(caseRoot, 'baseline-data');
      for (const output of [baseline, migrated, privateData]) fs.mkdirSync(output);
      const request = { source: source.mainPath, target: target.mainPath, sourceGame: game, targetGame: 'NBA 2K27', shrinkwrapBody: true, hairEnabled: false, facialHairEnabled: false };
      if (includeBeard) {
        const beard = await rpc.call('facial-hair-options', request);
        assert(beard.candidates.length && beard.slots.length);
        Object.assign(request, { facialHairEnabled: true, facialHairSource: beard.candidates[0].path, facialHairTargetKey: beard.slots[0].key });
      }
      const settingsFile = path.join(caseRoot, 'settings.json'), requestFile = path.join(caseRoot, 'request.json');
      fs.writeFileSync(settingsFile, JSON.stringify({ ...settings, output_dir: baseline }));
      fs.writeFileSync(requestFile, JSON.stringify({ ...request, outputDir: baseline }));
      const old = new WpfRpc(privateData, { ...settings, output_dir: baseline });
      let original;
      try {
        const { jobId } = await old.call('swap:start', { ...request, outputDir: baseline });
        const completed = await old.waitForPush(['job:complete', 'job:failed'], row => row.jobId === jobId);
        assert.equal(completed.channel, 'job:complete', JSON.stringify(completed.value));
        original = completed.value; assert.equal(original.validation.errors, 0);
      } finally { await old.close(); fs.writeFileSync(path.join(caseRoot, 'wpf.log'), old.log); }
      const result = await rpc.call('run-swap', { ...request, outputDir: migrated }, 600000); assert.equal(result.validation.errors, 0);
      const compared = spawnSync(python, ['-B', path.join(__dirname, 'wpf-output-compare.py'), baseline, migrated], { windowsHide: true, encoding: 'utf8' });
      assert.equal(compared.status, 0, compared.stderr);
      const comparison = JSON.parse(compared.stdout);
      report.cases.push({ game, source: png, includeBeard, wpfChestCorrection: original.automaticChestCorrection || null, tauriChestCorrection: result.automaticChestCorrection || null, ...comparison });
      fs.writeFileSync(path.join(root, 'artifacts/wpf-conversion-parity-report.json'), JSON.stringify(report, null, 2));
      assert.equal(comparison.missing.length + comparison.extra.length + comparison.different.length, 0, JSON.stringify(comparison));
      console.log('Matched ' + comparison.archives + ' archives and ' + comparison.entries + ' entries');
    }
  } finally {
    await rpc.close();
    fs.writeFileSync(path.join(root, 'artifacts/wpf-conversion-parity-report.json'), JSON.stringify(report, null, 2));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
