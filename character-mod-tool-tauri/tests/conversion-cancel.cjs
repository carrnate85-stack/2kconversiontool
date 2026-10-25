const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { Rpc, root } = require('./rpc-client.cjs');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
(async () => {
  const folder = fs.mkdtempSync(path.join(root, 'artifacts/cancel-swap-'));
  const rpc = new Rpc();
  const report = { checkedUtc: new Date().toISOString() };
  try {
    const source = await rpc.call('extract', { gameLabel: 'NBA 2K23', png: '4100' });
    const target = await rpc.call('extract', { gameLabel: 'NBA 2K27', png: '06827' });
    const startEvents = rpc.events.length;
    const swap = rpc.call('run-swap', { source: source.mainPath, target: target.mainPath, sourceGame: 'NBA 2K23', targetGame: 'NBA 2K27',
      outputDir: folder, shrinkwrapBody: true, hairEnabled: false, facialHairEnabled: false, _cmtJobId: 'owned-cancel-test' });
    const rejected = assert.rejects(swap, /cancelled/i);
    const deadline = Date.now() + 30000;
    while (!rpc.events.slice(startEvents).some(row => row.command === 'run-swap' && row.type === 'job-started' && row.jobId === 'owned-cancel-test')) {
      assert(Date.now() < deadline, 'Conversion did not publish its startup event'); await delay(20);
    }
    assert.equal(await rpc.call('cancel-command', { command: 'run-swap', jobId: 'a-stale-job' }), false);
    assert.equal(await rpc.call('cancel-command', { command: 'run-swap', jobId: 'owned-cancel-test' }), true);
    await rejected;
    const entries = fs.readdirSync(folder);
    assert(!entries.some(name => /^png.*\.iff$/i.test(name)), 'Cancelled Blender phase must not publish a final player');
    await assert.rejects(rpc.call('run-swap', { source: 'C:/missing/source.iff', target: target.mainPath }), /not found|does not exist|file/i);
    const inspected = await rpc.call('inspect', { filePath: target.mainPath });
    assert(inspected.path);
    report.cancelled = true; report.staleCancelIgnored = true; report.nativeRegistrationAcknowledged = true; report.finalPlayerPublished = false; report.nextJobNotBlocked = true;
    console.log(JSON.stringify(report, null, 2));
  } finally {
    await rpc.close();
    fs.writeFileSync(path.join(root, 'artifacts/conversion-cancel-report.json'), JSON.stringify(report, null, 2));
    assert.equal(path.dirname(fs.realpathSync(folder)), fs.realpathSync(path.join(root, 'artifacts')));
    fs.rmSync(folder, { recursive: true });
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
