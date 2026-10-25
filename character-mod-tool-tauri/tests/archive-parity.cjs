const assert = require('assert/strict');
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const { execFileSync } = require('child_process');
const { Rpc, root } = require('./rpc-client.cjs');
const hash = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
(async () => {
  const data = fs.mkdtempSync(path.join(root, 'artifacts/archive-parity-'));
  const rpc = new Rpc({ env: { CMT_TEST_DATA: path.join(data, 'app') } });
  const report = {};
  try {
    const runtime = await rpc.call('runtime-info');
    await rpc.call('bootstrap');
    const original = await rpc.call('extract', { gameLabel: 'NBA 2K23', png: '4100' });
    const source = path.dirname(original.mainPath);
    const engine = fs.readdirSync(path.join(runtime.data, 'Runtime')).find(name => name.startsWith('engine-'));
    const python = path.join(runtime.data, 'Runtime', engine, 'Backend/python_runtime/python.exe');
    const target = await rpc.call('extract', { gameLabel: 'NBA 2K27', png: '06827' });
    const baseline = { source: original.mainPath, sourceGame: 'NBA 2K23', target: target.mainPath, targetGame: 'NBA 2K27' };
    const baseHair = await rpc.call('hair-options', baseline), baseBeard = await rpc.call('facial-hair-options', baseline);
    assert(baseBeard.candidates.length);
    execFileSync(python, ['-B', path.join(__dirname, 'archive-fixtures.py'), source, data, path.basename(original.mainPath)], { windowsHide: true });
    const rar = 'C:/Program Files/WinRAR/Rar.exe';
    assert(fs.existsSync(rar), 'WinRAR is needed only to generate these test fixtures');
    for (const [name, options] of [['player-stored.rar', ['-m0']], ['player-solid.rar', ['-m3', '-s']]]) {
      execFileSync(rar, ['a', '-idq', '-cfg-', '-mt2', '-md16m', ...options, path.join(data, name), 'nested'], { cwd: path.join(data, 'package'), windowsHide: true });
    }
    let staged;
    for (const name of ['player.zip', 'player-stored.rar', 'player-solid.rar']) {
      const file = path.join(data, name), before = hash(file);
      const result = await rpc.call('inspect', { filePath: file, stageRoot: path.join(data, 'not-allowed') });
      assert(path.relative(runtime.session, result.path).split(path.sep)[0] !== '..');
      assert.equal(result.packagePath, file);
      const folder = path.dirname(result.path);
      const names = fs.readdirSync(folder).sort();
      assert.deepEqual(names, fs.readdirSync(source).filter(name => name.endsWith('.iff')).sort());
      for (const name of names) assert.equal(hash(path.join(folder, name)), hash(path.join(source, name)));
      assert.equal(hash(file), before);
      const request = { source: result.path, sourceGame: 'NBA 2K23', target: target.mainPath, targetGame: 'NBA 2K27' };
      const hair = await rpc.call('hair-options', request);
      const beard = await rpc.call('facial-hair-options', request);
      assert.equal(hair.candidates.length, baseHair.candidates.length);
      assert.equal(beard.candidates.length, baseBeard.candidates.length);
      report[name] = { stagedFiles: names.length, hair: hair.candidates.length, beard: beard.candidates.length };
      staged = result.path;
    }
    for (const [name, pattern] of [['traversal', /Unsafe/], ['duplicate', /exactly one|duplicate/i], ['multiple', /exactly one/], ['invalid-main', /readable IFF/], ['link', /regular files/]]) {
      const before = fs.readdirSync(runtime.session).filter(name => name.startsWith('character_mod_electron_source_'));
      await assert.rejects(rpc.call('inspect', { filePath: path.join(data, name + '.zip') }), pattern);
      assert.deepEqual(fs.readdirSync(runtime.session).filter(name => name.startsWith('character_mod_electron_source_')), before);
    }
    report.rejection = 'unsafe paths, duplicates, multiple players, links and invalid IFF rejected without leaked staging';
    const request = { source: staged, target: target.mainPath, sourceGame: 'NBA 2K23', targetGame: 'NBA 2K27',
      outputDir: path.join(data, 'exports'), shrinkwrapBody: true, hairEnabled: false, facialHairEnabled: true };
    const hair = await rpc.call('hair-options', request), beard = await rpc.call('facial-hair-options', request);
    if (hair.candidates.length) Object.assign(request, { hairEnabled: true, hairSource: hair.candidates[0].path, hairTargetKey: hair.slots[0].key });
    Object.assign(request, { facialHairSource: beard.candidates[0].path, facialHairTargetKey: beard.slots[0].key });
    const result = await rpc.call('run-swap', request, 600000);
    assert.equal(result.validation.errors, 0, JSON.stringify(result.validation));
    const before = hash(result.output);
    await rpc.call('reset');
    assert(!fs.existsSync(staged));
    assert.equal(hash(result.output), before);
    report.conversion = { errors: result.validation.errors, warnings: result.validation.warnings, resetPreservesExport: true };
    console.log(JSON.stringify(report, null, 2));
  } finally {
    await rpc.close();
    fs.writeFileSync(path.join(root, 'artifacts/archive-parity-report.json'), JSON.stringify(report, null, 2));
    assert.equal(path.dirname(fs.realpathSync(data)), fs.realpathSync(path.join(root, 'artifacts')));
    fs.rmSync(data, { recursive: true });
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
