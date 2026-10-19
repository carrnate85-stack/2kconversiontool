import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const source=n=>fs.readFileSync(new URL('../browser-overrides/'+n,import.meta.url),'utf8');
class Node {
 constructor(tag,doc){this.tagName=tag.toUpperCase();this.ownerDocument=doc;this.children=[];this.parentElement=null;this.dataset={};this.style={};this.listeners={};this.disabled=false;this._value='';this._text='';this.classes=new Set();this.classList={contains:x=>this.classes.has(x),toggle:(x,on)=>{if(on??!this.classes.has(x))this.classes.add(x);else this.classes.delete(x);},add:x=>this.classes.add(x),remove:x=>this.classes.delete(x)};}
 append(...nodes){for(let n of nodes){if(typeof n==='string')n={textContent:n};n.parentElement=this;this.children.push(n);}}
 prepend(...nodes){this.children.unshift(...nodes);for(const n of nodes)n.parentElement=this;}
 replaceChildren(...nodes){this.children=[];this.append(...nodes);}
 after(...nodes){this.parentElement?.append(...nodes);}
 remove(){if(this.parentElement)this.parentElement.children=this.parentElement.children.filter(x=>x!==this);}
 add(n){this.append(n);}focus(){}showModal(){this.open=true;}close(){this.open=false;}click(){return this.fire('click');}closest(tag){let n=this;while(n){if(n.tagName?.toLowerCase()===tag)return n;n=n.parentElement;}return null;}
 setAttribute(k,v){this[k]=v;}
 addEventListener(k,fn){(this.listeners[k]??=[]).push(fn);}
 async fire(k){for(const fn of this.listeners[k]??[])await fn({target:this,preventDefault(){}});}
 get textContent(){return this._text+this.children.map(n=>n.textContent??'').join('');}set textContent(v){this._text=String(v);this.children=[];}
 get value(){return this._value;}set value(v){v=String(v);this._value=this.tagName==='SELECT'&&!this.children.some(n=>n.value===v)?'':v;}
 get options(){return this.children;}get selectedOptions(){return this.children.filter(n=>n.value===this.value);}set selectedIndex(i){this._value=this.children[i]?.value??'';}
 querySelectorAll(selector){const out=[];for(const n of this.children){if(!n.tagName)continue;const matches=selector.startsWith('.')?(n.className??'').split(' ').includes(selector.slice(1)):n.tagName.toLowerCase()===selector.split(':')[0]&&(!selector.endsWith(':checked')||n.checked);if(matches)out.push(n);out.push(...n.querySelectorAll(selector));}return out;}
 querySelector(s){return this.querySelectorAll(s)[0]??null;}
}
function dom(){const nodes=new Map();const doc={createElement:t=>new Node(t,doc),createTextNode:t=>({textContent:t}),getElementById:id=>{if(!nodes.has(id)){const n=new Node(id==='playerEditFrom'?'select':'input',doc);n.id=id;nodes.set(id,n);}return nodes.get(id);}};doc.body=new Node('body',doc);return {doc,nodes,host:new Node('div',doc)};}
function moduleApi(name,extra={}){const module={exports:{}};vm.runInNewContext(source(name),{module,...extra});return module.exports;}
function catalogPage(){const {doc}=dom();for(const id of ['playerDnaImport','studioDNA','playerEditorModal','studioTabStats'])doc.body.append(doc.getElementById(id));const listeners={},calls=[],cap={existingRowsOnly:true,nonsharedOnly:true,structure:false,bulkApply:false,maxStagedRows:1};const season=(id,year)=>({id,year,totals:{gp:1},primaryTeam:{name:'Raw',city:''},awards:{}});const window={VersionTwoSeasonStaging:moduleApi('season-import-staging.js'),playerSeasonMath:{display:()=>({})},playerSeasonAwards:{fields:[]},modLauncher:{player23Catalog:async r=>{calls.push(r.action);if(r.action==='status')return {loaded:true,catalogId:'C',players:1,seasons:2};if(r.action==='search')return {total:1,players:[{id:1,name:'Source',categories:['current'],context:[]}]};if(r.action==='player')return {player:{id:1,name:'Source',context:[]},seasons:[season(11,2025),season(12,2024)],sectionCounts:{},warnings:[]};if(r.action==='selection')return {data:{seasonHistory:[{year:2025}]}};throw Error(r.action);},playerSeasons:async()=>({capabilities:cap,teams:[{index:1,name:'Raw',displayName:'Raw',normal:true},{index:2,name:'Wrong',displayName:'Wrong · Normal',normal:false}]})},importSelectedPlayerSeasons:async()=>{window.dispatchEvent({type:'player-seasons:import-noop',detail:{recordIndex:4,message:'Already matches. No changes staged.'}});return true;},addEventListener:(k,f)=>(listeners[k]??=[]).push(f),dispatchEvent:e=>{for(const f of listeners[e.type]??[])f(e);}};const context={window,document:doc,setTimeout,clearTimeout,Option:function(text,value){const n=doc.createElement('option');n.textContent=text;n.value=value;return n;}};vm.runInNewContext(source('player23-catalog-ui.js'),context);window.dispatchEvent({type:'player-editor:open',detail:{source:'live-roster',recordIndex:4,firstName:'A',lastName:'B',cyberfaceId:1}});return {doc,window,calls,find:id=>doc.body.querySelectorAll('button').find(n=>n.id===id),all:tag=>doc.body.querySelectorAll(tag)};}

