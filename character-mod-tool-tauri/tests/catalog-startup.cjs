const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { Rpc, root } = require('./rpc-client.cjs');
(async () => {
  const report = { version: JSON.parse(fs.readFileSync(path.join(root, 'dist/BUILD.json'), 'utf8').replace(/^\uFEFF/, '')).version,
    completed: false, cases: [], failures: [] };
  try {
    for (let cycle = 0; cycle < 12; cycle++) {
      const rpc = new Rpc({ env: { CMT_TEST_DATA: path.join(root, 'artifacts/native-dialog-data') } });
      const started = Date.now();
      try {
        await rpc.call('bootstrap');
        const result = await rpc.call('native-hair-catalog', { assetType: cycle % 2 ? 'facialhair' : 'hair' });
        const count = result.assets.length;
        assert.equal(count, cycle % 2 ? 1090 : 5361);
        report.cases.push({ cycle, assetType: result.assetType, count, milliseconds: Date.now() - started });
        console.log(cycle + ': ' + count + ' assets');
      } catch (error) {
        report.failures.push({ cycle, error: String(error), stderr: rpc.errors });
      } finally { await rpc.close(); }
    }
    assert.deepEqual(report.failures, []);
    report.completed = true;
  } finally {
    fs.writeFileSync(path.join(root, 'artifacts/catalog-startup-report.json'), JSON.stringify(report, null, 2));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
