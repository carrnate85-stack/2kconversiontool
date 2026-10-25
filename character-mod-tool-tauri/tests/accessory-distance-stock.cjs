const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const { Rpc, root, currentExe } = require('./rpc-client.cjs');
(async () => {
  const rpc = new Rpc({ env: { CMT_TEST_DATA: path.join(root, 'artifacts/native-dialog-data') } });
  const report = { version: path.basename(currentExe()), completed: false };
  try {
    await rpc.call('bootstrap');
    const stock = await rpc.call('extract', { gameLabel: 'NBA 2K27', png: '06827' });
    const engine = path.resolve(root, '../character-mod-tool-webview2/dist/Character-Mod-Tool-WebView2-0.1.31/App/Backend');
    report.packages = JSON.parse(execFileSync(path.join(engine, 'python_runtime/python.exe'), [
      '-B', path.join(__dirname, 'accessory-distance-inspect.py'), engine, stock.mainPath,
      path.join(root, 'artifacts/ui-conversion-output/png06827.iff'),
    ], { windowsHide: true, encoding: 'utf8' }));
    assert(report.packages.every(pkg => pkg.allSourceHashesUnchanged));
    report.completed = true;
    console.log(JSON.stringify(report, null, 2));
  } finally {
    await rpc.close();
    fs.writeFileSync(path.join(root, 'artifacts/accessory-distance-stock-report.json'), JSON.stringify(report, null, 2));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
