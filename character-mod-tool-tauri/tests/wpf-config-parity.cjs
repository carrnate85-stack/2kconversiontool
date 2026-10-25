const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');
const { Rpc, root } = require('./rpc-client.cjs');
const { WpfRpc } = require('./wpf-rpc.cjs');
const production = path.resolve(root, '../character-mod-tool-webview2/dist/Character-Mod-Tool-WebView2-0.1.31/App/Backend');
(async () => {
  const folder = fs.mkdtempSync(path.join(root, 'artifacts/wpf-config-'));
  const rpc = new Rpc({ env: { CMT_TEST_DATA: path.join(folder, 'tauri-data') } });
  const report = { baseline: 'WPF 0.1.31 packaged backend', checkedUtc: new Date().toISOString(), completed: false, steps: [] };
  const mappings = { wpf: {}, tauri: {} };
  mappings.documents = { wpf: {}, tauri: {} };
  mappings.sharedDds = { wpf: {}, tauri: {} };
  let old;
  try {
    const { settings } = await rpc.call('bootstrap');
    const source = await rpc.call('extract', { gameLabel: 'NBA 2K27', png: '04090' });
    const snapshots = Object.fromEntries(fs.readdirSync(path.dirname(source.mainPath)).filter(name => /\.iff$/i.test(name))
      .map(name => [name, crypto.createHash('sha256').update(fs.readFileSync(path.join(path.dirname(source.mainPath), name))).digest('hex')]));
    const targets = {};
    for (const side of ['wpf', 'tauri']) {
      const directory = path.join(folder, side); fs.mkdirSync(directory);
      for (const name of Object.keys(snapshots)) fs.copyFileSync(path.join(path.dirname(source.mainPath), name), path.join(directory, name));
      targets[side] = path.join(directory, path.basename(source.mainPath));
    }
    old = new WpfRpc(path.join(folder, 'baseline-data'), settings);
    const configs = await rpc.call('configs-list', { target: targets.tauri });
    const original = configs.defaultConfig;
    report.originalConfigs = configs.configs.map(row => row.name); assert(configs.configs.length > 1);
    const compare = label => {
      const mappingPath = path.join(folder, 'asset-ids.json'); fs.writeFileSync(mappingPath, JSON.stringify(mappings));
      const result = spawnSync(path.join(production, 'python_runtime/python.exe'), ['-B', path.join(__dirname, 'wpf-output-compare.py'), path.join(folder, 'wpf'), path.join(folder, 'tauri'), mappingPath], { windowsHide: true, encoding: 'utf8' });
      assert.equal(result.status, 0, result.stderr); const comparison = JSON.parse(result.stdout);
      report.steps.push({ label, ...comparison });
      fs.writeFileSync(path.join(root, 'artifacts/wpf-config-parity-report.json'), JSON.stringify(report, null, 2));
      assert.equal(comparison.different.length + comparison.missing.length + comparison.extra.length, 0, JSON.stringify(comparison));
      console.log(label + ': ' + comparison.archives + ' archives, ' + comparison.entries + ' matching entries');
    };
    const mutate = async (label, channel, command, request) => {
      const baseline = await old.call(channel, { ...request, target: targets.wpf });
      const migrated = await rpc.call(command, { ...request, target: targets.tauri });
      if (command === 'native-hair-install') {
        assert(baseline.targetKey && migrated.targetKey);
        const common = request.assetType + '_parity_renamed_fixed_id';
        mappings.wpf[baseline.targetKey] = common; mappings.tauri[migrated.targetKey] = common;
        if (request.assetType === 'facialhair') {
          mappings.wpf[baseline.targetKey.replace(/^facialhair/, 'facialHair')] = common;
          mappings.tauri[migrated.targetKey.replace(/^facialhair/, 'facialHair')] = common;
        }
        report.uniqueAssetIdsNormalized = true;
        // Native extraction retains binary TXTR. Decode that metadata without rewriting the fixture.
        const decodedItems = {};
        for (const [side, result] of [['wpf', baseline], ['tauri', migrated]]) {
          const item = result.outputs.find(file => path.basename(file).includes('_item_' + result.targetKey)); assert(item);
          const decoded = await rpc.call('read-archive', { path: item }); assert.deepEqual(decoded.warnings, []);
          const archiveName = path.basename(item).replace(result.targetKey, common);
          const binaryNames = spawnSync(path.join(production, 'python_runtime/python.exe'), ['-B', '-c',
            "import json,sys,zipfile; z=zipfile.ZipFile(sys.argv[1]); print(json.dumps([n for n in z.namelist() if n.lower().endswith('.txtr') and z.read(n).startswith(bytes.fromhex('91f81fe0'))]))", item], { windowsHide: true, encoding: 'utf8' });
          assert.equal(binaryNames.status, 0, binaryNames.stderr); const binary = new Set(JSON.parse(binaryNames.stdout));
          mappings.documents[side][archiveName] = {}; mappings.sharedDds[side][archiveName] = {};
          decodedItems[side] = {};
          for (const member of decoded.members) {
            const bytes = Buffer.from(member.data, 'base64');
            if (binary.has(member.name)) mappings.documents[side][archiveName][member.name] = JSON.parse(bytes.toString());
            if (/\.dds$/i.test(member.name)) {
              const hash = crypto.createHash('sha256').update(bytes).digest('hex');
              mappings.sharedDds[side][archiveName][member.name] = hash; decodedItems[side][member.name] = hash;
            }
          }
        }
        assert(Object.keys(decodedItems.wpf).length); assert.deepEqual(decodedItems.wpf, decodedItems.tauri);
        report.binaryTxtrDecodedForComparison = true;
      }
      compare(label);
      return { baseline, migrated };
    };
    await mutate('Create config', 'configs:change', 'configs-change', { config: original, action: 'create', name: 'PARITY_CONFIG' });
    await mutate('Clear new config items', 'configs:change', 'configs-change', { config: 'PARITY_CONFIG', action: 'save', items: [] });
    await mutate('Rename config', 'configs:change', 'configs-change', { config: 'PARITY_CONFIG', action: 'rename', name: 'PARITY_RENAMED' });
    await mutate('Set default config', 'configs:change', 'configs-change', { config: 'PARITY_RENAMED', action: 'default' });
    const fit = await rpc.call('body-fit', { target: targets.tauri });
    const changes = { chestTHIN: -.25, chestTHICK: 1.75 };
    const dimension = Object.keys(fit.targetBodyFit).find(key => /spine2.*x/i.test(key));
    assert(dimension); changes[dimension] = -3.5;
    await mutate('Body fit negatives and over-one', 'body:fit-save', 'body-fit-save', { changes });
    for (const assetType of ['hair', 'facialhair']) {
      const catalog = await rpc.call('native-hair-catalog', { assetType });
      const asset = catalog.assets.find(row => row.sourcePng === '06827' && !row.tangentspace) || catalog.assets.find(row => row.sourcePng === '06827');
      assert(asset);
      await mutate('Install scoped ' + assetType, 'native-hair:install', 'native-hair-install', { config: 'PARITY_RENAMED', configScoped: true, assetType, assetId: asset.id, copyItem: true });
    }
    const band = { config: 'PARITY_RENAMED', configScoped: true, mode: 'add', style: 'Stock NBA 2K27 Headband', color: '#2244CC', addLogo: false };
    const beforeBand = {};
    for (const side of ['wpf', 'tauri']) beforeBand[side] = Object.fromEntries(fs.readdirSync(path.join(folder, side)).filter(name => /\.iff$/i.test(name))
      .map(name => [name, crypto.createHash('sha256').update(fs.readFileSync(path.join(folder, side, name))).digest('hex')]));
    await assert.rejects(old.call('headband:apply', { ...band, target: targets.wpf }), /slot is shared/);
    await assert.rejects(rpc.call('headband-apply', { ...band, target: targets.tauri }), /slot is shared/);
    for (const side of ['wpf', 'tauri']) for (const [name, hash] of Object.entries(beforeBand[side])) assert.equal(crypto.createHash('sha256').update(fs.readFileSync(path.join(folder, side, name))).digest('hex'), hash);
    compare('Shared headband overwrite safely rejected');
    report.sharedHeadbandGuardPreserved = true;
    // The stock VCZ bug found in 0.2.11 has a separate fitting-only regression.
    const converted = path.join(root, 'artifacts/highest-hair-output');
    for (const side of ['wpf', 'tauri']) {
      for (const name of fs.readdirSync(converted).filter(name => /^png6720.*\.iff$/i.test(name))) fs.copyFileSync(path.join(converted, name), path.join(folder, side, name));
      targets[side] = path.join(folder, side, 'png6720.iff');
    }
    const convertedConfigs = await rpc.call('configs-list', { target: targets.tauri });
    const installedBand = await mutate('Install 2K27 headband on converted player', 'headband:apply', 'headband-apply', { ...band, config: convertedConfigs.defaultConfig });
    const bufferHashes = async file => {
      const archive = await rpc.call('read-archive', { path: file });
      assert.deepEqual(archive.warnings, []);
      return Object.fromEntries(archive.members.filter(member => /^(VertexBuffer|IndexBuffer)\./i.test(member.name))
        .map(member => [member.name, crypto.createHash('sha256').update(Buffer.from(member.data, 'base64')).digest('hex')]));
    };
    const bandBefore = await bufferHashes(installedBand.migrated.headband);
    assert(Object.keys(bandBefore).some(name => /^VertexBuffer\./i.test(name)));
    const sourceHeadband = path.resolve(root, '../research/nba2k20_native_export_4090/png4090_headband.iff');
    const sourceBandHash = crypto.createHash('sha256').update(fs.readFileSync(sourceHeadband)).digest('hex');
    await mutate('Transfer legacy headband shape to 2K27', 'headband:apply', 'headband-apply', {
      ...band, config: convertedConfigs.defaultConfig, mode: 'transfer', sourceHeadband
    });
    assert.equal(crypto.createHash('sha256').update(fs.readFileSync(sourceHeadband)).digest('hex'), sourceBandHash);
    const bandAfter = await bufferHashes(installedBand.migrated.headband);
    assert(Object.keys(bandBefore).some(name => /^VertexBuffer\./i.test(name) && bandBefore[name] !== bandAfter[name]), 'Source headband transfer must change vertex data');
    for (const name of Object.keys(bandBefore).filter(name => /^IndexBuffer\./i.test(name))) assert.equal(bandAfter[name], bandBefore[name], 'Transfer must preserve target topology');
    report.headbandVerticesChanged = true;
    report.headbandTopologyPreserved = true;
    report.headbandSourcePreserved = true;
    for (const [name, hash] of Object.entries(snapshots)) assert.equal(crypto.createHash('sha256').update(fs.readFileSync(path.join(path.dirname(source.mainPath), name))).digest('hex'), hash);
    report.sourcePreserved = true;
    report.completed = true;
  } finally {
    if (old) { await old.close(); fs.writeFileSync(path.join(folder, 'wpf.log'), old.log); }
    await rpc.close();
    fs.writeFileSync(path.join(root, 'artifacts/wpf-config-parity-report.json'), JSON.stringify(report, null, 2));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
