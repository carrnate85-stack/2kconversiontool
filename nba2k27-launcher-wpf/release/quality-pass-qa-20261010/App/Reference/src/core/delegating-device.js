const crypto = require('node:crypto');

const MAX_ROUTE_ACTIVITY = 100;

function isAssetMiss(error) {
  return error?.code === 'ASSET_NOT_FOUND' || /^Asset not found:/i.test(String(error?.message || ''));
}

function assertDeviceId(value) {
  if (!Number.isSafeInteger(value) || value <= 0 || value > 0xffffffff) {
    throw new Error('A file device must have a nonzero uint32 ID.');
  }
  return value;
}

class OfflineFileDeviceRegistry {
  constructor() {
    this.devices = new Map();
    this.defaultId = 0;
  }

  register(device) {
    if (!device || typeof device !== 'object') throw new Error('A file device object is required.');
    const id = assertDeviceId(device.id);
    if (device.registry || device.selfLinked === false) throw new Error('The file device is already linked or registered.');
    if (this.devices.has(id)) throw new Error(`File-device ID 0x${id.toString(16)} is already registered.`);
    this.devices.set(id, device);
    device.registry = this;
    device.selfLinked = false;
    return { id, registered: true, deviceCount: this.devices.size };
  }

  setDefault(id) {
    const normalized = assertDeviceId(id);
    if (!this.devices.has(normalized)) throw new Error('The default file-device ID is not registered.');
    this.defaultId = normalized;
    return normalized;
  }

  select(explicitId = 0) {
    const id = explicitId || this.defaultId;
    if (!id) throw new Error('No default file device is registered.');
    const device = this.devices.get(id);
    if (!device) throw new Error(`File-device ID 0x${id.toString(16)} is not registered.`);
    return device;
  }

  unregister(device) {
    if (!device || device.registry !== this || this.devices.get(device.id) !== device) {
      throw new Error('The file device is not registered with this manager.');
    }
    device.onUnregister();
    this.devices.delete(device.id);
    if (this.defaultId === device.id) this.defaultId = 0;
    device.registry = null;
    device.selfLinked = true;
    return { id: device.id, unregistered: true, deviceCount: this.devices.size };
  }

  snapshot() {
    return {
      defaultId: this.defaultId,
      deviceIds: [...this.devices.keys()],
      deviceCount: this.devices.size,
    };
  }
}

class DelegatingFileDevice {
  constructor({ id, modDevice, stockDevice }) {
    this.id = assertDeviceId(id);
    if (!modDevice || !stockDevice) throw new Error('Both mod and stock file devices are required.');
    this.modDevice = modDevice;
    this.stockDevice = stockDevice;
    this.registry = null;
    this.selfLinked = true;
    this.disposed = false;
    this.handles = new Map();
    this.activity = [];
    this.routeCounts = { mod: 0, stock: 0 };
  }

  requireActive() {
    if (this.disposed) throw new Error('The delegating file device is disposed.');
    if (!this.registry || this.selfLinked) throw new Error('The delegating file device is not registered.');
  }

  record(operation, logicalPath, source) {
    this.activity.push({ operation, logicalPath, source });
    if (this.activity.length > MAX_ROUTE_ACTIVITY) this.activity.shift();
  }

  async route(operation, logicalPath) {
    this.requireActive();
    try {
      const result = await this.modDevice[operation](logicalPath);
      this.routeCounts.mod += 1;
      this.record(operation, logicalPath, 'mod');
      return { source: 'mod', device: this.modDevice, result };
    } catch (error) {
      if (!isAssetMiss(error)) throw error;
    }
    const result = await this.stockDevice[operation](logicalPath);
    this.routeCounts.stock += 1;
    this.record(operation, logicalPath, 'stock');
    return { source: 'stock', device: this.stockDevice, result };
  }

  async query(logicalPath) {
    const routed = await this.route('query', logicalPath);
    return { ...routed.result, source: routed.source };
  }

  async open(logicalPath) {
    const routed = await this.route('open', logicalPath);
    const handle = crypto.randomUUID();
    this.handles.set(handle, {
      source: routed.source,
      device: routed.device,
      childHandle: routed.result.handle,
      logicalPath: routed.result.logicalPath || logicalPath,
    });
    return {
      ...routed.result,
      childHandle: undefined,
      handle,
      source: routed.source,
    };
  }

  requireHandle(handle) {
    this.requireActive();
    const state = this.handles.get(handle);
    if (!state) throw new Error('Unknown or closed delegating-device handle.');
    return state;
  }

