'use strict';
// Read-only detection is independent of editable snapshots and their tokens.
class BlacktopAutoOpen {
  constructor({ settings, detect, panel, prepareLineup, onObservation, onError, interval = 100 }) {
    Object.assign(this, { settings, detect, panel, prepareLineup, onObservation, onError, interval });
    this.lineupSignature = null;
    this.session = null; this.entered = false; this.inside = 0; this.outside = 0;
    this.stopped = true;
  }
  observe(value) {
    if (typeof value?.selection !== 'boolean' || typeof value.session !== 'string') return;
    if (value.session !== this.session) {
      this.session = value.session; this.entered = false; this.inside = 0; this.outside = 0;
    }
    if (!value.selection) {
      this.inside = 0;
      if (++this.outside >= 2) { this.entered = false; if(this.panel.wanted)this.panel.hide(); }
      return;
    }
    this.outside = 0;
    // Manual opening also counts as this visit's opening; never fight dismissal.
    if (this.panel.wanted) this.entered = true;
    if (value.lineupReady !== true) {
      this.inside = 0;
      // Re-arm only on an explicit incomplete lineup, not a loading model or
      // unknown read. Dismissing a still-complete lineup must remain respected.
      const required = value.requiredPlayers, selected = value.selectedPlayers;
      if (Number.isInteger(required) && required >= 2 && required <= 10 && required % 2 === 0
          && Number.isInteger(selected) && selected >= 0 && selected < required) {
        this.entered = false;
        if(this.panel.wanted)this.panel.hide();
      }
      return;
    }
    this.inside++;
    if (this.inside >= 2 && !this.entered && value.foreground === true) {
      this.entered = true;
      this.panel.show();
    }
  }
  start() { if (!this.stopped) return; this.stopped = false; void this.tick(); }
  async tick() {
    let nextDelay = 2000;
    let stage = 'settings';
    try {
      const settings = await this.settings();
      if (!this.stopped && (settings.autoOpenBlacktop === true || this.panel?.wanted || this.prepareLineup)) {
        stage = 'detect';
        const value = await this.detect(settings.gameRoot);
        if (!this.stopped) {
          // Ball observation must never hold up the selection-time outfit handoff.
          if(this.onObservation)void Promise.resolve().then(()=>this.onObservation(value)).catch(error=>this.onError?.('ball-observation',error));
          this.panel?.setGameForeground?.(value.foreground);
          const signature=value.selection===true&&value.playersReady===true&&Array.isArray(value.lineupPlayers)?JSON.stringify([value.session,value.lineupPlayers]):null;
          stage = 'prepare-lineup';
          if(signature&&signature===this.lineupSignature&&this.prepareLineup)await this.prepareLineup(settings.gameRoot,value);
          this.lineupSignature=signature;
          if(this.stopped)return;
          stage = 'panel';
          if(settings.autoOpenBlacktop === true)this.observe(value);
          // Fast only on selection; one helper at a time, never overlapping.
          if (value.selection === true) nextDelay = this.interval;
        }
      }
    } catch (error) { this.onError?.(stage,error); /* Unknown/transient state must not re-arm a dismissed panel. */ }
    finally {
      if (!this.stopped) { this.timer = setTimeout(() => this.tick(), nextDelay); this.timer.unref?.(); }
    }
  }
  stop() { this.stopped = true; clearTimeout(this.timer); }
}
module.exports = { BlacktopAutoOpen };
