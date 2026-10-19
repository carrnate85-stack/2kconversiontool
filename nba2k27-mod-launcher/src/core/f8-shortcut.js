'use strict';
const { spawn } = require('node:child_process');
const {stopChild}=require('./child-lifecycle');
class F8Shortcut {
  constructor({ helperPath, getGameRoot, parentPid = process.pid, launch = spawn, onError = () => {} }) {
    Object.assign(this, { helperPath, getGameRoot, parentPid, launch, onError });
    this.state = 'stopped'; this.error = ''; this.child = null; this.generation = 0;
  }
  status() { return { mode: 'F8-only polling', state: this.state, error: this.error }; }
  register(key, callback) {
    if (key !== 'F8') return false;
    if (this.state === 'starting' || this.state === 'ready') return true;
    const generation = ++this.generation;
    this.state = 'starting'; this.error = '';
    Promise.resolve().then(() => this.getGameRoot()).then(gameRoot => {
      if (generation !== this.generation) return;
      const child = this.launch(this.helperPath, [String(this.parentPid), gameRoot], { windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
      this.child = child;
      let buffer = '', stderr = '';
      const current = () => generation === this.generation && this.child === child;
      const fail = message => {
        if (!current()) return;
        this.unregister('F8'); this.state = 'failed'; this.error = message;
        this.onError(new Error(message));
      };
      this.timer = setTimeout(() => fail('F8 listener startup timed out.'), 5000); this.timer.unref?.();
      child.stdout.on('data', chunk => {
        if (!current()) return;
        buffer += chunk.toString();
        if (buffer.length > 4096) return fail('Invalid F8 listener response.');
        let end;
        while ((end = buffer.indexOf('\n')) >= 0) {
          const line = buffer.slice(0, end).trim(); buffer = buffer.slice(end + 1);
          if (line === 'READY') { clearTimeout(this.timer); this.state = 'ready'; }
          else if (line === 'F8' && this.state === 'ready') { try { callback(); } catch (error) { this.onError(error); } }
          else return fail('Unexpected F8 listener response.');
        }
      });
      child.stderr.on('data', chunk => { stderr = (stderr + chunk.toString()).slice(-1024); });
      child.on('error', error => fail(error.message));
      child.stdin.on('error', error => { if (current()) fail(error.message); });
      child.on('exit', () => fail(stderr.trim() || 'F8 listener stopped. Reopen the game panel to retry.'));
    }).catch(error => {
      if (generation !== this.generation) return;
      this.state = 'failed'; this.error = error.message; this.onError(error);
    });
    return true;
  }
  unregister(key) {
    if (key !== 'F8') return;
    ++this.generation; clearTimeout(this.timer);
    const child = this.child; this.child = null; this.state = 'stopped';
    if (child) this.stopPromise=stopChild(child,{graceful:value=>{value.stdin.destroy();value.kill();}});
    return child;
  }
  waitForStop(){return this.stopPromise||Promise.resolve(true);}
}
module.exports = { F8Shortcut };
