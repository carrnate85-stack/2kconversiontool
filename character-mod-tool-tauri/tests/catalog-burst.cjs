const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { spawn } = require('node:child_process');
const { chromium } = require('C:/Users/carrn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright');
const { root, currentExe } = require('./rpc-client.cjs');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const baseline = process.argv.includes('--baseline');
(async () => {
  const exeArgument = process.argv.indexOf('--exe');
  if (baseline) assert(exeArgument >= 0, 'Pass --exe explicitly to record a comparison build');
  const exe = exeArgument >= 0 ? path.resolve(process.argv[exeArgument + 1]) : currentExe();
  const baselineStem = 'catalog-burst-' + path.basename(exe).replace(/^Character-Mod-Tool-Tauri-Preview-/, '').replace(/\.exe$/i, '') + '-baseline';
  const child = spawn(exe, ['--ui-test'], { windowsHide: true, stdio: 'ignore' });
  const report = { version: path.basename(exe), baseline, completed: false, errors: [] };
  let browser, page;
  try {
    for (let i = 0; i < 100; i++) {
      try { browser = await chromium.connectOverCDP('http://127.0.0.1:9764'); break; } catch { await delay(200); }
    }
    assert(browser);
    for (let i = 0; i < 100; i++) {
      page = browser.contexts().flatMap(c => c.pages()).find(p => p.url().includes('tauri.localhost'));
      if (page) break;
      await delay(100);
    }
    assert(page);
    page.on('pageerror', error => report.errors.push(error.message));
    await page.waitForFunction(() => document.getElementById('startupLoading')?.hidden, null, { timeout: 30000 });
    report.burst = await page.evaluate(async baseline => {
      const api = window.characterTool;
      const OriginalWorker = window.Worker;
      let traffic = [];
      window.Worker = class extends OriginalWorker {
        constructor(...args) {
          super(...args);
          this.addEventListener('message', ({ data }) => {
            if (data.kind === 'native' && data.command === 'native-catalog-geometry') traffic.push('geometry');
            if (data.result) traffic.push('decoded');
            if (data.error) traffic.push('rejected');
          });
        }
        postMessage(message, transfer) {
          if (message.type === 'catalog') traffic.push(message.latestOnly ? 'latest-request' : 'ordinary-request');
          return super.postMessage(message, transfer);
        }
      };
      const digest = async value => {
        const bytes = typeof value === 'string' ? new TextEncoder().encode(value)
          : new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
        return Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', bytes)), n => n.toString(16).padStart(2, '0')).join('');
      };
      const fingerprint = async preview => {
        const result = {};
        for (const part of ['asset', 'base']) {
          result[part] = preview[part] ? {
            meshes: await Promise.all(preview[part].meshes.map(async mesh => ({
              positions: await digest(mesh.positions), uvs: await digest(mesh.uvs),
              indices: await digest(mesh.indices), sourceVertexIds: await digest(mesh.sourceVertexIds),
              groups: mesh.groups,
            }))),
            textures: await digest(JSON.stringify(preview[part].textures)),
          } : null;
        }
        return result;
      };
      const requests = [
        'char/sig/png06827_geo_hair_dreads.iff',
        'char/sig/png06827_geo_hair_dreads_tangentspace.iff',
        'char/sig/png06572_geo_hair_curly_afro_tangentspace.iff',
        'char/sig/png00143_geo_hair_straight.iff',
      ].map(assetId => ({ assetType: 'hair', assetId, includeBase: true }));
      let expected;
      for (const request of requests) expected = await fingerprint(await api.previewNativeHair(request));
      const samples = [];
      try {
        for (let repeat = 0; repeat < 3; repeat++) {
          traffic = [];
          const started = performance.now();
          const results = await Promise.allSettled(Array.from({ length: 12 }, (_, i) =>
            api.previewNativeHair(requests[i % requests.length])));
          const elapsedMs = performance.now() - started;
          const latest = results.at(-1);
          if (latest.status !== 'fulfilled') throw new Error('Latest selection failed: ' + latest.reason);
          if (results.slice(0, -1).some(result => result.status !== 'rejected')) throw new Error('An obsolete API selection was published');
          if (JSON.stringify(await fingerprint(latest.value)) !== JSON.stringify(expected)) throw new Error('Latest geometry, UVs or textures changed');
          const count = value => traffic.filter(item => item === value).length;
          const sample = { elapsedMs, requests: count('latest-request') + count('ordinary-request'),
            latestRequests: count('latest-request'), decoded: count('decoded'), rejectedBeforeDecode: count('rejected'),
            geometryRequests: count('geometry'), latestHeadCacheHit: latest.value.stats.headCacheHit,
            latestStats: latest.value.stats };
          if (!baseline && (sample.requests !== 12 || sample.decoded > 2 || sample.rejectedBeforeDecode < 10
            || sample.latestRequests !== 12 || !sample.latestHeadCacheHit)) throw new Error('Obsolete selections still decoded: ' + JSON.stringify(sample));
          samples.push(sample);
        }
        const revisit = await api.previewNativeHair({ assetType: 'hair', assetId: requests[1].assetId, includeBase: false });
        if (!revisit.stats.headCacheHit || !revisit.stats.tangentFrameCacheHit || revisit.base !== null) throw new Error('Donor head/frame cache did not survive the burst');
        const pending = Array.from({ length: 12 }, (_, i) => api.previewNativeHair(requests[i % requests.length]));
        const settled = Promise.allSettled(pending);
        await api.cancelNativeHairPreview();
        if ((await settled).some(result => result.status !== 'rejected')) throw new Error('Cancelled burst published an obsolete selection');
        const resumed = await api.previewNativeHair(requests.at(-1));
        if (!resumed.stats.headCacheHit || JSON.stringify(await fingerprint(resumed)) !== JSON.stringify(expected)) throw new Error('Cancelled burst did not resume with intact output and native head cache');
        return { samples, latestOutputUnchanged: true, donorHeadAndFramesRetained: true,
          cancelDrainAndResume: true, cache: await api.previewCacheStats() };
      } finally { window.Worker = OriginalWorker; }
    }, baseline);
    report.scopeBarrier = await page.evaluate(async ({ target, baseline }) => {
      const api = window.characterTool;
      const originalCore = window.__TAURI__.core;
      const originalInvoke = originalCore.invoke;
      const OriginalWorker = window.Worker;
      let release, cancelSeen, stagedSeen, catalogPosts = 0;
      const gate = new Promise(resolve => { release = resolve; });
      const stoppingSeen = new Promise(resolve => { cancelSeen = resolve; });
      const preparationSeen = new Promise(resolve => { stagedSeen = resolve; });
      const hookedCore = { ...originalCore, invoke: async (transport, args) => {
        if (args?.command === 'cancel-preview' && args.payload.kind === 'catalog') {
          cancelSeen();
          await gate;
        }
        if (args?.command === 'native-preview-resources' && args.payload._cmtPreviewKind === 'staged') stagedSeen();
        return originalInvoke(transport, args);
      } };
      window.__TAURI__.core = hookedCore;
      if (window.__TAURI__.core !== hookedCore) throw new Error('Could not install the isolated IPC delay hook');
      window.Worker = class extends OriginalWorker {
        postMessage(message, transfer) {
          if (message.type === 'catalog') catalogPosts++;
          return super.postMessage(message, transfer);
        }
      };
      let stopping, cached, staged;
      try {
        stopping = api.cancelNativeHairPreview();
        await Promise.race([stoppingSeen, new Promise((_, reject) =>
          setTimeout(() => reject(new Error('Catalog cancellation did not reach the delay hook')), 2000))]);
        cached = api.previewNativeHair({ assetType: 'hair',
          assetId: 'char/sig/png00143_geo_hair_straight.iff', includeBase: true });
        cached.catch(() => {});
        await new Promise(resolve => setTimeout(resolve, 30));
        const postsBeforeDrain = catalogPosts;
        staged = api.loadStagedPreview({ target });
        staged.catch(() => {});
        const stagedDispatchedBeforeDrain = await Promise.race([
          preparationSeen.then(() => true), new Promise(resolve => setTimeout(() => resolve(false), 1000)),
        ]);
        release();
        await stopping;
        const [catalogOutcome, stagedOutcome] = await Promise.allSettled([cached, staged]);
        const catalogResult = catalogOutcome.status === 'fulfilled' ? catalogOutcome.value : null;
        const stagedResult = stagedOutcome.status === 'fulfilled' ? stagedOutcome.value : null;
        const result = { cachedCatalogWaitedForDrain: postsBeforeDrain === 0,
          stagedDispatchedBeforeDrain, cachedHeadRetained: Boolean(catalogResult?.stats.headCacheHit),
          stagedMeshes: stagedResult?.meshes.length || 0, stagedTextures: stagedResult?.textures.length || 0,
          catalogError: catalogOutcome.status === 'rejected' ? String(catalogOutcome.reason) : null,
          stagedError: stagedOutcome.status === 'rejected' ? String(stagedOutcome.reason) : null };
        if (!baseline && (!result.cachedCatalogWaitedForDrain || !result.stagedDispatchedBeforeDrain
          || !result.cachedHeadRetained || !result.stagedMeshes || !result.stagedTextures)) throw new Error('Preview stop scopes regressed: ' + JSON.stringify(result));
        return result;
      } finally {
        release();
        await Promise.allSettled([stopping, cached, staged].filter(Boolean));
        window.__TAURI__.core = originalCore;
        window.Worker = OriginalWorker;
      }
    }, { target: path.resolve(root, '../character-mod-tool-rust/artifacts/conversion-output/png06827.iff'), baseline });
    await page.locator('#hairTab').click();
    await page.waitForFunction(() => window.hairBasePreview && window.hairAssetPreview
      && state.nativeHair.assets.length && !state.nativeHair.loading, null, { timeout: 90000 });
    report.returnToDisplayed = await page.evaluate(async baseline => {
      const api = window.characterTool;
      const original = api.previewNativeHair;
      const first = state.nativeHair.assets.find(asset => asset.id === 'char/sig/png06827_geo_hair_dreads.iff');
      const second = state.nativeHair.assets.find(asset => asset.id === 'char/sig/png00143_geo_hair_straight.iff');
      if (!first || !second) throw new Error('Expected UI burst assets are missing');
      await previewNativeHairAsset(first);
      const expected = window.hairAssetPreview.stats().payload;
      const held = await original({ assetType: 'hair', assetId: second.id, includeBase: true });
      let release;
      api.previewNativeHair = () => new Promise(resolve => { release = resolve; });
      try {
        const loading = previewNativeHairAsset(second);
        await previewNativeHairAsset(first);
        release(held);
        await loading;
        const current = window.hairAssetPreview.stats().payload;
        const result = { selectedKey: 'hair:' + first.id, displayedKey: state.nativeHair.displayedPreviewKey,
          expectedName: expected.name, renderedName: current.name,
          spinnerHidden: document.getElementById('nativeHairPreviewLoading').hidden };
        if (!baseline && (result.displayedKey !== result.selectedKey || result.renderedName !== result.expectedName
          || !result.spinnerHidden)) throw new Error('Returning to the displayed asset was overwritten: ' + JSON.stringify(result));
        return result;
      } finally { api.previewNativeHair = original; }
    }, baseline);
    await page.locator('.native-hair-preview-shell').screenshot({ path: path.join(root, 'artifacts',
      baseline ? baselineStem + '.png' : 'catalog-burst-latest.png') });
    report.resetBarriers = await page.evaluate(async baseline => {
      const originalCore = window.__TAURI__.core;
      const commands = [];
      let release, firstStop;
      const gate = new Promise(resolve => { release = resolve; });
      const seen = new Promise(resolve => { firstStop = resolve; });
      const hookedCore = { ...originalCore, invoke: async (transport, args) => {
        if (args?.command === 'cancel-preview') {
          commands.push(args.payload.kind); firstStop();
          await gate;
        }
        if (args?.command === 'reset') commands.push('cleanup');
        return originalCore.invoke(transport, args);
      } };
      window.__TAURI__.core = hookedCore;
      if (window.__TAURI__.core !== hookedCore) throw new Error('Could not install reset delay hook');
      let resetting;
      try {
        resetting = window.characterTool.resetSession();
        await Promise.race([seen, new Promise((_, reject) =>
          setTimeout(() => reject(new Error('Reset did not request cancellation')), 2000))]);
        await new Promise(resolve => setTimeout(resolve, 30));
        const bothStopsDispatchedBeforeDrain = commands.includes('staged') && commands.includes('catalog');
        const cleanupWaitedForDrain = !commands.includes('cleanup');
        release(); await resetting;
        const cache = await window.characterTool.previewCacheStats();
        const result = { bothStopsDispatchedBeforeDrain, cleanupWaitedForDrain,
          cleanupCompleted: commands.includes('cleanup'), cacheReleased: cache.cacheBytes === 0 && cache.heads === 0 };
        if (!baseline && Object.values(result).some(value => !value)) throw new Error('Reset stop ordering regressed: ' + JSON.stringify(result));
        return result;
      } finally {
        release(); await Promise.allSettled([resetting].filter(Boolean));
        window.__TAURI__.core = originalCore;
      }
    }, baseline);
    if (!baseline) report.resourceOwnership = await page.evaluate(async () => {
      const api = window.characterTool;
      const initial = await api.previewCacheStats();
      if (typeof initial.resourceLeases !== 'number') throw new Error('Native resource ownership counters unavailable');
      const hash = async bytes => Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', bytes)),
        byte => byte.toString(16).padStart(2, '0')).join('');
      const summarize = async preview => ({
        meshes: await Promise.all(preview.asset.meshes.map(async mesh => ({
          positions: await hash(new Float32Array(mesh.positions).buffer),
          uvs: await hash(new Float32Array(mesh.uvs).buffer),
          indices: await hash(new Uint32Array(mesh.indices).buffer),
        }))),
        textures: await Promise.all(preview.asset.textures.map(async texture => ({ name: texture.name,
          bytes: await hash(Uint8Array.from(atob(texture.bytes), byte => byte.charCodeAt(0))) }))),
      });
      let first, last, maximum = 0;
      for (let index = 0; index < 64; index++) {
        const preview = await api.previewNativeHair({ assetType: 'hair',
          assetId: 'char/sig/png06827_geo_hair_dreads.iff', includeBase: false,
          _testResourceOwner: index });
        if (!preview.stats.nativeGeometry) throw new Error('Ownership test unexpectedly fell back to compatibility geometry');
        if (index === 0) first = await summarize(preview);
        if (index === 63) last = await summarize(preview);
        const stats = await api.previewCacheStats(); maximum = Math.max(maximum, stats.resourceLeases);
        if (stats.resourceLeases > 48 || stats.registeredResources !== 3) throw new Error('Descriptor ownership accumulated obsolete native resources');
      }
      if (JSON.stringify(first) !== JSON.stringify(last)) throw new Error('Resource retirement changed real geometry/UV/texture output');
      await api.cancelNativeHairPreview();
      const cancelled = await api.previewCacheStats();
      if (cancelled.resourceLeases !== 48 || cancelled.registeredResources !== 3) throw new Error('Cancel discarded reusable descriptor resources');
      await api.resetSession();
      const reset = await api.previewCacheStats();
      if (reset.resourceLeases || reset.registeredResources) throw new Error('Reset retained native resource owners');
      return { descriptorIdentities: 64, maximumOwners: maximum, sharedResources: 3,
        nativeOutputUnchanged: true, cachedOwnersSurviveCancel: true, resetReleasesOwners: true };
    });
    assert(fs.existsSync(path.resolve(root, '../character-mod-tool-rust/artifacts/conversion-output/png06827.iff')), 'Reset removed an exported source fixture');
    assert.deepEqual(report.errors, []);
    report.completed = true;
    console.log(JSON.stringify(report, null, 2));
  } finally {
    if (page) await page.evaluate(() => window.__TAURI__.core.invoke('engine_call', { command: 'close-app', payload: {} })).catch(() => {});
    await browser?.close().catch(() => {});
    if (child.exitCode === null) await Promise.race([new Promise(resolve => child.once('exit', resolve)), delay(5000)]);
    if (child.exitCode === null) { child.kill(); await new Promise(resolve => child.once('exit', resolve)); }
    fs.writeFileSync(path.join(root, 'artifacts', baseline ? baselineStem + '.json' : 'catalog-burst-report.json'), JSON.stringify(report, null, 2));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
