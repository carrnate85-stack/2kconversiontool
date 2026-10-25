const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const net = require('node:net');
const crypto = require('node:crypto');
const { spawn, execFileSync } = require('node:child_process');
const { chromium } = require('C:/Users/carrn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright');
const { PNG } = require('C:/Users/carrn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/pngjs');
const { root, currentExe } = require('./rpc-client.cjs');
// Final-build acceptance: node tests/config-accessory-flow.cjs
// --baseline records failed assertions; --report-dir allows a fresh, non-overwriting recheck.
const baseline = process.argv.includes('--baseline');
const exeIndex = process.argv.indexOf('--exe');
const exe = exeIndex < 0 ? currentExe() : path.resolve(process.argv[exeIndex + 1]);
const reportIndex = process.argv.indexOf('--report-dir');
const reportBase = path.join(root, 'artifacts', baseline ? 'config-accessory-flow-baseline' : 'config-accessory-flow');
const reportDir = reportIndex < 0 ? reportBase : path.resolve(process.argv[reportIndex + 1]);
assert(reportDir === reportBase || reportDir.startsWith(reportBase + path.sep), '--report-dir must stay inside this test\'s report folder');
const reportPath = path.join(reportDir, 'report.json');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const digest = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const companionSnapshot = target => Object.fromEntries(fs.readdirSync(path.dirname(target))
  .filter(name => name.toLowerCase() === path.basename(target).toLowerCase() || name.toLowerCase().startsWith(path.basename(target, '.iff').toLowerCase() + '_'))
  .filter(name => /\.iff$/i.test(name)).sort().map(name => [name, digest(path.join(path.dirname(target), name))]));
