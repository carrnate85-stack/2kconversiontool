const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { Rpc, root } = require('./rpc-client.cjs');
const tests = path.resolve(root, '../character-mod-tool-webview2/tests');
(async () => {
  const folder = fs.mkdtempSync(path.join(root, 'artifacts/modern-eyes-'));
  const rpc = new Rpc({ env: { CMT_TEST_DATA: path.join(folder, 'data') } });
  const report = { checkedUtc: new Date().toISOString(), policyTests: 0, sourceGame: 'NBA 2K26' };
  try {
    const { settings } = await rpc.call('bootstrap');
    const backend = path.dirname(path.dirname(settings.head_swap_tool));
    const run = (name, args, env = {}) => {
      const result = spawnSync(settings.blender_exe, ['--background', '--factory-startup', '--python-exit-code', '1', '--python', path.join(tests, name), ...args], {
        windowsHide: true, encoding: 'utf8', timeout: 180000, maxBuffer: 8 * 1024 * 1024, env: { ...process.env, ...env }
      });
      const log = (result.stdout || '') + (result.stderr || '');
      fs.writeFileSync(path.join(folder, name + '.log'), log);
      assert.ifError(result.error); assert.equal(result.status, 0, log.slice(-5000));
      return log;
    };
    const policy = run('modern-eye-size.py', [], { CMT_EYE_BACKEND: backend });
    assert.match(policy, /Ran 7 tests/); assert.match(policy, /\bOK\b/); report.policyTests = 7;
    const conversion = JSON.parse(fs.readFileSync(path.join(root, 'artifacts/wpf-conversion-parity-report.json')));
    const modern = conversion.cases.find(row => row.game === 'NBA 2K26'); assert(modern);
    assert.equal(modern.different.length + modern.missing.length + modern.extra.length, 0);
    // The latest baseline comparison owns its fixture directory; use its saved output manifest.
    const candidates = fs.readdirSync(path.join(root, 'artifacts')).filter(name => name.startsWith('wpf-conversion-'));
    const fixture = candidates.map(name => path.join(root, 'artifacts', name, 'NBA-2K26'))
      .filter(name => fs.existsSync(path.join(name, 'tauri/png06827.iff')))
      .sort((a, b) => fs.statSync(b).mtimeMs - fs.statSync(a).mtimeMs)[0];
    assert(fixture, 'Run the WPF conversion comparison first');
    const target = await rpc.call('extract', { gameLabel: 'NBA 2K27', png: '06827' });
    const roundtrip = run('modern-eye-roundtrip.py', ['--', '--backend', backend,
      '--source', path.join(fixture, 'source/png1533.iff'), '--target', target.mainPath,
      '--output', path.join(fixture, 'tauri/png06827.iff'), '--source-game', 'NBA 2K26']);
    const row = roundtrip.split(/\r?\n/).find(line => line.startsWith('EYE_ROUNDTRIP|'));
    assert(row); Object.assign(report, JSON.parse(row.slice('EYE_ROUNDTRIP|'.length)));
    assert(report.unchangedMorphResources > 0);
    console.log(JSON.stringify(report, null, 2));
  } finally {
    await rpc.close();
    fs.writeFileSync(path.join(root, 'artifacts/modern-eyes-report.json'), JSON.stringify(report, null, 2));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
