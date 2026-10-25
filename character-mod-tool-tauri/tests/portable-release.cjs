const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { Rpc, root } = require('./rpc-client.cjs');
const python = path.resolve(root, '../character-mod-tool-webview2/dist/Character-Mod-Tool-WebView2-0.1.31/App/Backend/python_runtime/python.exe');
const version = JSON.parse(fs.readFileSync(path.join(root, 'dist/BUILD.json'), 'utf8').replace(/^\uFEFF/, '')).version;
const report = { version, completed: false, mutationsMocked: false };
const fixture = fs.mkdtempSync(path.join(root, 'artifacts/portable-release-'));
let rpc;
function run(exe, args, timeout = 180000) {
  const result = spawnSync(exe, args, { windowsHide: true, encoding: 'utf8', timeout, maxBuffer: 8 * 1024 * 1024 });
  assert.ifError(result.error);
  assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`.slice(-7000));
  return result.stdout + result.stderr;
}
(async () => {
  try {
    // Validate and extract the actual shipping archives, not a hand-built fixture.
    report.packages = JSON.parse(run(python, ['-B', '-c', `
import hashlib,json,pathlib,sys,zipfile
root=pathlib.Path(sys.argv[1]); destination=pathlib.Path(sys.argv[2]); version=sys.argv[3]
expected=hashlib.sha256((root/f'dist/Character-Mod-Tool-Tauri-Preview-{version}.exe').read_bytes()).hexdigest()
rows=[]
for label in ('Standard','With-Blender-5.1.2'):
    with zipfile.ZipFile(root/f'dist/Character-Mod-Tool-{version}-{label}.zip') as z:
        names=z.namelist(); assert len(names)==len(set(names)); assert z.testzip() is None
        prefix=f'Character Mod Tool {version}/'
        assert all(n.startswith(prefix) and '..' not in pathlib.PurePosixPath(n).parts and '\\\\' not in n for n in names)
        assert hashlib.sha256(z.read(prefix+'Character-Mod-Tool.exe')).hexdigest()==expected
        bundled=label.startswith('With-')
        assert (prefix+'Blender/blender.exe' in names)==bundled
        assert (prefix+'Sources/blender-5.1.2.tar.xz' in names)==bundled
        if bundled:
            assert hashlib.sha256(z.read(prefix+'Sources/blender-5.1.2.tar.xz')).hexdigest()=='aedfd030231979b629b18049d93b92fd2794952ca0cf39ebde54e84438bed05a'
            with zipfile.ZipFile(root/'artifacts/vendor/blender-5.1.2/blender-5.1.2-windows-x64.zip') as original:
                for info in original.infolist():
                    if info.is_dir(): continue
                    relative=info.filename.removeprefix('blender-5.1.2-windows-x64/')
                    assert hashlib.sha256(z.read(prefix+'Blender/'+relative)).digest()==hashlib.sha256(original.read(info)).digest(),relative
        long_destination=chr(92)*2+'?'+chr(92)+str((destination/label).resolve())
        z.extractall(long_destination)
        rows.append(dict(label=label,members=len(names),crcPassed=True,executableIdentical=True,officialBlenderIdentical=bundled))
print(json.dumps(rows))
`, root, fixture, version], 300000));
    const appFolder = path.join(fixture, 'With-Blender-5.1.2', `Character Mod Tool ${version}`);
    const data = path.join(fixture, 'data');
    rpc = new Rpc({ exe: path.join(appFolder, 'Character-Mod-Tool.exe'), env: { CMT_TEST_DATA: data } });
    let setup = await rpc.call('bootstrap');
    assert.equal(path.resolve(setup.settings.blender_exe), path.join(appFolder, 'Blender/blender.exe'));
    report.freshAutoDetected = true;
    await rpc.call('save-settings', setup.settings);
    assert.equal(JSON.parse(fs.readFileSync(path.join(data, 'settings.json'))).blender_exe, '');
    await rpc.close(); rpc = null;
    const moved = path.join(fixture, 'Moved release with spaces');
    fs.renameSync(appFolder, moved);
    rpc = new Rpc({ exe: path.join(moved, 'Character-Mod-Tool.exe'), env: { CMT_TEST_DATA: data } });
    setup = await rpc.call('bootstrap');
    assert.equal(path.resolve(setup.settings.blender_exe), path.join(moved, 'Blender/blender.exe'));
    report.relocatedAndSavedSettingsPassed = true;
    const standardExe = path.join(fixture, 'Standard', `Character Mod Tool ${version}`, 'Character-Mod-Tool.exe');
    const custom = path.join(fixture, 'Custom Blender', 'blender.exe');
    fs.mkdirSync(path.dirname(custom));
    fs.copyFileSync(setup.settings.blender_exe, custom);
    let saved = await rpc.call('save-settings', { blender_exe: custom });
    assert.equal(saved.settings.blender_exe, custom);
    saved = await rpc.call('save-settings', { blender_exe: '' });
    assert.equal(saved.settings.blender_exe, setup.settings.blender_exe);
    report.explicitOverrideAndResetPassed = true;
    report.blenderVersion = run(setup.settings.blender_exe, ['--background', '--factory-startup', '--version']).split(/\r?\n/)[0];
    assert.match(report.blenderVersion, /^Blender 5\.1\.2/);
    let source;
    if (process.env.CMT_PORTABLE_SOURCE) {
      const original = path.resolve(process.env.CMT_PORTABLE_SOURCE);
      const copy = path.join(fixture, 'legacy-source');
      fs.cpSync(path.dirname(original), copy, { recursive: true });
      source = { mainPath: path.join(copy, path.basename(original)) };
      report.sourceFixture = { game: 'NBA 2K23', player: path.basename(original), savedPackage: true };
    } else {
      source = await rpc.call('extract', { gameLabel: 'NBA 2K23', png: '4100' });
      report.sourceFixture = { game: 'NBA 2K23', player: 'png4100.iff', savedPackage: false };
    }
    const target = await rpc.call('extract', { gameLabel: 'NBA 2K27', png: '06827' });
    const request = { source: source.mainPath, target: target.mainPath, sourceGame: 'NBA 2K23', targetGame: 'NBA 2K27',
      outputDir: path.join(fixture, 'output'), shrinkwrapBody: true, hairEnabled: false, facialHairEnabled: false };
    const result = await rpc.call('run-swap', request, 300000);
    assert.equal(result.validation.errors, 0);
    assert(fs.statSync(result.output).size > 0);
    report.fullSwap = { source: report.sourceFixture, target: 'NBA 2K27 png06827', validationErrors: 0, outputBytes: fs.statSync(result.output).size };
    const engine = path.dirname(path.dirname(setup.settings.head_swap_tool));
    const returned = path.join(fixture, 'returned.iff'), signal = path.join(fixture, 'returned.json');
    const log = run(setup.settings.blender_exe, ['--background', '--factory-startup', '--python-exit-code', '1', '--python',
      path.join(engine, 'electron/blender_open_output.py'), '--', '--iff', result.output, '--addon', setup.settings.head_swap_tool,
      '--return-iff', returned, '--signal', signal, '--auto-send', '--test-nudge', '--test-edit-mode']);
    fs.writeFileSync(path.join(root, `artifacts/portable-blender-roundtrip-${version}.log`), log);
    assert.equal(JSON.parse(fs.readFileSync(signal)).status, 'success');
    run(python, ['-B', '-c', 'import sys,zipfile; z=zipfile.ZipFile(sys.argv[1]); assert z.testzip() is None; z.close()', returned]);
    report.blenderEditModeRoundTripPassed = true;
    await rpc.close(); rpc = null;
    rpc = new Rpc({ exe: standardExe, env: { CMT_TEST_DATA: path.join(fixture, 'standard-data') } });
    const standard = await rpc.call('bootstrap');
    assert(!standard.settings.blender_exe.startsWith(fixture));
    report.standardInstalledDiscoveryPreserved = true;
    report.completed = true;
    console.log(JSON.stringify(report, null, 2));
  } finally {
    if (rpc) await rpc.close();
    fs.writeFileSync(path.join(root, `artifacts/portable-release-check-${version}.json`), JSON.stringify(report, null, 2));
    assert.equal(fs.realpathSync(path.dirname(fixture)), fs.realpathSync(path.join(root, 'artifacts')));
    fs.rmSync(fixture, { recursive: true });
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
