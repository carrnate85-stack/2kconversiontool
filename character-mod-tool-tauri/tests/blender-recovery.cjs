const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const crypto = require('node:crypto');
const { Rpc, root, currentExe } = require('./rpc-client.cjs');

const hash = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const fixture = path.resolve(root, '../character-mod-tool-rust/artifacts/conversion-output/png06827.iff');
const data = fs.mkdtempSync(path.join(os.tmpdir(), 'cmt-return-acceptance-'));
const original = hash(fixture);
const report = { version:path.basename(currentExe()), executableSha256:hash(currentExe()), completed:false };
let rpc;

function abandoned(owner, suffix) {
  const session = path.join(owner, 'Sessions', '4000000000-' + suffix);
  const folder = path.join(session, 'blender-test');
  fs.mkdirSync(folder, { recursive:true });
  const returned = path.join(folder, 'png06827.iff');
  fs.copyFileSync(fixture, returned);
  fs.writeFileSync(path.join(folder, 'handoff.json'), JSON.stringify({ target:fixture, expectedDigest:original }));
  fs.writeFileSync(path.join(folder, 'backup.iff'), 'reference only');
  fs.writeFileSync(path.join(folder, 'static-jersey.iff'), 'reference only');
  return { session, returned };
}

function removeOwnedFixture() {
  assert.equal(path.dirname(fs.realpathSync(data)), fs.realpathSync(os.tmpdir()));
  assert(path.basename(data).startsWith('cmt-return-acceptance-'));
  const check = folder => {
    for (const entry of fs.readdirSync(folder, { withFileTypes:true })) {
      const file = path.join(folder, entry.name), metadata = fs.lstatSync(file);
      assert(!metadata.isSymbolicLink(), 'Fixture cleanup refuses directory redirects');
      if (metadata.isDirectory()) check(file);
    }
  };
  check(data);
  fs.rmSync(data, { recursive:true });
}

(async () => {
  try {
    const first = abandoned(data, '123');
    const live = path.join(data, 'Sessions', process.pid + '-456', 'blender-live');
    fs.mkdirSync(live, { recursive:true });
    fs.copyFileSync(fixture, path.join(live, 'live.iff'));
    rpc = new Rpc({ env:{ CMT_TEST_DATA:data } });
    await rpc.call('preview-cache-stats');
    assert(!fs.existsSync(first.session), 'Abandoned session was not retired');
    assert(fs.existsSync(path.join(live, 'live.iff')), 'Another live owner was touched');
    const folder = path.join(data, 'Blender Recovery');
    const copies = () => fs.readdirSync(folder).filter(name => name.endsWith('.iff'));
    assert.equal(copies().length, 1, 'References were incorrectly recovered');
    const saved = path.join(folder, copies()[0]);
    assert.equal(hash(saved), original);
    const health = await rpc.call('health');
    const check = health.groups.find(group => group.id === 'blender-recovery')?.checks[0];
    assert(check, 'Native recovery was omitted from the packaged Health report');
    assert.equal(check.repair.action, 'open-path');
    assert.equal(path.resolve(check.repair.path), folder);
    assert(check.message.includes('not disposable cache'));
    await rpc.call('health-repair', { action:'clean-caches' });
    assert.equal(hash(saved), original, 'Cache cleanup removed user work');
    await rpc.close(); rpc = null;
    const second = abandoned(data, '124');
    rpc = new Rpc({ env:{ CMT_TEST_DATA:data } });
    await rpc.call('preview-cache-stats');
    assert(!fs.existsSync(second.session));
    assert.equal(copies().length, 1, 'Restart duplicated an identical return');
    await rpc.close(); rpc = null;
    const blocked = path.join(data, 'blocked');
    fs.mkdirSync(blocked);
    const pending = abandoned(blocked, '125');
    fs.writeFileSync(path.join(blocked, 'Blender Recovery'), 'occupied');
    rpc = new Rpc({ env:{ CMT_TEST_DATA:blocked } });
    await rpc.call('preview-cache-stats');
    await rpc.close(); rpc = null;
    assert.equal(hash(pending.returned), original, 'Storage failure discarded the original export');
    assert.equal(hash(fixture), original, 'Original player fixture changed');
    Object.assign(report, { recoveredExactBytes:true, referencesExcluded:true, liveOwnerPreserved:true,
      healthDiscoversRecovery:true, cacheCleanupPreservesWork:true, restartDeduplicates:true,
      unavailableStorageKeepsOriginalSession:true, sourceUnchanged:true, completed:true });
  } finally {
    if (rpc) await rpc.close();
    removeOwnedFixture();
    report.fixtureRemoved = !fs.existsSync(data);
    fs.writeFileSync(path.join(root, 'artifacts/blender-recovery-report.json'), JSON.stringify(report,null,2));
  }
  console.log(JSON.stringify(report,null,2));
})().catch(error => { console.error(error); process.exitCode=1; });
