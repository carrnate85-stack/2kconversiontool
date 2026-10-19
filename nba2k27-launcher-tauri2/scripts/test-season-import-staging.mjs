import {gunzipSync} from 'node:zlib';
import test from 'node:test';
import {createHash} from 'node:crypto';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const module={exports:{}};
vm.runInNewContext(fs.readFileSync(new URL('../browser-overrides/season-import-staging.js',import.meta.url),'utf8'),{module});
const {create,supported}=module.exports;
const cap={existingRowsOnly:true,nonsharedOnly:true,structure:false,bulkApply:false,maxStagedRows:1};
const row={slot:1,token:'fresh-native',year:2025};
const reply={capabilities:cap,rows:[row],plans:[{slot:1,token:'fresh-native',changes:{points:2}}]};
function fixture(){
 let who={recordIndex:4,firstName:'A',lastName:'B',cyberfaceId:1},unsaved=false;
 const calls=[],staged=[],queue=[];
 const controller=create({api:async r=>{calls.push(r);return queue.length?await queue.shift():(r.action==='teams'?{capabilities:cap}:reply);},identity:()=>who,hasUnsaved:()=>unsaved,stage:r=>{staged.push(r);unsaved=true;},onCapabilities:()=>{}});
 return {controller,calls,staged,queue,change(){who={...who,recordIndex:5};controller.invalidate();},dirty(){unsaved=true;}};
}
test('explicit native capability stages one fresh existing row and invokes no writer',async()=>{
 const f=fixture();const commit=await f.controller.prepare([{year:2025}],{preserveTotals:{2025:['plusMinus']}});
 assert.equal(f.staged.length,0);assert.deepEqual(f.calls.map(c=>c.action),['teams','review-existing-import']);
 assert.deepEqual(f.calls[1].preserveTotals,{2025:['plusMinus']});commit();assert.equal(f.staged.length,1);assert.throws(commit,/changed/);
});
test('unsupported capabilities preserve legacy fallback and malformed/multiple review cannot stage',async()=>{
 const f=fixture();f.queue.push({capabilities:{existingRowsOnly:true}});assert.equal(await f.controller.prepare([{}]),null);
 assert.equal(f.calls.length,1);assert.equal(supported({...cap,bulkApply:true}),false);
 await assert.rejects(f.controller.prepare([{},{}]),/one existing/);assert.equal(f.staged.length,0);
 const g=fixture();g.queue.push({capabilities:cap},{...reply,plans:[{slot:1,token:'stale',changes:{}}]});await assert.rejects(g.controller.prepare([{}]),/no longer matches/);assert.equal(g.staged.length,0);
});
test('player changes during async review and before commit reject stale reply without writer',async()=>{
 const f=fixture();let resolve;f.queue.push({capabilities:cap},new Promise(r=>resolve=r));
 const pending=f.controller.prepare([{}]);await new Promise(r=>setImmediate(r));f.change();resolve(reply);
 await assert.rejects(pending,/player changed/);assert.equal(f.staged.length,0);
 const g=fixture();const commit=await g.controller.prepare([{}]);g.change();assert.throws(commit,/destination changed/);assert.equal(g.staged.length,0);
});
test('existing dirty edits prevent review and stage, with no automatic save or apply action',async()=>{
 const f=fixture();f.dirty();await assert.rejects(f.controller.prepare([{}]),/Save or reset/);assert.deepEqual(f.calls.map(c=>c.action),[]);
 const g=fixture();const commit=await g.controller.prepare([{}]);g.dirty();assert.throws(commit,/destination changed/);assert.equal(g.staged.length,0);
 assert.ok([...f.calls,...g.calls].every(r=>['teams','review-existing-import'].includes(r.action)));
});
test('packaged local overrides include controller before adapter and exact local bytes',()=>{
 const assets=JSON.parse(gunzipSync(fs.readFileSync(new URL('../assets/wpf-browser.json.gz',import.meta.url))));
 const index=Buffer.from(assets.find(a=>a.path==='/renderer/index.html').bytes).toString();
 assert.ok(index.indexOf('season-import-staging.js')<index.indexOf('season-import-adapter.js'));
 for(const name of ['player-seasons.js','player-stats-import-ui.js','season-import-staging.js','season-import-adapter.js']){
  const local=fs.readFileSync(new URL('../browser-overrides/'+name,import.meta.url));
  assert.deepEqual(Buffer.from(assets.find(a=>a.path==='/renderer/'+name).bytes),local);
 }
});

test('all upstream WPF source assets remain byte-identical to pre-change bundle; only named project overrides change',()=>{
 const baseline=JSON.parse(fs.readFileSync(new URL('../browser-overrides/upstream-pins.json',import.meta.url),'utf8'));
 const current=JSON.parse(gunzipSync(fs.readFileSync(new URL('../assets/wpf-browser.json.gz',import.meta.url))));
 const overridden=new Set(['/renderer/index.html','/renderer/player-seasons.js','/renderer/player-stats-import-ui.js','/renderer/player-dna.js','/renderer/editor-presets.js','/renderer/team-staff.js','/renderer/team-rotations.js','/renderer/player23-catalog-ui.js']);
 for(const asset of baseline){
  const upstream=fs.readFileSync(new URL('../../nba2k27-mod-launcher/src'+asset.path,import.meta.url));
  assert.equal(createHash('sha256').update(upstream).digest('hex'),asset.sha256,'Upstream changed: '+asset.path);
  if(!overridden.has(asset.path))assert.deepEqual(Buffer.from(current.find(a=>a.path===asset.path).bytes),upstream,'Unexpected override: '+asset.path);
 }
 const html=Buffer.from(current.find(a=>a.path==='/renderer/index.html').bytes).toString();
 assert.ok(html.indexOf('player-seasons.js')<html.indexOf('season-import-adapter.js'));
 assert.ok(html.indexOf('player-season-import-ui.js')<html.indexOf('season-import-adapter.js'));
 assert.ok(html.indexOf('player-stats-import-ui.js')<html.indexOf('season-import-adapter.js'));
 assert.ok(html.indexOf('season-import-staging.js')<html.indexOf('season-import-adapter.js'));
});

