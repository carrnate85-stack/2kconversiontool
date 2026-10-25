const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { Rpc, root, currentExe } = require('./rpc-client.cjs');
(async () => {
  const rpc = new Rpc({ env: { CMT_TEST_DATA: path.join(root, 'artifacts/native-dialog-data') } });
  const report = { version: path.basename(currentExe()), completed: false, cases: [] };
  try {
    await rpc.call('bootstrap');
    for (const assetType of ['hair', 'facialhair']) {
      const start = performance.now();
      const native = await rpc.call('native-hair-catalog', { assetType });
      const nativeMs = performance.now() - start;
      assert.equal(native.native, true, native.nativeFallback);
      assert.equal(native.previewDescriptors, undefined, 'Unused eager descriptors should not cross IPC');
      const legacyStart = performance.now();
      const original = await rpc.call('native-hair-catalog', { assetType, nativeCatalog: false });
      const originalMs = performance.now() - legacyStart;
      assert(original.previewDescriptors.length > 100, 'Comparison must use the complete original reader');
      assert.deepEqual(native.assets, original.assets, 'An asset row/name/order/flag differs from the original reader');
      const cachedStart = performance.now();
      assert.deepEqual(await rpc.call('native-hair-catalog', { assetType }), native);
      const cachedMs = performance.now() - cachedStart;
      report.cases.push({ assetType, rows: native.assets.length,
        nativeMs: Math.round(nativeMs), originalMs: Math.round(originalMs), cachedMs: Math.round(cachedMs),
        nativeBytes: Buffer.byteLength(JSON.stringify(native)), originalBytes: Buffer.byteLength(JSON.stringify(original)),
        allRowsAndFieldsMatch: true });
    }
    const stats = await rpc.call('preview-cache-stats');
    assert.equal(stats.manifestIndexed, true);
    assert.equal(stats.cacheBytes, 0, 'Listing should not decode geometry or textures');
    report.sharedIndexOnly = true;
    report.completed = true;
    console.log(JSON.stringify(report, null, 2));
  } finally {
    await rpc.close();
    fs.writeFileSync(path.join(root, 'artifacts/native-accessory-catalog-report.json'), JSON.stringify(report, null, 2));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
