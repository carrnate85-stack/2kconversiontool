const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { Rpc, root } = require('./rpc-client.cjs');
(async () => {
  const rpc = new Rpc({ env: { CMT_TEST_DATA: path.join(root, 'artifacts/native-dialog-data') } });
  const report = { version: JSON.parse(fs.readFileSync(path.join(root, 'dist/BUILD.json'), 'utf8').replace(/^\uFEFF/, '')).version, completed: false, cases: [], unavailable: [] };
  try {
    const bootstrap = await rpc.call('bootstrap');
    for (const gameLabel of ['NBA 2K18', 'NBA 2K19', 'NBA 2K20', 'NBA 2K23', 'NBA 2K25', 'NBA 2K26', 'NBA 2K27']) {
      if (!bootstrap.games[gameLabel]) { report.unavailable.push(gameLabel); continue; }
      const start = performance.now();
      const first = await rpc.call('catalog', { gameLabel });
      const firstMs = performance.now() - start;
      const events = rpc.events.length;
      const cachedStart = performance.now();
      const cached = await rpc.call('catalog', { gameLabel });
      const cachedMs = performance.now() - cachedStart;
      assert(first.rows.length > 0);
      assert.deepEqual(cached, first, 'Cached catalog changed fields for ' + gameLabel);
      assert.equal(rpc.events.length, events, 'Unchanged manifest launched another worker for ' + gameLabel);
      report.cases.push({ gameLabel, rows: first.rows.length, firstMs: Math.round(firstMs), cachedMs: Math.round(cachedMs), cacheHitWithoutWorker: true });
    }
    assert(report.cases.length >= 3, 'Installed-generation cache coverage is unexpectedly missing');
    report.completed = true;
    console.log(JSON.stringify(report, null, 2));
  } finally {
    await rpc.close();
    fs.writeFileSync(path.join(root, 'artifacts/manifest-cache-performance-report.json'), JSON.stringify(report, null, 2));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
