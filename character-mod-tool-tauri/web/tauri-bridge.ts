import type { AssetDescriptor, AssetRequest, BlenderRequest, CatalogPreview, CloseSeamsRequest, EngineEvent, Events, Metadata, NativeCommands, Preview, PreviewMesh, PreviewOptions, ReferenceBundle, Resources, StagePlayerRequest, SwapRequest, TargetRequest } from './contracts';

type SnapshotResources = Resources & { snapshotId: string };
type BridgeCommands = NativeCommands & {
  'blender-snapshot-prepare': SnapshotResources;
  'blender-snapshot-fallback': SnapshotResources;
  'blender-snapshot-release': boolean;
};

// The preview worker sends texture bytes as a Uint8Array; native IPC is JSON.
function textureBase64(bytes: string | Uint8Array) {
  if (typeof bytes === 'string') return bytes;
  let text = '';
  for (let index = 0; index < bytes.length; index += 0x8000) text += String.fromCharCode(...bytes.subarray(index, index + 0x8000));
  return btoa(text);
}

function initializeBridge() {
  'use strict';
  const invoke = <K extends keyof BridgeCommands>(command: K, payload: unknown = {}) => window.__TAURI__.core.invoke<BridgeCommands[K]>('engine_call', { command, payload });
  const subscribers = new Map<keyof Events, Set<(value: unknown) => void>>();
  const publish = <K extends keyof Events>(name: K, value: Events[K]) => {
    for (const callback of [...(subscribers.get(name) || [])]) {
      try { callback(value); } catch (error) { console.error(`Character tool ${name} listener failed`, error); }
    }
  };
  const subscribe = <K extends keyof Events>(name: K) => (callback: (value: Events[K]) => void) => {
    if (!subscribers.has(name)) subscribers.set(name, new Set());
    const listener = (value: unknown) => callback(value as Events[K]);
    subscribers.get(name)!.add(listener);
    return () => { subscribers.get(name)?.delete(listener); };
  };
  let activeJob = '';
  let cancelledJob = '';
  let jobStarted: { jobId: string; promise: Promise<boolean>; resolve: (started: boolean) => void } | null = null;
  const blenderOpened = new Map<string, { resolve: (event: EngineEvent) => void; reject: (error: Error) => void }>();
  const targetKey = (target: string) => {
    const path = target.replace(/\//g, '\\').replace(/^\\\\\?\\(?:UNC\\)?/i, match => /UNC/i.test(match) ? '\\\\' : '').toLowerCase();
    const prefix = path.startsWith('\\\\') ? '\\\\' : path.startsWith('\\') ? '\\' : '';
    const parts: string[] = [];
    for (const part of path.slice(prefix.length).split('\\')) {
      if (!part || part === '.') continue;
      if (part === '..' && parts.length && parts.at(-1) !== '..' && !parts.at(-1)!.endsWith(':') && !(prefix === '\\\\' && parts.length <= 2)) parts.pop();
      else if (part !== '..' || !prefix && !parts[0]?.endsWith(':')) parts.push(part);
    }
    return prefix + parts.join('\\');
  };
  const blenderTargets = new Map<string, { id: string; phase: 'waiting' | 'preparing' | 'opened' }>();
  const blenderPreparations = new Set<Promise<void>>();
  const eventsReady = window.__TAURI__.event.listen<EngineEvent>('engine-event', ({ payload: event }) => {
    if (event.command === 'run-swap' && activeJob && (!event.jobId || event.jobId === activeJob)) {
      if (event.type === 'job-started') { if (jobStarted?.jobId === activeJob) jobStarted.resolve(true); }
      else publish('job:event', { ...event, jobId: activeJob });
    }
    if (event.type === 'blender') {
      const target = targetKey(String(event.requestedTarget || event.target || ''));
      const handoffId = String(event.handoffId || '');
      const handoff = blenderTargets.get(target);
      if (!handoffId || handoff?.id !== handoffId) return;
      if (event.state === 'opened') handoff.phase = 'opened';
      if (event.state === 'complete') invalidatePlayerPreview(event.requestedTarget || event.target);
      if (['complete', 'closed', 'error'].includes(event.state || '')) blenderTargets.delete(target);
      const opening = blenderOpened.get(handoffId);
      if (opening && ['opened', 'closed', 'error'].includes(event.state || '')) {
        blenderOpened.delete(handoffId);
        if (event.state === 'opened') opening.resolve(event);
        else opening.reject(new Error(event.message || 'Blender closed before opening the player.'));
      }
      publish('blender:event', event);
    }
  });
  const previewCommands = new Set<keyof BridgeCommands>(['native-preview-resources', 'native-preview-hair', 'native-catalog-geometry', 'preview-resources', 'native-hair-preview', 'read-archive', 'read-file', 'file-stat']);
  type PreviewKind = 'staged' | 'catalog' | 'independent';
  const pendingPreviewCalls = new Map<Promise<unknown>, PreviewKind>();
  const playerEditCommands = new Set<keyof BridgeCommands>(['body-fit-save', 'body-chest-fix', 'preview-chest-fix', 'preview-close-seams', 'headband-apply', 'native-hair-install', 'configs-change']);
  const pendingPlayerEdits = new Map<Promise<unknown>, string>();
  let binaryPreview = false;
  let binaryGeometryPreview = false;
  async function binaryReply(command: string, payload: unknown, kind: PreviewKind) {
    await eventsReady;
    const request = window.__TAURI__.core.invoke<ArrayBuffer>('preview_binary', { command, payload });
    pendingPreviewCalls.set(request, kind);
    try {
      const buffer = await request;
      if (!(buffer instanceof ArrayBuffer)) throw new Error('Host returned an invalid binary preview response.');
      return buffer;
    } finally { pendingPreviewCalls.delete(request); }
  }
  const call = async <K extends keyof BridgeCommands>(command: K, payload: unknown): Promise<BridgeCommands[K]> => {
    await eventsReady;
    if (playerEditCommands.has(command)) {
      const target = (payload as { target?: unknown } | null)?.target;
      const key = typeof target === 'string' ? targetKey(target) : '';
      if (command === 'preview-close-seams' && (!key || blenderTargets.has(key))) {
        throw new Error(!key ? 'Stage a player before closing seams.' : "Finish this player's Blender round trip before closing seams.");
      }
      const preparing = key ? blenderTargets.get(key)?.phase === 'preparing'
        : [...blenderTargets.values()].some(handoff => handoff.phase === 'preparing');
      if (preparing) throw new Error("Finish opening this player's Blender preview before applying changes.");
    }
    const request = invoke(command, payload);
    if (playerEditCommands.has(command)) {
      const target = (payload as { target?: unknown } | null)?.target;
      pendingPlayerEdits.set(request, typeof target === 'string' ? targetKey(target) : '');
    }
    if (previewCommands.has(command)) {
      const tagged = payload && typeof payload === 'object' ? (payload as { _cmtPreviewKind?: PreviewKind })._cmtPreviewKind : undefined;
      const kind = tagged || (['native-preview-hair', 'native-hair-preview', 'native-catalog-geometry'].includes(command) ? 'catalog'
        : ['native-preview-resources', 'preview-resources'].includes(command) ? 'staged' : 'independent');
      pendingPreviewCalls.set(request, kind);
    }
    try {
      const result = await request;
      if (playerEditCommands.has(command)) invalidatePlayerPreview((payload as { target?: unknown } | null)?.target);
      else if (command === 'run-swap') invalidatePlayerPreview((result as { output?: unknown }).output);
      return result;
    } catch (error) { throw error instanceof Error ? error : new Error(String(error)); }
    finally { pendingPreviewCalls.delete(request); pendingPlayerEdits.delete(request); }
  };
  const previewStopping: Record<'staged' | 'catalog', Promise<unknown>> = {
    staged: Promise.resolve(), catalog: Promise.resolve(),
  };
  function stopPreview(kind: 'staged' | 'catalog') {
    const stopped = previewStopping[kind].catch(() => {}).then(async () => {
      await call('cancel-preview', { kind });
      await Promise.allSettled([...pendingPreviewCalls].filter(([, scope]) => scope === kind).map(([request]) => request));
      return true;
    });
    previewStopping[kind] = stopped;
    return stopped;
  }
  type DecoderMessage = { kind: 'native'; command: keyof NativeCommands; payload: unknown; id: number } | { kind?: 'result'; id: number; error?: { message?: string } | string; result: unknown };
  class Decoder {
    worker: Worker | null;
    pending: Map<number, { resolve: (value: unknown) => void; reject: (error: Error) => void }>;
    sequence: number;
    private generation = 0;
    constructor(private readonly kind: PreviewKind = 'independent') { this.worker = null; this.pending = new Map(); this.sequence = 0; }
    decode(type: 'preview', payload: { filePath: string; options: PreviewOptions }): Promise<Preview>;
    decode(type: 'catalog', payload: AssetDescriptor & { includeBase: boolean }): Promise<CatalogPreview>;
    async decode(type: 'preview' | 'catalog', payload: unknown): Promise<unknown> {
      const generation = this.generation;
      if (this.kind !== 'independent') await previewStopping[this.kind].catch(() => {});
      if (generation !== this.generation) throw new Error('Preview request cancelled.');
      if (!this.worker) {
        const worker = this.worker = new Worker('preview-worker.bundle.js');
        worker.onmessage = async ({ data }: MessageEvent<DecoderMessage>) => {
          if (this.worker !== worker) return;
          if (data.kind === 'native') {
            try {
              const payload: Record<string, unknown> = { ...(data.payload as Record<string, unknown>), _cmtPreviewKind: this.kind };
              const raw = (binaryPreview && ['read-archive', 'read-file'].includes(data.command)
                && String(payload.path || '').startsWith('cmt-preview://'))
                || (binaryGeometryPreview && data.command === 'native-catalog-geometry' && payload.nativeBinaryGeometry !== false);
              const value = raw ? await binaryReply(data.command, payload, this.kind) : await call(data.command, payload);
              if (this.worker === worker) worker.postMessage({ kind: 'native-reply', id: data.id, value },
                value instanceof ArrayBuffer ? [value] : []);
            }
            catch (error) { if (this.worker === worker) worker.postMessage({ kind: 'native-reply', id: data.id, error: String(error) }); }
            return;
          }
          const task = this.pending.get(data.id);
          if (!task) return;
          this.pending.delete(data.id);
          if (data.error) task.reject(new Error(typeof data.error === 'string' ? data.error : data.error.message || String(data.error))); else task.resolve(data.result);
        };
        worker.onerror = event => { if (this.worker === worker) this.cancel(event.message || 'Preview decoder failed.'); };
      }
      return new Promise((resolve, reject) => {
        const id = ++this.sequence;
        this.pending.set(id, { resolve, reject });
        this.worker!.postMessage({ id, type, payload, latestOnly: this.kind === 'catalog' && type === 'catalog' });
      });
    }
    cancel(message = 'Preview request cancelled.') { this.generation++; this.worker?.terminate(); this.worker = null; for (const task of this.pending.values()) task.reject(new Error(message)); this.pending.clear(); }
  }
  const staged = new Decoder('staged'), catalog = new Decoder('catalog');
  let stagedEpoch = 0, catalogEpoch = 0;
  const pendingStaging = new Set<Promise<unknown>>();
  type HeldDescriptor = { asset: AssetDescriptor; users: number; retired: boolean; released: boolean };
  const descriptors = new Map<string, HeldDescriptor>();
  async function releaseResources(resource: unknown) {
    const lease = resource && typeof resource === 'object' ? (resource as { resourceLease?: unknown }).resourceLease : undefined;
    if (typeof lease !== 'string') return;
    try { await call('release-preview-resources', { lease }); }
    catch (error) { console.warn('Preview resource cleanup failed', error); }
  }
  async function retireDescriptor(entry: HeldDescriptor) {
    entry.retired = true;
    if (!entry.users && !entry.released) {
      entry.released = true;
      await releaseResources(entry.asset);
    }
  }
  function clearDescriptors() {
    const entries = [...descriptors.values()]; descriptors.clear();
    entries.forEach(entry => { void retireDescriptor(entry); });
  }
  function cacheDescriptor(key: string, entry: HeldDescriptor) {
    const previous = descriptors.get(key);
    descriptors.delete(key);
    if (previous && previous !== entry) void retireDescriptor(previous);
    descriptors.set(key, entry);
    while (descriptors.size > 48) {
      const oldest = descriptors.keys().next().value!;
      const retired = descriptors.get(oldest)!; descriptors.delete(oldest);
      void retireDescriptor(retired);
    }
  }
  let assetPreparation: Promise<unknown> = Promise.resolve();
  const stagedPreviews = new Map<string, { target: string; preview: Preview }>();
  let stagedRead: { target: string; valid: boolean } | null = null;
  function invalidatePlayerPreview(target: unknown) {
    const key = typeof target === 'string' && target ? targetKey(target) : '';
    if (stagedRead && (!key || stagedRead.target === key)) stagedRead.valid = false;
    for (const [fingerprint, cached] of stagedPreviews) {
      if (!key || cached.target === key) stagedPreviews.delete(fingerprint);
    }
  }
  let settingsEpoch = 0, catalogSequence = 0, acceptedCatalogSequence = 0;
  let catalogRevision: string | undefined;
  function invalidateCatalogSource() {
    catalogEpoch++; catalog.cancel(); clearDescriptors();
  }
  function settingsCommitted() {
    settingsEpoch++; catalogRevision = undefined;
    invalidateCatalogSource();
    stagedEpoch++; staged.cancel(); stagedPreviews.clear();
  }
  async function prepareResources(request: TargetRequest, current = () => true, kind: PreviewKind = 'staged') {
    if (kind !== 'independent') await previewStopping[kind].catch(() => {});
    if (!current()) throw new Error('Preview request cancelled.');
    try { return await call('native-preview-resources', { ...request, _cmtPreviewKind: kind }); }
    catch (error) {
      if (!current()) throw new Error('Preview request cancelled.');
      const reason = String(error).slice(0, 2000);
      try {
        const fallback = await call('preview-resources', { ...request, _cmtPreviewKind: kind });
        return { ...fallback, warnings: [...fallback.warnings, 'Native resource fallback: ' + reason] };
      } catch (fallbackError) {
        if (!current()) throw new Error('Preview request cancelled.');
        throw new Error('Preview preparation failed. Native: ' + reason + '\nCompatibility: '
          + String(fallbackError).slice(0, 2000), { cause: fallbackError });
      }
    }
  }
  async function loadStaged(request: TargetRequest) {
    const epoch = ++stagedEpoch; staged.cancel();
    await eventsReady;
    const key = targetKey(request.target);
    while (true) {
      if (epoch !== stagedEpoch) throw new Error('Preview request cancelled.');
      const writes = [...pendingPlayerEdits].filter(([, target]) => !target || target === key).map(([write]) => write);
      if (!writes.length) break;
      await Promise.allSettled(writes);
    }
    const read = { target: key, valid: true }; stagedRead = read;
    const current = () => epoch === stagedEpoch && read.valid;
    let resources: Awaited<ReturnType<typeof prepareResources>> | undefined;
    try {
    resources = await prepareResources(request, current);
    if (!current()) throw new Error('Preview request cancelled.');
    let preview: Preview;
    const cached = resources.fingerprint ? stagedPreviews.get(resources.fingerprint) : undefined;
    if (cached?.target === key) {
      preview = structuredClone(cached.preview);
      preview.stats.previewCacheHit = true;
    } else {
      try { preview = await staged.decode('preview', { filePath: resources.filePath, options: resources.options }); }
      catch (error) {
        if (!resources.native || !current()) throw error;
        const fallback = await call('preview-resources', request);
        await releaseResources(resources); resources = fallback;
        if (!current()) throw new Error('Preview request cancelled.');
        preview = await staged.decode('preview', { filePath: resources.filePath, options: resources.options });
        preview.skipped.push('Native preview fallback: ' + String(error));
      }
      if (!current()) throw new Error('Preview request cancelled.');
      preview.stats.nativeAssetReader = Boolean(resources.native);
      if (resources.fingerprint) {
        if (stagedPreviews.size >= 2) stagedPreviews.delete(stagedPreviews.keys().next().value!);
        stagedPreviews.set(resources.fingerprint, { target: key, preview: structuredClone(preview) });
      }
    }
    if (!current()) throw new Error('Preview request cancelled.');
    Object.assign(preview, { player: resources.player, name: resources.name, activeItems: resources.activeItems, companionNames: resources.options.companionPaths.map(item => item.name) });
    preview.skipped.push(...resources.warnings);
    return preview;
    } finally {
      await releaseResources(resources);
      if (stagedRead === read) stagedRead = null;
      if (!current()) throw new Error('Preview request cancelled.');
    }
  }
  async function openBlender(request: BlenderRequest) {
    const target = targetKey(request.target);
    if (blenderTargets.has(target)) throw new Error('This staged player is already open in a Blender round trip.');
    const handoffId = crypto.randomUUID();
    const handoff = { id: handoffId, phase: 'waiting' as 'waiting' | 'preparing' | 'opened' };
    blenderTargets.set(target, handoff);
    const sourceEpoch = settingsEpoch;
    let preparationFinished!: () => void;
    const preparation = new Promise<void>(resolve => { preparationFinished = resolve; });
    blenderPreparations.add(preparation);
    const release = () => { if (blenderTargets.get(target) === handoff) blenderTargets.delete(target); };
    let snapshotId: string | undefined;
    try {
    await eventsReady;
    while (true) {
      const writes = [...pendingPlayerEdits].filter(([, key]) => !key || key === target).map(([write]) => write);
      if (!writes.length) break;
      await Promise.allSettled(writes);
    }
    const currentSource = () => sourceEpoch === settingsEpoch;
    if (!currentSource()) throw new Error('Settings changed while Blender was preparing. Reopen the preview.');
    handoff.phase = 'preparing';
    let resources = await call('blender-snapshot-prepare', { ...request, handoffId, _cmtPreviewKind: 'independent' });
    snapshotId = resources.snapshotId;
    if (!snapshotId) throw new Error('The host did not create an immutable Blender snapshot.');
    if (!currentSource()) throw new Error('Settings changed while Blender was preparing. Reopen the preview.');
    let referenceBundle: ReferenceBundle | undefined;
    if (request.includeHair || request.includeFacialHair || request.includeTextures) {
      const decoder = new Decoder();
      try {
        const decode = () => decoder.decode('preview', { filePath: resources.filePath, options: { ...resources.options, skipTextures: !request.includeTextures, maxTriangles: 1000000, maxModels: 1000, projectTangent: 'all' } });
        let preview: Preview;
        try { preview = await decode(); }
        catch (error) {
          if (!resources.native || !currentSource()) throw error;
          decoder.cancel();
          const fallback = await call('blender-snapshot-fallback', { ...request, handoffId, snapshotId });
          if (fallback.snapshotId !== snapshotId) throw new Error('Blender fallback belongs to a different snapshot.');
          resources = fallback;
          if (!currentSource()) throw new Error('Settings changed while Blender was preparing. Reopen the preview.');
          preview = await decode();
        }
        const ordinary = new Set(resources.options.companionPaths.filter(x => x.kind === 'geometry').map(x => x.name.toLowerCase()));
        const kind = (mesh: PreviewMesh) => /facialhair|facial_hair|beard|goatee|scruff/i.test(`${mesh.sourceName} ${mesh.name}`) ? 'facialhair' : /(?:^|[_\s])hair(?:[_\s.]|$)/i.test(`${mesh.sourceName} ${mesh.name}`) ? 'hair' : '';
        const meshes = preview.meshes.filter(mesh => mesh.sourceKind === 'geometry' && (kind(mesh) === 'hair' && request.includeHair || kind(mesh) === 'facialhair' && request.includeFacialHair) && (!String(mesh.sourceName).includes('_tangentspace.iff') || !ordinary.has(String(mesh.sourceName).toLowerCase().replace('_tangentspace.iff', '.iff'))))
          .map(mesh => ({ name: mesh.name, kind: kind(mesh), sourceName: mesh.sourceName, positions: Array.from(mesh.positions), uvs: Array.from(mesh.uvs || []), indices: Array.from(mesh.indices), groups: mesh.groups || [] }));
        const textures = request.includeTextures ? preview.textures.filter(x => x.bytes).map(texture => ({ name: texture.name, bytes: textureBase64(texture.bytes!), binding: texture.binding || null, assignments: texture.assignments || [], inputColorSpace: texture.inputColorSpace || 'none', range: texture.range || null })) : [];
        referenceBundle = { meshes, textures, eye: { irisColor: resources.player.irisColor, irisSize: resources.player.irisSize } };
      } finally { decoder.cancel(); }
    }
    if (!currentSource()) throw new Error('Settings changed while Blender was preparing. Reopen the preview.');
    return await new Promise<EngineEvent & { referenceCount: number; textureCount: number }>((resolve, reject) => {
      const opening = { resolve: (event: EngineEvent) => resolve({ ...event, referenceCount: referenceBundle?.meshes.length || 0, textureCount: referenceBundle?.textures.length || 0 }), reject };
      blenderOpened.set(handoffId, opening);
      call('blender-open', { ...request, referenceBundle, handoffId, snapshotId }).then(() => {
        if (blenderOpened.get(handoffId) === opening) {
          blenderOpened.delete(handoffId);
          release();
          reject(new Error('Blender closed before opening the player.'));
        }
      }).catch(error => {
        if (blenderOpened.get(handoffId) === opening) { blenderOpened.delete(handoffId); reject(new Error(String(error))); }
        if (blenderTargets.get(target) === handoff) {
          release();
          publish('blender:event', { state: 'error', target: request.target, handoffId, message: String(error) });
        }
      });
    });
    } catch (error) { release(); throw error; }
    finally {
      if (snapshotId) {
        try { await call('blender-snapshot-release', { snapshotId, handoffId }); }
        catch (error) { console.warn('Blender snapshot cleanup failed', error); }
      }
      preparationFinished(); blenderPreparations.delete(preparation);
    }
  }
  const api = {
    async bootstrap() { const value = await call('bootstrap', {}); return { ...value, electronVersion: '2 (Rust)', projectRoot: 'Local Tauri runtime' }; },
    runHealthCheck: () => call('health', {}),
    async repairHealth(request: unknown) { await call('health-repair', request); settingsCommitted(); return { report: await call('health', {}), bootstrap: await api.bootstrap() }; },
    listConfigs: (request: TargetRequest) => call('configs-list', request), changeConfig: (request: unknown) => call('configs-change', request),
    chooseCharacter: (role: string) => call('choose-file', { title: `Choose ${role} character` }),
    chooseHair: () => call('choose-file', { title: 'Choose source hair geometry', iffOnly: true }),
    chooseFolder: (kind: string) => call('choose-folder', { title: `Choose ${kind} folder` }),
    chooseSetting: (request: { field: string; currentPath: string }) => {
      const fields: Record<string, string> = { settings26: 'NBA 2K26 folder', settings27: 'NBA 2K27 folder', settingsBlender: 'Blender executable', settingsAddon: 'Head Swap Tool folder', settingsMdt: 'Mesh Data Transfer folder', settingsOutput: 'output folder' };
      if (!Object.hasOwn(fields, request.field)) return Promise.reject(new Error('Unknown settings path.'));
      return call(request.field === 'settingsBlender' ? 'choose-file' : 'choose-folder', { title: 'Choose ' + fields[request.field], exe: request.field === 'settingsBlender', current: request.currentPath });
    },
    inspectCharacter: (filePath: string) => call('inspect', { filePath }), loadCatalog: (gameLabel: string) => call('catalog', { gameLabel }),
    async stagePlayer(request: StagePlayerRequest) {
      const operation = call('stage-player', request);
      pendingStaging.add(operation);
      try { return await operation; } finally { pendingStaging.delete(operation); }
    },
    async extractPlayer(request: { gameLabel: string; png: string; role?: string }) { const result = await call('extract', request); result.inspected = await call('inspect', { filePath: result.mainPath }); return result; },
    async resetSession() {
      stagedEpoch++; catalogEpoch++;
      staged.cancel(); catalog.cancel();
      clearDescriptors(); stagedPreviews.clear();
      try {
        await Promise.all([stopPreview('staged'), stopPreview('catalog')]);
        await Promise.allSettled([...blenderPreparations]);
        await Promise.allSettled([...pendingStaging]);
        return await call('reset', {});
      } finally {
        await Promise.allSettled(pendingPlayerEdits.keys());
      }
    },
    previewCacheStats: () => call('preview-cache-stats', {}),
    loadHairOptions: (request: unknown) => call('hair-options', request), loadFacialHairOptions: (request: unknown) => call('facial-hair-options', request),
    async loadNativeHairCatalog(request: AssetRequest) {
      const epoch = settingsEpoch, sequence = ++catalogSequence;
      const result = await call('native-hair-catalog', request);
      if (epoch !== settingsEpoch || sequence < acceptedCatalogSequence && result.sourceRevision !== catalogRevision) {
        throw new Error('Preview request cancelled. The catalog source changed.');
      }
      if (result.sourceRevision !== catalogRevision) {
        invalidateCatalogSource(); catalogRevision = result.sourceRevision;
      }
      acceptedCatalogSequence = Math.max(acceptedCatalogSequence, sequence);
      return { assetType: result.assetType, assets: result.assets || [], gameRoot: result.gameRoot, sourceRevision: result.sourceRevision };
    },
    async previewNativeHair(request: AssetRequest) {
      const epoch = ++catalogEpoch, key = JSON.stringify(request);
      const borrowed: HeldDescriptor[] = [];
      const borrow = (entry: HeldDescriptor) => { entry.users++; borrowed.push(entry); return entry; };
      try {
      let entry = descriptors.get(key);
      if (!entry) {
        const preparation = assetPreparation.catch(() => {}).then(async () => {
          await previewStopping.catalog.catch(() => {});
          if (epoch !== catalogEpoch) throw new Error('Preview request cancelled.');
          try { return await call('native-preview-hair', request); }
          catch { if (epoch !== catalogEpoch) throw new Error('Preview request cancelled.'); return call('native-hair-preview', request); }
        });
        assetPreparation = preparation;
        const asset = await preparation;
        if (epoch !== catalogEpoch) { await releaseResources(asset); throw new Error('Preview request cancelled.'); }
        entry = borrow({ asset, users: 0, retired: false, released: false });
        cacheDescriptor(key, entry);
      } else {
        borrow(entry); cacheDescriptor(key, entry);
      }
      if (epoch !== catalogEpoch) throw new Error('Preview request cancelled.');
      let value: CatalogPreview;
      try { value = await catalog.decode('catalog', { ...entry.asset, includeBase: request.includeBase !== false, nativeGeometry: request.nativeGeometry !== false, nativeBinaryGeometry: request.nativeBinaryGeometry !== false }); }
      catch (error) {
        if (!entry.asset.native || epoch !== catalogEpoch) throw error;
        const asset = await call('native-hair-preview', request);
        if (epoch !== catalogEpoch) { await releaseResources(asset); throw new Error('Preview request cancelled.'); }
        entry = borrow({ asset, users: 0, retired: false, released: false });
        cacheDescriptor(key, entry);
        value = await catalog.decode('catalog', { ...asset, includeBase: request.includeBase !== false });
      }
      if (epoch !== catalogEpoch) throw new Error('Preview request cancelled.');
      return value;
      } finally {
        for (const entry of borrowed) {
          entry.users--;
          if (entry.retired) await retireDescriptor(entry);
        }
      }
    },
    cancelNativeHairPreview() { catalogEpoch++; catalog.cancel(); return stopPreview('catalog'); },
    async loadNativeHairFitBase(request: TargetRequest) { const preview = await catalog.decode('preview', { filePath: request.target, options: { hairFitBust: true, skipTextures: true, preferHighestDetail: true, maxTriangles: 120000, maxModels: 8 } }); preview.meshes.forEach(x => { x.sourceKind = 'hair-fit-base'; }); return { ...preview, player: { mode: 'hair-fit-base', bodyFit: {}, jerseyAvailable: false }, activeItems: [] }; },
    loadNativeHairTarget: (request: unknown) => call('native-hair-target', request), installNativeHair: (request: unknown) => call('native-hair-install', request),
    loadBodyFit: (request: unknown) => call('body-fit', request), saveBodyFit: (request: unknown) => call('body-fit-save', request), applyChestFix: (request: unknown) => call('body-chest-fix', request),
    loadHeadbandOptions: (request: unknown) => call('headband-options', request), applyHeadband: (request: unknown) => call('headband-apply', request),
    loadStagedPreview: loadStaged, cancelStagedPreview() { stagedEpoch++; staged.cancel(); return stopPreview('staged'); },
    openPreviewInBlender: openBlender, applyPreviewChestCorrection: (request: unknown) => call('preview-chest-fix', request),
    closePreviewSeams: (request: CloseSeamsRequest) => call('preview-close-seams', request),
    readRosterPlayer: (request: unknown) => call('cli-read', request), setRosterPlayerField: (request: unknown) => call('cli-set', request),
    getRosterLanguage: () => call('cli-language-get', {}), setRosterLanguage: (request: unknown) => call('cli-language-set', request),
    async saveSettings(settings: unknown) { const result = await call('save-settings', settings); settingsCommitted(); return result; },
    loadTemplates: () => call('templates-load', {}), saveTemplates: (templates: unknown) => call('templates-save', templates),
    async startSwap(request: SwapRequest) {
      if (activeJob) throw new Error('A conversion is already running.');
      const jobId = crypto.randomUUID(); activeJob = jobId;
      let acknowledge!: (started: boolean) => void;
      jobStarted = { jobId, promise: new Promise<boolean>(resolve => { acknowledge = resolve; }), resolve: acknowledge };
      const finish = () => {
        if (activeJob === jobId) activeJob = '';
        if (jobStarted?.jobId === jobId) { jobStarted.resolve(false); jobStarted = null; }
      };
      (async () => {
        await new Promise(resolve => setTimeout(resolve, 0));
        await eventsReady;
        if (cancelledJob === jobId) throw new Error('Conversion cancelled.');
        return call('run-swap', { ...request, _cmtJobId: jobId });
      })().then(result => {
        finish(); publish('job:complete', { ...result, jobId, result, output: result.output });
      }).catch(error => {
        const cancelled = cancelledJob === jobId || /cancelled|canceled/i.test(String(error));
        finish(); publish('job:failed', { jobId, error: String(error), message: cancelled ? 'Conversion cancelled.' : String(error), cancelled });
      });
      return { jobId };
    },
    cancelSwap: async (jobId?: string) => {
      if (!activeJob || jobId && jobId !== activeJob) return false;
      const cancelling = activeJob;
      cancelledJob = cancelling;
      const started = await jobStarted?.promise;
      if (started && activeJob === cancelling) await call('cancel-command', { command: 'run-swap', jobId: cancelling });
      return true;
    },
    openPath: (path: string) => call('open-path', { path }), showItem: (path: string) => call('show-item', { path }),
    launchLegacyApp: () => Promise.reject(new Error('Use your production launcher for the legacy interface.')),
    onJobEvent: subscribe('job:event'), onJobComplete: subscribe('job:complete'), onJobFailed: subscribe('job:failed'), onBlenderEvent: subscribe('blender:event')
  };
  window.characterTool = api;
  call('runtime-info', {}).then(info => {
    binaryPreview = info.previewBinary === true;
    binaryGeometryPreview = info.previewGeometryBinary === true;
    if (!info.smoke) return;
    const timer = setInterval(() => { const label = document.getElementById('engineStatus')?.textContent || ''; if (!label.includes('Engine ')) return; clearInterval(timer); call('smoke-ready', { title: document.title, nav: Array.from(document.querySelectorAll('.nav-item')).map(x => x.textContent.trim()), engineLabel: label }); }, 100);
  }).catch(console.error);
  return api;
}
export type CharacterTool = ReturnType<typeof initializeBridge>;
initializeBridge();
