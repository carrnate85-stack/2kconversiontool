import test from 'node:test';import assert from 'node:assert/strict';import fs from 'node:fs';import vm from 'node:vm';
function fixture(){
 const listeners={},calls=[];let hidden=false,observer;
 const modal={classList:{contains:()=>hidden}};
 const player={source:'live-roster',recordIndex:7,firstName:'Ada',lastName:'Player',cyberfaceId:42,teamName:'Team'};
 const image='data:image/png;base64,eA==';let handler=async r=>r.choose?{id:'native-candidate',dataUrl:image}:r.action==='commit'?image:r.action==='cancel'?{canceled:true}:'stock';
 const api={playerEditorArtwork:async r=>{calls.push(r);return handler(r);}};
 const window={modLauncher:api,addEventListener:(k,f)=>(listeners[k]??=[]).push(f)};
 class MutationObserver{constructor(fn){observer=fn;}observe(){}}
 vm.runInNewContext(fs.readFileSync(new URL('../browser-overrides/manual-artwork-v2.js',import.meta.url),'utf8'),{window,document:{getElementById:()=>modal},MutationObserver});
 const emit=(k,detail)=>{for(const f of listeners[k]||[])f({detail});};emit('player-editor:open',player);
 return {window,calls,player,image,emit,setHandler:f=>handler=f,hide:()=>{hidden=true;observer();},request:kind=>({kind,key:kind==='logo'?player.teamName:JSON.stringify([player.firstName,player.lastName,player.cyberfaceId]),choose:true})};
}
test('manual portrait and logo use owned prepare then commit, lookup remains unchanged',async()=>{
 for(const kind of ['portrait','logo']){const f=fixture();assert.equal(await f.window.modLauncher.playerEditorArtwork({kind,key:'lookup'}),'stock');f.calls.length=0;
 assert.equal(await f.window.modLauncher.playerEditorArtwork(f.request(kind)),f.image);assert.deepEqual(f.calls.map(r=>r.action||'prepare'),['prepare','commit']);
 assert.deepEqual(JSON.parse(JSON.stringify(f.calls[0].destination)),{recordIndex:7,firstName:'Ada',lastName:'Player',cyberfaceId:42});assert.equal(f.calls[1].id,'native-candidate');}
});
test('late chooser after close, hidden, new visit or committed player never publishes',async()=>{
 for(const change of ['closed','hidden','open','committed']){const f=fixture();let release;f.setHandler(r=>r.choose?new Promise(resolve=>release=()=>resolve({id:'native',dataUrl:f.image})):{canceled:true});
 const pending=f.window.modLauncher.playerEditorArtwork(f.request('portrait'));
 if(change==='hidden')f.hide();else f.emit(`player-editor:${change}`,change==='closed'?undefined:change==='committed'?{player:{...f.player}}:{...f.player});
 release();assert.equal(await pending,null);assert.equal(f.calls.filter(r=>r.action==='commit').length,0);assert.equal(f.calls.filter(r=>r.action==='cancel').length,1);}
});
test('copied logical key, preview and concurrent chooser are refused before native publication',async()=>{
 const f=fixture();await assert.rejects(f.window.modLauncher.playerEditorArtwork({...f.request('portrait'),key:'wrong'}),/destination changed/);assert.equal(f.calls.length,0);
 f.emit('player-editor:open',{...f.player,source:'layout-preview'});await assert.rejects(f.window.modLauncher.playerEditorArtwork(f.request('portrait')),/live player/);assert.equal(f.calls.length,0);
 f.emit('player-editor:open',f.player);let release;f.setHandler(r=>r.choose?new Promise(resolve=>release=()=>resolve(null)):null);const pending=f.window.modLauncher.playerEditorArtwork(f.request('portrait'));
 await assert.rejects(f.window.modLauncher.playerEditorArtwork(f.request('logo')),/Another artwork chooser/);release();assert.equal(await pending,null);assert.equal(f.calls.length,1);
});
test('commit failure retains no UI success and best-effort cancels exact candidate',async()=>{
 const f=fixture();f.setHandler(async r=>{if(r.choose)return {id:'native',dataUrl:f.image};if(r.action==='commit')throw Error('Library changed');return {canceled:true};});
 await assert.rejects(f.window.modLauncher.playerEditorArtwork(f.request('portrait')),/Library changed/);assert.deepEqual(f.calls.map(r=>r.action||'prepare'),['prepare','commit','cancel']);assert.equal(f.calls[2].id,'native');
});
test('invalid receipt and canceled file dialog never submit commit',async()=>{
 for(const reply of [null,{id:'native',dataUrl:'data:text/html,evil'},{id:'native',dataUrl:'data:image/png;base64,'+'x'.repeat(2*1024*1024)}]){const f=fixture();f.setHandler(async()=>reply);try{await f.window.modLauncher.playerEditorArtwork(f.request('portrait'));}catch{}assert.equal(f.calls.filter(r=>r.action==='commit').length,0);}
});
