const assert = require('assert/strict');
const fs = require('fs');
const path = require('path');
const { Rpc, root } = require('./rpc-client.cjs');
const leaseOnly = process.argv.includes('--lease-only');
(async () => {
  const data = fs.mkdtempSync(path.join(root, 'artifacts/runtime-recovery-'));
  const report = { mode: leaseOnly ? 'active-lease-only' : 'full-damage-and-lease' };
  let rpc;
  const launch = async () => {
    rpc = new Rpc({ env: { CMT_TEST_DATA: data } });
    const started = Date.now();
    await rpc.call('bootstrap');
    return Date.now() - started;
  };
  try {
    report.first = await launch();
    await rpc.close();
    const cache = path.join(data, 'Runtime');
    const engines = () => fs.readdirSync(cache).filter(name => name.startsWith('engine-'));
    const engine = path.join(cache, engines()[0]);
    const bridge = path.join(engine, 'Backend/prototype_bridge.py');
    const python = path.join(engine, 'Backend/python_runtime/python.exe');
    const support = ['prototype_services.py', 'prototype_accessories.py',
      'prototype_blender_snapshot.py', 'prototype_player_writes.py', 'prototype_blender_paths.py', 'prototype_seams.py', 'prototype_staging.py', 'python_bridge.py',
      'app_settings.py', 'character_manifest.py', 'character_mod_tool.py', 'nba2k25_support.py']
      .map(name => path.join(engine, 'Backend', name));
    const originals = new Map([bridge, python, ...support].map(file => [file, fs.readFileSync(file)]));
    fs.mkdirSync(path.join(data, 'outputs'), { recursive: true });
    fs.writeFileSync(path.join(data, 'outputs/keep.iff'), 'preserved export');
    const userTool = path.join(data, 'user-tools/keep.py');
    fs.mkdirSync(path.dirname(userTool), { recursive: true });
    fs.writeFileSync(userTool, 'preserved user tool');
    const settings = fs.readFileSync(path.join(data, 'settings.json'));
    report.cached = await launch();
    assert(!rpc.events.some(event => /Preparing|Repairing/.test(event.message)));
    await rpc.close();
    const cases = leaseOnly ? [] : [bridge, python, ...support, path.join(engine, 'ready.sha256')]
      .map(file => ({ file, damage: 'missing' }));
    if (!leaseOnly) cases.push(...support.slice(0, 4).map(file => ({ file, damage: 'empty' })));
    const broken = () => fs.readdirSync(cache).filter(name => name.startsWith('.broken-') &&
      fs.lstatSync(path.join(cache, name)).isDirectory());
    const preserved = new Set();
    for (const { file, damage } of cases) {
      if (damage === 'missing') fs.unlinkSync(file);
      else fs.writeFileSync(file, Buffer.alloc(0));
      const elapsed = await launch();
      assert(rpc.events.some(event => /Repairing/.test(event.message)));
      for (const [restored, bytes] of originals) assert.deepEqual(fs.readFileSync(restored), bytes);
      assert.equal(engines().length, 1);
      assert.equal(fs.readFileSync(path.join(data, 'outputs/keep.iff'), 'utf8'), 'preserved export');
      assert.deepEqual(fs.readFileSync(path.join(data, 'settings.json')), settings);
      assert.equal(fs.readFileSync(userTool, 'utf8'), 'preserved user tool');
      assert(!fs.readdirSync(cache).some(name => name.startsWith('.preparing-')));
      const added = broken().filter(name => !preserved.has(name));
      assert.equal(added.length, damage === 'empty' ? 1 : 0);
      for (const name of added) {
        // Changed bytes are retained by the ownership journal, never deleted.
        assert.equal(fs.statSync(path.join(cache, name, path.relative(engine, file))).size, 0);
        preserved.add(name);
      }
      report[`${damage}:${path.relative(engine, file)}`] = elapsed;
      await rpc.close();
    }
    report.cachedAfterRepair = await launch();
    assert(!rpc.events.some(event => /Preparing|Repairing/.test(event.message)));
    const second = new Rpc({ env: { CMT_TEST_DATA: data } });
    const attemptedRepair = new Rpc({ env: { CMT_TEST_DATA: data } });
    try {
      await second.call('bootstrap');
      const before = fs.readdirSync(cache).filter(name => /^\.(preparing|broken)-/.test(name)).sort();
      fs.unlinkSync(support[0]);
      await assert.rejects(attemptedRepair.call('bootstrap'), error => {
        report.activeRuntimeRefusal = error.message;
        return /runtime is in use|Another instance is preparing the conversion engine/.test(error.message);
      });
      assert.deepEqual(fs.readdirSync(cache).filter(name => /^\.(preparing|broken)-/.test(name)).sort(), before);
      report.activeRuntimeProtection = 'two active instances prevent repair of their runtime';
    } finally {
      fs.writeFileSync(support[0], originals.get(support[0]));
      await attemptedRepair.close();
      await second.close();
    }
    const checks = (await rpc.call('health')).groups.find(group => group.id === 'application').checks;
    assert(checks.every(check => check.status === 'pass'), JSON.stringify(checks));
    assert(!fs.readdirSync(cache).some(name => name.startsWith('.preparing-')));
    assert.deepEqual(new Set(broken()), preserved);
    report.preservedChangedBackups = preserved.size;
    console.log(JSON.stringify(report, null, 2));
  } finally {
    if (rpc) await rpc.close();
    fs.writeFileSync(path.join(root, 'artifacts/runtime-recovery-report.json'), JSON.stringify(report, null, 2));
    assert.equal(path.dirname(fs.realpathSync(data)), fs.realpathSync(path.join(root, 'artifacts')));
    fs.rmSync(data, { recursive: true });
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
