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
function playerPage(){const {doc}=dom(),listeners={},staged=[],reviews=[];let approve=async()=>true;const schema=[{key:'A'}];const window={document:doc,playerRatingSchema:schema,modLauncher:{playerSeasons:async()=>({capabilities:{existingRowsOnly:true,nonsharedOnly:true,structure:false,bulkApply:false,maxStagedRows:1}})},playerRatingChanges:()=>[],stagePlayerRatingValues:v=>staged.push(v),playerMeasurements:{readHeight:()=>200,readWingspan:()=>210,writeHeight:()=>{throw Error('unsupported height touched');},writeWingspan:()=>{throw Error('unsupported wingspan touched');}},EditorDestinationCapabilities:policy,ImportReview:{playerRows:d=>{reviews.push(d);return [];},confirm:r=>approve(r)},addEventListener:(k,f)=>(listeners[k]??=[]).push(f),dispatchEvent:e=>{for(const f of listeners[e.type]??[])f(e);}};
 for(const id of ['playerEditHeight','playerEditWingspan','playerEditFrom'])doc.getElementById(id).disabled=true;
 doc.getElementById('playerEditFirstName').value='Old';doc.getElementById('playerAudioFullName').value='10';doc.getElementById('playerEditWeight').value='200';
 const context={window,document:doc,Event:class{constructor(type){this.type=type;}},CustomEvent:class{constructor(type,v){this.type=type;this.detail=v.detail;}}};vm.runInNewContext(source('player-dna.js'),context);
 const player={source:'live-roster',ratingsRaw:[1],firstName:'Old',editCapabilities:{fullNameId:false,heightCm:false,wingspanCm:false,fromName:false}};const open=()=>window.dispatchEvent({type:'player-editor:open',detail:player});open();return {window,doc,player,open,staged,reviews,setApprove:fn=>approve=fn};}

test('legacy online canceled chooser clears opening feedback without any staging/apply',async()=>{
 const f=playerPage();delete f.player.editCapabilities;let applied=0;f.window.modLauncher.playerDna=async()=>null;f.window.applyTwokdbRatings=async()=>{applied++;};
 const panel=f.doc.getElementById('studioDNA');await panel.querySelectorAll('button').find(n=>n.id==='playerDna2kdbRead').fire('click');assert.match(panel.textContent,/Import canceled. Nothing was written/);assert.doesNotMatch(panel.textContent,/Opening the ratings source/);assert.equal(applied,0);assert.equal(f.window.isPlayerDnaBusy(),false);
});
