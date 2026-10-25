const assert = require('assert/strict');
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const { Rpc, root } = require('./rpc-client.cjs');
const hash = p => crypto.createHash('sha256').update(fs.readFileSync(p)).digest('hex');
(async () => {
  const data = fs.mkdtempSync(path.join(root, 'artifacts/service-parity-'));
  const rpc = new Rpc({ env: { CMT_TEST_DATA: data } });
  const report = {};
  let originalLanguage;
  try {
    const runtime = await rpc.call('runtime-info');
    assert.equal(runtime.data.toLowerCase(), data.toLowerCase());
    const initial = await rpc.call('bootstrap');
    const settings = { ...initial.settings, output_dir: path.join(data, 'outputs'), nba2k23_root: initial.settings.nba2k23_root };
    await rpc.call('save-settings', settings);
    const loaded = await rpc.call('bootstrap');
    assert.equal(loaded.settings.output_dir, settings.output_dir);
    const templates = { hair: { png: '06827', name: '', hair: 'dreads' }, invalid: { png: 'oops' } };
    const saved = await rpc.call('templates-save', templates);
    assert.equal(saved.templates.hair.name, 'png06827');
    assert(!saved.templates.invalid);
    assert.deepEqual((await rpc.call('templates-load')).templates, saved.templates);
    fs.writeFileSync(path.join(data, 'target-templates.json'), '{damaged');
    const damaged = await rpc.call('templates-load');
    assert.deepEqual(damaged.templates, {});
    assert(damaged.warning);
    assert.equal(fs.readFileSync(path.join(data, 'target-templates.json'), 'utf8'), '{damaged');
    await rpc.call('templates-save', templates);
    report.settingsAndTemplates = 'roundtrip and corrupt-file recovery passed';
    originalLanguage = await rpc.call('cli-language-get');
    for (const language of ['en', 'fr', originalLanguage.language]) {
      assert.equal((await rpc.call('cli-language-set', { language })).language, language);
      assert.equal((await rpc.call('cli-language-get')).language, language);
    }
    const cli = path.join(data, 'roster/roster.exe');
    assert.equal(hash(cli), '6f09fceb91caf946ff82feeee677256d50958ba9879b01478cef4e6e3d2a3fcc');
    await assert.rejects(rpc.call('cli-set', { field: 'CyberfaceID', value: '-1' }), /between 0 and 65535/);
    await assert.rejects(rpc.call('cli-set', { field: 'PlayerKey', value: '5' }), /read-only/);
    await assert.rejects(rpc.call('cli-language-set', { language: 'invalid' }), /Unsupported language/);
    report.roster = 'verified bundled CLI, language readback and input guards passed';
    const output = path.join(data, 'outputs/keep.iff');
    fs.mkdirSync(path.dirname(output), { recursive: true });
    fs.writeFileSync(output, 'export sentinel');
    const before = hash(output);
    for (const cache of [path.join(data, 'manifest-cache'), path.join(runtime.session, 'hair-cache'), path.join(runtime.session, 'preview-resources')]) {
      fs.mkdirSync(cache, { recursive: true }); fs.writeFileSync(path.join(cache, 'sentinel'), 'test cache');
    }
    const health = await rpc.call('health');
    const application = health.groups.find(g => g.id === 'application');
    for (const check of application.checks) assert.equal(check.status, 'pass', JSON.stringify(check));
    await rpc.call('health-repair', { action: 'clean-caches' });
    assert.equal(hash(output), before);
    assert.deepEqual((await rpc.call('templates-load')).templates, saved.templates);
    await rpc.call('reset');
    assert.equal(hash(output), before);
    report.cleanup = 'health repair and reset preserve exports and templates';
    console.log(JSON.stringify(report, null, 2));
  } finally {
    if (originalLanguage) await rpc.call('cli-language-set', originalLanguage).catch(error => { throw new Error('Could not restore CLI language: ' + error.message); });
    fs.writeFileSync(path.join(root, 'artifacts/settings-roster-report.json'), JSON.stringify(report, null, 2));
    await rpc.close();
    assert.equal(path.dirname(fs.realpathSync(data)), fs.realpathSync(path.join(root, 'artifacts')));
    fs.rmSync(data, { recursive: true });
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
