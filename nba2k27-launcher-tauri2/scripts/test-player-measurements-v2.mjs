import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
function fixture(){
 const listeners={},opened=[],calls=[];let handler=async p=>({...p,heightCm:205.74,wingspanCm:218.44,editCapabilities:{heightCm:true,wingspanCm:true}});
 const document={getElementById:()=>null};
 const window={document,modLauncher:{playerMeasurements:p=>{calls.push(p);return handler(p);}},openPlayerEditor:p=>opened.push(p),addEventListener:(name,fn)=>(listeners[name]??=[]).push(fn)};
 vm.runInNewContext(fs.readFileSync(new URL('../frontend/browser-adapter.js',import.meta.url),'utf8'),{window,document});
 const emit=(name,value={})=>{for(const f of listeners[name]||[])f(value);};emit('DOMContentLoaded');
 const player={source:'live-roster',recordIndex:4,firstName:'Tayshaun',lastName:'Prince',cyberfaceId:923,editCapabilities:{heightCm:false,wingspanCm:false}};
 return {window,opened,calls,player,emit,setHandler:h=>handler=h};
}
test('native measurements arrive before the editor takes its draft baseline',async()=>{
 const f=fixture();await f.window.openPlayerEditor(f.player);
 assert.equal(f.calls.length,1);assert.equal(f.opened.length,1);assert.equal(f.opened[0].heightCm,205.74);assert.equal(f.opened[0].wingspanCm,218.44);assert.equal(f.opened[0].editCapabilities.heightCm,true);assert.equal(f.player.editCapabilities.heightCm,false);
});
test('late measurements cannot reopen a closed player or overwrite another selection',async()=>{
 for(const reason of ['close','escape','switch']){
  const f=fixture();let resolve;f.setHandler(p=>new Promise(r=>resolve=()=>r({...p,heightCm:200,editCapabilities:{heightCm:true}})));
  const pending=f.window.openPlayerEditor(f.player);
  if(reason==='switch')await f.window.openPlayerEditor({source:'catalog'});else f.emit(reason==='close'?'player-editor:closed':'keydown',{key:'Escape'});
  resolve();await pending;assert.equal(f.opened.filter(p=>p.source==='live-roster').length,0);
 }
});
test('wrong identity, failed probe and shared measurements stay unavailable',async()=>{
 for(const fault of ['identity','error','shared']){
  const f=fixture();f.setHandler(async p=>{if(fault==='error')throw Error('Read failed');return {...p,recordIndex:fault==='identity'?9:p.recordIndex,heightCm:200,editCapabilities:{heightCm:false},measurementStatus:'Shared source'};});
  await f.window.openPlayerEditor(f.player);assert.equal(f.opened[0].heightCm,null);assert.equal(f.opened[0].editCapabilities.heightCm,false);assert.ok(f.opened[0].measurementStatus);
 }
});
