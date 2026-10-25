const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { Rpc, root, currentExe } = require('./rpc-client.cjs');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const exeIndex = process.argv.indexOf('--exe');
const exe = exeIndex < 0 ? currentExe() : path.resolve(process.argv[exeIndex + 1]);
const baseline = process.argv.includes('--baseline');
const reportPath = path.join(root, 'artifacts', baseline ? 'runtime-startup-maintenance-baseline.json' : 'runtime-startup-maintenance-report.json');

function seedCurrent(folder) {
  const fd = fs.openSync(exe, 'r'), footer = Buffer.alloc(64), size = fs.fstatSync(fd).size;
  try { assert.equal(fs.readSync(fd, footer, 0, 64, size - 64), 64); } finally { fs.closeSync(fd); }
  assert.equal(footer.subarray(0, 16).toString(), 'CMT-RUST-PAYLOAD');
  assert.equal(footer.readBigUInt64LE(16) + footer.readBigUInt64LE(24), BigInt(size - 64));
  const payload = footer.subarray(32).toString('hex');
  const source = path.join(process.env.LOCALAPPDATA, 'CharacterModToolTauri', 'Runtime', 'engine-' + payload.slice(0, 16));
  assert.equal(fs.readFileSync(path.join(source, 'ready.sha256'), 'utf8'), payload);
  const inventory = JSON.parse(fs.readFileSync(path.join(source, 'inventory.json')));
  assert.equal(inventory.payload, payload);
  const names = Object.keys(inventory.files);
  assert(names.length > 0 && names.length < 20000);
  const destination = path.join(folder, 'Runtime', path.basename(source));
  for (const name of [...names, 'inventory.json', 'ready.sha256']) {
    assert(!path.isAbsolute(name) && !name.split(/[\\/]/).includes('..'));
    const from = path.join(source, name), to = path.join(destination, name);
    assert(fs.lstatSync(from).isFile() && !fs.lstatSync(from).isSymbolicLink());
    fs.mkdirSync(path.dirname(to), { recursive: true }); fs.copyFileSync(from, to, fs.constants.COPYFILE_EXCL);
    if (inventory.files[name]) assert.equal(hash(fs.readFileSync(to)), inventory.files[name].sha256, name);
  }
  fs.writeFileSync(path.join(folder, 'settings.json'), JSON.stringify({ output_dir: path.join(folder, 'outputs') }));
  return { payload, files: names.length, current: destination };
}

function oldRuntime(folder, letter, modified) {
  const payload = letter.repeat(64), destination = path.join(folder, 'Runtime', 'engine-' + payload.slice(0, 16));
  const files = {}, bytes = Buffer.alloc(16 * 1024, 0x35), digest = hash(bytes);
  const names = ['Backend/python_runtime/python.exe', 'Backend/prototype_bridge.py', ...Array.from({ length: 2048 }, (_, index) => `Backend/owned-fixture/group-${Math.floor(index / 64)}/file-${index}.bin`)];
  for (const name of names) {
    const file = path.join(destination, name); fs.mkdirSync(path.dirname(file), { recursive: true }); fs.writeFileSync(file, bytes);
    files[name] = { bytes: bytes.length, sha256: digest };
  }
  fs.writeFileSync(path.join(destination, 'inventory.json'), JSON.stringify({ payload, files }));
  const ready = path.join(destination, 'ready.sha256'); fs.writeFileSync(ready, payload); fs.utimesSync(ready, modified, modified);
  return destination;
}

(async () => {
  const fixture = fs.mkdtempSync(path.join(root, 'artifacts/runtime-startup-owned-'));
  const real = fs.realpathSync(fixture), report = { executable: exe, executableSha256: hash(fs.readFileSync(exe)), baseline, completed: false };
  let rpc;
  try {
    const seedStarted = Date.now(); const seeded = seedCurrent(fixture);
    report.seed = { payload: seeded.payload, filesByteVerified: seeded.files, milliseconds: Date.now() - seedStarted };
    const oldest = oldRuntime(fixture, 'c', new Date(1000));
    const previous = oldRuntime(fixture, 'b', new Date(2000));
    rpc = new Rpc({ exe, env: { CMT_TEST_DATA: fixture } });
    const started = Date.now(); await rpc.call('bootstrap'); report.bootstrapMilliseconds = Date.now() - started;
    report.bootstrapEvents = rpc.events.map(event => event.message).filter(Boolean);
    report.oldestStillPresentAtBootstrap = fs.existsSync(oldest) || fs.readdirSync(path.join(fixture, 'Runtime')).some(name => name.startsWith('.retired-' + 'c'.repeat(16)));
    const completion = Date.now();
    while (fs.existsSync(oldest) || fs.readdirSync(path.join(fixture, 'Runtime')).some(name => name.startsWith('.retired-' + 'c'.repeat(16)))) {
      assert(Date.now() - completion < 120000, 'Verified background retention did not complete'); await delay(100);
    }
    report.maintenanceAfterBootstrapMilliseconds = Date.now() - completion;
    assert(fs.existsSync(previous), 'The prior runtime was removed'); assert(fs.existsSync(seeded.current), 'The current runtime was removed');
    const health = await rpc.call('health');
    report.maintenanceHealth = health.groups.find(group => group.id === 'runtime-maintenance') || null;
    if (!baseline) {
      assert(report.oldestStillPresentAtBootstrap, 'Startup waited for old-cache deletion');
      assert(report.maintenanceHealth, 'Background maintenance health state missing');
      assert.equal(report.maintenanceHealth.checks[0].details.status, 'complete');
    }
    report.verifiedOldestRemoved = true; report.currentAndPriorPreserved = true; report.completed = true;
    console.log(JSON.stringify(report, null, 2));
  } finally {
    await rpc?.close();
    fs.writeFileSync(reportPath, JSON.stringify(report, null, 2));
    assert.equal(fs.realpathSync(fixture), real); assert(!fs.lstatSync(fixture).isSymbolicLink());
    assert.equal(path.dirname(real), fs.realpathSync(path.join(root, 'artifacts')));
    fs.rmSync(real, { recursive: true, maxRetries: 20, retryDelay: 200 });
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
