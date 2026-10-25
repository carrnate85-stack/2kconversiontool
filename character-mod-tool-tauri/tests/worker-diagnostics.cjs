const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const readline = require('node:readline');
const { spawn } = require('node:child_process');
const { Rpc, root, currentExe } = require('./rpc-client.cjs');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
(async () => {
  const data = path.join(root, 'artifacts/native-dialog-data');
  const rpc = new Rpc({ env: { CMT_TEST_DATA: data } });
  const report = { version: path.basename(currentExe()), completed: false };
  let killer;
  try {
    await rpc.call('bootstrap');
    const parent = rpc.child.pid;
    assert(Number.isInteger(parent) && parent > 0);
    const quote = value => "'" + value.replace(/'/g, "''") + "'";
    // Kill only the test host's direct catalog worker, never a user Blender/game process.
    const script = [
      "$ErrorActionPreference = 'Stop'",
      '$parentId = ' + parent,
      '$expected = ' + quote(currentExe()),
      '$runtime = ' + quote(path.join(data, 'Runtime') + path.sep),
      '$deadline = [DateTime]::UtcNow.AddSeconds(15)',
      "[Console]::WriteLine('READY')",
      'while ([DateTime]::UtcNow -lt $deadline) {',
      '  $owner = Get-Process -Id $parentId -ErrorAction Stop',
      "  if ($owner.Path -ine $expected) { throw 'Owned RPC process changed' }",
      '  $workers = @(Get-CimInstance Win32_Process -Filter "ParentProcessId = $parentId AND Name = \'python.exe\'")',
      '  foreach ($worker in $workers) {',
      "    if ($worker.CommandLine -notmatch 'prototype_bridge\\.py.+native-hair-catalog') { continue }",
      "    if (!$worker.ExecutablePath.StartsWith($runtime, [StringComparison]::OrdinalIgnoreCase)) { throw 'Worker is outside the isolated runtime' }",
      '    Stop-Process -Id $worker.ProcessId -Force',
      "    [Console]::WriteLine('STOPPED:' + $worker.ProcessId)",
      '    exit 0',
      '  }',
      '  Start-Sleep -Milliseconds 50',
      '}',
      "throw 'No owned catalog worker found'",
    ].join('\n');
    killer = spawn('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', script], { windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
    let ready;
    const readyPromise = new Promise(resolve => { ready = resolve; });
    const lines = [];
    readline.createInterface({ input: killer.stdout }).on('line', line => { lines.push(line); if (line === 'READY') ready(); });
    let stderr = '';
    killer.stderr.on('data', bytes => { stderr += bytes; });
    const killed = new Promise((resolve, reject) => {
      killer.on('error', reject);
      killer.on('exit', code => code === 0 ? resolve() : reject(new Error('Worker-stop helper failed: ' + stderr)));
    });
    killed.catch(() => {});
    await Promise.race([readyPromise, killed.then(() => { throw new Error('Helper exited before readiness'); })]);
    await assert.rejects(rpc.call('native-hair-catalog', { assetType: 'hair', nativeCatalog: false }), error => {
      report.message = error.message;
      return /native-hair-catalog.*exit code .*0x[0-9A-F]{8}/.test(error.message)
        && /ended without a result/.test(error.message);
    });
    await killed;
    assert(lines.some(line => line.startsWith('STOPPED:')));
    const log = path.join(data, 'Logs/worker-failure.json');
    const before = fs.readFileSync(log);
    const failure = JSON.parse(before);
    assert.equal(failure.command, 'native-hair-catalog');
    assert(Number.isInteger(failure.exit_code));
    assert(failure.exit_code_hex.startsWith('0x'));
    assert(before.length < 65536);
    await assert.rejects(rpc.call('inspect', { filePath: 'C:/missing/cmt-diagnostic-test.iff' }), /not found|does not exist/i);
    assert.deepEqual(fs.readFileSync(log), before, 'Ordinary validation must not overwrite crash diagnostics');
    const catalog = rpc.call('native-hair-catalog', { assetType: 'facialhair', nativeCatalog: false });
    const cancelled = assert.rejects(catalog, /Operation cancelled/);
    await delay(10);
    assert.equal(await rpc.call('cancel-command', { command: 'native-hair-catalog' }), true);
    await cancelled;
    assert.deepEqual(fs.readFileSync(log), before, 'Expected cancellation must not overwrite crash diagnostics');
    assert.equal((await rpc.call('native-hair-catalog', { assetType: 'hair' })).assets.length, 5361);
    report.exitCode = failure.exit_code;
    report.exitCodeHex = failure.exit_code_hex;
    report.logBytes = before.length;
    report.ordinaryValidationPreserved = true;
    report.cancellationPreserved = true;
    report.recoveredCatalogCount = 5361;
    report.completed = true;
    console.log(JSON.stringify(report, null, 2));
  } finally {
    if (killer && killer.exitCode === null) killer.kill();
    await rpc.close();
    fs.writeFileSync(path.join(root, 'artifacts/worker-diagnostics-report.json'), JSON.stringify(report, null, 2));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
