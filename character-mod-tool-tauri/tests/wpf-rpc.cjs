const fs = require('node:fs');
const path = require('node:path');
const readline = require('node:readline');
const { spawn } = require('node:child_process');
const production = path.resolve(__dirname, '../../character-mod-tool-webview2/dist/Character-Mod-Tool-WebView2-0.1.31/App');
class WpfRpc {
  constructor(data, settings) {
    fs.mkdirSync(path.join(data, 'CharacterModTool'), { recursive: true });
    fs.writeFileSync(path.join(data, 'CharacterModTool/settings.json'), JSON.stringify(settings));
    this.next = 0; this.pending = new Map(); this.waiters = new Set(); this.pushes = []; this.log = '';
    this.ready = new Promise((resolve, reject) => { this.onReady = resolve; this.onFailure = reject; });
    this.child = spawn(path.join(production, 'Backend/node.exe'), [path.join(production, 'worker/host.cjs')], {
      windowsHide: true, stdio: 'pipe', env: { ...process.env, WPF_APP: production, WPF_REFERENCE: path.join(production, 'Reference'),
        WPF_PROJECT_ROOT: path.join(production, 'Backend'), WPF_PACKAGED: '1', WPF_PARENT_PID: String(process.pid), CMT_WEBVIEW2_VERSION: '0.1.31',
        LOCALAPPDATA: data, APPDATA: data, TEMP: data, TMP: data, PYTHONDONTWRITEBYTECODE: '1' }
    });
    this.child.stderr.on('data', data => { this.log += String(data); });
    readline.createInterface({ input: this.child.stdout }).on('line', line => {
      let row; try { row = JSON.parse(line); } catch { this.log += line + '\n'; return; }
      if (row.kind === 'ready') this.onReady(row);
      else if (row.kind === 'fatal') this.onFailure(new Error(row.message));
      else if (row.kind === 'host') this.child.stdin.write(JSON.stringify({ kind: 'host-reply', id: row.id, value: true }) + '\n');
      else if (row.kind === 'push') {
        this.pushes.push(row);
        for (const waiter of this.waiters) if (waiter.channels.includes(row.channel) && waiter.predicate(row.value)) waiter.finish(row);
      } else if (row.kind === 'reply') {
        const task = this.pending.get(row.id); if (!task) return;
        this.pending.delete(row.id); clearTimeout(task.timer);
        row.error ? task.reject(new Error(row.error)) : task.resolve(row.value);
      }
    });
    this.child.on('exit', () => {
      this.onFailure(new Error('WPF test worker exited: ' + this.log.slice(-3000)));
      for (const task of this.pending.values()) { clearTimeout(task.timer); task.reject(new Error('WPF worker exited')); }
      this.pending.clear();
      for (const waiter of this.waiters) waiter.fail(new Error('WPF worker exited before its event'));
    });
  }
  async call(channel, ...args) {
    await this.ready;
    return new Promise((resolve, reject) => {
      const id = ++this.next;
      const timer = setTimeout(() => { this.pending.delete(id); reject(new Error(channel + ' timed out')); }, 600000);
      this.pending.set(id, { resolve, reject, timer });
      this.child.stdin.write(JSON.stringify({ kind: 'invoke', id, session: 'parity-test', channel, args }) + '\n');
    });
  }
  async waitForPush(channels, predicate) {
    const existing = this.pushes.find(row => channels.includes(row.channel) && predicate(row.value));
    if (existing) return existing;
    return new Promise((resolve, reject) => {
      const waiter = { channels, predicate, finish: row => { clearTimeout(timer); this.waiters.delete(waiter); resolve(row); }, fail: error => { clearTimeout(timer); this.waiters.delete(waiter); reject(error); } };
      const timer = setTimeout(() => waiter.fail(new Error('WPF conversion event timed out')), 600000);
      this.waiters.add(waiter);
    });
  }
  async close() {
    if (this.child.exitCode !== null) return;
    this.child.stdin.write(JSON.stringify({ kind: 'shutdown' }) + '\n');
    await new Promise(resolve => { const timer = setTimeout(() => { this.child.kill(); resolve(); }, 15000); this.child.once('exit', () => { clearTimeout(timer); resolve(); }); });
  }
}
module.exports = { WpfRpc };
