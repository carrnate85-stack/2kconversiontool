'use strict';

// Shared between browser tabs: startup retries must not launch concurrent scans.
class RosterAutoAttach {
  constructor({ running, view, scan, now = Date.now }) {
    Object.assign(this, { running, view, scan, now });
    this.pending = null;
    this.nextAttempt = 0;
    this.attached = false;
  }
  poll() {
    if (this.pending) return this.pending;
    this.pending = this.check().finally(() => { this.pending = null; });
    return this.pending;
  }
  async check() {
    if (!await this.running()) {
      this.attached = false;
      return { status: 'waiting', message: 'Waiting for NBA 2K27. The roster will load automatically.' };
    }
    if (this.attached && this.view().status === 'ready') return this.view();
    if (this.now() < this.nextAttempt || this.view().status === 'scanning') {
      return { status: 'waiting', message: 'Waiting for the game roster to finish loading…' };
    }
    this.nextAttempt = this.now() + 15000;
    try {
      const result = await this.scan();
      if (result.status === 'ready' && result.players?.some(p => p.source === 'live-roster')) {
        this.attached = true;
        return result;
      }
    } catch { /* The process can exist before its roster is readable. Retry later. */ }
    return { status: 'waiting', message: 'Waiting for a readable roster. You can also use Refresh live players.' };
  }
}
module.exports = { RosterAutoAttach };
