const crypto = require('node:crypto');
const { readAssetRange, resolveAsset } = require('./independent-provider');

const MAX_OPEN_ASSETS = 512;
const MAX_PROVIDER_READ = 1024 * 1024;

class AssetProviderService {
  constructor(modsRoot, options = {}) {
    this.modsRoot = modsRoot;
    this.maxOpenAssets = Math.min(Math.max(Number(options.maxOpenAssets || MAX_OPEN_ASSETS), 1), MAX_OPEN_ASSETS);
    this.handles = new Map();
  }

  async query(logicalPath) {
    const asset = await resolveAsset(this.modsRoot, logicalPath);
    return {
      logicalPath: asset.logicalPath,
      size: asset.size,
      modifiedMs: asset.modifiedMs,
    };
  }

  async open(logicalPath) {
    if (this.handles.size >= this.maxOpenAssets) throw new Error('The provider virtual-handle limit has been reached.');
    const asset = await resolveAsset(this.modsRoot, logicalPath);
    const handle = crypto.randomUUID();
    this.handles.set(handle, {
      logicalPath: asset.logicalPath,
      size: asset.size,
      position: 0,
      sequentialQueue: Promise.resolve(),
    });
    return { handle, logicalPath: asset.logicalPath, size: asset.size, position: 0 };
  }

  requireHandle(handle) {
    const state = this.handles.get(handle);
    if (!state) throw new Error('Unknown or closed provider handle.');
    return state;
  }

  async read(handle, length, offset) {
    const state = this.requireHandle(handle);
    if (!Number.isSafeInteger(length) || length < 0 || length > MAX_PROVIDER_READ) {
      throw new Error(`Provider reads must be from 0 to ${MAX_PROVIDER_READ} bytes.`);
    }
    const positioned = offset !== undefined && offset !== null;
    const perform = async () => {
      const readOffset = positioned ? offset : state.position;
      if (!Number.isSafeInteger(readOffset) || readOffset < 0) throw new Error('Provider read offset must be a non-negative safe integer.');
      const result = await readAssetRange(this.modsRoot, state.logicalPath, readOffset, length);
      if (!positioned) state.position = readOffset + result.data.length;
      return {
        handle,
        logicalPath: state.logicalPath,
        size: state.size,
        offset: readOffset,
        bytesRead: result.data.length,
        nextPosition: state.position,
        eof: readOffset + result.data.length >= state.size,
        data: result.data.toString('base64'),
      };
    };
    if (positioned) return perform();
    const pending = state.sequentialQueue.then(perform, perform);
    state.sequentialQueue = pending.then(() => undefined, () => undefined);
    return pending;
  }

  close(handle) {
    if (!this.handles.delete(handle)) throw new Error('Unknown or already closed provider handle.');
    return { handle, closed: true, openHandles: this.handles.size };
  }

  closeAll() {
    const closed = this.handles.size;
    this.handles.clear();
    return closed;
  }

  async dispatch(request = {}) {
    switch (request.operation) {
      case 'query': return { operation: 'query', ...(await this.query(request.logicalPath)) };
      case 'open': return { operation: 'open', ...(await this.open(request.logicalPath)) };
      case 'read': return { operation: 'read', ...(await this.read(request.handle, request.length, request.offset)) };
      case 'close': return { operation: 'close', ...this.close(request.handle) };
      default: throw new Error('Unknown provider operation.');
    }
  }
}

module.exports = { AssetProviderService, MAX_OPEN_ASSETS, MAX_PROVIDER_READ };
