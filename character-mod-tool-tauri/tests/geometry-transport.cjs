const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawn } = require('node:child_process');
const { chromium } = require('C:/Users/carrn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright');
const { root, currentExe } = require('./rpc-client.cjs');
const baseline = process.argv.includes('--baseline'), argument = process.argv.indexOf('--exe');
const exe = argument >= 0 ? path.resolve(process.argv[argument + 1]) : currentExe();
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
(async () => {
  const child = spawn(exe, ['--ui-test'], { windowsHide: true, stdio: 'ignore' });
  const report = { version: path.basename(exe), executableSha256: crypto.createHash('sha256').update(fs.readFileSync(exe)).digest('hex'), baseline, completed: false, errors: [] };
  let browser, page;
  try {
    for (let i = 0; i < 100; i++) { try { browser = await chromium.connectOverCDP('http://127.0.0.1:9764'); break; } catch { await delay(200); } }
    assert(browser);
    for (let i = 0; i < 100; i++) { page = browser.contexts().flatMap(c => c.pages()).find(p => p.url().includes('tauri.localhost')); if (page) break; await delay(100); }
    assert(page); page.on('pageerror', error => report.errors.push(error.message));
    await page.waitForFunction(() => document.getElementById('startupLoading')?.hidden);
    report.cases = await page.evaluate(async baseline => {
      const api = window.characterTool, rows = [];
      const hash = async value => Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', value)), n => n.toString(16).padStart(2, '0')).join('');
      const summary = async result => {
        const parts = {};
        for (const kind of ['asset', 'base']) {
          const part = result[kind]; if (!part) { parts[kind] = null; continue; }
          const meshes = [];
          for (const mesh of part.meshes) {
            const arrays = {};
            for (const [name, value] of Object.entries(mesh)) if (ArrayBuffer.isView(value)) arrays[name] = { kind: value.constructor.name, bytes: value.byteLength, sha256: await hash(new Uint8Array(value.buffer, value.byteOffset, value.byteLength)) };
            meshes.push({ name: mesh.name, sourceName: mesh.sourceName, groups: mesh.groups, arrays });
          }
          parts[kind] = { meshes, textures: await hash(new TextEncoder().encode(JSON.stringify(part.textures))) };
        }
        return parts;
      };
      for (const [assetType, assetId, includeBase] of [
        ['hair', 'char/sig/png06827_geo_hair_dreads.iff', true],
        ['facialhair', 'char/sig/png06827_geo_facialhair_beard_tangentspace.iff', true],
        ['hair', 'char/sig/png06572_geo_hair_curly_afro_tangentspace.iff', true],
        ['facialhair', 'char/sig/png06827_geo_facialhair_beard_tangentspace.iff', false],
      ]) {
        const request = { assetType, assetId, includeBase };
        const reference = await api.previewNativeHair({ ...request, nativeBinaryGeometry: false });
        if (!reference.stats.nativeGeometry) throw Error('Native JSON reference fell back');
        const expected = await summary(reference), samples = [], jsonSamples = [];
        let latest;
        for (let i = 0; i < 9; i++) {
          const start = performance.now(); latest = await api.previewNativeHair(request); samples.push(performance.now() - start);
          if (!baseline) {
            const jsonStart = performance.now(); const json = await api.previewNativeHair({ ...request, nativeBinaryGeometry: false }); jsonSamples.push(performance.now() - jsonStart);
            if (json.stats.nativeTransport.jsonReplies !== 1) throw Error('Explicit JSON comparison did not use JSON geometry');
          }
        }
        if (JSON.stringify(await summary(latest)) !== JSON.stringify(expected)) throw Error('Geometry, source IDs, UVs, groups or textures changed');
        if (!baseline && latest.stats.nativeTransport.jsonReplies !== 0) throw Error('Geometry still crossed JSON IPC');
        if (!baseline && latest.base && latest.base.meshes[0].positions.buffer === latest.asset.meshes[0].positions.buffer) throw Error('Donor cache would retain the whole asset buffer');
        const beforeHead = latest.base?.meshes[0].positions[0]; latest.asset.meshes[0].positions[0] += 10;
        if (latest.base && latest.base.meshes[0].positions[0] !== beforeHead) throw Error('Asset mutation changed the donor head');
        const reread = await api.previewNativeHair(request);
        if (JSON.stringify(await summary(reread)) !== JSON.stringify(expected)) throw Error('Returned-buffer mutation corrupted a retained native cache');
        rows.push({ assetType, assetId, includeBase, samples, jsonSamples, transport: reread.stats.nativeTransport, workerTransport: reread.stats.workerTransport, headCacheHit: reread.stats.headCacheHit, summary: expected });
      }
      return rows;
    }, baseline);
    if (!baseline) report.fallbacks = await page.evaluate(async () => {
      const core = window.__TAURI__.core, rows = [], pending = new Map();
      const worker = new Worker('preview-worker.bundle.js');
      let sequence = 0, failures = 0;
      worker.onmessage = async ({ data }) => {
        if (data.kind === 'native') {
          try {
            if (data.command === 'native-catalog-geometry') {
              failures++; throw Error('Controlled binary geometry failure');
            }
            const raw = ['read-archive', 'read-file'].includes(data.command);
            const value = await core.invoke(raw ? 'preview_binary' : 'engine_call', {
              command: data.command, payload: { ...data.payload, _cmtPreviewKind: 'independent' },
            });
            worker.postMessage({ kind: 'native-reply', id: data.id, value }, value instanceof ArrayBuffer ? [value] : []);
          } catch (error) { worker.postMessage({ kind: 'native-reply', id: data.id, error: String(error) }); }
        } else {
          const task = pending.get(data.id);
          if (task) { pending.delete(data.id); data.error ? task.reject(Error(JSON.stringify(data.error))) : task.resolve(data.result); }
        }
      };
      worker.onerror = error => { for (const task of pending.values()) task.reject(Error(error.message)); pending.clear(); };
      const decode = payload => new Promise((resolve, reject) => {
        const id = ++sequence; pending.set(id, { resolve, reject }); worker.postMessage({ id, type: 'catalog', payload });
      });
      const hash = async bytes => Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', bytes)), n => n.toString(16).padStart(2, '0')).join('');
      const summarize = async value => {
        const parts = {};
        for (const name of ['asset', 'base']) {
          const part = value[name], meshes = [];
          for (const mesh of part.meshes) {
            const arrays = {};
            for (const [key, array] of Object.entries(mesh)) if (ArrayBuffer.isView(array)) arrays[key] = await hash(new Uint8Array(array.buffer, array.byteOffset, array.byteLength));
            meshes.push({ name: mesh.name, groups: mesh.groups, arrays });
          }
          parts[name] = { meshes, textures: await hash(new TextEncoder().encode(JSON.stringify(part.textures))) };
        }
        return parts;
      };
      try {
        for (const [assetType, assetId] of [
          ['hair', 'char/sig/png06827_geo_hair_dreads.iff'],
          ['facialhair', 'char/sig/png06827_geo_facialhair_beard_tangentspace.iff'],
          ['hair', 'char/sig/png06572_geo_hair_curly_afro_tangentspace.iff'],
        ]) {
          const descriptor = await core.invoke('engine_call', { command: 'native-preview-hair', payload: { assetType, assetId } });
          try {
            const request = { ...descriptor, includeBase: true };
            const expected = await summarize(await decode({ ...request, nativeGeometry: false }));
            failures = 0;
            const result = await decode(request);
            if (failures !== 1 || result.stats.nativeGeometry) throw Error('Binary failure did not use the original decoder: ' + JSON.stringify({ failures, native: result.stats.nativeGeometry }));
            if (!result.asset.skipped.some(message => message.includes('Controlled binary geometry failure'))) throw Error('Fallback lost its diagnostic');
            if (JSON.stringify(await summarize(result)) !== JSON.stringify(expected)) throw Error('Failure fallback changed original-reader output');
            rows.push({ assetId, failures, geometryAndTexturesMatch: true });
          } finally {
            await core.invoke('engine_call', { command: 'release-preview-resources', payload: { lease: descriptor.resourceLease } });
          }
        }
      } finally { worker.terminate(); }
      return rows;
    });
    const median = values => [...values].sort((a,b) => a-b)[Math.floor(values.length/2)];
    report.timings = report.cases.map(row => ({ asset: row.assetId, includeBase: row.includeBase, medianMs: median(row.samples), jsonMedianMs: row.jsonSamples.length ? median(row.jsonSamples) : null, transport: row.transport, workerTransport: row.workerTransport }));
    if (!baseline) {
      const previous = JSON.parse(fs.readFileSync(path.join(root, 'artifacts/geometry-transport-baseline.json'), 'utf8'));
      assert(previous.completed && previous.baseline);
      assert.deepEqual(report.cases.map(row => row.summary), previous.cases.map(row => row.summary), 'Bitwise output differs from the released baseline');
      report.previousVersion = previous.version;
      report.comparison = report.timings.map((row,i) => ({ ...row, priorMedianMs: median(previous.cases[i].samples) }));
    }
    assert.deepEqual(report.errors, []); report.completed = true;
    console.log(JSON.stringify({ ...report, cases: undefined }, null, 2));
  } finally {
    if (page) await page.evaluate(() => window.__TAURI__.core.invoke('engine_call', { command: 'close-app', payload: {} })).catch(() => {});
    await browser?.close().catch(() => {});
    if (child.exitCode === null) await Promise.race([new Promise(resolve => child.once('exit', resolve)), delay(5000)]);
    if (child.exitCode === null) { child.kill(); await new Promise(resolve => child.once('exit', resolve)); }
    fs.writeFileSync(path.join(root, 'artifacts/geometry-transport-' + (baseline ? 'baseline' : 'report') + '.json'), JSON.stringify(report, null, 2));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
