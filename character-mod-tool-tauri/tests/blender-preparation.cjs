const assert = require('node:assert/strict');
const path = require('node:path');
const vm = require('node:vm');
const esbuild = require('../build-tools/node_modules/esbuild');
const finish = require('./test-watchdog.cjs')('Blender preparation ownership');
const baseline = process.argv.includes('--baseline'), failures = [];
const source = esbuild.buildSync({ entryPoints:[path.join(__dirname,'../web/tauri-bridge.ts')], bundle:true,
  write:false, platform:'browser', format:'iife', target:'es2022' }).outputFiles[0].text;
const tick = () => new Promise(resolve => setTimeout(resolve,10));
function setup() {
  const calls = [], workers = []; let event, id = 0;
  const window = { __TAURI__:{ event:{ listen:async(_,handler)=>{event=handler;} }, core:{
    invoke:async(_, {command,payload})=>{
      if(command==='runtime-info')return {};
      if(command==='cancel-preview'||command==='release-preview-resources'||command==='blender-snapshot-release')return true;
      if(command==='save-settings')return {};
      return new Promise((resolve,reject)=>calls.push({command,payload,resolve,reject}));
    },
  }}};
  class Worker {
    constructor(){workers.push(this);}
    postMessage(message){this.message=message;}
    terminate(){this.terminated=true;}
    finish(){this.onmessage({data:{id:this.message.id,result:{meshes:[],textures:[],stats:{},skipped:[]}}});}
  }
  vm.runInNewContext(source,{window,Worker,console,ArrayBuffer,structuredClone,setTimeout,setInterval,clearInterval,
    crypto:{randomUUID:()=>`preparation-${++id}`}});
  return {api:window.characterTool,calls,workers,
    emit:row=>event({payload:row}),
    resource(){return calls.find(row=>row.command==='blender-snapshot-prepare');},
    replyResource(){this.resource().resolve({native:true,filePath:'fixture',snapshotId:'fixture',options:{companionPaths:[]},player:{},warnings:[]});},
    opened(){const call=calls.find(row=>row.command==='blender-open');assert(call);event({payload:{type:'blender',state:'opened',target:call.payload.target,handoffId:call.payload.handoffId}});return call;},
  };
}
async function check(label,run){try{await run();}catch(error){if(!baseline)throw new Error(label+': '+error.message,{cause:error});failures.push({label,error:error.message});}}
(async()=>{
  await check('A pre-existing player write settles before reference preparation',async()=>{
    const env=setup(),saving=env.api.saveBodyFit({target:'A'});await tick();
    const opening=env.api.openPreviewInBlender({target:'A',includeTextures:true}).catch(String);await tick();
    const early=Boolean(env.resource());env.calls.find(row=>row.command==='body-fit-save').resolve({});await saving;await tick();
    env.replyResource();await tick();env.workers[0].finish();await tick();env.opened();await opening;
    assert.equal(early,false);
  });
  for(const method of ['saveBodyFit','applyChestFix','applyPreviewChestCorrection','applyHeadband','installNativeHair','changeConfig']) {
    await check(method+' cannot change the same player during reference preparation',async()=>{
      const env=setup(),opening=env.api.openPreviewInBlender({target:'C:/A.iff',includeTextures:true}).catch(String);await tick();
      const count=env.calls.length;let rejected=false;
      const edit=env.api[method]({target:'c:\\a.iff'}).catch(error=>{rejected=/Blender.*prepar|finish opening/i.test(String(error));});
      await tick();const call=env.calls[count];if(call)call.resolve({});await edit;
      env.replyResource();await tick();env.workers[0].finish();await tick();env.opened();await opening;
      assert(rejected,'Same-player mutation reached the engine during preparation');assert.equal(call,undefined);
    });
  }
  await check('Other players can still be edited during preparation',async()=>{
    const env=setup(),opening=env.api.openPreviewInBlender({target:'A',includeTextures:true});await tick();
    const saving=env.api.saveBodyFit({target:'B'});await tick();env.calls.find(row=>row.command==='body-fit-save').resolve({});await saving;
    env.replyResource();await tick();env.workers[0].finish();await tick();env.opened();await opening;
  });
  await check('The protection ends after the immutable Blender input opens',async()=>{
    const env=setup(),opening=env.api.openPreviewInBlender({target:'A',includeTextures:true});await tick();
    env.replyResource();await tick();env.workers[0].finish();await tick();env.opened();await opening;
    const saving=env.api.saveBodyFit({target:'A'});await tick();env.calls.find(row=>row.command==='body-fit-save').resolve({});await saving;
  });
  await check('Settings changes retire a prepared reference bundle before launching',async()=>{
    const env=setup(),opening=env.api.openPreviewInBlender({target:'A',includeTextures:true}).catch(String);await tick();
    env.replyResource();await tick();await env.api.saveSettings({});env.workers[0].finish();await tick();
    const launched=env.calls.find(row=>row.command==='blender-open');if(launched)env.opened();const result=await opening;
    assert.match(String(result),/settings.*changed|changed.*settings/i);assert.equal(launched,undefined);
  });
  await check('A failed preparation releases the edit guard',async()=>{
    const env=setup(),opening=env.api.openPreviewInBlender({target:'A',includeTextures:true}).catch(String);await tick();
    env.resource().reject(Error('Snapshot failure'));
    assert.match(await opening,/failure/i);
    const saving=env.api.saveBodyFit({target:'A'});await tick();env.calls.find(row=>row.command==='body-fit-save').resolve({});await saving;
  });
  if(baseline){assert(failures.length);console.log(JSON.stringify({baseline:true,failures},null,2));}
  else console.log('Blender preparation: existing writes, six guarded mutations, independent targets, opened/failure release and settings changes passed.');
})().catch(error=>{console.error(error);process.exitCode=1}).finally(finish);
