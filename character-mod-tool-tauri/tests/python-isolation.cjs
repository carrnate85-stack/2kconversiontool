const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { Rpc, root, currentExe } = require('./rpc-client.cjs');
(async () => {
  const baseline = process.argv.includes('--baseline');
  const data = fs.mkdtempSync(path.join(root, 'artifacts/python-isolation-'));
  const ambient = path.join(data, 'ambient');
  fs.mkdirSync(ambient);
  fs.writeFileSync(path.join(ambient, 'json.py'), 'raise RuntimeError("Ambient module must not be imported")\n');
  const rpc = new Rpc({ env: { CMT_TEST_DATA: data, PYTHONHOME: path.join(data, 'missing-python'),
    PYTHONPATH: ambient } });
  const report = { version: path.basename(currentExe()), baseline, completed: false };
  try {
    if (baseline) {
      await assert.rejects(rpc.call('bootstrap'), error => { report.error = error.message; return true; });
      report.ambientConfigurationBreaksOldWorker = true;
    } else {
      const initial = await rpc.call('bootstrap');
      assert(initial.engineVersion);
      const saved = await rpc.call('templates-save', { hair: { png: '06827', hair: 'dreads' } });
      assert.equal(saved.templates.hair.png, '06827');
      assert.deepEqual((await rpc.call('templates-load')).templates, saved.templates);
      report.bootstrapAndPersistenceIgnoreAmbientPython = true;
    }
    report.completed = true;
    console.log(JSON.stringify(report, null, 2));
  } finally {
    await rpc.close();
    fs.writeFileSync(path.join(root, 'artifacts', baseline ? 'python-isolation-0.2.22-baseline.json' : 'python-isolation-report.json'),
      JSON.stringify(report, null, 2));
    assert.equal(path.dirname(fs.realpathSync(data)), fs.realpathSync(path.join(root, 'artifacts')));
    fs.rmSync(data, { recursive: true });
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