test('normal native team bool works without legacy suffix and explicit false is never overridden',()=>{
 const teams=[{index:1,name:'Raw',displayName:'Raw',normal:true},{index:2,displayName:'Forged · Normal',normal:false},{index:3,displayName:'Legacy · Normal'},{index:4,displayName:'Historic'}];
 assert.deepEqual(Array.from(module.exports.normalTeams(teams),t=>t.index),[1,3]);
});

test('actual season load continues read when legacy capability request is unsupported, while native capability is passed through',async()=>{
 const source=fs.readFileSync(new URL('../browser-overrides/player-seasons.js',import.meta.url),'utf8');
 const load=source.slice(source.indexOf('async function load(){'),source.indexOf('async function saveRow('));
 for(const available of [false,true]){
  const calls=[],rendered=[],capabilities=[];
  const context={player:{source:'live-roster'},busy:false,generation:1,loaded:false,note:{},awardNote:{},identity:()=>({recordIndex:4}),lock:()=>{},render:r=>rendered.push(r),window:{VersionTwoSeasonStaging:module.exports,setVersionTwoSeasonCapabilities:c=>capabilities.push(c),modLauncher:{playerSeasons:async r=>{calls.push(r.action);if(r.action==='teams'){if(!available)throw Error('unsupported');return {capabilities:cap};}return [row];}}}};
  vm.createContext(context);vm.runInContext(load,context);await context.load();
  assert.deepEqual(calls,['teams','read']);assert.equal(rendered.length,1);assert.equal(context.loaded,true);assert.equal(capabilities[0],available?cap:null);
 }
});

test('only recognized legacy team-directory shape enables original structure controls',()=>{
 const resolve=module.exports.capabilitiesForDirectory;
 assert.equal(resolve({teams:[{index:0,name:'Team',displayName:'Team · Normal'}]}).structure,true);
 assert.equal(resolve({teams:[{index:0,name:'Team',displayName:'Team · Normal'}],capabilities:cap}).structure,false);
 assert.equal(resolve(null),null);assert.equal(resolve({teams:[]}),null);
 assert.equal(resolve({teams:[{index:0,name:'Team',displayName:'Team · Normal',normal:false}]}),null);
});


test('an identical fresh existing season is a truthful no-op and never stages a dirty draft',async()=>{
 const f=fixture();f.queue.push({capabilities:cap},{capabilities:cap,rows:[{...row,totals:{points:2},awards:{mvp:0}}],plans:[{slot:1,token:row.token,changes:{points:2,'award:mvp':0}}]});const commit=await f.controller.prepare([{year:2025}]);assert.equal(commit.existingRowsOnly,true);assert.equal(commit.noChanges,true);assert.equal(commit().noChanges,true);assert.equal(f.staged.length,0);
});

test('a later failed review leaves an earlier prepared local commit valid',async()=>{
 const f=fixture(),commit=await f.controller.prepare([{year:2025}]);f.queue.push({capabilities:cap},{capabilities:cap,rows:[],plans:[]});await assert.rejects(f.controller.prepare([{year:2025}]),/Refresh/);commit();assert.equal(f.staged.length,1);
});

test('actual import adapter emits no-op feedback without staging or an apply RPC',async()=>{
 const listeners={},events=[],calls=[];let staged=0;const window={VersionTwoSeasonStaging:module.exports,hasUnsavedSeasonChanges:()=>false,stageVersionTwoSeasonReview:()=>staged++,preparePlayerSeasonTransfer:()=>{throw Error('legacy fallback');},importSelectedPlayerSeasons:()=>{throw Error('legacy apply');},addEventListener:(k,f)=>listeners[k]=f,dispatchEvent:e=>events.push(e),modLauncher:{playerSeasons:async r=>{calls.push(r.action);return r.action==='teams'?{capabilities:cap}:{capabilities:cap,rows:[row],plans:[{slot:row.slot,token:row.token,changes:{}}]};}}};vm.runInNewContext(fs.readFileSync(new URL('../browser-overrides/season-import-adapter.js',import.meta.url),'utf8'),{window,CustomEvent:class{constructor(type,{detail}){this.type=type;this.detail=detail;}}});listeners['player-editor:open']({detail:{source:'live-roster',recordIndex:4,firstName:'A',lastName:'B',cyberfaceId:1}});assert.equal(await window.importSelectedPlayerSeasons([{year:2025}]),true);assert.equal(staged,0);assert.deepEqual(calls,['teams','review-existing-import']);assert.equal(events[0].type,'player-seasons:import-noop');assert.match(events[0].detail.message,/No changes/);
});

test('structural add/remove support does not bypass or disable the separate single-row import review',async()=>{
 const enabled={...cap,structure:true};assert.equal(supported(enabled),true);assert.equal(supported({...enabled,bulkApply:true}),false);assert.equal(supported({...enabled,maxStagedRows:25}),false);assert.equal(supported({...enabled,structure:'true'}),false);
 const f=fixture();f.queue.push({capabilities:enabled},{...reply,capabilities:enabled});const commit=await f.controller.prepare([{year:2025}]);assert.equal(f.staged.length,0);commit();assert.equal(f.staged.length,1);assert.deepEqual(f.calls.map(c=>c.action),['teams','review-existing-import']);
});
