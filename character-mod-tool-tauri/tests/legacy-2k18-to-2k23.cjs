const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { Rpc, root } = require('./rpc-client.cjs');

function snapshot(folder) {
  return Object.fromEntries(fs.readdirSync(folder).sort().map(name => {
    const file = path.join(folder, name);
    return [name, crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex')];
  }));
}

(async () => {
  const fixtures = path.join(root, 'artifacts', 'legacy-2k18-to-2k23-fixtures');
  const sourceFolder = path.join(fixtures, 'source-2k18-png0926');
  const targetFolder = path.join(fixtures, 'target-2k23-png4090');
  const source = path.join(sourceFolder, 'png0926.iff');
  const target = path.join(targetFolder, 'png4090.iff');
  assert(fs.existsSync(source), 'Missing stock NBA 2K18 png0926 fixture');
  assert(fs.existsSync(target), 'Missing stock NBA 2K23 png4090 fixture');
  const beforeSource = snapshot(sourceFolder);
  const beforeTarget = snapshot(targetFolder);
  const outputDir = path.join(root, 'artifacts', 'legacy-2k18-to-2k23-output');
  fs.rmSync(outputDir, { recursive: true, force: true });
  fs.mkdirSync(outputDir, { recursive: true });

  const rpc = new Rpc();
  try {
    const result = await rpc.call('run-swap', {
      source,
      target,
      sourceGame: 'NBA 2K18',
      targetGame: 'NBA 2K23',
      outputDir,
      shrinkwrapBody: true,
      hairEnabled: false,
      facialHairEnabled: false,
    }, 600000);
    assert.equal(result.validation.errors, 0, JSON.stringify(result.validation));
    assert.equal(path.resolve(result.output), path.join(outputDir, 'png4090.iff'));
    assert(fs.existsSync(result.output), 'Converted NBA 2K23 player was not created');
    const faceOutput = path.join(outputDir, 'face4090.iff');
    assert(fs.existsSync(faceOutput), 'Converted NBA 2K23 face texture companion was not created');
    assert.deepEqual(snapshot(sourceFolder), beforeSource, 'Source fixture changed during conversion');
    assert.deepEqual(snapshot(targetFolder), beforeTarget, 'Target fixture changed during conversion');
    assert(!fs.readdirSync(outputDir).some(name => /_config_/i.test(name)), '2K23 route fabricated modern config archives');

    const backend = path.resolve(root, '../character-mod-tool-webview2/dist/Character-Mod-Tool-WebView2-0.1.31/App/Backend');
    const python = path.join(backend, 'python_runtime', 'python.exe');
    const verified = spawnSync(python, ['-B', path.join(__dirname, 'legacy-2k18-to-2k23-verify.py'), target, result.output,
      path.join(targetFolder, 'face4090.iff'), faceOutput], { windowsHide: true, encoding: 'utf8' });
    assert.equal(verified.status, 0, verified.stderr || verified.stdout);
    const report = { checkedUtc: new Date().toISOString(), result, verification: JSON.parse(verified.stdout) };
    fs.writeFileSync(path.join(root, 'artifacts', 'legacy-2k18-to-2k23-report.json'), JSON.stringify(report, null, 2));
    console.log(JSON.stringify(report, null, 2));
  } finally {
    await rpc.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