const tick=()=>new Promise(r=>setImmediate(r));
test('custom replacement ignores old search response and leaves destination drafts alone',async()=>{
 const f=catalogPage(); let old,phase=0,staged=0;
 f.window.stageCatalogPlayerDna=()=>staged++;
 f.window.modLauncher.player23Catalog=async r=>{
  if(r.action==='status')return {loaded:true,catalogId:'old',players:1,seasons:0};
  if(r.action==='import'){phase=1;return {loaded:true,catalogId:'new',players:1,seasons:0,bundled:false};}
  if(r.action==='search')return phase?{total:1,players:[{id:9,name:'New Source',categories:[],context:[]}]}:new Promise(resolve=>old=resolve);
  throw Error(r.action);
 };
 await f.window.openPlayer23Import({statsOnly:false});await tick();
 await f.find('player23ImportCatalog').fire('click');
 old({total:1,players:[{id:1,name:'Old Source',categories:[],context:[]}]});await tick();
 assert.ok(f.doc.body.textContent.includes('New Source'));assert.ok(!f.doc.body.textContent.includes('Old Source'));assert.equal(staged,0);assert.ok(f.doc.body.textContent.includes('Destination drafts are unchanged'));
});
test('custom import cancellation and failure preserve current source selection',async()=>{
 for(const fails of [false,true]){const f=catalogPage();await f.window.openPlayer23Import({statsOnly:false});await tick();const prior=f.window.modLauncher.player23Catalog;const ids=[];f.window.modLauncher.player23Catalog=async r=>{if(r.action==='import'){if(fails)throw Error('Invalid custom catalog');return null;}if(r.action==='search')ids.push(r.catalogId);return prior(r);};await f.find('player23ImportCatalog').fire('click');assert.ok(f.doc.body.textContent.includes('Source'));assert.ok(!f.doc.body.textContent.includes('Source catalog replaced'));if(!fails)assert.deepEqual(ids,['C']);}
});
test('pending destination season review refuses replacement without opening chooser',async()=>{const f=catalogPage();await f.window.openPlayer23Import({statsOnly:false});await tick();f.window.hasPendingSeasonImport=()=>true;await f.find('player23ImportCatalog').fire('click');assert.equal(f.calls.includes('import'),false);assert.ok(f.doc.body.textContent.includes('Finish or discard'));});

