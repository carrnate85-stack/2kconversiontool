const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const esbuild = require('../build-tools/node_modules/esbuild');
const { chromium } = require('C:/Users/carrn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright');
const finish = require('./test-watchdog.cjs')('direct player staging');
const root = path.resolve(__dirname, '..');
const renderer = path.resolve(root, '../character-mod-tool-webview2/Reference/src/renderer');
const app = esbuild.transformSync(fs.readFileSync(path.join(root, 'web/ui/app.ts'), 'utf8'),
  { loader: 'ts', target: 'es2022' }).code.replace(/bootstrap\(\);\s*$/, '');
const bridge = esbuild.buildSync({ entryPoints: [path.join(root, 'web/tauri-bridge.ts')], bundle: true,
  write: false, platform: 'browser', format: 'iife', target: 'es2022' }).outputFiles[0].text;
const tick = () => new Promise(resolve => setTimeout(resolve, 10));
async function uiChecks() {
  const browser = await chromium.launch({ channel: 'chrome', headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.route('**/*', route => route.abort());
    await page.setContent(fs.readFileSync(path.join(renderer, 'index.html'), 'utf8')
      .replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi, '')
      .replace(/<meta[^>]*http-equiv="Content-Security-Policy"[^>]*>/i, '')
      .replace(/<link\b[^>]*>/gi, ''));
    for (const file of ['styles.css', 'blue-theme.css', 'configs.css'])
      await page.addStyleTag({ content: fs.readFileSync(path.join(renderer, file), 'utf8') });
    await page.addStyleTag({ content: fs.readFileSync(path.join(root, 'web/ui/preview-layout.css'), 'utf8') });
    await page.addScriptTag({ content: fs.readFileSync(path.join(renderer, 'lucide.min.js'), 'utf8') });
    await page.addScriptTag({ content: app });
    const results = await page.evaluate(async () => {
      const check = (value, message) => { if (!value) throw Error(message); };
      const pending = [], inspected = [], extracted = [], logs = [];
      document.getElementById('startupLoading').remove();
      const storage = new Map();
      Object.defineProperty(window, 'localStorage', {value:{
        getItem:key=>storage.get(key)||null, setItem:(key,value)=>storage.set(key,value), removeItem:key=>storage.delete(key)
      }});
      let configReason = '';
      window.characterTool = {
        stagePlayer: request => new Promise((resolve, reject) => pending.push({request, resolve, reject})),
        inspectCharacter: async filePath => { inspected.push(filePath); return {path:filePath}; },
        cancelStagedPreview: async () => {},
        extractPlayer: async request => { extracted.push(request); return {
          mainPath:'C:/session/png2.iff', gameLabel:'NBA 2K27', warnings:[]}; },
        loadCatalog: async () => ({rows:[], gameRoot:'C:/Game'})
      };
      window.configsEditor = {refresh:async()=>{}, renderPreviewSelector:()=>{}, stagingBlockReason:()=>configReason};
      addLog = (message, kind) => logs.push({message,kind});
      loadStagedPreview = async () => { state.preview.loading = false; state.preview.dirty = false; };
      const switchView = setActiveView;
      check(state.activeView === 'player', 'New sessions did not default to Select Player');
      setupStagePlayer();
      switchView('player');
      check(!document.getElementById('selectPlayerView').hidden && document.getElementById('fullSwapView').hidden, 'Select Player view routing failed');
      check(document.querySelector('.player-start-or').textContent === 'OR', 'Missing OR divider');
      check(document.getElementById('selectPlayerTab').textContent.trim()==='Select Player', 'Select Player choice label changed');
      check(document.getElementById('fullSwapChoice').textContent.trim()==='Convert Player', 'Convert Player choice label changed');
      const row=document.querySelector('.topbar').getBoundingClientRect();
      const choiceBounds=[document.getElementById('selectPlayerTab'),document.querySelector('.player-start-or'),document.getElementById('fullSwapChoice')].map(x=>x.getBoundingClientRect());
      check(Math.abs((choiceBounds[0].left+choiceBounds[2].right)/2-(row.left+row.right)/2)<2, 'Player choices are not centered');
      check(!document.querySelector('.sidebar #selectPlayerTab') && document.querySelector('.topbar #playerChoices'), 'Choices are not in the header');
      check(document.getElementById('playerChoices').nextElementSibling.classList.contains('topbar-actions'), 'Player choices are not centered before header actions');
      check(document.getElementById('fullSwapTab').textContent.trim()==='Player', 'Sidebar is not a single Player tab');
      check(document.getElementById('fullSwapTab').querySelector('svg'), 'Player sidebar icon was replaced by text');
      document.getElementById('fullSwapChoice').click();
      check(document.getElementById('selectPlayerView').hidden && !document.getElementById('fullSwapView').hidden, 'Full Swap view routing failed');
      switchView('configs');
      check(document.getElementById('playerChoices').hidden, 'Choices leaked into an editing tab');
      document.getElementById('fullSwapTab').click();
      check(state.activeView==='full-swap' && !document.getElementById('playerChoices').hidden, 'Player tab forgot chosen workflow');
      document.getElementById('selectPlayerTab').click();
      check(state.activeView === 'player', 'Sidebar Select Player did not route');
      window.showPlayerEntry = () => { document.getElementById('stagePlayerDialog').close(); switchView('player'); };
      setActiveView = view => { state.activeView = view; };
      const button = document.getElementById('stagePlayer');
      check(button.disabled, 'Startup should block stage');
      state.bootstrap = {games:{'NBA 2K27':true}};
      updateReadiness();
      check(!button.disabled && button.querySelector('svg'), 'Stage action missing or disabled');
      check(!state.source && !state.target, 'Fixture unexpectedly has conversion selections');
      openStagePlayer();
      check(document.getElementById('stagePlayerDialog').open, 'Picker did not open');
      await stagePlayerSelection(async()=>null);
      check(!pending.length && !state.workingPlayer && !button.disabled, 'Cancel changed player or stayed busy');
      check(!document.getElementById('stagePlayerStatus').textContent, 'Cancel left stale loading text');
      const start = () => stagePlayerSelection(async()=>({filePath:'C:/original/png1.iff'}));
      const first = start();
      await Promise.resolve();
      check(pending.length === 1 && button.disabled && button.getAttribute('aria-busy') === 'true', 'Staging spinner/lock missing');
      check([...document.querySelectorAll('.sidebar .nav-item')].every(tab=>tab.disabled), 'Navigation permitted edits while staging');
      check([...document.querySelectorAll('.player-start-choices button')].every(tab=>tab.disabled), 'Mode switch permitted edits while staging');
      await start(); check(pending.length === 1, 'Duplicate stage reached native bridge');
      pending[0].resolve({mainPath:'C:/output/edit1/png1.iff',companions:3,warnings:[]});
      await first;
      check(state.workingPlayer.path === 'C:/output/edit1/png1.iff', 'Original staged instead of copy');
      check(state.body.targetPlayer.path === state.workingPlayer.path && !state.body.sourcePlayer, 'Body requires conversion');
      check(!state.source && !state.target && !state.workingSource && !state.workingBaseline, 'Stage changed conversion inputs');
      check(state.activeView === 'preview' && !button.disabled && !state.selectionBusy, 'Success did not release in Preview');
      const saved = state.workingPlayer.path;
      const failure = start(); await Promise.resolve();
      pending.at(-1).reject(Error('fixture copy failure')); await failure;
      check(state.workingPlayer.path === saved && !button.disabled, 'Failed copy replaced stage or stayed busy');
      check(logs.some(x=>x.message.includes('fixture copy failure')), 'Missing failure diagnostic');
      for (const [set,clear] of [
        [()=>state.body.dirtyFields.add('x'),()=>state.body.dirtyFields.clear()],
        [()=>state.preview.bodyFitInvalidInputs.set('x','-'),()=>state.preview.bodyFitInvalidInputs.clear()],
        [()=>state.activeJob='job',()=>state.activeJob=''],
        [()=>state.headband.applying=true,()=>state.headband.applying=false],
        [()=>state.nativeHair.loading=true,()=>state.nativeHair.loading=false],
        [()=>previewBlenderSessions.set('session',{}),()=>previewBlenderSessions.clear()],
        [()=>configReason='Save configuration changes',()=>configReason='']
      ]) {
        set(); updateStagePlayerControls(); check(button.disabled, 'Unsafe replacement allowed');
        const count=pending.length; await start(); check(pending.length===count,'Blocked operation reached backend');
        clear(); updateStagePlayerControls(); check(!button.disabled,'Guard did not release');
      }
      const stale=start(); await Promise.resolve();
      state.workingRequestId++; state.workingPlayer={path:'C:/newer.iff',name:'newer'};
      pending.at(-1).resolve({mainPath:'C:/output/stale.iff',companions:0,warnings:[]}); await stale;
      check(state.workingPlayer.path==='C:/newer.iff' && !inspected.includes('C:/output/stale.iff'), 'Stale copy replaced newer stage');
      await openManifest('stage');
      check([...document.querySelectorAll('[data-game]')].filter(x=>!x.hidden).every(x=>x.dataset.game==='NBA 2K27'), 'Legacy manifest exposed as editable');
      state.selectedManifest={png:'2'}; state.manifestGame='NBA 2K27';
      const manifest=useSelectedManifestPlayer();
      await Promise.resolve(); await Promise.resolve();
      check(extracted.at(-1).role==='working' && extracted.at(-1).gameLabel==='NBA 2K27','Wrong manifest extraction');
      check(pending.at(-1).request.filePath==='C:/session/png2.iff','Manifest bypassed durable copy');
      pending.at(-1).resolve({mainPath:'C:/output/edit2/png2.iff',companions:2,warnings:[]}); await manifest;
      check(state.workingPlayer.path==='C:/output/edit2/png2.iff' && !document.getElementById('manifestDialog').open,'Manifest stage failed: '+JSON.stringify({path:state.workingPlayer.path,logs}));
      openStagePlayer();
      return ['file and manifest stage without conversion', 'durable copy committed to Preview and Body',
        'cancel/failure retain old player', 'draft, busy, Blender and duplicate guards', 'stale completion rejected'];
    });
    for (const width of [1280,900]) {
      await page.setViewportSize({width,height:900});
      const bounds = await page.evaluate(() => {
        const ids=['stagePlayerDialog','stagePlayerFile','stagePlayerManifest','stagePlayerClose'];
        return ids.map(id=>{const r=document.getElementById(id).getBoundingClientRect();return {id,x:r.x,right:r.right,y:r.y,bottom:r.bottom};});
      });
      for (const b of bounds) assert.ok(b.x>=0 && b.right<=width && b.y>=0 && b.bottom<=900, JSON.stringify(b));
    }
    await page.evaluate(()=>window.showPlayerEntry());
    await page.addScriptTag({content:esbuild.transformSync(fs.readFileSync(path.join(root,'web/ui/sidebar-ui.ts'),'utf8'),{loader:'ts'}).code});
    await page.setViewportSize({width:1280,height:850});
    await page.screenshot({path:path.join(root,'artifacts/select-player-entry.png'),animations:'disabled'});
    assert.deepEqual(errors,[]);
    return results.concat('header-centered Select Player OR Convert Player choices and dialog fit 1280px and 900px windows');
  } finally { await browser.close(); }
}
async function bridgeChecks() {
  const calls=[],pending=[];
  const window={__TAURI__:{event:{listen:async()=>{}},core:{invoke:async(_,{command,payload})=>{
    calls.push(command);
    if(command==='stage-player') return new Promise((resolve,reject)=>pending.push({resolve,reject,payload}));
    if(command==='runtime-info')return {};
    if(['cancel-preview','reset','release-preview-resources'].includes(command))return true;
    throw Error('Unexpected command '+command);
  }}}};
  vm.runInNewContext(bridge,{window,console,structuredClone,setTimeout,setInterval,clearInterval});
  const api=window.characterTool;
  const staging=api.stagePlayer({filePath:'C:/original.iff'}); await tick();
  const reset=api.resetSession(); await tick();
  assert.ok(!calls.includes('reset'),'Reset deleted source before staging drained');
  pending[0].resolve({mainPath:'C:/output/copy.iff'}); await staging; await reset;
  assert.ok(calls.includes('reset'));
  const failed=api.stagePlayer({filePath:'C:/bad.iff'}); await tick();
  pending[1].reject(Error('failed')); await assert.rejects(failed,/failed/);
  await api.resetSession();
  return 'bridge reset drains staging and releases rejected requests';
}
(async()=>console.log(JSON.stringify({headless:true,checks:[...await uiChecks(),await bridgeChecks()]},null,2)))()
  .catch(error=>{console.error(error);process.exitCode=1;}).finally(finish);

