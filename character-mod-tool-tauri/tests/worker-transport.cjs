const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { spawn } = require('node:child_process');
const { chromium } = require('C:/Users/carrn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright');
const { root, currentExe } = require('./rpc-client.cjs');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const baseline = process.argv.includes('--baseline');
(async () => {
  const child = spawn(currentExe(), ['--ui-test'], { windowsHide: true, stdio: 'ignore' });
  const report = { version: path.basename(currentExe()), completed: false, errors: [] };
  let browser, page;
  try {
    for (let i = 0; i < 100; i++) { try { browser = await chromium.connectOverCDP('http://127.0.0.1:9764'); break; } catch { await delay(200); } }
    assert(browser);
    for (let i = 0; i < 100; i++) { page = browser.contexts().flatMap(c => c.pages()).find(p => p.url().includes('tauri.localhost')); if (page) break; await delay(100); }
    assert(page);
    page.on('pageerror', error => report.errors.push(error.message));
    await page.waitForFunction(() => document.getElementById('startupLoading')?.hidden);
    const measured = await page.evaluate(async target => {
      const invoke = (command, payload) => window.__TAURI__.core.invoke('engine_call', { command, payload });
      const worker = new Worker('preview-worker.bundle.js');
      const pending = new Map();
      let sequence = 0;
      worker.onmessage = async ({ data }) => {
        if (data.kind === 'native') {
          try { worker.postMessage({ kind: 'native-reply', id: data.id,
            value: await invoke(data.command, { ...data.payload, _cmtPreviewKind: 'independent' }) }); }
          catch (error) { worker.postMessage({ kind: 'native-reply', id: data.id, error: String(error) }); }
        } else {
          const request = pending.get(data.id);
          if (request) { pending.delete(data.id); data.error ? request.reject(new Error(JSON.stringify(data.error))) : request.resolve(data.result); }
        }
      };
      worker.onerror = error => { for (const task of pending.values()) task.reject(new Error(error.message)); pending.clear(); };
      const decode = (type, payload) => new Promise((resolve, reject) => {
        const id = ++sequence; pending.set(id, { resolve, reject }); worker.postMessage({ id, type, payload });
      });
      const hash = async bytes => Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', bytes)),
        byte => byte.toString(16).padStart(2, '0')).join('');
      const summarize = async payload => {
        if (!payload) return null;
        const meshes = [];
        for (const mesh of payload.meshes) {
          const arrays = {};
          for (const [key, value] of Object.entries(mesh)) {
            if (!ArrayBuffer.isView(value)) continue;
            arrays[key] = { kind: value.constructor.name, bytes: value.byteLength,
              sha256: await hash(new Uint8Array(value.buffer, value.byteOffset, value.byteLength)) };
          }
          meshes.push({ name: mesh.name, source: mesh.sourceName, groups: mesh.groups, arrays });
        }
        return { meshes, textures: await Promise.all(payload.textures.map(async texture => ({
          name: texture.name, bytes: texture.bytes ? await hash(Uint8Array.from(atob(texture.bytes), x => x.charCodeAt(0))) : '',
          binding: texture.binding, range: texture.range,
        }))) };
      };
      const rows = [];
      let lastStaged;
      let cloneBenchmark;
      try {
        const beard = await invoke('native-preview-hair', {
          assetType: 'facialhair', assetId: 'char/sig/png06827_geo_facialhair_beard_tangentspace.iff',
        });
        for (const [label, options] of [
          ['native first', {}], ['native cached head', {}],
          ['fallback first', { nativeGeometry: false }], ['fallback cached head', { nativeGeometry: false }],
          ['fallback borrowed head', { nativeGeometry: false, includeBase: false }],
          ['fallback head after borrow', { nativeGeometry: false }],
        ]) {
          const start = performance.now();
          const result = await decode('catalog', { ...beard, includeBase: true, ...options });
          rows.push({ label, ms: performance.now() - start, transport: result.stats.workerTransport,
            headCacheHit: result.stats.headCacheHit, frameCache: result.stats.fallbackFrameCache,
            summary: { asset: await summarize(result.asset), base: await summarize(result.base) } });
        }
        const afro = await invoke('native-preview-hair', {
          assetType: 'hair', assetId: 'char/sig/png06572_geo_hair_curly_afro_tangentspace.iff',
        });
        const hair = await decode('catalog', { ...afro, includeBase: true });
        rows.push({ label: 'tangent afro', transport: hair.stats.workerTransport,
          summary: { asset: await summarize(hair.asset), base: await summarize(hair.base) } });
        const resources = await invoke('native-preview-resources', { target });
        for (const label of ['staged first', 'staged repeated']) {
          const start = performance.now();
          const result = await decode('preview', { filePath: resources.filePath, options: resources.options });
          lastStaged = result;
          rows.push({ label, ms: performance.now() - start, transport: result.stats.workerTransport,
            summary: await summarize(result) });
        }
        const buffersOf = object => {
          const buffers = new Set(), seen = new Set();
          const visit = value => {
            if (!value || typeof value !== 'object' || seen.has(value)) return;
            seen.add(value);
            if (ArrayBuffer.isView(value)) { if (value.buffer.byteLength) buffers.add(value.buffer); return; }
            if (value instanceof ArrayBuffer) { if (value.byteLength) buffers.add(value); return; }
            Object.values(value).forEach(visit);
          };
          visit(object); return [...buffers];
        };
        const clone = [], transfer = [];
        for (let i = 0; i < 5; i++) {
          const copiedInput = structuredClone(lastStaged);
          let start = performance.now();
          const copied = structuredClone(copiedInput);
          clone.push(performance.now() - start);
          if (!copied.meshes[0].positions.length) throw new Error('Copy benchmark lost geometry');
          const movedInput = structuredClone(lastStaged);
          const buffers = buffersOf(movedInput);
          start = performance.now();
          const moved = structuredClone(movedInput, { transfer: buffers });
          transfer.push(performance.now() - start);
          if (!moved.meshes[0].positions.length || buffers.some(buffer => buffer.byteLength)) throw new Error('Transfer benchmark did not move ownership');
        }
        const buffers = buffersOf(lastStaged);
        cloneBenchmark = { samples: 5, cloneMs: clone, transferMs: transfer,
          cloneMedianMs: [...clone].sort((a, b) => a - b)[2],
          transferMedianMs: [...transfer].sort((a, b) => a - b)[2],
          arrayBytes: buffers.reduce((sum, buffer) => sum + buffer.byteLength, 0) };
      } finally { worker.terminate(); }
      return { cases: rows, cloneBenchmark };
    }, path.resolve(root, '../character-mod-tool-rust/artifacts/conversion-output/png06827.iff'));
    report.cases = measured.cases;
    report.controlledCloneBenchmark = measured.cloneBenchmark;
    assert.deepEqual(report.errors, []);
    for (const [a, b] of [[0, 1], [2, 3], [2, 5], [7, 8]]) {
      assert.deepEqual(report.cases[a].summary, report.cases[b].summary, 'Repeated transport changed or detached cached geometry');
    }
    assert(report.cases[1].headCacheHit && report.cases[3].headCacheHit && report.cases[5].headCacheHit);
    assert.equal(report.cases[4].summary.base, null);
    assert.equal(report.cases[3].frameCache.builds, report.cases[5].frameCache.builds);
    if (!baseline) {
      const original = JSON.parse(fs.readFileSync(path.join(root, 'artifacts/worker-transport-baseline-0.2.21.json'), 'utf8'));
      assert.deepEqual(report.cases.map(row => row.summary), original.cases.map(row => row.summary), 'Transferred geometry or texture bytes differ from the prior build');
      assert(report.cases.every(row => row.transport?.mode === 'transfer' && row.transport.bytes > 0 && row.transport.buffers > 0));
      report.allPriorGeometryAndTexturesMatch = true;
    }
    report.completed = true;
    console.log(JSON.stringify({ version: report.version, completed: report.completed,
      cases: report.cases.map(({ label, ms, transport, headCacheHit }) => ({ label, ms, transport, headCacheHit })),
      controlledCloneBenchmark: report.controlledCloneBenchmark,
      allPriorGeometryAndTexturesMatch: report.allPriorGeometryAndTexturesMatch }, null, 2));
  } finally {
    if (page) await page.evaluate(() => window.__TAURI__.core.invoke('engine_call', { command: 'close-app', payload: {} })).catch(() => {});
    await browser?.close().catch(() => {});
    if (child.exitCode === null) await Promise.race([new Promise(resolve => child.once('exit', resolve)), delay(5000)]);
    if (child.exitCode === null) child.kill();
    fs.writeFileSync(path.join(root, 'artifacts', baseline ? 'worker-transport-baseline-0.2.21.json' : 'worker-transport-report.json'), JSON.stringify(report, null, 2));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
