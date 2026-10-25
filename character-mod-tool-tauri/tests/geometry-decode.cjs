const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const zlib = require('node:zlib');
const { spawnSync } = require('node:child_process');
const { root, currentExe } = require('./rpc-client.cjs');
const folder = fs.mkdtempSync(path.join(root, 'artifacts/geometry-decode-'));
try {
  fs.writeFileSync(path.join(folder, 'manifest'), 'test-only manifest');
  const exe = process.env.CMT_TEST_DDS_EXE || currentExe();
  const execute = (args, input) => spawnSync(exe, args, { windowsHide: true, input, timeout: 15000 });
  const raw = Buffer.from([10, 11, 12, 13, 14, 15, 16, 17]);
  for (const input of [raw, zlib.gzipSync(raw)]) {
    const result = execute(['--decode-geometry', folder, '8'], input);
    assert.ifError(result.error); assert.equal(result.status, 0, String(result.stderr)); assert.deepEqual(result.stdout, raw);
  }
  for (const [args, input] of [
    [['--decode-geometry', folder, '0'], raw],
    [['--decode-geometry', folder, '134217729'], raw],
    [['--decode-geometry', folder, '4'], zlib.gzipSync(raw)],
    [['--decode-geometry', path.join(folder, 'missing-game'), '8'], raw],
    [['--decode-geometry', folder], raw],
    [['--decode-scene'], Buffer.from('not a scene')]
  ]) {
    const result = execute(args, input); assert.ifError(result.error); assert.notEqual(result.status, 0); assert.equal(result.stdout.length, 0);
  }
  const scene = Buffer.from('{"head":{"Model":{"hihead":{"Radius":3}}}}');
  const decoded = execute(['--decode-scene'], scene); assert.equal(decoded.status, 0, String(decoded.stderr));
  assert.deepEqual(JSON.parse(decoded.stdout), JSON.parse(scene));
  fs.writeFileSync(path.join(root, 'artifacts/geometry-decode-report.json'), JSON.stringify({ checkedUtc: new Date().toISOString(), tests: 9, passed: true, partialOutputOnError: false }, null, 2));
  console.log('Nine native geometry/scene decoding guards passed.');
} finally {
  assert.equal(path.dirname(fs.realpathSync(folder)), fs.realpathSync(path.join(root, 'artifacts')));
  assert(!fs.lstatSync(folder).isSymbolicLink()); fs.rmSync(folder, { recursive: true });
}
