const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');
const readline = require('readline');
const root = path.resolve(__dirname, '..');
const currentExe = () => {
  const version = JSON.parse(fs.readFileSync(path.join(root, 'dist/BUILD.json'), 'utf8').replace(/^\uFEFF/, '')).version;
  return path.join(root, `dist/Character-Mod-Tool-Tauri-Preview-${version}.exe`);
};
class Rpc {
  constructor(options = {}) {
    this.child = spawn(options.exe || currentExe(), ['--serve-rpc'], { windowsHide: true, stdio: 'pipe', env: { ...process.env, ...options.env } });
    this.next = 0; this.pending = new Map(); this.events = []; this.errors = '';
    this.child.stderr.on('data', data => { this.errors = (this.errors + data).slice(-12000); });
    readline.createInterface({ input: this.child.stdout }).on('line', line => {
      let row; try { row = JSON.parse(line); } catch { return; }
      if (row.kind === 'event') { this.events.push(row.value); return; }
      const task = this.pending.get(row.id); if (!task) return;
      clearTimeout(task.timer); this.pending.delete(row.id);
      row.error ? task.reject(new Error(row.error)) : task.resolve(row.value);
    });
    this.child.on('exit', () => { for (const task of this.pending.values()) { clearTimeout(task.timer); task.reject(new Error('Owned RPC exited: ' + this.errors)); } this.pending.clear(); });
  }
  call(command, payload = {}, timeout = 300000) {
    return new Promise((resolve, reject) => {
      const id = ++this.next;
      const timer = setTimeout(() => { this.pending.delete(id); reject(new Error(command + ' timed out')); }, timeout);
      this.pending.set(id, { resolve, reject, timer });
      this.child.stdin.write(JSON.stringify({ id, command, payload }) + '\n');
    });
  }
  async close() {
    this.child.stdin.end();
    if (this.child.exitCode !== null) return;
    await new Promise(resolve => { const timer = setTimeout(() => { this.child.kill(); resolve(); }, 15000); this.child.once('exit', () => { clearTimeout(timer); resolve(); }); });
  }
}
module.exports = { Rpc, root, currentExe };
