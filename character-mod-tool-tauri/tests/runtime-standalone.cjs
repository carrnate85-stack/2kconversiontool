const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const { Rpc, root, currentExe } = require('./rpc-client.cjs');
(async () => {
  const data = fs.mkdtempSync(path.join(root, 'artifacts/runtime-standalone-'));
  const windows = process.env.WINDIR || 'C:/Windows';
  const systemPath = windows + '/System32;' + windows;
  const rpc = new Rpc({ env: { CMT_TEST_DATA: data, PATH: systemPath, PYTHONPATH: '', PYTHONHOME: '' } });
  const metadata = JSON.parse(fs.readFileSync(path.join(root, 'dist/BUILD.json'), 'utf8').replace(/^\uFEFF/, ''));
  const report = { version: path.basename(currentExe()), completed: false };
  try {
    await rpc.call('bootstrap');
    const engine = path.join(data, 'Runtime/engine-' + metadata.payloadSha256.slice(0, 16), 'Backend');
    const code = [
      'import sys,os,json,io,importlib.util',
      'from pathlib import Path',
      'root=Path(sys.argv[1]); data=Path(sys.argv[2]); sys.path.insert(0,str(root))',
      'import app_settings',
      'app_settings._USER_DATA_DIR=data; app_settings.SETTINGS_PATH=data/"settings.json"',
      'app_settings.LOG_DIR=data/"logs"; app_settings.DIAGNOSTICS_DIR=data/"diagnostics"',
      'import character_mod_tool,python_bridge,prototype_services,character_manifest,nba2k25_support,nba2k25_facial_hair',
      'from hair_tools import hair_picker,legacy_facial_hair',
      'from PIL import Image',
      'import zstandard,rarfile,tkinter',
      'image=Image.new("RGB",(2,2),(30,60,90)); output=io.BytesIO(); image.save(output,format="PNG"); assert output.getvalue().startswith(b"\\x89PNG")',
      'sample=b"standalone codec check"*100; assert zstandard.ZstdDecompressor().decompress(zstandard.ZstdCompressor().compress(sample))==sample',
      'removed=["pip","PyInstaller","setuptools","altgraph","packaging","pefile","win32ctypes","capstone"]',
      'assert all(importlib.util.find_spec(name) is None for name in removed)',
      'assert sys.flags.isolated and sys.flags.no_user_site',
      'assert all(Path(module.__file__).is_relative_to(root) for module in [character_mod_tool,python_bridge,hair_picker,Image,zstandard,rarfile])',
      'print(json.dumps({"isolated":True,"requiredImports":True,"imageAndCompressionCodecs":True,"tcl":tkinter.Tcl().eval("info patchlevel"),"removedModulesAbsent":removed}))',
    ].join('\n');
    report.python = JSON.parse(execFileSync(path.join(engine, 'python_runtime/python.exe'), ['-I', '-B', '-c', code, engine, data],
      { windowsHide: true, encoding: 'utf8', env: { ...process.env, PATH: systemPath,
        CHARACTER_MOD_TOOL_ROOT: engine, CMT_RUST_DATA: data, CMT_RUST_EXE: currentExe() } }));
    const health = await rpc.call('health');
    assert.equal(health.counts.error, 0);
    report.healthErrors = 0;
    report.completed = true;
    console.log(JSON.stringify(report, null, 2));
  } finally {
    await rpc.close();
    fs.writeFileSync(path.join(root, 'artifacts/runtime-standalone-report.json'), JSON.stringify(report, null, 2));
    assert.equal(path.dirname(fs.realpathSync(data)), fs.realpathSync(path.join(root, 'artifacts')));
    fs.rmSync(data, { recursive: true });
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