test('committed late import invalidates source across close/reopen and reloads native generation',async()=>{
 const f=catalogPage();let finish,committed=false;const seen=[];
 f.window.modLauncher.player23Catalog=async r=>{
  if(r.action==='status')return {loaded:true,catalogId:committed?'new':'old',players:1,seasons:0};
  if(r.action==='import')return new Promise(resolve=>finish=()=>{committed=true;resolve({loaded:true,catalogId:'new',players:1,seasons:0});});
  if(r.action==='search'){seen.push(r.catalogId);return {total:0,players:[]};}throw Error(r.action);
 };
 await f.window.openPlayer23Import({statsOnly:false});await tick();
 const pending=f.find('player23ImportCatalog').fire('click');await tick();
 f.window.dispatchEvent({type:'player-editor:closed'});f.window.dispatchEvent({type:'player-editor:open',detail:{source:'live-roster',recordIndex:4,firstName:'A',lastName:'B',cyberfaceId:1}});
 await f.window.openPlayer23Import({statsOnly:false});finish();await pending;
 await f.window.openPlayer23Import({statsOnly:false});await tick();assert.equal(seen.at(-1),'new');
});
test('external source replacement expires stale search/player/selection and reopen refreshes native status',async()=>{
 for(const phase of ['search','player','selection']){
  const f=catalogPage(),prior=f.window.modLauncher.player23Catalog;let replaced=false,staged=0;const searched=[],statuses=[];const draft={firstName:'Local draft'};
  f.window.destinationDraft=draft;f.window.stageCatalogPlayerDna=()=>staged++;f.window.importSelectedPlayerSeasons=async()=>{staged++;return true;};
  f.window.modLauncher.player23Catalog=async r=>{
   if(r.action==='status'){statuses.push(replaced?'new':'C');return {loaded:true,catalogId:replaced?'new':'C',players:1,seasons:2};}
   if(r.action==='search')searched.push(r.catalogId);
   if(replaced&&r.catalogId!=='new')throw Error(phase==='player'?"Error invoking remote method 'player23Catalog': Error: Source catalog changed. Reopen the player search.":'Source catalog changed. Reopen the player search.');
   return prior(r);
  };
  await f.window.openPlayer23Import({statsOnly:true});await tick();
  const source=f.all('button').find(n=>n.className?.includes('player23-player'));
  if(phase==='selection'){await source.fire('click');const year=f.all('input').find(n=>n.value==='11');year.checked=true;await year.fire('change');}
  replaced=true;
  if(phase==='search')await f.find('player23Next').fire('click');
  else if(phase==='player')await source.fire('click');
  else await f.find('player23Stage').fire('click');
  await tick();
  assert.equal(staged,0);assert.equal(f.window.destinationDraft,draft);
  f.window.dispatchEvent({type:'player-editor:closed'});f.window.dispatchEvent({type:'player-editor:open',detail:{source:'live-roster',recordIndex:4,firstName:'A',lastName:'B',cyberfaceId:1}});
  await f.window.openPlayer23Import({statsOnly:true});await tick();assert.equal(statuses.at(-1),'new',phase);assert.equal(searched.at(-1),'new',phase);assert.equal(staged,0);
 }
});
test('ordinary search error preserves active source cache rather than discarding it',async()=>{
 const f=catalogPage(),prior=f.window.modLauncher.player23Catalog;let statuses=0,fail=false;
 f.window.modLauncher.player23Catalog=async r=>{if(r.action==='status')statuses++;if(fail&&r.action==='search')throw Error('Temporary search failure');return prior(r);};await f.window.openPlayer23Import({statsOnly:true});await tick();fail=true;await f.find('player23Next').fire('click');f.window.dispatchEvent({type:'player-editor:closed'});f.window.dispatchEvent({type:'player-editor:open',detail:{source:'live-roster',recordIndex:4,firstName:'A',lastName:'B',cyberfaceId:1}});fail=false;await f.window.openPlayer23Import({statsOnly:true});await tick();assert.equal(statuses,1);
});
