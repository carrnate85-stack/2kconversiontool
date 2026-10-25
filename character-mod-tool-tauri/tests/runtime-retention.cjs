const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { Rpc, root, currentExe } = require('./rpc-client.cjs');
(async () => {
  const data = fs.mkdtempSync(path.join(root, 'artifacts/runtime-retention-'));
  const runtime = path.join(data, 'Runtime');
  const report = { version: path.basename(currentExe()), completed: false };
  async function launch() {
    const rpc = new Rpc({ env: { CMT_TEST_DATA: data } });
    try {
      await rpc.call('bootstrap');
      const deadline = Date.now() + 120000;
      for (;;) {
        const health = await rpc.call('health');
        const state = health.groups.find(group => group.id === 'runtime-maintenance')?.checks[0]?.details;
        assert(state, 'Runtime maintenance diagnostics missing');
        if (['complete', 'deferred'].includes(state.status)) {
          assert.equal(state.warnings, false, JSON.stringify(state)); break;
        }
        assert(Date.now() < deadline, 'Background retention did not settle');
        await new Promise(resolve => setTimeout(resolve, 50));
      }
    } finally { await rpc.close(); }
  }
  function seed(digit, time) {
    const hash = digit.repeat(64);
    const directory = path.join(runtime, 'engine-' + hash.slice(0, 16));
    const files = {};
    for (const name of ['Backend/python_runtime/python.exe', 'Backend/prototype_bridge.py']) {
      const bytes = Buffer.from('fixture marker, never executed');
      const target = path.join(directory, name);
      fs.mkdirSync(path.dirname(target), { recursive: true });
      fs.writeFileSync(target, bytes);
      files[name] = { bytes: bytes.length, sha256: crypto.createHash('sha256').update(bytes).digest('hex') };
    }
    fs.writeFileSync(path.join(directory, 'inventory.json'), JSON.stringify({ payload: hash, files }));
    const marker = path.join(directory, 'ready.sha256');
    fs.writeFileSync(marker, hash);
    fs.utimesSync(marker, new Date(time * 1000), new Date(time * 1000));
    return directory;
  }
  try {
    await launch();
    const initial = fs.readdirSync(runtime).filter(name => name.startsWith('engine-'));
    assert.equal(initial.length, 1);
    const current = path.join(runtime, initial[0]);
    const inventory = JSON.parse(fs.readFileSync(path.join(current, 'inventory.json'), 'utf8'));
    assert(Object.keys(inventory.files).length > 1000);
    const prior = seed('b', 600);
    const removable = seed('c', 500);
    const modified = seed('d', 400);
    const userFiles = seed('e', 300);
    const legacy = seed('f', 200);
    const configured = seed('1', 100);
    fs.writeFileSync(path.join(modified, 'Backend/prototype_bridge.py'), 'user adjustment');
    fs.writeFileSync(path.join(userFiles, 'player.iff'), 'user export');
    fs.rmSync(path.join(legacy, 'inventory.json'));
    const settingsPath = path.join(data, 'settings.json');
    const settings = fs.existsSync(settingsPath) ? JSON.parse(fs.readFileSync(settingsPath, 'utf8')) : {};
    settings.output_dir = configured;
    fs.writeFileSync(settingsPath, JSON.stringify(settings));
    await launch();
    assert(!fs.existsSync(removable));
    for (const folder of [current, prior, modified, userFiles, legacy, configured]) assert(fs.existsSync(folder));
    assert.equal(fs.readFileSync(path.join(userFiles, 'player.iff'), 'utf8'), 'user export');
    settings.output_dir = path.join(data, 'outputs');
    fs.writeFileSync(settingsPath, JSON.stringify(settings));
    await launch();
    assert(!fs.existsSync(configured));
    for (const folder of [current, prior, modified, userFiles, legacy]) assert(fs.existsSync(folder));
    assert.equal(fs.readFileSync(path.join(modified, 'Backend/prototype_bridge.py'), 'utf8'), 'user adjustment');
    report.inventoryFiles = Object.keys(inventory.files).length;
    report.currentAndPreviousRetained = true;
    report.obsoleteUnmodifiedRemoved = true;
    report.modifiedUserAndLegacyPreserved = true;
    report.configuredPathProtectedUntilChanged = true;
    report.completed = true;
    console.log(JSON.stringify(report, null, 2));
  } finally {
    fs.writeFileSync(path.join(root, 'artifacts/runtime-retention-report.json'), JSON.stringify(report, null, 2));
    assert.equal(path.dirname(fs.realpathSync(data)), fs.realpathSync(path.join(root, 'artifacts')));
    fs.rmSync(data, { recursive: true });
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