  async read(handle, length, offset) {
    const state = this.requireHandle(handle);
    const result = await state.device.read(state.childHandle, length, offset);
    return { ...result, childHandle: undefined, handle, source: state.source };
  }

  close(handle) {
    const state = this.requireHandle(handle);
    const result = state.device.close(state.childHandle);
    this.handles.delete(handle);
    return { ...result, childHandle: undefined, handle, source: state.source, openHandles: this.handles.size };
  }

  closeAll() {
    let closed = 0;
    for (const [handle, state] of this.handles) {
      try { state.device.close(state.childHandle); } catch (_error) { /* best-effort deterministic cleanup */ }
      this.handles.delete(handle);
      closed += 1;
    }
    return closed;
  }

  onUnregister() {
    return this.closeAll();
  }

  dispose() {
    if (this.registry) this.registry.unregister(this);
    else this.closeAll();
    this.disposed = true;
    return { disposed: true, selfLinked: this.selfLinked, openHandles: this.handles.size };
  }

  status() {
    return {
      id: this.id,
      registered: Boolean(this.registry),
      selfLinked: this.selfLinked,
      disposed: this.disposed,
      openHandles: this.handles.size,
      routeCounts: { ...this.routeCounts },
      activity: [...this.activity],
    };
  }
}

class MemoryFileDevice {
  constructor(files = {}) {
    this.files = new Map(Object.entries(files).map(([name, value]) => [name, Buffer.from(value)]));
    this.handles = new Map();
  }

  async query(logicalPath) {
    const data = this.files.get(logicalPath);
    if (!data) {
      const error = new Error(`Asset not found: ${logicalPath}`);
      error.code = 'ASSET_NOT_FOUND';
      throw error;
    }
    return { logicalPath, size: data.length };
  }

  async open(logicalPath) {
    const metadata = await this.query(logicalPath);
    const handle = crypto.randomUUID();
    this.handles.set(handle, { logicalPath, position: 0 });
    return { ...metadata, handle };
  }

  async read(handle, length, offset) {
    const state = this.handles.get(handle);
    if (!state) throw new Error('Unknown or closed memory-device handle.');
    const data = this.files.get(state.logicalPath);
    const start = offset ?? state.position;
    const chunk = data.subarray(start, start + length);
    if (offset === undefined || offset === null) state.position += chunk.length;
    return { handle, logicalPath: state.logicalPath, bytesRead: chunk.length, data: chunk.toString('base64') };
  }

  close(handle) {
    if (!this.handles.delete(handle)) throw new Error('Unknown or closed memory-device handle.');
    return { handle, closed: true, openHandles: this.handles.size };
  }
}

async function runOfflineDelegationSelfTest() {
  const modDevice = new MemoryFileDevice({
    'shared.iff': 'mod-wins',
    'unique.iff': 'unique-mod',
  });
  const stockDevice = new MemoryFileDevice({
    'shared.iff': 'stock-loses',
    'stock-only.iff': 'stock-fallback',
  });
  const registry = new OfflineFileDeviceRegistry();
  const device = new DelegatingFileDevice({ id: 0x4d4f4453, modDevice, stockDevice });
  registry.register(device);
  registry.setDefault(device.id);

  const shared = await registry.select().open('shared.iff');
  const sharedRead = await registry.select().read(shared.handle, 64);
  registry.select().close(shared.handle);
  const fallback = await registry.select().open('stock-only.iff');
  const fallbackRead = await registry.select().read(fallback.handle, 64);
  registry.select().close(fallback.handle);
  const cleanup = device.dispose();

  const modValue = Buffer.from(sharedRead.data, 'base64').toString('utf8');
  const stockValue = Buffer.from(fallbackRead.data, 'base64').toString('utf8');
  const passed = modValue === 'mod-wins'
    && shared.source === 'mod'
    && stockValue === 'stock-fallback'
    && fallback.source === 'stock'
    && cleanup.selfLinked
    && cleanup.openHandles === 0
    && registry.snapshot().deviceCount === 0;
  return {
    passed,
    mode: 'offline-memory-simulation',
    modPrecedence: shared.source === 'mod' && modValue === 'mod-wins',
    stockFallback: fallback.source === 'stock' && stockValue === 'stock-fallback',
    cleanup,
    registry: registry.snapshot(),
    hooksInstalled: false,
    processAttached: false,
  };
}

module.exports = {
  DelegatingFileDevice,
  MemoryFileDevice,
  OfflineFileDeviceRegistry,
  isAssetMiss,
  runOfflineDelegationSelfTest,
};
