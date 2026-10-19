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
const policy=moduleApi('editor-destination-capabilities.js');
function playerPage(){const {doc}=dom(),listeners={},staged=[],reviews=[];let approve=async()=>true;const schema=[{key:'A'}];const window={document:doc,playerRatingSchema:schema,modLauncher:{},playerRatingChanges:()=>[],stagePlayerRatingValues:v=>staged.push(v),playerMeasurements:{readHeight:()=>200,readWingspan:()=>210,writeHeight:()=>{throw Error('unsupported height touched');},writeWingspan:()=>{throw Error('unsupported wingspan touched');}},EditorDestinationCapabilities:policy,ImportReview:{playerRows:d=>{reviews.push(d);return [];},confirm:r=>approve(r)},addEventListener:(k,f)=>(listeners[k]??=[]).push(f),dispatchEvent:e=>{for(const f of listeners[e.type]??[])f(e);}};
 for(const id of ['playerEditHeight','playerEditWingspan','playerEditFrom'])doc.getElementById(id).disabled=true;
 doc.getElementById('playerEditFirstName').value='Old';doc.getElementById('playerAudioFullName').value='10';doc.getElementById('playerEditWeight').value='200';
 const context={window,document:doc,Event:class{constructor(type){this.type=type;}},CustomEvent:class{constructor(type,v){this.type=type;this.detail=v.detail;}}};vm.runInNewContext(source('player-dna.js'),context);
 const player={source:'live-roster',ratingsRaw:[1],firstName:'Old',editCapabilities:{fullNameId:false,heightCm:false,wingspanCm:false,fromName:false}};const open=()=>window.dispatchEvent({type:'player-editor:open',detail:player});open();return {window,doc,player,open,staged,reviews,setApprove:fn=>approve=fn};}

const history=[{year:2025,team:'A'},{year:2026,team:'B'}];
const full={structure:true,bulkApply:true,existingRowsOnly:false,nonsharedOnly:false,maxStagedRows:25};
function page(caps=full){
 const f=playerPage(),calls=[],prepared=[];
 f.doc.defaultView=f.window;
 // Track chooser event subscriptions independently while retaining the page's real events.
 const emit=f.window.dispatchEvent,subscriptions=new Map();
 f.window.addEventListener=(k,fn)=>{if(!subscriptions.has(k))subscriptions.set(k,new Set());subscriptions.get(k).add(fn);};
 f.window.removeEventListener=(k,fn)=>subscriptions.get(k)?.delete(fn);
 f.window.dispatchEvent=e=>{emit(e);for(const fn of [...(subscriptions.get(e.type)||[])])fn(e);};
 f.window.modLauncher.playerSeasons=async r=>{calls.push(r);return {capabilities:caps};};
 f.window.DnaSeasonSelection=moduleApi('dna-season-selection.js',{document:f.doc});
 f.window.preparePlayerSeasonTransfer=async h=>{prepared.push(h);return ()=>({staged:true});};
 return {...f,calls,prepared,subscriptions};
}
const dna=()=>({name:'Source',info:{firstName:'New'},fields:{A:2},seasonHistory:history});
const settle=()=>new Promise(r=>setImmediate(r));
const buttons=f=>f.doc.body.querySelectorAll('button');
const click=(f,text)=>{const b=buttons(f).find(b=>b.textContent===text);assert.ok(b,'missing '+text);return b.fire('click');};
test('strict native bulk capability is queried before actual chooser; all seasons stay intact and no writes occur',async()=>{
 const f=page();let capReply;f.window.modLauncher.playerSeasons=r=>{f.calls.push(r);return new Promise(resolve=>capReply=resolve);};
 const pending=f.window.stageCatalogPlayerDna(dna());await settle();assert.equal(f.doc.body.querySelectorAll('dialog').length,0);assert.equal(f.staged.length,0);assert.equal(f.calls[0].action,'teams');
 capReply({capabilities:full});await settle();assert.ok(f.doc.body.textContent.includes('Each destination change receives a separate review'));
 await click(f,'Use all source seasons');assert.equal(await pending,true);assert.equal(f.prepared[0].length,2);assert.equal(f.staged.length,1);assert.equal(f.doc.getElementById('playerEditFirstName').value,'New');assert.equal(f.calls.length,1);assert.ok([...f.subscriptions.values()].every(s=>s.size===0));
});
test('every partial or unknown native bulk capability exposes only existing-one chooser',async()=>{
 for(const caps of [null,{}, {...full,structure:false},{...full,bulkApply:false},{...full,existingRowsOnly:true},{...full,nonsharedOnly:true},{...full,maxStagedRows:24},{...full,maxStagedRows:'25'}]){
  const f=page(caps),pending=f.window.stageCatalogPlayerDna(dna());await settle();assert.equal(buttons(f).some(b=>b.textContent==='Use all source seasons'),false);assert.ok(f.doc.body.textContent.includes('one existing, unshared season'));
  const select=f.doc.body.querySelector('select');select.selectedIndex=1;await click(f,'Use selected season');assert.equal(await pending,true);assert.equal(f.prepared[0].length,1);assert.equal(f.prepared[0][0].year,2026);
 }
});
test('capability lookup stale visit, committed identity or draft changes discard before chooser/review/staging',async()=>{
 for(const kind of ['visit','commit','draft','hidden']){
  const f=page();let resolve;f.window.modLauncher.playerSeasons=()=>new Promise(r=>resolve=r);const pending=f.window.stageCatalogPlayerDna(dna());await settle();
  if(kind==='visit'){f.window.dispatchEvent({type:'player-editor:closed'});f.open();}
  else if(kind==='commit')f.window.dispatchEvent({type:'player-editor:committed',detail:{player:{...f.player}}});
  else if(kind==='draft')f.doc.getElementById('playerEditFirstName').value='My local edit';
  else f.doc.getElementById('playerEditorModal').classList.add('hidden');
  resolve({capabilities:full});assert.equal(await pending,false);assert.equal(f.doc.body.querySelectorAll('dialog').length,0);assert.equal(f.reviews.length,0);assert.equal(f.staged.length,0);assert.equal(f.prepared.length,0);
 }
});
test('native capability failure refuses entire staging and leaves local fields unchanged',async()=>{
 const f=page();f.window.modLauncher.playerSeasons=async()=>{throw Error('Native source changed');};await assert.rejects(f.window.stageCatalogPlayerDna(dna()),/Native source changed/);assert.equal(f.reviews.length,0);assert.equal(f.staged.length,0);assert.equal(f.doc.getElementById('playerEditFirstName').value,'Old');assert.equal(f.window.isPlayerDnaBusy(),false);
});
test('actual chooser cancel and close/reopen discard late responses without season or scalar staging',async()=>{
 for(const action of ['cancel','reopen']){
  const f=page(),pending=f.window.stageCatalogPlayerDna(dna());await settle();
  if(action==='cancel')await click(f,'Cancel import');else{f.window.dispatchEvent({type:'player-editor:closed'});f.open();}
  assert.equal(await pending,false);assert.equal(f.staged.length,0);assert.equal(f.prepared.length,0);assert.equal(f.doc.getElementById('playerEditFirstName').value,'Old');assert.equal(f.doc.body.querySelectorAll('dialog').length,0);
 }
});
test('skip seasons retains supported ordinary changes and does not prepare history',async()=>{
 const f=page(),pending=f.window.stageCatalogPlayerDna(dna());await settle();await click(f,'Skip season history');assert.equal(await pending,true);assert.equal(f.prepared.length,0);assert.equal(f.staged.length,1);assert.equal(f.reviews[0].seasonHistory,undefined);
});
test('destination draft changed during bulk chooser is refused before any stage',async()=>{
 const f=page(),pending=f.window.stageCatalogPlayerDna(dna());await settle();f.doc.getElementById('playerEditFirstName').value='User draft';await click(f,'Use all source seasons');assert.equal(await pending,false);assert.equal(f.prepared.length,0);assert.equal(f.staged.length,0);assert.equal(f.doc.getElementById('playerEditFirstName').value,'User draft');
});

