'use strict';
const fs = require('node:fs');
const path = require('node:path');
// One bounded file per launcher process; no screenshots, player data or keyboard text.
class PanelDiagnostics {
  constructor({ directory, version, pid = process.pid, maxBytes = 1024 * 1024 }) {
    this.file = path.join(directory, `blacktop-panel-${version}-${pid}.jsonl`);
    this.version = version; this.pid = pid; this.maxBytes = maxBytes;
    this.bytes = 0; this.initialized = false; this.capped = false;
  }
  write(record) {
    if (this.capped) return;
    if (!this.initialized) {
      fs.mkdirSync(path.dirname(this.file), { recursive: true });
      // Append if a process ID was reused, never replace an earlier diagnostic.
      try { this.bytes = fs.statSync(this.file).size; } catch (error) { if (error.code !== 'ENOENT') throw error; }
      this.initialized = true;
    }
    const line = JSON.stringify({ ...record, time: new Date().toISOString(), version: this.version, pid: this.pid }) + '\n';
    const size = Buffer.byteLength(line);
    if (this.bytes + size > this.maxBytes) { this.capped = true; return; }
    fs.appendFileSync(this.file, line, 'utf8'); this.bytes += size;
  }
}
module.exports = { PanelDiagnostics };
