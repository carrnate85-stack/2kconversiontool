import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
describe('original browser native adapter', () => {
  it('uses original localeCompare ordering without changing RPC arguments or non-list results', async () => {
    const rows = ['Zulu', 'Álpha', 'alpha', 'Alpha', 'Émile', '9', '10'].map((name, i) => ({ name, id: String(i) }));
    const calls = [];
    const saved = { id: 'id', name: 'saved' };
    const modLauncher = { editorPresets: async r => { calls.push(r); return r.action === 'list' ? [...rows] : saved; } };
    runInNewContext(readFileSync(new URL('./browser-adapter.js', import.meta.url), 'utf8'), { window: { modLauncher, addEventListener() {} } });
    const request = { action: 'list', kind: 'equipment' };
    expect(await modLauncher.editorPresets(request)).toEqual([...rows].sort((a, b) => a.name.localeCompare(b.name)));
    expect(calls[0]).toBe(request);
    expect(await modLauncher.editorPresets({ action: 'save' })).toBe(saved);
  });
});
describe('Version 2 unmapped player measurements', () => {
  function page() {
    const listeners = {};
    const inputs = Object.fromEntries(['playerEditHeight', 'playerEditHeightInches', 'playerEditWingspan', 'playerEditWeight'].map(id => [id, { id, value: '6', disabled: false, dataset: {}, placeholder: '', title: '' }]));
    const playerMeasurements = {
      readHeight: () => { throw new Error('Enter a valid height in feet.'); },
      readWingspan: () => { throw new Error('Enter a valid wingspan in inches.'); },
    };
    const window = {
      document: { getElementById: id => inputs[id] || null },
      addEventListener: (name, fn) => { (listeners[name] ??= []).push(fn); },
      playerMeasurements,
    };
    runInNewContext(readFileSync(new URL('./browser-adapter.js', import.meta.url), 'utf8'), { window, document: window.document });
    return { window, inputs, open: detail => { for(const fn of listeners['player-editor:open']) fn({detail}); } };
  }
  it('blanks, disables and reads null only for live players without mapped measurements', () => {
    const { window, inputs, open } = page();
    open({ source: 'live-roster', heightCm: null, wingspanCm: null, weightLbs: null });
    for (const input of Object.values(inputs)) {
      expect(input.value).toBe('');
      expect(input.disabled).toBe(true);
      expect(input.dataset.rustUnmapped).toBe('1');
    }
    expect(window.playerMeasurements.readHeight(inputs.playerEditHeight, inputs.playerEditHeightInches)).toBeNull();
    expect(window.playerMeasurements.readWingspan(inputs.playerEditWingspan)).toBeNull();
    // Blank weight becomes Number('') === 0, which the backend treats as unchanged.
    expect(Number(inputs.playerEditWeight.value)).toBe(0);
    // Version 2 now maps weight: only height and wingspan stay disabled.
    for (const input of Object.values(inputs)) { input.disabled = false; input.value = '6'; }
    open({ source: 'live-roster', heightCm: null, wingspanCm: null, weightLbs: 221.5 });
    expect(inputs.playerEditWeight.disabled).toBe(false);
    expect(inputs.playerEditWeight.dataset.rustUnmapped).toBeUndefined();
    expect(inputs.playerEditHeight.disabled).toBe(true);
    expect(window.playerMeasurements.readWingspan(inputs.playerEditWingspan)).toBeNull();
    // A legacy player with real measurements keeps the original validation.
    for (const input of Object.values(inputs)) input.disabled = false;
    open({ source: 'live-roster', heightCm: 200.66, wingspanCm: 210.82, weightLbs: 220 });
    expect(inputs.playerEditHeight.dataset.rustUnmapped).toBeUndefined();
    expect(inputs.playerEditHeight.disabled).toBe(false);
    expect(() => window.playerMeasurements.readHeight(inputs.playerEditHeight, inputs.playerEditHeightInches)).toThrow();
  });
});


describe('row-specific native audio capability', () => {
  it('requires a fresh explicit capability on both open and committed rows', () => {
    const listeners = {}; const audio = {disabled:false,title:''};
    const document = {getElementById:id=>id==='playerAudioFullName'?audio:null};
    const window = {document,addEventListener:(name,fn)=>{(listeners[name]??=[]).push(fn);}};
    runInNewContext(readFileSync(new URL('./browser-adapter.js',import.meta.url),'utf8'),{window,document});
    const send=(name,detail)=>{for(const fn of listeners[name]??[])fn({detail});};
    send('player-editor:open',{editCapabilities:{fullNameId:false}}); expect(audio.disabled).toBe(true);
    send('player-editor:committed',{player:{editCapabilities:{fullNameId:true}}}); expect(audio.disabled).toBe(false);
    send('player-editor:open',{editCapabilities:{fullNameId:'true'}}); expect(audio.disabled).toBe(true);
  });
});
