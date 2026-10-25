const fs = require('fs');
const path = require('path');
const assert = require('assert/strict');
const crypto = require('crypto');
const { Rpc, root } = require('./rpc-client.cjs');
(async () => {
  const rpc = new Rpc(), report = { checkedUtc: new Date().toISOString(), cases: {}, unavailable: {} };
  try {
    let bona;
    try {
      const catalog = await rpc.call('catalog', { gameLabel: 'NBA 2K25' });
      const rows = catalog.players || catalog.rows;
      assert(Array.isArray(rows));
      bona = rows.find(row => /bona/i.test(row.name));
      assert(bona, 'Adem Bona absent from the 2K25 catalog');
    } catch (error) {
      if (!/manifest could not be found/i.test(error.message)) throw error;
      report.unavailable['NBA 2K25'] = error.message;
    }
    const cases = [
      ['NBA 2K18', '0926'],
      ['NBA 2K19', '0926'],
      ['NBA 2K20', '4090', path.join(root, '../research/nba2k20_native_export_4090/png4090.iff')],
      ['NBA 2K26', '1533', 'C:/Users/carrn/Desktop/PCBasket26/PCB26/mods/char/sig/png1533.iff']
    ];
    if (bona) cases.push(['NBA 2K25', bona.png]);
    for (const [game, id, file] of cases) {
      console.log('Checking ' + game + ' source ' + id);
      if (file && !fs.existsSync(file)) { report.unavailable[game] = 'Standalone source is unavailable: ' + file; continue; }
      const source = file ? { mainPath: file } : await rpc.call('extract', { gameLabel: game, png: id });
      const prefix = path.basename(source.mainPath, '.iff').replace(/^png/i, '');
      const sourceFolder = path.dirname(source.mainPath);
      const snapshot = () => Object.fromEntries(fs.readdirSync(sourceFolder).filter(name => new RegExp('^(?:png|face)' + prefix + '(?:_|\\.iff$)', 'i').test(name)).map(name => [name, crypto.createHash('sha256').update(fs.readFileSync(path.join(sourceFolder, name))).digest('hex')]));
      const before = snapshot();
      const target = await rpc.call('extract', { gameLabel: 'NBA 2K27', png: '06827' });
      const request = { source: source.mainPath, target: target.mainPath, sourceGame: game, targetGame: 'NBA 2K27',
        outputDir: path.join(root, 'artifacts/generation-parity', game.replace(/ /g, '-')),
        shrinkwrapBody: true, hairEnabled: false, facialHairEnabled: false };
      const hair = await rpc.call('hair-options', request);
      const beard = await rpc.call('facial-hair-options', request);
      if (hair.candidates.length && hair.slots.length) Object.assign(request, { hairEnabled: true, hairSource: hair.candidates[0].path, hairTargetKey: hair.slots[0].key });
      if (beard.candidates.length && beard.slots.length) Object.assign(request, { facialHairEnabled: true, facialHairSource: beard.candidates[0].path, facialHairTargetKey: beard.slots[0].key });
      if (game === 'NBA 2K25') assert(beard.candidates.length, 'Bona facial hair not detected');
      const started = Date.now();
      const result = await rpc.call('run-swap', request, 600000);
      assert.equal(result.validation.errors, 0, JSON.stringify(result.validation));
      assert(fs.existsSync(result.output));
      const fit = await rpc.call('body-fit', { source: source.mainPath, target: result.output });
      const configs = await rpc.call('configs-list', { target: result.output });
      const preview = await rpc.call('native-preview-resources', { target: result.output });
      assert(configs.configs.length);
      assert(Object.keys(fit.targetBodyFit).length > 20);
      assert(preview.options.companionPaths.length);
      assert.deepEqual(snapshot(), before, 'Conversion must not change source player files');
      report.cases[game] = { source: id, sourceUnchanged: true, milliseconds: Date.now() - started, hairCandidates: hair.candidates.length, beardCandidates: beard.candidates.length,
        bodyFitFields: Object.keys(fit.targetBodyFit).length, result, previewCompanions: preview.options.companionPaths.length };
      fs.writeFileSync(path.join(root, 'artifacts/generation-parity-report.json'), JSON.stringify(report, null, 2));
      console.log(game + ': passed with ' + result.validation.warnings + ' warnings');
    }
  } finally {
    fs.writeFileSync(path.join(root, 'artifacts/generation-parity-report.json'), JSON.stringify(report, null, 2));
    await rpc.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
