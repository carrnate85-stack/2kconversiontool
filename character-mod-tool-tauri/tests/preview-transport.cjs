const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { spawn } = require('node:child_process');
const { chromium } = require('C:/Users/carrn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright');
const { root, currentExe } = require('./rpc-client.cjs');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
function workerProbe() {
  self.onmessage = async ({ data }) => {
    try {
      const archive = Packet.unpackPreviewPacket(data);
      async function digest(rows) {
        const encoder = new TextEncoder();
        const names = rows.map(row => encoder.encode(row.name));
        const bytes = new Uint8Array(rows.reduce((sum, row, i) => sum + row.size + names[i].length, 0));
        let offset = 0;
        rows.forEach((row, i) => { bytes.set(names[i], offset); offset += names[i].length;
          bytes.set(row.data, offset); offset += row.size; });
        return Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', bytes)))
          .map(value => value.toString(16).padStart(2, '0')).join('');
      }
      const content = await digest(archive.members);
      const geometry = await digest(archive.members.filter(row => !/\.(dds|txtr)$/i.test(row.name)));
      self.postMessage({ members: archive.members.length, rawBytes: archive.members.reduce((sum, row) => sum + row.size, 0),
        contentKey: content, geometryKey: geometry, metadataMatchesBytes: content === archive.contentKey && geometry === archive.geometryKey });
    } catch (error) { self.postMessage({ error: error.message }); }
  };
}
(async () => {
  const child = spawn(currentExe(), ['--ui-test'], { windowsHide: true, stdio: 'ignore' });
  let browser, page;
  const report = { version: path.basename(currentExe()), completed: false };
  try {
    for (let i = 0; i < 100; i++) {
      try { browser = await chromium.connectOverCDP('http://127.0.0.1:9764'); break; } catch { await delay(200); }
    }
    assert(browser);
    for (let i = 0; i < 100; i++) {
      page = browser.contexts().flatMap(context => context.pages()).find(page => page.url().includes('tauri.localhost'));
      if (page) break; await delay(100);
    }
    assert(page);
    await page.waitForFunction(() => document.getElementById('startupLoading')?.hidden, null, { timeout: 30000 });
    const packetCode = require('../build-tools/node_modules/esbuild').buildSync({
      entryPoints: [path.join(root, 'worker/packet.mjs')], bundle: true, write: false,
      format: 'iife', globalName: 'Packet', platform: 'browser',
    }).outputFiles[0].text;
    report.archiveTransport = await page.evaluate(async ({ target, probeSource, baseline }) => {
      const invoke = (command, payload) => window.__TAURI__.core.invoke('engine_call', { command, payload });
      const resources = await invoke('native-preview-resources', { target });
      const paths = [resources.filePath, ...resources.options.companionPaths.map(row => row.path)]
        .filter(path => path.startsWith('cmt-preview://'));
      const samples = [], rows = [];
      for (let pass = 0; pass < 3; pass++) {
        const started = performance.now();
        for (const uri of paths) {
          const archive = await invoke('read-archive', { path: uri });
          if (!pass) rows.push({
            name: uri.split('/').at(-1), members: archive.members.length,
            rawBytes: archive.members.reduce((sum, row) => sum + row.size, 0),
            base64Chars: archive.members.reduce((sum, row) => sum + row.data.length, 0),
            jsonBytes: new TextEncoder().encode(JSON.stringify(archive)).byteLength,
            geometryKey: archive.geometryKey, contentKey: archive.contentKey,
          });
        }
        samples.push(performance.now() - started);
      }
      let binary;
      if (!baseline) {
        const capability = await invoke('runtime-info', {});
        if (!capability.previewBinary) throw new Error('Host does not advertise binary preview transport');
        const url = URL.createObjectURL(new Blob([probeSource], { type: 'text/javascript' }));
        const worker = new Worker(url);
        const samplesMs = [], packets = [];
        try {
          for (let pass = 0; pass < 3; pass++) {
            let elapsed = 0, wireBytes = 0;
            for (let i = 0; i < paths.length; i++) {
              const start = performance.now();
              const packet = await window.__TAURI__.core.invoke('preview_binary', {
                command: 'read-archive', payload: { path: paths[i], _cmtPreviewKind: 'independent' },
              });
              elapsed += performance.now() - start;
              if (!(packet instanceof ArrayBuffer)) throw new Error('Binary response is not an ArrayBuffer');
              wireBytes += packet.byteLength;
              if (!pass) {
                const bytes = packet.byteLength;
                const result = await new Promise((resolve, reject) => {
                  worker.onmessage = ({ data }) => data.error ? reject(new Error(data.error)) : resolve(data);
                  worker.onerror = error => reject(new Error(error.message));
                  worker.postMessage(packet, [packet]);
                  if (packet.byteLength !== 0) reject(new Error('Packet was copied instead of transferred'));
                });
                if (!result.metadataMatchesBytes || result.contentKey !== rows[i].contentKey
                    || result.geometryKey !== rows[i].geometryKey || result.rawBytes !== rows[i].rawBytes) {
                  throw new Error('Binary archive bytes differ from compatibility output: ' + rows[i].name);
                }
                packets.push({ name: rows[i].name, bytes, ...result });
              }
            }
            samplesMs.push(elapsed);
            binary = { samplesMs, wireBytes, packets };
          }
          for (const [command, target] of [['run-swap', paths[0]], ['read-archive', 'C:/not-a-resource.iff'],
            ['read-file', 'cmt-preview://expired/resource.dds']]) {
            let rejected = false;
            try { await window.__TAURI__.core.invoke('preview_binary', { command, payload: { path: target } }); }
            catch { rejected = true; }
            if (!rejected) throw new Error('Binary endpoint accepted an unsafe or expired request');
          }
        } finally { worker.terminate(); URL.revokeObjectURL(url); }
      }
      return { paths: paths.length, samplesMs: samples, rows, binary, cache: await invoke('preview-cache-stats', {}) };
    }, { target: path.resolve(root, '../character-mod-tool-rust/artifacts/conversion-output/png06827.iff'),
      probeSource: packetCode + '\n(' + workerProbe.toString() + ')();', baseline: process.argv.includes('--baseline') });
    if (!process.argv.includes('--baseline')) {
      const baseline = JSON.parse(fs.readFileSync(path.join(root, 'artifacts/preview-transport-0.2.24-baseline.json'), 'utf8'));
      assert.deepEqual(report.archiveTransport.rows, baseline.archiveTransport.rows, 'Archive member sizes, hashes or compatibility bytes changed');
      assert(report.archiveTransport.binary.wireBytes < report.archiveTransport.rows.reduce((sum, row) => sum + row.jsonBytes, 0));
    }
    report.completed = true;
    console.log(JSON.stringify(report, null, 2));
  } finally {
    fs.writeFileSync(path.join(root, 'artifacts/preview-transport-' + (process.argv.includes('--baseline') ? '0.2.24-baseline' : 'report') + '.json'), JSON.stringify(report, null, 2));
    if (page) await page.evaluate(() => window.__TAURI__.core.invoke('engine_call', { command: 'close-app', payload: {} })).catch(() => {});
    if (browser) await browser.close().catch(() => {});
    await delay(500);
    if (child.exitCode === null) child.kill();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