function statsPage(capabilities){
 const {doc}=dom(),listeners={};doc.body.append(doc.getElementById('playerSeasonImportReview'));
 const window={VersionTwoSeasonStaging:moduleApi('season-import-staging.js'),playerSeasonMath:{display:()=>({})},
  modLauncher:{basketballReferenceStats:async()=>({name:'Source',warnings:[],seasons:[{label:'2024–25',year:2025,team:'A',totals:{gp:2,points:4}}]}),playerSeasons:async()=>({capabilities,teams:[{index:1,name:'A',displayName:'A',normal:true}]})},
  addEventListener:(k,fn)=>(listeners[k]??=[]).push(fn),hasPendingSeasonImport:()=>false,isSavingPlayerSeasons:()=>false};
 vm.runInNewContext(source('player-stats-import-ui.js'),{window,document:doc,Option:function(text,value){const n=doc.createElement('option');n.textContent=text;n.value=value;return n;}});
 for(const fn of listeners['player-editor:open'])fn({detail:{source:'live-roster',recordIndex:1,firstName:'A',lastName:'B',cyberfaceId:3}});
 return {doc,button:id=>doc.body.querySelectorAll('button').find(n=>n.id===id)};
}
test('actual stats source notes describe structural review versus one-existing Save without claiming game writes',async()=>{
 for(const existing of [false,true]){
  const f=statsPage(existing?{structure:false,bulkApply:false,existingRowsOnly:true,nonsharedOnly:true,maxStagedRows:1}:full);
  await f.button('playerReferenceLoad').fire('click');
  assert.match(f.doc.body.textContent,existing?/Stage one existing, unshared season.*Save season/:/New years and shared rows receive a structural review before Apply selected seasons/);
  assert.doesNotMatch(f.doc.body.textContent,/applied and verified|written to the game/);
  const boxes=f.doc.body.querySelectorAll('input').filter(n=>n.type==='checkbox');assert.equal(boxes.length,1);assert.equal(boxes[0].checked,false);
 }
});
