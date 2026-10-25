const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');
const { Rpc, root, currentExe } = require('./rpc-client.cjs');
const production = path.resolve(root, '../character-mod-tool-webview2/dist/Character-Mod-Tool-WebView2-0.1.31/App/Backend');
(async () => {
  const folder = fs.mkdtempSync(path.join(root, 'artifacts/stock-headband-'));
  const rpc = new Rpc({ env: { CMT_TEST_DATA: path.join(folder, 'data') } });
  const report = { checkedUtc: new Date().toISOString(), completed: false, sourceGeometryPreserved: false };
  try {
    const { settings } = await rpc.call('bootstrap');
    const source = await rpc.call('extract', { gameLabel: 'NBA 2K27', png: '06827' });
    const sourceHash = crypto.createHash('sha256').update(fs.readFileSync(source.mainPath)).digest('hex');
    const stage = path.join(folder, 'stage'); fs.mkdirSync(stage);
    for (const name of fs.readdirSync(path.dirname(source.mainPath)).filter(name => /\.iff$/i.test(name))) fs.copyFileSync(path.join(path.dirname(source.mainPath), name), path.join(stage, name));
    const target = path.join(stage, path.basename(source.mainPath));
    const before = path.join(folder, 'original.iff'); fs.copyFileSync(target, before);
    const configs = await rpc.call('configs-list', { target });
    report.result = await rpc.call('headband-apply', { target, config: configs.defaultConfig, configScoped: true, mode: 'add', style: 'Stock NBA 2K27 Headband', color: '#2244CC', addLogo: false });
    const verify = spawnSync(path.join(production, 'python_runtime/python.exe'), ['-B', '-c',
      "import json,sys,zipfile; a=zipfile.ZipFile(sys.argv[1]); b=zipfile.ZipFile(sys.argv[2]); excluded={'appearance_info.json','appearance.rdat'}; names=[n for n in a.namelist() if n.lower() not in excluded]; assert all(n in b.namelist() and a.read(n)==b.read(n) for n in names); print(json.dumps({'unchangedPlayerResources':len(names),'compressedGeometryUnchanged':sum(a.read(n)[12:16]==b'VCZ\\x00' for n in names)}))", before, target], { windowsHide: true, encoding: 'utf8' });
    assert.equal(verify.status, 0, verify.stderr); Object.assign(report, JSON.parse(verify.stdout));
    assert(report.compressedGeometryUnchanged > 0);
    assert.equal(crypto.createHash('sha256').update(fs.readFileSync(source.mainPath)).digest('hex'), sourceHash);
    report.sourceGeometryPreserved = true;
    const resources = await rpc.call('native-preview-resources', { target, config: configs.defaultConfig });
    assert(resources.activeItems.includes('headband'));
    const headband = resources.options.companionPaths.find(row => /_geo_headband\.iff$/i.test(row.name)); assert(headband);
    const geometry = await rpc.call('read-archive', { path: headband.path });
    assert(geometry.members.some(row => /\.scne$/i.test(row.name)));
    report.previewHeadband = true;
    report.progress = rpc.events.filter(row => /compressed buffers for headband fitting/.test(row.message || '')).map(row => row.message);
    assert(report.progress.length);
    const health = await rpc.call('health'); assert.equal(health.counts.error, 0);
    report.completed = true;
    console.log(JSON.stringify(report, null, 2));
  } finally {
    await rpc.close(); fs.writeFileSync(path.join(root, 'artifacts/stock-headband-report.json'), JSON.stringify(report, null, 2));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
