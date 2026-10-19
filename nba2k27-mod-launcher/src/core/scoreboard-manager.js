const fs = require('node:fs/promises');
const path = require('node:path');
const { execFile } = require('node:child_process');
const { promisify } = require('node:util');
const { choices: catalogChoices } = require('../data/scoreboards.json');
// Retain native inventory for resource recognition, but withdraw Open Run after
// repeated Blacktop load failures. Both launcher and F8 use this allowlist.
const pickerChoices = catalogChoices.filter(choice => !['mycareer-story', 'flashback-openrun'].includes(choice.id));
const execute = promisify(execFile);

class ScoreboardManager {
  constructor({ helperPath, directory, settings, publish = () => {}, invoke, layoutsReady = () => false }) {
    this.layoutsReady=layoutsReady;
    Object.assign(this, { helperPath, directory, settings, publish });
    this.invoke = invoke || (async (id, root, action='apply') => {
      let result;
      try { result = await execute(helperPath, [action, id, root, path.join(directory, 'last-change.json')], { windowsHide: true, timeout: 8000, maxBuffer: 65536 }); }
      catch (error) { if (!error.stdout) throw new Error('Scoreboard helper could not complete. Restart the launcher to retry.'); result = error; }
      const value = JSON.parse(result.stdout);
      if (!['pending', 'applied', 'error',...(action==='check'?['ready']:[])].includes(value.phase) || typeof value.message !== 'string') throw new Error('Invalid scoreboard helper response.');
      return value;
    });
    this.state = { selected: 'default', phase: 'idle', message: 'Uses the stock Blacktop scoreboard.', choices: pickerChoices };
    this.revision = 0;
    this.transientResourceSince = null;
    this.stopped = false;
    this.saves = Promise.resolve();
  }
  async load() {
    try {
      const saved = JSON.parse(await fs.readFile(path.join(this.directory, 'selection.json'), 'utf8'));
      const selected = saved.selected === 'mycareer-story' ? 'broadcast'
        : saved.selected === 'flashback-openrun' ? 'default' : saved.selected;
      if (pickerChoices.some(choice => choice.id === selected)) this.state.selected = selected;
    } catch (error) { if (error.code !== 'ENOENT') this.state.message = 'Saved scoreboard choice could not be read; using Default.'; }
    if (this.state.selected !== 'default') this.state = { ...this.state, phase: 'pending', message: 'Saved — waiting for Blacktop.' };
    return this.view();
  }
  view() { return { ...this.state }; }
  async refreshStatus() {
    if(this.state.phase!=='error'||this.busy)return this.view();
    const revision=this.revision;this.busy=true;
    try {
      const {gameRoot}=await this.settings();
      const result=await this.invoke(this.state.selected,gameRoot,'check');
      // Never retry a failed write implicitly. A safe read-only preflight can
      // tell the user that an explicit re-selection is now possible.
      if(revision===this.revision&&result.phase==='applied')this.emit(result);
      else if(revision===this.revision&&result.phase==='ready')this.emit({phase:'error',message:'Preflight now passes. Select this scoreboard again to apply it; no change was made.'});
    }catch{}finally{this.busy=false;}
    return this.view();
  }
  emit(value) { this.state = { ...this.state, ...value }; this.publish(this.view()); }
  select(id) {
    if (!pickerChoices.some(choice => choice.id === id)) return Promise.reject(new Error('Unknown scoreboard choice.'));
    const save = async () => {
      await fs.mkdir(this.directory, { recursive: true });
      const file = path.join(this.directory, 'selection.json');
      await fs.writeFile(file + '.tmp', JSON.stringify({ schema: 1, selected: id }));
      await fs.rename(file + '.tmp', file);
      this.revision++;
      this.transientResourceSince = null;
      this.emit({ selected: id, phase: 'pending', message: 'Saved — preparing for the next Blacktop match.' });
      void this.tick();
      return this.view();
    };
    const result = this.saves.then(save); this.saves = result.catch(() => {}); return result;
  }
  start() { this.stopped = false; void this.tick(); }
  stop() { this.stopped = true; clearTimeout(this.timer); }
  async tick() {
    if (this.stopped || this.busy) return;
    clearTimeout(this.timer);
    if (this.state.phase === 'error' || (this.state.selected === 'default' && this.state.phase !== 'pending')) return;
    this.busy = true;
    const revision = this.revision, selected = this.state.selected;
    try {
      const { gameRoot } = await this.settings();
      if(pickerChoices.find(c=>c.id===selected)?.requiresLayoutAdapter&&!this.layoutsReady()){
        if(!this.stopped&&revision===this.revision)this.emit({phase:'pending',message:'Launch the game through this test launcher to prepare the scoreboard.'});
        return;
      }
      const result = await this.invoke(selected, gameRoot);
      if (!this.stopped && revision === this.revision) {
        if(result.phase==='error'&&result.message==='Unrecognized resource table.'){
          if(this.transientResourceSince===null)this.transientResourceSince=Date.now();
          if(Date.now()-this.transientResourceSince<45000){
            this.emit({phase:'pending',message:'Waiting for game scoreboard resources.'});
            return;
          }
        }else this.transientResourceSince=null;
        this.emit({ phase: result.phase, message: result.message });
      }
    } catch (error) {
      if (!this.stopped && revision === this.revision) this.emit({ phase: 'error', message: error.message });
    } finally {
      this.busy = false;
      if (!this.stopped) { this.timer = setTimeout(() => void this.tick(), revision === this.revision ? 3000 : 0); this.timer.unref?.(); }
    }
  }
}
module.exports = { ScoreboardManager, pickerChoices };
