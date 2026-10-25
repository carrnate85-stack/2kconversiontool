const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { Rpc, root } = require('./rpc-client.cjs');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
(async () => {
  const rpc = new Rpc({ env: { CMT_TEST_DATA: path.join(root, 'artifacts/accessory-preview-data') } });
  const report = { version: JSON.parse(fs.readFileSync(path.join(root, 'dist/BUILD.json'), 'utf8').replace(/^\uFEFF/, '')).version, completed: false, cases: [] };
  try {
    await rpc.call('bootstrap');
    for (const milliseconds of [5, 15, 30]) {
      await rpc.call('reset');
      const descriptor = await rpc.call('native-preview-hair', { assetType: 'hair',
        assetId: 'char/sig/png06572_geo_hair_curly_afro_tangentspace.iff' });
      const started = Date.now();
      const geometry = rpc.call('native-catalog-geometry', { ...descriptor, includeBase: true })
        .then(result => ({ ok: true, vertices: result.stats.tangentHairProjectedVertices }),
          error => ({ ok: false, error: String(error) }));
      await delay(milliseconds);
      await rpc.call('cancel-preview', { kind: 'staged' });
      const result = await geometry;
      report.cases.push({ unrelatedCancel: 'staged', activePreview: 'catalog', milliseconds, elapsed: Date.now() - started, ...result });
      console.log(JSON.stringify(report.cases.at(-1)));
    }
    assert(report.cases.every(item => item.ok), 'Cancelling the main preview interrupted native catalog geometry');
    for (const [scope, cancelKind, shouldComplete] of [['staged', 'catalog', true], ['catalog', 'catalog', false], ['independent', 'catalog', true]]) {
      await rpc.call('reset');
      const descriptor = await rpc.call('native-preview-hair', { assetType: 'hair',
        assetId: 'char/sig/png06572_geo_hair_curly_afro_tangentspace.iff' });
      const geometry = rpc.call('native-catalog-geometry', { ...descriptor, includeBase: true, _cmtPreviewKind: scope })
        .then(result => ({ ok: true, vertices: result.stats.tangentHairProjectedVertices }), error => ({ ok: false, error: String(error) }));
      await delay(5);
      await rpc.call('cancel-preview', { kind: cancelKind });
      const result = await geometry;
      report.cases.push({ activePreview: scope, cancelledPreview: cancelKind, ...result });
      assert.equal(result.ok, shouldComplete, JSON.stringify(result));
      if (!shouldComplete) assert.match(result.error, /cancelled/i);
    }
    report.completed = true;
  } finally {
    await rpc.close();
    fs.writeFileSync(path.join(root, 'artifacts/preview-cancel-isolation-report.json'), JSON.stringify(report, null, 2));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
