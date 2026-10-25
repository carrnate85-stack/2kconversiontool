const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { spawn, execFileSync } = require('node:child_process');
const { chromium } = require('C:/Users/carrn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright');
const { Rpc, root, currentExe } = require('./rpc-client.cjs');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));

function workers(data) {
  const escaped = data.replace(/'/g, "''");
  const command = `@(Get-CimInstance Win32_Process | Where-Object { ($_.Name -eq 'python.exe' -or $_.Name -eq 'blender.exe') -and $_.CommandLine -and $_.CommandLine.IndexOf('${escaped}', [StringComparison]::OrdinalIgnoreCase) -ge 0 } | Select-Object ProcessId, Name) | ConvertTo-Json -Compress`;
  const output = execFileSync('powershell.exe', ['-NoProfile', '-Command', command], { windowsHide: true, encoding: 'utf8' }).trim();
  const value = output ? JSON.parse(output) : [];
  return Array.isArray(value) ? value : [value];
}

async function check(command, expectedKind) {
  const data = fs.mkdtempSync(path.join(root, 'artifacts/webview-failure-'));
  const outputDir = path.join(data, 'outputs');
  let rpc, child, browser;
  const report = { command };
  try {
    rpc = new Rpc({ env: { CMT_TEST_DATA: data } });
    const sources = {};
    for (const [role, gameLabel, png] of [['source', 'NBA 2K23', '4100'], ['target', 'NBA 2K27', '06827']]) {
      const extracted = await rpc.call('extract', { gameLabel, png });
      const folder = path.join(data, role); fs.mkdirSync(folder);
      for (const name of fs.readdirSync(path.dirname(extracted.mainPath))) {
        const from = path.join(path.dirname(extracted.mainPath), name);
        if (fs.statSync(from).isFile()) fs.copyFileSync(from, path.join(folder, name));
      }
      sources[role] = path.join(folder, path.basename(extracted.mainPath));
    }
    await rpc.close(); rpc = null;
    fs.mkdirSync(outputDir, { recursive: true });
    child = spawn(currentExe(), ['--ui-test'], { windowsHide: true, stdio: 'ignore', env: { ...process.env, CMT_TEST_DATA: data } });
    for (let i = 0; i < 100; i++) {
      try { browser = await chromium.connectOverCDP('http://127.0.0.1:9764'); break; } catch { await delay(200); }
    }
    assert(browser, 'Owned crash-test WebView did not start');
    let page;
    for (let i = 0; i < 100; i++) {
      page = browser.contexts().flatMap(context => context.pages()).find(page => page.url().includes('tauri.localhost'));
      if (page) break; await delay(100);
    }
    assert(page, 'Owned crash-test page not found');
    await page.waitForFunction(() => document.getElementById('engineStatus')?.textContent.includes('Engine '));
    const runtime = await page.evaluate(() => window.__TAURI__.core.invoke('engine_call', { command: 'runtime-info', payload: {} }));
    assert.equal(path.resolve(runtime.data).toLowerCase(), path.resolve(data).toLowerCase());
    const settings = JSON.parse(fs.readFileSync(path.join(data, 'Logs/webview-settings.json'), 'utf8'));
    assert.deepEqual(settings, { contextMenus: false, devtools: false, hostObjects: false, statusBar: false, zoomHotkeys: false, webMessaging: true });
    report.nativeSettings = settings;
    await page.evaluate(async request => {
      window.characterTool.onJobFailed(() => {});
      await window.characterTool.startSwap(request);
    }, { ...sources, sourceGame: 'NBA 2K23', targetGame: 'NBA 2K27', outputDir, shrinkwrapBody: true, hairEnabled: false, facialHairEnabled: false });
    const deadline = Date.now() + 15000;
    let active = [];
    while (!(active = workers(data)).length) { assert(Date.now() < deadline, 'Owned conversion worker did not start'); await delay(100); }
    report.workersBeforeCrash = active;
    const devtools = command === 'Page.crash' ? await page.context().newCDPSession(page) : await browser.newBrowserCDPSession();
    devtools.send(command).catch(() => {});
    const exitDeadline = Date.now() + 30000;
    while (child.exitCode === null) { assert(Date.now() < exitDeadline, 'App did not exit after renderer failure'); await delay(100); }
    assert.equal(child.exitCode, 1, 'Renderer failure must produce a non-success exit');
    const diagnostic = JSON.parse(fs.readFileSync(path.join(data, 'Logs/webview-failure.json'), 'utf8'));
    assert.equal(diagnostic.kind, expectedKind); assert.equal(diagnostic.fatal, true);
    assert.equal(workers(data).length, 0, 'Conversion workers survived fatal renderer shutdown');
    assert(!fs.readdirSync(outputDir).some(name => /^png.*\.iff$/i.test(name)), 'Interrupted conversion published a final player');
    report.exitCode = child.exitCode; report.kind = diagnostic.kind; report.workersAfterCrash = 0; report.finalPlayerPublished = false;
    return report;
  } finally {
    if (rpc) await rpc.close();
    if (child && child.exitCode === null) {
      try { await browser?.contexts()[0]?.pages()[0]?.evaluate(() => window.__TAURI__.core.invoke('engine_call', { command: 'close-app', payload: {} })); } catch {}
      for (let i = 0; i < 100 && child.exitCode === null; i++) await delay(100);
      if (child.exitCode === null) child.kill();
    }
    for (const worker of workers(data)) {
      try { execFileSync('powershell.exe', ['-NoProfile', '-Command', `Stop-Process -Id ${Number(worker.ProcessId)} -Force`], { windowsHide: true }); } catch {}
    }
    await browser?.close().catch(() => {});
    assert.equal(path.dirname(fs.realpathSync(data)), fs.realpathSync(path.join(root, 'artifacts')));
    fs.rmSync(data, { recursive: true });
  }
}

(async () => {
  const report = { checkedUtc: new Date().toISOString(), cases: [] };
  for (const [command, kind] of [['Page.crash', 1], ['Browser.crash', 0]]) report.cases.push(await check(command, kind));
  fs.writeFileSync(path.join(root, 'artifacts/webview-failure-report.json'), JSON.stringify(report, null, 2));
  console.log(JSON.stringify(report, null, 2));
})().catch(error => { console.error(error); process.exitCode = 1; });
