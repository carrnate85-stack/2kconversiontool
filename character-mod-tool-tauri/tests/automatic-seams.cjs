const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { Rpc, root, currentExe } = require('./rpc-client.cjs');
const digest = p => crypto.createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const snapshot = dir => Object.fromEntries(fs.readdirSync(dir).filter(n => /\.iff$/i.test(n)).map(n => [n, digest(path.join(dir, n))]));

(async () => {
  const source = process.env.CMT_SEAM_SOURCE;
  if (!source || !fs.existsSync(source)) throw Error('Set CMT_SEAM_SOURCE to a saved NBA 2K23 main IFF with companions.');
  const version = JSON.parse(fs.readFileSync(path.join(root, 'dist/BUILD.json'), 'utf8').replace(/^\uFEFF/, '')).version;
  const folder = fs.mkdtempSync(path.join(root, 'artifacts/automatic-seams-'));
  const data = path.join(folder, 'data'); fs.mkdirSync(data);
  const sourceCopy = path.join(folder, 'source');
  fs.cpSync(path.dirname(source), sourceCopy, { recursive: true });
  const sourceBefore = snapshot(path.dirname(source));
  const rpc = new Rpc({ env: { CMT_TEST_DATA: data } });
  try {
    await rpc.call('bootstrap');
    await rpc.call('save-settings', { blender_exe: 'C:/Program Files/Blender Foundation/Blender 5.1/blender.exe',
      game_root_2k27: 'C:/Program Files (x86)/Steam/steamapps/common/NBA 2K27' });
    const target = await rpc.call('extract', { gameLabel: 'NBA 2K27', png: '06827' });
    const targetBefore = snapshot(path.dirname(target.mainPath));
    const start = Date.now();
    console.log('Running isolated headless NBA 2K23 -> NBA 2K27 Full Swap.');
    const result = await rpc.call('run-swap', { source: path.join(sourceCopy, path.basename(source)),
      target: target.mainPath, sourceGame: 'NBA 2K23', targetGame: '',
      outputDir: path.join(folder, 'output'), shrinkwrapBody: true, hairEnabled: false, facialHairEnabled: false }, 300000);
    assert.equal(result.validation.errors, 0);
    assert.equal(result.automaticSeamCorrection.applied, true, JSON.stringify(result.automaticSeamCorrection));
    assert(fs.existsSync(result.automaticSeamCorrection.backup));
    const outputHash = digest(result.output);
    const repeat = await rpc.call('preview-close-seams', { target: result.output });
    assert.equal(repeat.applied, false);
    assert.equal(digest(result.output), outputHash);
    assert.deepEqual(snapshot(path.dirname(source)), sourceBefore);
    assert.deepEqual(snapshot(path.dirname(target.mainPath)), targetBefore);
    const preview = await rpc.call('preview-resources', { target: result.output });
    assert.equal(path.resolve(preview.filePath), path.resolve(result.output));
    const report = { version, executableSha256: digest(currentExe()), elapsedMs: Date.now() - start,
      source: path.basename(source), sourceGame: 'NBA 2K23', target: 'NBA 2K27 png06827',
      validationErrors: result.validation.errors, automaticSeamCorrection: result.automaticSeamCorrection,
      repeatClickNoop: true, sourceAndTargetUnchanged: true, previewReadable: true,
      desktopUntouched: true, output: result.output, outputHash };
    fs.writeFileSync(path.join(root, `artifacts/automatic-seams-${version}.json`), JSON.stringify(report, null, 2));
    console.log(JSON.stringify(report, null, 2));
  } finally { await rpc.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
