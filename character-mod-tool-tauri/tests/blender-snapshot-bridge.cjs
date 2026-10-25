const assert = require('node:assert/strict');
const path = require('node:path');
const vm = require('node:vm');
const esbuild = require('../build-tools/node_modules/esbuild');
const finish = require('./test-watchdog.cjs')('Blender immutable snapshot bridge');
const source = esbuild.buildSync({entryPoints:[path.join(__dirname, '../web/tauri-bridge.ts')],
  bundle:true,write:false,platform:'browser',format:'iife',target:'es2022'}).outputFiles[0].text;
const tick = () => new Promise(resolve => setTimeout(resolve, 5));
function setup() {
  const calls = [], workers = []; let event, id = 0, generation = 0, sealedGeneration;
  const resources = {snapshotId:'owned-lease',native:true,filePath:'cmt-preview://blender-snapshot/owned/main.iff',
    options:{companionPaths:[]},player:{irisColor:'BROWN'},warnings:[]};
  const window = {__TAURI__:{event:{listen:async(_, cb)=>{event=cb;}},core:{invoke:async(_, {command,payload})=>{
    calls.push({command,payload});
    if(command==='blender-snapshot-prepare') {sealedGeneration=generation;return structuredClone(resources);}
    if(command==='blender-snapshot-fallback') return {...structuredClone(resources),native:false,filePath:'frozen/input/main.iff'};
    if(command==='blender-open') {
      assert.equal(payload.snapshotId, resources.snapshotId);
      if(generation!==sealedGeneration)throw Error('Blender sources changed while preparing. Reopen the preview.');
      event({payload:{type:'blender',state:'opened',target:payload.target,handoffId:payload.handoffId}});
      return new Promise(()=>{});
    }
    if(command==='run-swap') {generation++;return {output:payload.target};}
    if(command==='save-settings') {generation++;return {};}
    return true;
  }}}};
  class Worker {
    constructor(){workers.push(this);}
    postMessage(message){this.message=message;}
    terminate(){this.terminated=true;}
    finish(error){this.onmessage({data:{id:this.message.id,error,result:{meshes:[],textures:[],stats:{},skipped:[]}}});}
  }
  vm.runInNewContext(source,{window,Worker,console,ArrayBuffer,structuredClone,setTimeout,setInterval,clearInterval,
    crypto:{randomUUID:()=>`snapshot-owner-${++id}`}});
  return {api:window.characterTool,calls,workers,replaceDependency(){generation++;}};
}
async function check(name, run){await run();console.log('PASS '+name);}
(async()=>{
  await check('held decode plus completed Full Swap rejects the old generation at launch',async()=>{
    const env=setup(), opening=env.api.openPreviewInBlender({target:'C:/player.iff',includeTextures:true}).catch(String);
    await tick(); assert.equal(env.workers.length,1);
    await env.api.startSwap({target:'C:/player.iff'}); await tick(); await tick();
    assert(env.calls.some(x=>x.command==='run-swap'));
    env.workers[0].finish(); assert.match(await opening,/sources changed/i);
    assert.equal(env.calls.filter(x=>x.command==='blender-snapshot-release').length,1);
  });
  await check('held decode plus dependency replacement rejects at launch',async()=>{
    const env=setup(), opening=env.api.openPreviewInBlender({target:'C:/player.iff',includeHair:true}).catch(String);
    await tick();env.replaceDependency();env.workers[0].finish();assert.match(await opening,/sources changed/i);
  });
  for(const alias of ['c:/./player.iff','C:/folder/../player.iff','c:\\player.iff','\\\\?\\C:\\player.iff']) {
    await check('lexical alias is guarded: '+alias,async()=>{
      const env=setup(),opening=env.api.openPreviewInBlender({target:'C:/player.iff',includeTextures:true});await tick();
      await assert.rejects(env.api.openPreviewInBlender({target:alias}),/already open/i);
      await assert.rejects(env.api.saveBodyFit({target:alias}),/finish opening/i);
      env.workers[0].finish();await opening;
    });
  }
  await check('decoder fallback uses the same lease without rereading live resources',async()=>{
    const env=setup(),opening=env.api.openPreviewInBlender({target:'C:/player.iff',config:'custom',includeHair:true});
    await tick();env.workers[0].finish('decoder failed');await tick();
    assert.equal(env.workers.length,2);assert.equal(env.workers[1].message.payload.filePath,'frozen/input/main.iff');
    env.workers[1].finish();await opening;
    assert.equal(env.calls.find(x=>x.command==='blender-snapshot-fallback').payload.snapshotId,'owned-lease');
    assert(!env.calls.some(x=>['preview-resources','native-preview-resources'].includes(x.command)));
  });
  await check('UNC share roots and extended UNC aliases retain the same guard',async()=>{
    const env=setup(),opening=env.api.openPreviewInBlender({target:'//Server/Share/player.iff',includeHair:true});await tick();
    await assert.rejects(env.api.openPreviewInBlender({target:'\\\\?\\UNC\\SERVER\\SHARE\\folder\\..\\player.iff'}),/already open/i);
    await assert.rejects(env.api.saveBodyFit({target:'//server/share/../player.iff'}),/finish opening/i);
    env.workers[0].finish();await opening;
  });
  await check('settings change retires lease before launching and releases exactly once',async()=>{
    const env=setup(),opening=env.api.openPreviewInBlender({target:'C:/player.iff',includeTextures:true}).catch(String);
    await tick();await env.api.saveSettings({});env.workers[0].finish();assert.match(await opening,/settings changed/i);
    assert(!env.calls.some(x=>x.command==='blender-open'));assert.equal(env.calls.filter(x=>x.command==='blender-snapshot-release').length,1);
  });
  await check('no-reference open still captures and claims an immutable main',async()=>{
    const env=setup();await env.api.openPreviewInBlender({target:'C:/player.iff'});
    assert.equal(env.workers.length,0);assert.equal(env.calls.filter(x=>x.command==='blender-snapshot-prepare').length,1);
    assert.equal(env.calls.filter(x=>x.command==='blender-snapshot-release').length,1);
  });
})().catch(error=>{console.error(error);process.exitCode=1;}).finally(finish);
