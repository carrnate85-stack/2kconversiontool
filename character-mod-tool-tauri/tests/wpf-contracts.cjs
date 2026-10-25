const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const esbuild = require('../build-tools/node_modules/esbuild');
const root = path.resolve(__dirname, '..');
const reference = path.join(root, '../character-mod-tool-webview2/dist/Character-Mod-Tool-WebView2-0.1.31/App/Reference/src');
let original;
vm.runInNewContext(fs.readFileSync(path.join(reference, 'preload.js'), 'utf8'), {
  require: name => {
    assert.equal(name, 'electron');
    return { contextBridge: { exposeInMainWorld: (_key, value) => { original = value; } }, ipcRenderer: {} };
  }
});
const calls = [];
const window = { __TAURI__: {
  event: { listen: async () => {} },
  core: { invoke: async (_name, request) => {
    calls.push(request);
    return request.command === 'runtime-info' ? { smoke: false } : { contract: request.command };
  } }
} };
const script = esbuild.buildSync({ entryPoints: [path.join(root, 'web/tauri-bridge.ts')], bundle: true, write: false, platform: 'browser', format: 'iife' }).outputFiles[0].text;
vm.runInNewContext(script, { window, console, structuredClone, setInterval, clearInterval, setTimeout });
const api = window.characterTool;
const delegates = {
  runHealthCheck: 'health', listConfigs: 'configs-list', changeConfig: 'configs-change',
  loadHairOptions: 'hair-options', loadFacialHairOptions: 'facial-hair-options',
  loadNativeHairTarget: 'native-hair-target', installNativeHair: 'native-hair-install',
  loadBodyFit: 'body-fit', saveBodyFit: 'body-fit-save', applyChestFix: 'body-chest-fix',
  loadHeadbandOptions: 'headband-options', applyHeadband: 'headband-apply',
  applyPreviewChestCorrection: 'preview-chest-fix', readRosterPlayer: 'cli-read',
  setRosterPlayerField: 'cli-set', getRosterLanguage: 'cli-language-get', setRosterLanguage: 'cli-language-set',
  saveSettings: 'save-settings', loadTemplates: 'templates-load', saveTemplates: 'templates-save'
};
(async () => {
  const missing = Object.keys(original).filter(key => typeof api[key] !== 'function');
  assert.deepEqual(missing, [], 'Every WPF public API must remain available');
  for (const [name, command] of Object.entries(delegates)) {
    const payload = { target: 'C:/test/player.iff', changes: { chestTHIN: -0.25 }, language: 'en' };
    const result = await api[name](payload);
    assert.equal(result.contract, command, name);
    if (!['runHealthCheck', 'getRosterLanguage', 'loadTemplates'].includes(name)) assert.equal(calls.at(-1).payload, payload, name + ' payload must not be rewritten');
  }
  await api.chooseCharacter('source');
  assert.equal(calls.at(-1).command, 'choose-file');
  assert.equal(calls.at(-1).payload.exe, undefined);
  await api.chooseHair(); assert.equal(calls.at(-1).payload.iffOnly, true);
  for (const field of ['settings26', 'settings27', 'settingsBlender', 'settingsAddon', 'settingsMdt', 'settingsOutput']) {
    await api.chooseSetting({ field, currentPath: 'C:/existing' });
    assert.equal(calls.at(-1).command, field === 'settingsBlender' ? 'choose-file' : 'choose-folder');
    assert.equal(calls.at(-1).payload.current, 'C:/existing');
  }
  await assert.rejects(api.chooseSetting({ field: 'invalid', currentPath: '' }), /Unknown settings/);
  await assert.rejects(api.launchLegacyApp(), /production launcher/);
  const originalHtml = fs.readFileSync(path.join(reference, 'renderer/index.html'), 'utf8');
  const builtHtml = fs.readFileSync(path.join(root, 'frontend/index.html'), 'utf8');
  const ids = html => [...html.matchAll(/\bid="([^"]+)"/g)].map(match => match[1]);
  assert.deepEqual(ids(builtHtml), ids(originalHtml), 'WPF controls and tabs must not disappear from the Tauri frontend');
  const report = { baseline: 'WPF 0.1.31', host: 'Tauri ' + JSON.parse(fs.readFileSync(path.join(root, 'tauri.conf.json'))).version, checkedUtc: new Date().toISOString(), apiMethods: Object.keys(original).length,
    missing, delegatedContracts: Object.keys(delegates).length, preservedControlIds: ids(originalHtml).length,
    intentionalDifferences: ['Legacy/backend launcher is disabled as requested.', 'Native Rust previews and approved strand rendering replace the older renderer.'],
    runtimeTestsStillRequired: ['Native file-picker interaction', 'Live game roster writes', 'In-game acceptance of converted players'] };
  fs.writeFileSync(path.join(root, 'artifacts/wpf-contracts-report.json'), JSON.stringify(report, null, 2));
  console.log(JSON.stringify(report, null, 2));
})().catch(error => { console.error(error); process.exitCode = 1; });
