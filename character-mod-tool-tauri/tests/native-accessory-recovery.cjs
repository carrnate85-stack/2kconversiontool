const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');
const { Rpc, root, currentExe } = require('./rpc-client.cjs');
const hash = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const python = path.resolve(root, '../character-mod-tool-webview2/dist/Character-Mod-Tool-WebView2-0.1.31/App/Backend/python_runtime/python.exe');

(async () => {
  const setup = JSON.parse(fs.readFileSync(path.join(root, 'artifacts/blender-test-setup.json')));
  const source = setup.target, stem = path.basename(source, '.iff');
  const sources = fs.readdirSync(path.dirname(source)).filter(name => name === stem + '.iff' || name.startsWith(stem + '_') && name.endsWith('.iff')).map(name => path.join(path.dirname(source), name));
  const original = Object.fromEntries(sources.map(file => [file, hash(file)]));
  const folder = fs.mkdtempSync(path.join(root, 'artifacts/native-recovery-'));
  const fixture = path.join(folder, 'player'); fs.mkdirSync(fixture);
  for (const file of sources) fs.copyFileSync(file, path.join(fixture, path.basename(file)));
  const target = path.join(fixture, path.basename(source));
  const transaction = path.join(fixture, `.cmt-accessory-${stem}-test`);
  const synthetic = path.join(fixture, stem + '_geo_recovery_test.iff');
  const unknown = path.join(transaction, 'user.blend');
  const exe = currentExe();
  const report = { version: path.basename(exe), executableSha256: hash(exe), readsMocked: false, completed: false };
  const rpc = new Rpc({ exe, env: { CMT_TEST_DATA: path.join(folder, 'data') } });
  const interrupt = () => {
    const script = String.raw`
import hashlib,json,shutil,sys,zipfile
from pathlib import Path
t,f,s,u=map(Path,sys.argv[1:])
f.mkdir(); (f/'rollback').mkdir(); (f/'prepared').mkdir()
shutil.copy2(t,f/'rollback'/t.name)
before=hashlib.sha256(t.read_bytes()).hexdigest()
with zipfile.ZipFile(t,'a') as z: z.writestr('cmt-recovery-test.txt',b'interrupted')
s.write_bytes(b'new interrupted companion'); u.write_bytes(b'user work must survive')
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
j={'version':1,'owner':'cmt-accessory-install','target':t.name,'state':'publishing','active':t.name,
   'records':[{'name':s.name,'before':None,'after':sha(s)},{'name':t.name,'before':before,'after':sha(t)}],
   'ownedFiles':['rollback/'+t.name]}
(f/'journal.json').write_text(json.dumps(j))
`;
    const result = spawnSync(python, ['-B', '-c', script, target, transaction, synthetic, unknown], { windowsHide: true, encoding: 'utf8', timeout: 30000 });
    assert.equal(result.status, 0, result.stderr);
  };
  try {
    await rpc.call('bootstrap');
    await rpc.call('save-settings', setup.settings);
    const initial = await rpc.call('native-preview-resources', { target });
    assert(initial.resourceLease);
    interrupt();
    const recovered = await rpc.call('native-preview-resources', { target });
    assert(recovered.resourceLease);
    assert.equal(hash(target), original[source]);
    assert(!fs.existsSync(synthetic));
    assert.equal(fs.readFileSync(unknown, 'utf8'), 'user work must survive');
    assert.equal(JSON.parse(fs.readFileSync(path.join(transaction, 'journal.json'))).state, 'rolled_back');
    await rpc.call('release-preview-resources', { lease: initial.resourceLease });
    await rpc.call('release-preview-resources', { lease: recovered.resourceLease });
    const again = await rpc.call('native-preview-resources', { target });
    assert(again.resourceLease); await rpc.call('release-preview-resources', { lease: again.resourceLease });
    report.nativeReadRecovered = true;
    report.retainedEvidenceAndUnknownBlenderFile = true;
    report.subsequentReadSucceeded = true;
    const stale = await rpc.call('native-preview-resources', { target });
    const companion = sources.find(file => file !== source);
    assert(companion, 'A companion is required for the partial-rollback conflict test');
    const localCompanion = path.join(fixture, path.basename(companion));
    const conflict = path.join(fixture, `.cmt-accessory-${stem}-conflict`);
    const script = String.raw`
import hashlib,json,shutil,sys,zipfile
from pathlib import Path
t,c,f=map(Path,sys.argv[1:]); f.mkdir(); (f/'rollback').mkdir()
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
shutil.copy2(t,f/'rollback'/t.name); shutil.copy2(c,f/'rollback'/c.name)
before,cbefore=sha(t),sha(c)
with zipfile.ZipFile(t,'a') as z: z.writestr('cmt-partial-rollback-test.txt',b'interrupted')
c.write_bytes(b'newer external companion edits')
j={'version':1,'owner':'cmt-accessory-install','target':t.name,'state':'publishing','active':t.name,
   'records':[{'name':c.name,'before':cbefore,'after':'0'*64},{'name':t.name,'before':before,'after':sha(t)}],
   'ownedFiles':['rollback/'+t.name,'rollback/'+c.name]}
(f/'journal.json').write_text(json.dumps(j))
`;
    const seeded = spawnSync(python, ['-B', '-c', script, target, localCompanion, conflict], { windowsHide: true, encoding: 'utf8', timeout: 30000 });
    assert.equal(seeded.status, 0, seeded.stderr);
    await assert.rejects(rpc.call('native-preview-resources', { target }), /changed file/i);
    assert.equal(hash(target), original[source], 'Partial rollback did not restore the main before the conflict');
    assert.equal(fs.readFileSync(localCompanion, 'utf8'), 'newer external companion edits');
    await assert.rejects(rpc.call('read-archive', { path: stale.filePath }), /expired/i);
    assert(fs.existsSync(path.join(conflict, 'rollback', path.basename(target))));
    fs.copyFileSync(companion, localCompanion);
    report.partialRollbackConflictBlocksReadAndRetiresCache = true;
    for (const file of sources) {
      assert.equal(hash(file), original[file]);
      assert.equal(hash(path.join(fixture, path.basename(file))), original[file]);
    }
    report.originalsAndCompanionsUnchanged = true;
    report.completed = true;
    console.log(JSON.stringify(report, null, 2));
  } finally {
    await rpc.close();
    fs.writeFileSync(path.join(root, 'artifacts/native-accessory-recovery-report.json'), JSON.stringify(report, null, 2));
    assert.equal(path.dirname(fs.realpathSync(folder)), fs.realpathSync(path.join(root, 'artifacts')));
    assert(!fs.lstatSync(folder).isSymbolicLink());
    fs.rmSync(folder, { recursive: true, maxRetries: 10, retryDelay: 200 });
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
