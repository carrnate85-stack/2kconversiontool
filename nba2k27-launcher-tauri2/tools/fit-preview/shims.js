// Browser stand-ins for the Node APIs used by WPF's fast-clothing-preview
// worker and vendor/archive-preview. Bundled ahead of the verbatim WPF
// modules by tools/build-fit-preview-runtime.mjs. Game resources arrive from
// Rust (fit_assets.rs) through the launcher page; nothing here reads disk.
'use strict';
// WPF reads process.env.FAST_FIT_QA_PROFILE for optional timing logs.
const process = { env: {} };
const hostCalls = new Map();
let hostSequence = 0;
function host(op, args, signal, transfer) {
  const id = ++hostSequence;
  return new Promise((resolve, reject) => {
    const abort = () => { hostCalls.delete(id); reject(signal.reason || Error('Preview cancelled.')); };
    if (signal?.aborted) return abort();
    signal?.addEventListener('abort', abort, { once: true });
    hostCalls.set(id, {
      resolve: value => { signal?.removeEventListener('abort', abort); resolve(value); },
      reject: error => { signal?.removeEventListener('abort', abort); reject(error); },
    });
    self.postMessage({ host: { id, op, args } }, transfer || []);
  });
}

// ---- Buffer (subset of Node's API used by the bundled WPF modules) ----
const utf8Decoder = new TextDecoder('utf-8'), utf8Encoder = new TextEncoder();
const B64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
const B64_LOOKUP = new Uint8Array(256).fill(255);
for (let i = 0; i < 64; i++) B64_LOOKUP[B64.charCodeAt(i)] = i;
B64_LOOKUP['-'.charCodeAt(0)] = 62; B64_LOOKUP['_'.charCodeAt(0)] = 63;
function toBase64(bytes) {
  let out = '';
  const chunk = [];
  for (let i = 0; i < bytes.length; i += 3) {
    const a = bytes[i], b = bytes[i + 1], c = bytes[i + 2];
    chunk.push(B64[a >> 2], B64[((a & 3) << 4) | ((b ?? 0) >> 4)],
      i + 1 < bytes.length ? B64[((b & 15) << 2) | ((c ?? 0) >> 6)] : '=',
      i + 2 < bytes.length ? B64[c & 63] : '=');
    if (chunk.length >= 32768) { out += chunk.join(''); chunk.length = 0; }
  }
  return out + chunk.join('');
}
function fromBase64(text) {
  const clean = text.replace(/[^A-Za-z0-9+/\-_]/g, '');
  const out = new NodeBuffer(Math.floor(clean.length * 3 / 4));
  let bits = 0, value = 0, n = 0;
  for (let i = 0; i < clean.length; i++) {
    value = (value << 6) | B64_LOOKUP[clean.charCodeAt(i)]; bits += 6;
    if (bits >= 8) { bits -= 8; out[n++] = (value >> bits) & 255; }
  }
  return n === out.length ? out : out.subarray(0, n);
}
class NodeBuffer extends Uint8Array {
  static from(value, encoding) {
    if (typeof value === 'string') {
      if (encoding === 'base64') return fromBase64(value);
      if (encoding === 'hex') { const out = new NodeBuffer(value.length >> 1); for (let i = 0; i < out.length; i++) out[i] = parseInt(value.substr(i * 2, 2), 16); return out; }
      if (encoding === 'ascii' || encoding === 'latin1' || encoding === 'binary') { const out = new NodeBuffer(value.length); for (let i = 0; i < value.length; i++) out[i] = value.charCodeAt(i) & 255; return out; }
      const bytes = utf8Encoder.encode(value); return new NodeBuffer(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    }
    if (Object.prototype.toString.call(value) === '[object ArrayBuffer]') return new NodeBuffer(value, encoding || 0, arguments[2] ?? value.byteLength - (encoding || 0));
    if (ArrayBuffer.isView(value)) { const out = new NodeBuffer(value.byteLength); out.set(new Uint8Array(value.buffer, value.byteOffset, value.byteLength)); return out; }
    if (Array.isArray(value) || value?.length !== undefined) { const out = new NodeBuffer(value.length); for (let i = 0; i < value.length; i++) out[i] = value[i] & 255; return out; }
    throw TypeError('Unsupported Buffer.from value');
  }
  static alloc(size, fill = 0) { const out = new NodeBuffer(size); if (fill) out.fill(fill); return out; }
  static allocUnsafe(size) { return new NodeBuffer(size); }
  static isBuffer(value) { return value instanceof NodeBuffer; }
  static byteLength(value) { return typeof value === 'string' ? utf8Encoder.encode(value).length : value.byteLength; }
  static concat(list, total) {
    total ??= list.reduce((sum, item) => sum + item.length, 0);
    const out = new NodeBuffer(total); let offset = 0;
    for (const item of list) { out.set(item.subarray(0, Math.min(item.length, total - offset)), offset); offset += item.length; if (offset >= total) break; }
    return out;
  }
  get _view() { return this.__view ??= new DataView(this.buffer, this.byteOffset, this.byteLength); }
  _check(offset, size) { if (!Number.isInteger(offset) || offset < 0 || offset + size > this.length) throw new RangeError(`The value of "offset" is out of range. It must be >= 0 and <= ${this.length - size}. Received ${offset}`); }
  readUInt8(o = 0) { this._check(o, 1); return this[o]; }
  readInt8(o = 0) { this._check(o, 1); return this._view.getInt8(o); }
  readUInt16LE(o = 0) { this._check(o, 2); return this._view.getUint16(o, true); }
  readUInt16BE(o = 0) { this._check(o, 2); return this._view.getUint16(o, false); }
  readInt16LE(o = 0) { this._check(o, 2); return this._view.getInt16(o, true); }
  readUInt32LE(o = 0) { this._check(o, 4); return this._view.getUint32(o, true); }
  readUInt32BE(o = 0) { this._check(o, 4); return this._view.getUint32(o, false); }
  readInt32LE(o = 0) { this._check(o, 4); return this._view.getInt32(o, true); }
  readFloatLE(o = 0) { this._check(o, 4); return this._view.getFloat32(o, true); }
  readDoubleLE(o = 0) { this._check(o, 8); return this._view.getFloat64(o, true); }
  readBigUInt64LE(o = 0) { this._check(o, 8); return this._view.getBigUint64(o, true); }
  readBigInt64LE(o = 0) { this._check(o, 8); return this._view.getBigInt64(o, true); }
  writeUInt8(v, o = 0) { this._check(o, 1); this[o] = v; return o + 1; }
  writeUInt16LE(v, o = 0) { this._check(o, 2); this._view.setUint16(o, v, true); return o + 2; }
  writeUInt32LE(v, o = 0) { this._check(o, 4); this._view.setUint32(o, v, true); return o + 4; }
  writeInt32LE(v, o = 0) { this._check(o, 4); this._view.setInt32(o, v, true); return o + 4; }
  writeFloatLE(v, o = 0) { this._check(o, 4); this._view.setFloat32(o, v, true); return o + 4; }
  // Node's Buffer#slice is a view, unlike Uint8Array#slice.
  slice(start, end) { return this.subarray(start, end); }
  equals(other) { if (other.length !== this.length) return false; for (let i = 0; i < this.length; i++) if (this[i] !== other[i]) return false; return true; }
  toString(encoding = 'utf8', start = 0, end = this.length) {
    start = Math.max(0, start | 0); end = Math.min(this.length, end ?? this.length);
    const bytes = this.subarray(start, end);
    if (encoding === 'base64') return toBase64(bytes);
    if (encoding === 'hex') { let out = ''; for (const b of bytes) out += b.toString(16).padStart(2, '0'); return out; }
    if (encoding === 'ascii' || encoding === 'latin1' || encoding === 'binary') {
      let out = ''; for (let i = 0; i < bytes.length; i += 8192) out += String.fromCharCode.apply(null, encoding === 'ascii' ? Array.from(bytes.subarray(i, i + 8192), b => b & 127) : bytes.subarray(i, i + 8192));
      return out;
    }
    return utf8Decoder.decode(bytes);
  }
  toJSON() { return { type: 'Buffer', data: Array.from(this) }; }
}
self.Buffer = NodeBuffer;

// ---- crypto.createHash('sha256') (synchronous, as the worker uses it) ----
const K256 = new Uint32Array([0x428a2f98,0x71374491,0xb5c0fbcf,0xe9b5dba5,0x3956c25b,0x59f111f1,0x923f82a4,0xab1c5ed5,0xd807aa98,0x12835b01,0x243185be,0x550c7dc3,0x72be5d74,0x80deb1fe,0x9bdc06a7,0xc19bf174,0xe49b69c1,0xefbe4786,0x0fc19dc6,0x240ca1cc,0x2de92c6f,0x4a7484aa,0x5cb0a9dc,0x76f988da,0x983e5152,0xa831c66d,0xb00327c8,0xbf597fc7,0xc6e00bf3,0xd5a79147,0x06ca6351,0x14292967,0x27b70a85,0x2e1b2138,0x4d2c6dfc,0x53380d13,0x650a7354,0x766a0abb,0x81c2c92e,0x92722c85,0xa2bfe8a1,0xa81a664b,0xc24b8b70,0xc76c51a3,0xd192e819,0xd6990624,0xf40e3585,0x106aa070,0x19a4c116,0x1e376c08,0x2748774c,0x34b0bcb5,0x391c0cb3,0x4ed8aa4a,0x5b9cca4f,0x682e6ff3,0x748f82ee,0x78a5636f,0x84c87814,0x8cc70208,0x90befffa,0xa4506ceb,0xbef9a3f7,0xc67178f2]);
function sha256(bytes) {
  const length = bytes.length, padded = new Uint8Array(((length + 9 + 63) >> 6) << 6);
  padded.set(bytes); padded[length] = 0x80;
  const view = new DataView(padded.buffer);
  view.setUint32(padded.length - 4, (length * 8) >>> 0); view.setUint32(padded.length - 8, Math.floor(length / 0x20000000));
  const h = new Uint32Array([0x6a09e667,0xbb67ae85,0x3c6ef372,0xa54ff53a,0x510e527f,0x9b05688c,0x1f83d9ab,0x5be0cd19]), w = new Uint32Array(64);
  for (let p = 0; p < padded.length; p += 64) {
    for (let i = 0; i < 16; i++) w[i] = view.getUint32(p + i * 4);
    for (let i = 16; i < 64; i++) {
      const a = w[i - 15], b = w[i - 2];
      const s0 = ((a >>> 7) | (a << 25)) ^ ((a >>> 18) | (a << 14)) ^ (a >>> 3);
      const s1 = ((b >>> 17) | (b << 15)) ^ ((b >>> 19) | (b << 13)) ^ (b >>> 10);
      w[i] = (w[i - 16] + s0 + w[i - 7] + s1) >>> 0;
    }
    let [a, b, c, d, e, f, g, k] = h;
    for (let i = 0; i < 64; i++) {
      const S1 = ((e >>> 6) | (e << 26)) ^ ((e >>> 11) | (e << 21)) ^ ((e >>> 25) | (e << 7));
      const t1 = (k + S1 + ((e & f) ^ (~e & g)) + K256[i] + w[i]) >>> 0;
      const S0 = ((a >>> 2) | (a << 30)) ^ ((a >>> 13) | (a << 19)) ^ ((a >>> 22) | (a << 10));
      const t2 = (S0 + ((a & b) ^ (a & c) ^ (b & c))) >>> 0;
      k = g; g = f; f = e; e = (d + t1) >>> 0; d = c; c = b; b = a; a = (t1 + t2) >>> 0;
    }
    h[0] += a; h[1] += b; h[2] += c; h[3] += d; h[4] += e; h[5] += f; h[6] += g; h[7] += k;
  }
  const out = new NodeBuffer(32), outView = new DataView(out.buffer);
  for (let i = 0; i < 8; i++) outView.setUint32(i * 4, h[i]);
  return out;
}
const cryptoShim = {
  createHash(kind) {
    if (kind !== 'sha256') throw Error('Unsupported hash');
    const parts = [];
    return {
      update(value) { parts.push(typeof value === 'string' ? NodeBuffer.from(value) : value); return this; },
      digest(encoding) { const out = sha256(NodeBuffer.concat(parts)); return encoding ? out.toString(encoding) : out; },
    };
  },
  randomUUID: () => self.crypto.randomUUID(),
};

// ---- In-memory "files": decoded resources handed to the WPF builders ----
const files = new Map();
let fileSequence = 0;
function putFile(bytes) { const name = `mem/${++fileSequence}.iff`; files.set(name, NodeBuffer.from(bytes.buffer, bytes.byteOffset, bytes.byteLength)); return name; }
const fsShim = {
  // render() prepares its cache folder; the browser cache is IndexedDB.
  async mkdir() {},
  async lstat() { return { isSymbolicLink: () => false, isFile: () => false, isDirectory: () => true }; },
  async readFile(name) { const bytes = files.get(name); if (!bytes) { const error = Error(`Preview resource is unavailable: ${name}`); error.code = 'ENOENT'; throw error; } return bytes; },
};
const pathShim = {
  join: (...parts) => parts.filter(Boolean).join('/').replace(/\/+/g, '/'),
  resolve: value => value,
  dirname: value => String(value).replace(/\/[^/]*$/, '') || '.',
  basename: (value, ext) => { const leaf = String(value).replace(/\\/g, '/').split('/').pop(); return ext && leaf.endsWith(ext) ? leaf.slice(0, -ext.length) : leaf; },
  extname: value => { const m = /\.[^./]*$/.exec(String(value)); return m ? m[0] : ''; },
};
pathShim.posix = pathShim;
pathShim.sep = '/';
// Size estimate in place of v8.serialize(value).byteLength (memory budget only).
function estimate(value, depth = 0) {
  if (value == null || depth > 64) return 8;
  if (typeof value === 'string') return value.length * 2 + 8;
  if (typeof value !== 'object') return 8;
  if (ArrayBuffer.isView(value)) return value.byteLength + 16;
  if (value instanceof ArrayBuffer) return value.byteLength + 16;
  let total = 16;
  if (value instanceof Map) { for (const [k, v] of value) total += estimate(k, depth + 1) + estimate(v, depth + 1); return total; }
  for (const key in value) total += key.length * 2 + estimate(value[key], depth + 1);
  return total;
}
const v8Shim = { serialize: value => ({ byteLength: estimate(value) }) };
const osShim = { availableParallelism: () => Math.max(2, self.navigator?.hardwareConcurrency || 4), constants: { priority: {} } };
const workerShim = {
  parentPort: {
    listeners: [],
    on(type, listener) { if (type === 'message') this.listeners.push(listener); },
    postMessage(message) { self.postMessage(message); },
  },
  workerData: null,
};
// Rust stores every member uncompressed. A test harness may supply a raw
// inflater as self.inflateRawSync to feed WPF-decoder (deflated) output.
const zlibShim = { inflateRawSync(bytes, options) { if (typeof self.inflateRawSync !== 'function') throw Error('Compressed preview members are not expected from the Rust decoder.'); return NodeBuffer.from(self.inflateRawSync(bytes, options)); } };
const unavailable = name => () => { throw Error(`${name} is not available in the browser preview runtime.`); };
const builtins = {
  'node:fs/promises': fsShim, 'node:path': pathShim, 'node:crypto': cryptoShim, 'node:v8': v8Shim,
  'node:os': osShim, 'node:worker_threads': workerShim, 'node:zlib': zlibShim,
  'node:util': { promisify: fn => fn }, 'node:child_process': { execFile: unavailable('child_process.execFile') },
};

// ---- IndexedDB stand-in for WPF's 256 MiB app-owned disk cache ----
const DB_LIMIT = 256 * 1024 * 1024;
let dbPromise = null;
function db() {
  return dbPromise ??= new Promise((resolve, reject) => {
    const request = indexedDB.open('fit-preview-cache', 1);
    request.onupgradeneeded = () => { request.result.createObjectStore('values'); request.result.createObjectStore('meta'); };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  }).catch(error => { dbPromise = null; throw error; });
}
function idb(stores, mode, work) {
  return db().then(database => new Promise((resolve, reject) => {
    const tx = database.transaction(stores, mode);
    let result;
    Promise.resolve(work(...stores.map(s => tx.objectStore(s)))).then(value => { result = value; });
    tx.oncomplete = () => resolve(result);
    tx.onerror = tx.onabort = () => reject(tx.error || Error('Preview cache transaction failed.'));
  }));
}
const req = request => new Promise((resolve, reject) => { request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error); });
const diskCache = {
  async get(id) {
    try {
      return await idb(['values', 'meta'], 'readwrite', async (values, meta) => {
        const value = await req(values.get(id));
        if (value !== undefined) { const m = await req(meta.get(id)); if (m) meta.put({ ...m, time: Date.now() }, id); }
        return value;
      });
    } catch { return undefined; }
  },
  async put(id, value, size) {
    if (size > 32 * 1024 * 1024) return;
    try { await idb(['values', 'meta'], 'readwrite', (values, meta) => { values.put(value, id); meta.put({ size, time: Date.now() }, id); }); } catch { /* optional cache */ }
  },
  async prune() {
    const entries = [];
    await idb(['meta'], 'readonly', meta => new Promise(resolve => {
      const cursor = meta.openCursor();
      cursor.onsuccess = () => { const c = cursor.result; if (!c) return resolve(); entries.push({ id: c.key, ...c.value }); c.continue(); };
      cursor.onerror = () => resolve();
    }));
    let total = entries.reduce((sum, item) => sum + item.size, 0);
    const remove = [];
    for (const item of entries.sort((a, b) => a.time - b.time)) { if (total <= DB_LIMIT) break; remove.push(item.id); total -= item.size; }
    if (remove.length) await idb(['values', 'meta'], 'readwrite', (values, meta) => { for (const id of remove) { values.delete(id); meta.delete(id); } });
  },
};