const assertSnapshot = (target, before) => {
  const after = companionSnapshot(target);
  assert.deepEqual(after, before, 'Original extracted player/companions changed');
};
function directorySnapshot(directory, prefix = '') {
  const result = {};
  for (const entry of fs.readdirSync(directory, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
    const file = path.join(directory, entry.name), relative = prefix + entry.name;
    assert(!entry.isSymbolicLink(), 'Unexpected link in owned player fixture: ' + relative);
    if (entry.isDirectory()) {
      result[relative + '/'] = 'directory';
      Object.assign(result, directorySnapshot(file, relative + '/'));
    } else {
      assert(entry.isFile(), 'Unexpected nonregular fixture file: ' + relative);
      result[relative] = digest(file);
    }
  }
  return result;
}

// ZIP parsing and reference validation are independent of the application's audit/results.
// The legacy reader is used only to decode the game's binary structured documents.
const inspector = String.raw`
import hashlib,json,re,sys,zipfile
from pathlib import Path
sys.path.insert(0,sys.argv[1])
from hair_tools.hair_picker import read_hair_document
target=Path(sys.argv[2])
def sha(data): return hashlib.sha256(data).hexdigest()
def read(file):
    with zipfile.ZipFile(file) as z:
        assert z.testzip() is None, str(file)+' corrupt ZIP'
        names=z.namelist()
        assert len(names)==len(set(n.casefold() for n in names)), str(file)+' duplicate members'
        return {n:z.read(n) for n in names}
main=read(target)
entry=next(n for n in main if Path(n).name.lower()=='appearance_info.json')
appearance=read_hair_document(main[entry])
accessory=appearance['accessory_items']
result={'appearance':appearance,'entryHashes':{n:sha(b) for n,b in main.items()},'appearanceEntry':entry,'references':[],'errors':[]}
def walk(value):
    if isinstance(value,dict):
        yield value
        for child in value.values(): yield from walk(child)
    elif isinstance(value,list):
        for child in value: yield from walk(child)
if len(sys.argv)>3:
    config=next(c for c in accessory['configurations'] if c['name']==sys.argv[3])
    for name in config.get('items',[]):
        item=next(i for i in accessory['items'] if i['name']==name)
        kind=str(item.get('type','')).lower()
        if kind not in ('hair','facialhair') and not kind.startswith('headband'): continue
        key=re.sub('^facialHair','facialhair',name)
        mode=str(item.get('tangentspace','')).lower().strip()
        required=[False] if kind.startswith('headband') and not mode else [True] if mode=='yes' else [False] if mode=='no' else [False,True]
        for tangent in required:
            file=target.with_name(target.stem+'_geo_'+key+('_tangentspace' if tangent else '')+'.iff')
            row={'item':name,'type':kind,'tangent':tangent,'file':file.name,'mesh':item.get('mesh'),'meshlo':item.get('meshlo'),'meshes':[],'errors':[]}
            if not file.is_file(): row['errors'].append('Missing required geometry variant')
            else:
                data=read(file)
                models=[]
                for n,b in data.items():
                    if n.lower().endswith('.scne'):
                        models.extend(node for node in walk(read_hair_document(b)) if node.get('VertexStream') and node.get('Prim'))
                available=set()
                for model in models:
                    for prim in model['Prim']:
                        if prim.get('Mesh'): available.add(prim['Mesh'])
                    for descriptor in walk(model):
                        binary=descriptor.get('Binary')
                        if isinstance(binary,str) and (binary not in data or not data[binary]):
                            row['errors'].append('Missing/empty binary: '+binary)
                row['meshes']=sorted(available)
                fields=('mesh','meshlo') if kind in ('hair','facialhair') or 'meshlo' in item else ('mesh',)
                for field in fields:
                    if not isinstance(item.get(field),str) or not item[field] or item[field] not in available:
                        row['errors'].append('Unresolved '+field+': '+str(item.get(field)))
                if not models: row['errors'].append('No renderable models')
            result['references'].append(row)
            result['errors'].extend(name+': '+error for error in row['errors'])
print(json.dumps(result))
`;

function pixels(bytes) {
  const png = PNG.sync.read(bytes);
  let colored = 0;
  for (let i = 0; i < png.data.length; i += 4) {
    const [r, g, b] = png.data.subarray(i, i + 3);
    if (Math.max(r, g, b) - Math.min(r, g, b) > 20 && Math.max(r, g, b) > 40) colored++;
  }
  return { width: png.width, height: png.height, coloredPixels: colored };
}

async function requireFreePort() {
  await new Promise((resolve, reject) => {
    const server = net.createServer();
    server.once('error', () => reject(Error('CDP 9764 already occupied; refusing to connect to an unrelated window')));
    server.listen(9764, '127.0.0.1', () => server.close(resolve));
  });
}

function verifyPortOwner(pid) {
  const script = `$ErrorActionPreference='Stop'; $connections=@(Get-NetTCPConnection -LocalPort 9764 -State Listen); if (!$connections.Count) { throw 'CDP listener missing' }; foreach ($connection in $connections) { $owner=[int]$connection.OwningProcess; $seen=@{}; while ($owner -ne ${pid}) { if ($seen.ContainsKey($owner)) { throw 'CDP process ancestry cycle' }; $seen[$owner]=$true; $p=Get-CimInstance Win32_Process -Filter ('ProcessId='+$owner); if (!$p -or $p.ParentProcessId -eq 0) { throw 'CDP listener is not owned by this test' }; $owner=[int]$p.ParentProcessId } }; 'owned'`;
  assert.equal(execFileSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', script], { windowsHide: true, encoding: 'utf8', timeout: 20000 }).trim(), 'owned');
}

(async () => {
  assert(fs.existsSync(exe), 'EXE missing: ' + exe);
  assert(!fs.existsSync(reportPath), 'Refusing to overwrite ' + reportPath + '; archive the previous report before rerunning');
  await requireFreePort();
  fs.mkdirSync(reportDir, { recursive: true });
  const reportFd = fs.openSync(reportPath, 'wx');
  const fixture = fs.mkdtempSync(path.join(os.tmpdir(), 'config-accessory-flow-'));
  const fixtureReal = fs.realpathSync(fixture);
  const data = path.join(fixture, 'data');
  const playerDir = path.join(fixture, 'player');
  fs.mkdirSync(playerDir);
  const report = { baseline, exe, exeSha256: digest(exe), fixture, startedUtc: new Date().toISOString(), completed: false,
    mockedUiSteps: [], setupViaPublicApi: ['bootstrap', 'saveSettings', 'extractPlayer', 'setWorkingPlayer'], steps: [], findings: [], errors: [],
    unverifiedFollowups: [{ topic: 'Fury head-bone binding', observation: 'Fury defaults to bone 48; the explicit stock 2K27 style uses bone 49.',
      status: 'Unverified separate follow-up, not proof of a bug or a gap in this generic membership test.' }] };
  let child, browser, page, target, originalDisk, originalConfigs, backend, manifest, manifestHash;
  const originals = [];
  const check = async (label, action) => {
    console.log(label);
    try { const value = await action(); report.steps.push({ label, passed: true, ...(value === undefined ? {} : { value }) }); return true; }
    catch (error) { report.findings.push({ label, error: error.stack || String(error) }); report.steps.push({ label, passed: false }); console.error(label + ': ' + error.message); if (!baseline) throw error; return false; }
  };
  const invoke = (command, payload = {}) => page.evaluate(({ command, payload }) => window.__TAURI__.core.invoke('engine_call', { command, payload }), { command, payload });
  const configs = () => page.evaluate(target => window.characterTool.listConfigs({ target }), target);
  const disk = config => JSON.parse(execFileSync(path.join(backend, 'python_runtime/python.exe'), ['-B', '-c', inspector, backend, target, ...(config ? [config] : [])], { windowsHide: true, encoding: 'utf8', timeout: 60000, maxBuffer: 32 * 1024 * 1024 }));
  const configName = 'CMT_ACCESSORIES';
  async function verifyPreserved() {
    const now = disk();
    const before = originalDisk.appearance.accessory_items, after = now.appearance.accessory_items;
    for (const config of before.configurations) assert.deepEqual(after.configurations.find(row => row.name === config.name), config, 'Original config changed: ' + config.name);
    for (const item of before.items) assert.deepEqual(after.items.find(row => row.name === item.name), item, 'Original accessory definition changed: ' + item.name);
    for (const [key, value] of Object.entries(before)) if (!['items', 'configurations'].includes(key)) assert.deepEqual(after[key], value, 'Original accessory metadata/default changed: ' + key);
    const withoutAccessories = value => Object.fromEntries(Object.entries(value).filter(([key]) => key !== 'accessory_items'));
    assert.deepEqual(withoutAccessories(now.appearance), withoutAccessories(originalDisk.appearance), 'Unrelated appearance/body fields changed');
    for (const [name, hash] of Object.entries(originalDisk.entryHashes)) if (name !== originalDisk.appearanceEntry) assert.equal(now.entryHashes[name], hash, 'Original main-archive member changed: ' + name);
    for (const [name, hash] of Object.entries(report.privateBefore)) if (name !== path.basename(target)) assert.equal(digest(path.join(playerDir, name)), hash, 'Untouched original companion changed: ' + name);
    if (report.newConfigFace) assert.equal(digest(report.newConfigFace.path), report.newConfigFace.sha256, 'New config face package changed during accessory edits');
    for (const source of originals) assertSnapshot(source.target, source.hashes);
    return { configs: before.configurations.length, companionHashes: Object.keys(report.privateBefore).length - 1, originalSources: originals.length };
  }
  async function waitEditor(name) {
    await page.waitForFunction(name => document.getElementById('configEditorName')?.textContent === name && !document.getElementById('saveConfig').disabled, name, { timeout: 30000 });
  }
  async function preview(config, suffix) {
    const requestBefore = await page.evaluate(() => state.preview.requestId);
    await page.locator('#previewConfigSelect').selectOption(config);
    await page.waitForFunction(({ target, config, requestBefore }) => state.preview.requestId > requestBefore && state.preview.loadedPath === target && !state.preview.loading && window.configsEditor.active() === config,
      { target, config, requestBefore }, { timeout: 180000 });
    const summary = await page.evaluate(() => { const stats = window.modelPreview.stats(); return { config: document.getElementById('previewConfig').textContent,
      items: document.getElementById('previewActiveItems').textContent, textures: Number(document.getElementById('previewTextureCount').textContent.replace(/,/g, '')),
      models: Number(document.getElementById('previewMeshCount').textContent.replace(/,/g, '')), stats: { materialTextures: stats.materialTextures, renderer: stats.renderer, decodedTextures: stats.decodedTextures },
      renderedMeshes: (stats.payload?.meshes || []).map(mesh => ({ name: mesh.name, sourceName: mesh.sourceName, vertices: mesh.positions.length / 3, groups: mesh.groups })), bounds: window.modelPreview.bounds(),
      warnings: document.getElementById('previewWarnings').textContent, status: document.getElementById('previewStatus').textContent }; });
    assert.equal(summary.config, config, 'Preview rendered the wrong configuration');
    assert(summary.models > 0 && summary.textures > 0, 'Preview has no decoded models/textures');
    assert([...summary.bounds.min, ...summary.bounds.max].every(Number.isFinite), 'Preview has nonfinite bounds');
    assert(summary.stats.materialTextures.some(row => row.map), 'No rendered material has a texture map');
    const expected = (await configs()).configs.find(row => row.name === config).items;
    const active = summary.items === 'Base player only' ? [] : summary.items.split(' | ');
    const token = name => name.toLowerCase().replace(/[^a-z0-9]+/g, '_').replace(/^_|_$/g, '');
    assert.deepEqual(active.map(token).sort(), expected.map(token).sort(), 'Rendered active accessory membership differs from disk');
    const bytes = await page.locator('#previewCanvas').screenshot({ path: path.join(reportDir, suffix + '.png') });
    const pixelSummary = pixels(bytes); assert(pixelSummary.coloredPixels > 500, 'Preview canvas is blank/untextured');
    return { ...summary, pixels: pixelSummary, canvasSha256: crypto.createHash('sha256').update(bytes).digest('hex') };
  }
  try {
    child = spawn(exe, ['--ui-test'], { windowsHide: true, stdio: 'ignore', env: { ...process.env, CMT_TEST_DATA: data } });
    report.ownedProcessId = child.pid;
    let spawnError; child.once('error', error => { spawnError = error; });
    for (let i = 0; i < 150; i++) {
      if (spawnError) throw spawnError;
      if (child.exitCode !== null) throw Error('Owned EXE exited before CDP startup');
      try {
        const response = await fetch('http://127.0.0.1:9764/json/version');
        if (response.ok) { verifyPortOwner(child.pid); browser = await chromium.connectOverCDP('http://127.0.0.1:9764'); break; }
      } catch (error) { if (String(error).includes('not owned')) throw error; }
      await delay(200);
    }
    assert(browser, 'Owned UI-test CDP did not start');
    for (let i = 0; i < 100; i++) {
      const pages = browser.contexts().flatMap(context => context.pages()).filter(candidate => candidate.url().includes('tauri.localhost'));
      assert(pages.length <= 1, 'Ambiguous Tauri test pages');
      if (pages.length === 1) { page = pages[0]; break; } await delay(100);
    }
    assert(page, 'Owned Tauri page missing'); page.setDefaultTimeout(30000);
    page.on('pageerror', error => report.errors.push(error.message));
    await page.waitForFunction(() => document.getElementById('engineStatus')?.textContent.includes('Engine ') && document.getElementById('startupLoading')?.hidden, null, { timeout: 60000 });
    const boot = await page.evaluate(() => window.characterTool.bootstrap());
    const runtime = await invoke('runtime-info');
    assert.equal(path.resolve(runtime.data).toLowerCase(), path.resolve(data).toLowerCase(), 'EXE did not isolate CMT_TEST_DATA');
    manifest = path.join(boot.settings.game_root_2k27, 'manifest'); manifestHash = digest(manifest);
    await page.evaluate(async ({ settings, output }) => window.characterTool.saveSettings({ ...settings, output_dir: output }), { settings: boot.settings, output: path.join(fixture, 'extracted') });
    const source = await page.evaluate(() => window.characterTool.extractPlayer({ gameLabel: 'NBA 2K27', png: '04090' }));
    assert(path.resolve(source.mainPath).startsWith(path.resolve(fixture) + path.sep), 'Extraction escaped owned fixture');
    originals.push({ target: source.mainPath, hashes: companionSnapshot(source.mainPath) });
    const donor = await page.evaluate(() => window.characterTool.extractPlayer({ gameLabel: 'NBA 2K27', png: '06827' }));
    assert(path.resolve(donor.mainPath).startsWith(path.resolve(fixture) + path.sep), 'Donor extraction escaped owned fixture');
    originals.push({ target: donor.mainPath, hashes: companionSnapshot(donor.mainPath) });
    for (const name of Object.keys(originals[0].hashes)) fs.copyFileSync(path.join(path.dirname(source.mainPath), name), path.join(playerDir, name), fs.constants.COPYFILE_EXCL);
    target = path.join(playerDir, path.basename(source.mainPath));
    backend = path.dirname(path.dirname((await page.evaluate(() => window.characterTool.bootstrap())).settings.head_swap_tool));
    report.inspectorBackend = backend;
    const canonical = value => path.toNamespacedPath(fs.realpathSync.native(value)).toLowerCase();
    assert(canonical(backend).startsWith(canonical(data) + path.sep), 'Archive inspector must use the owned packaged runtime: ' + backend);
    assert(fs.existsSync(path.join(backend, 'python_runtime/python.exe')), 'Cannot locate packaged independent archive reader');
    originalDisk = disk(); originalConfigs = await configs();
    assert(originalConfigs.configs.length > 1, 'Acceptance fixture must have multiple configurations');
    report.originalConfigs = originalConfigs; report.privateBefore = companionSnapshot(target);
    report.sourceHashes = originals; report.manifestBefore = manifestHash;
    await page.evaluate(async target => { await setWorkingPlayer(target); }, target);
    await page.locator('#configsTab').click(); await waitEditor(originalConfigs.defaultConfig);
    await check('Create and rename a config through real controls', async () => {
      await page.locator('#newConfig').click();
      await page.locator('dialog.config-create input[name="name"]').fill('CMT_ACCEPTANCE');
      await page.locator('dialog.config-create select[name="base"]').selectOption(originalConfigs.defaultConfig);
      await page.locator('dialog.config-create button[type="submit"]').click(); await waitEditor('CMT_ACCEPTANCE');
      await page.locator('#renameConfig').click();
      await page.locator('dialog.config-create input[name="name"]').fill(configName);
      await page.locator('dialog.config-create button[type="submit"]').click(); await waitEditor(configName);
      for (const id of ['configHair', 'configFacialhair', 'configHeadband']) await page.locator('#' + id).selectOption('');
      for (const checkbox of await page.locator('#configOther input:checked').all()) await checkbox.uncheck();
      await page.locator('#saveConfig').click();
      await page.waitForFunction(() => document.getElementById('configsStatus').textContent.startsWith('Saved ') && !document.getElementById('saveConfig').disabled);
      const saved = await configs(); assert.deepEqual(saved.configs.find(row => row.name === configName).items, []);
      assert.equal(saved.defaultConfig, originalConfigs.defaultConfig);
      const face = path.join(playerDir, path.basename(target, '.iff') + '_config_' + configName.toLowerCase() + '.iff');
      assert.equal(digest(face), report.privateBefore[path.basename(target, '.iff') + '_config_' + originalConfigs.defaultConfig.toLowerCase() + '.iff']);
      report.newConfigFace = { path: face, sha256: digest(face) };
      return verifyPreserved();
    });
    assert((await configs()).configs.some(row => row.name === configName), 'New config unavailable; dependent steps cannot run');
    for (const assetType of ['hair', 'facialhair']) {
      await check('Install config-only native ' + assetType + ' through real controls', async () => {
        await page.locator('#configsTab').click(); await waitEditor(configName);
        await page.locator(assetType === 'hair' ? '#configChooseHair' : '#configChooseBeard').click();
        await page.waitForFunction(assetType => !state.nativeHair.loading && !state.nativeHair.targetLoading && state.nativeHair.assetType === assetType && state.nativeHair.assets.length > 0, assetType, { timeout: 120000 });
        const asset = await page.evaluate(assetType => state.nativeHair.assets.find(row => row.sourcePng === '06827' && row.hasItem && (assetType !== 'hair' || !row.tangentspace)), assetType);
        assert(asset, 'Required real manifest donor not available');
        report[assetType + 'Donor'] = asset;
        await page.locator('#nativeHairSearch').fill(asset.sourcePng);
        const row = page.locator('#nativeHairRows tr[data-asset-id=' + JSON.stringify(asset.id) + ']');
        const exactRow = page.locator('#nativeHairRows tr').filter({ hasText: 'png' + asset.sourcePng });
        const selected = await exactRow.evaluateAll((rows, id) => rows.filter(row => row.dataset.assetId === id).length, asset.id);
        assert.equal(selected, 1, 'Manifest donor is not represented by exactly one real row');
        await row.click();
        await page.waitForFunction(id => !state.nativeHair.activePreviewKey && state.nativeHair.displayedPreviewKey === state.nativeHair.assetType + ':' + id, asset.id, { timeout: 180000 });
        await page.waitForFunction(() => !document.getElementById('installNativeHair').disabled);
        if (assetType === 'hair') await page.locator('#nativeHairCopyItem').check();
        await page.locator('#installNativeHair').click();
        await page.waitForFunction(() => !state.nativeHair.loading && /success|error/.test(document.getElementById('nativeHairStatus').className), null, { timeout: 180000 });
        const status = await page.locator('#nativeHairStatus').innerText();
        assert((await page.locator('#nativeHairStatus').getAttribute('class')).includes('success'), status);
        const saved = await configs(); const active = saved.configs.find(row => row.name === configName);
        const added = saved.items.filter(item => active.items.includes(item.name) && item.type.toLowerCase() === assetType);
        assert.equal(added.length, 1, 'Expected one new ' + assetType + ' in selected config');
        assert(!originalConfigs.items.some(item => item.name === added[0].name), 'Install reused an original shared item');
        report[assetType + 'Installed'] = added[0];
        await verifyPreserved(); return { item: added[0], status };
      });
      await check('Independently inspect required regular/tangent primary/distance ' + assetType + ' references', async () => {
        const inspected = disk(configName);
        const rows = inspected.references.filter(row => row.type === assetType);
        assert(rows.length > 0, 'No installed ' + assetType + ' references to inspect');
        report[assetType + 'References'] = rows;
        assert.deepEqual(rows.flatMap(row => row.errors.map(error => row.file + ': ' + error)), []);
        return rows;
      });
    }
    await check('Assign existing native headband through real Configs membership controls', async () => {
      await page.locator('#configsTab').click(); await waitEditor(configName);
      const existing = originalConfigs.items.find(item => item.name === 'headband' && item.type.toLowerCase() === 'headband');
      assert(existing, 'Expected existing native headband definition is missing');
      assert(originalConfigs.configs.some(config => config.items.includes(existing.name)), 'Fixture must retain an original config using the native headband');
      const before = (await configs()).configs.find(config => config.name === configName).items;
      assert(!before.includes(existing.name), 'Positive case must add headband membership');
      const filesBefore = directorySnapshot(playerDir);
      await page.locator('#configHeadband').selectOption(existing.name);
      await page.locator('#saveConfig').click();
      await page.waitForFunction(() => document.getElementById('configsStatus').textContent.startsWith('Saved ') && !document.getElementById('saveConfig').disabled);
      const saved = await configs();
      assert.deepEqual([...saved.configs.find(config => config.name === configName).items].sort(), [...before, existing.name].sort());
      const filesAfter = directorySnapshot(playerDir);
      assert.deepEqual(Object.keys(filesAfter), Object.keys(filesBefore), 'Membership save added or removed fixture files');
      for (const [name, hash] of Object.entries(filesBefore)) if (name !== path.basename(target)) assert.equal(filesAfter[name], hash, 'Membership save changed a companion: ' + name);
      report.nativeHeadbandMembership = { item: existing.name, geometryReusedWithoutRecolor: true };
      await verifyPreserved(); return report.nativeHeadbandMembership;
    });
    await check('Shared stock headband recolor is rejected with every fixture file unchanged', async () => {
      await page.locator('#configsTab').click(); await waitEditor(configName);
      await page.locator('#configAddHeadband').click();
      await page.waitForFunction(() => !state.headband.loading && !document.getElementById('addHeadband').disabled, null, { timeout: 90000 });
      await page.locator('#headbandConfig').selectOption(configName);
      await page.locator('#headbandStyle').selectOption('Stock NBA 2K27 Headband');
      await page.locator('#headbandColor').fill('#2244cc');
      if (await page.locator('#headbandLogo').isChecked()) await page.locator('label[for="headbandLogo"]').click();
      assert.equal(await page.locator('#headbandLogo').isChecked(), false);
      const before = directorySnapshot(playerDir);
      await page.locator('#addHeadband').click();
      await page.waitForFunction(() => !state.headband.loading && !state.headband.applying && /success|error/.test(document.getElementById('headbandStatus').className), null, { timeout: 180000 });
      const status = await page.locator('#headbandStatus').innerText();
      assert((await page.locator('#headbandStatus').getAttribute('class')).includes('error'), 'Shared stock recolor must be rejected: ' + status);
      assert.match(status, /native headband slot is shared with:/i, 'Rejection must come from shared-slot protection');
      assert.deepEqual(directorySnapshot(playerDir), before, 'Rejected stock recolor changed fixture files');
      report.sharedStockRecolorGuard = { rejected: true, status, unchangedFiles: before };
      await verifyPreserved(); return { rejected: true, status, unchangedEntries: Object.keys(before).length };
    });
    await check('Disk membership, defaults and native headband geometry references', async () => {
      const saved = await configs(); report.finalConfigs = saved;
      const inspected = disk(configName); report.finalReferences = inspected.references;
      const active = inspected.appearance.accessory_items.configurations.find(row => row.name === configName);
      assert.deepEqual(saved.configs.find(row => row.name === configName).items, active.items);
      for (const kind of ['hair', 'facialhair', 'headband']) {
        const items = inspected.appearance.accessory_items.items.filter(item => active.items.includes(item.name) && (kind === 'headband' ? item.type.toLowerCase().startsWith(kind) : item.type.toLowerCase() === kind));
        assert.equal(items.length, 1, 'Expected exactly one selected ' + kind);
        if (kind === 'headband') assert.equal(items[0].name, report.nativeHeadbandMembership?.item, 'Headband must reuse the selected native definition');
        else for (const config of originalDisk.appearance.accessory_items.configurations) assert(!config.items.includes(items[0].name), 'Installed accessory leaks into original config');
      }
      // Hair/beard references have their own strict checks above, avoiding duplicate baseline findings.
      const headbands = inspected.references.filter(row => row.type.startsWith('headband'));
      assert(headbands.length > 0, 'Selected native headband has no inspected geometry');
      assert.deepEqual(headbands.flatMap(row => row.errors), []); return verifyPreserved();
    });
    await check('Real textured Preview and original/new/original config switching', async () => {
      await page.locator('#previewTab').click();
      await page.waitForFunction(() => !state.preview.loading && state.preview.loadedPath, null, { timeout: 180000 });
      report.previewOriginal = await preview(originalConfigs.defaultConfig, 'preview-original');
      const nativeHeadbandConfig = originalConfigs.configs.find(config => config.items.includes(report.nativeHeadbandMembership.item));
      assert(nativeHeadbandConfig, 'Original native headband config is unavailable for material comparison');
      report.previewNativeHeadbandOriginal = await preview(nativeHeadbandConfig.name, 'preview-native-headband-original');
      report.previewNew = await preview(configName, 'preview-new');
      const newItems = (await configs()).configs.find(row => row.name === configName).items;
      for (const name of newItems) {
        const key = name.replace(/^facialHair/, 'facialhair').toLowerCase();
        const meshes = report.previewNew.renderedMeshes.filter(mesh => String(mesh.sourceName).toLowerCase().includes('_geo_' + key));
        assert(meshes.some(mesh => mesh.vertices > 0), 'Installed accessory has no rendered geometry: ' + name);
        const materials = new Set(meshes.flatMap(mesh => (mesh.groups || []).map(group => group.material)));
        const bound = report.previewNew.stats.materialTextures.filter(row => materials.has(row.name));
        assert(bound.length > 0, 'Selected accessory has no rendered material: ' + name);
        if (name === report.nativeHeadbandMembership.item) {
          // Reusing a stock slot must preserve its authored material, not invent a texture override.
          const originalMeshes = report.previewNativeHeadbandOriginal.renderedMeshes.filter(mesh => String(mesh.sourceName).toLowerCase().includes('_geo_' + key));
          const originalMaterials = new Set(originalMeshes.flatMap(mesh => (mesh.groups || []).map(group => group.material)));
          const originalBound = report.previewNativeHeadbandOriginal.stats.materialTextures.filter(row => originalMaterials.has(row.name));
          assert(originalBound.length > 0, 'Original native headband has no rendered material');
          const surface = rows => rows.map(({ name, map, normalMap, color, roughness, metalness }) => ({ name, map, normalMap, color, roughness, metalness }));
          assert.deepEqual(surface(bound), surface(originalBound), 'Reused native headband material differs from its original config');
          report.nativeHeadbandRender = { originalConfig: nativeHeadbandConfig.name, materialPreserved: true, materials: surface(bound) };
        } else assert(bound.some(row => row.map), 'Installed accessory lacks a rendered texture: ' + name);
      }
      assert.notEqual(report.previewOriginal.canvasSha256, report.previewNew.canvasSha256, 'Config switch did not change rendered pixels');
      const canvas = page.locator('#previewCanvas'), before = await canvas.screenshot();
      const box = await canvas.boundingBox();
      await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2); await page.mouse.down();
      await page.mouse.move(box.x + box.width / 2 + 80, box.y + box.height / 2, { steps: 10 }); await page.mouse.up();
      await delay(300); assert(!before.equals(await canvas.screenshot()), 'Real Preview does not rotate');
      report.previewOriginalAgain = await preview(originalConfigs.defaultConfig, 'preview-original-again');
      await verifyPreserved(); return { configSwitches: 4, texturedAccessories: newItems.filter(name => name !== report.nativeHeadbandMembership.item), nativeHeadbandMaterialPreserved: true, rotation: true };
    });
    await check('Originals and untouched companions unchanged after all UI work', verifyPreserved);
    await check('No page errors', () => assert.deepEqual(report.errors, []));
    report.completed = report.findings.length === 0;
  } catch (error) {
    report.fatal = error.stack || String(error); if (!baseline) process.exitCode = 1;
  } finally {
    try {
      for (const source of originals) assertSnapshot(source.target, source.hashes);
      report.extractedOriginalsUnchangedBeforeAppExit = originals.length > 0;
    } catch (error) { report.originalVerificationError = error.stack || String(error); process.exitCode = 1; }
    if (page) {
      report.lastUi = await page.evaluate(() => ({ configs: document.getElementById('configsStatus')?.textContent, nativeHair: document.getElementById('nativeHairStatus')?.textContent,
        headband: document.getElementById('headbandStatus')?.textContent, preview: document.getElementById('previewStatus')?.textContent })).catch(() => null);
      await page.screenshot({ path: path.join(reportDir, 'last-ui.png') }).catch(() => {});
      await invoke('close-app').catch(() => {});
    }
    if (child?.pid && child.exitCode === null && child.signalCode === null) {
      await new Promise(resolve => { const timer = setTimeout(() => { child.kill(); }, 10000); child.once('exit', () => { clearTimeout(timer); resolve(); }); });
    }
    if (browser) await browser.close().catch(() => {});
    try {
      assert(!child?.pid || child.exitCode !== null || child.signalCode !== null, 'Owned application has not exited; refusing fixture cleanup');
      report.ownedProcessExited = !!child?.pid && (child.exitCode !== null || child.signalCode !== null);
      if (manifestHash) assert.equal(digest(manifest), manifestHash, 'Game manifest changed');
      assert.equal(digest(exe), report.exeSha256, 'Published EXE changed during test');
      report.originalsAndManifestUnchanged = report.extractedOriginalsUnchangedBeforeAppExit === true;
      assert.equal(fs.realpathSync(fixture), fixtureReal); assert(!fs.lstatSync(fixture).isSymbolicLink());
      assert.equal(path.dirname(fixtureReal), fs.realpathSync(os.tmpdir()));
      assert(path.basename(fixtureReal).startsWith('config-accessory-flow-'));
      fs.rmSync(fixtureReal, { recursive: true }); report.ownedFixtureCleanedAfterAppExit = true;
    } catch (error) { report.cleanupError = error.stack || String(error); process.exitCode = 1; }
    report.completed = report.completed && !report.fatal && !report.cleanupError && !report.originalVerificationError;
    report.finishedUtc = new Date().toISOString();
    fs.writeFileSync(reportFd, JSON.stringify(report, null, 2)); fs.closeSync(reportFd);
    console.log(JSON.stringify({ completed: report.completed, baseline, findings: report.findings.map(row => ({ label: row.label, error: row.error.split('\n')[0] })), fatal: report.fatal?.split('\n')[0], reportPath }, null, 2));
    if (report.fatal || (!baseline && report.findings.length)) process.exitCode = 1;
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
