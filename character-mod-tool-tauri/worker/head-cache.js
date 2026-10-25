import { readFile, realpath, stat } from './fs.js';
class HeadPreviewCache {
  constructor() { this.entries = new Map(); this.bytes = 0; }
  async get(filePath, decode, { clone = true } = {}) {
    const native = String(filePath).startsWith('cmt-preview://');
    const resolved = native ? filePath : await realpath(filePath);
    let key = resolved;
    if (!native) {
      const metadata = await stat(resolved);
      const bytes = await readFile(resolved);
      const digest = Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', bytes)), x => x.toString(16).padStart(2, '0')).join('');
      key = `${resolved.toLowerCase()}|${metadata.size}|${digest}`;
    }
    let entry = this.entries.get(key);
    const cached = !!entry;
    if (!entry) {
      const value = await decode(resolved);
      value.meshes?.forEach(mesh => { mesh.geometryKey ||= `${key}:${mesh.name}`; });
      const seen = new Set();
      const size = object => {
        if (!object || typeof object !== 'object' || seen.has(object)) return 0;
        seen.add(object);
        if (ArrayBuffer.isView(object)) return object.byteLength;
        if (object instanceof ArrayBuffer) return object.byteLength;
        return Object.values(object).reduce((total, child) => total + size(child), 0);
      };
      entry = { value, bytes: size(value) };
      if (entry.bytes <= 24 * 1024 * 1024) {
        while (this.entries.size >= 8 || this.bytes + entry.bytes > 24 * 1024 * 1024) {
          const oldest = this.entries.keys().next().value;
          this.bytes -= this.entries.get(oldest).bytes;
          this.entries.delete(oldest);
        }
        this.entries.set(key, entry); this.bytes += entry.bytes;
      }
    } else {
      this.entries.delete(key); this.entries.set(key, entry);
    }
    return { value: clone ? structuredClone(entry.value) : entry.value, cached };
  }
}
export { HeadPreviewCache };
