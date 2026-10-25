const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { Rpc, root, currentExe } = require('./rpc-client.cjs');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
(async () => {
  const data = path.join(root, 'artifacts/native-dialog-data');
  const rpc = new Rpc({ env: { CMT_TEST_DATA: data } });
  const log = path.join(data, 'Logs/worker-failure.json');
  const before = fs.existsSync(log) ? fs.readFileSync(log) : null;
  const report = { version: path.basename(currentExe()), completed: false };
  try {
    await rpc.call('bootstrap');
    const loading = rpc.call('native-hair-catalog', { assetType: 'hair' });
    const cancelled = assert.rejects(loading, /Operation cancelled/);
    const reads = () => rpc.events.filter(row => /Reading NBA 2K27 hair manifest assets/.test(row.message || '')).length;
    for (let i = 0; i < 400 && !reads(); i++) await delay(5);
    assert.equal(reads(), 1, 'Expected native listing to register before cancellation');
    assert.equal(await rpc.call('cancel-command', { command: 'native-hair-catalog' }), true);
    await cancelled;
    assert.equal(reads(), 1, 'Cancelled native work must not start the compatibility reader');
    const recovered = await rpc.call('native-hair-catalog', { assetType: 'hair' });
    assert.equal(recovered.native, true);
    const reference = await rpc.call('native-hair-catalog', { assetType: 'hair', nativeCatalog: false });
    assert(reference.previewDescriptors.length > 100, 'Comparison must use the complete compatibility reader');
    assert.deepEqual(recovered.assets, reference.assets, 'Cancellation recovery lost or changed manifest rows');
    if (before) assert.deepEqual(fs.readFileSync(log), before);
    else assert.equal(fs.existsSync(log), false);
    report.cancelledWithoutFallback = true;
    report.recoveredRows = recovered.assets.length;
    report.allRowsMatchIndependentReader = true;
    report.diagnosticsUnchanged = true;
    report.completed = true;
    console.log(JSON.stringify(report, null, 2));
  } finally {
    await rpc.close();
    fs.writeFileSync(path.join(root, 'artifacts/native-catalog-cancel-report.json'), JSON.stringify(report, null, 2));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
