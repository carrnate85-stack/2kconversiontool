const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');
const esbuild=require('../build-tools/node_modules/esbuild');
const finish=require('./test-watchdog.cjs')('preview renderer ownership');
const rendererSource=fs.readFileSync(path.join(__dirname,'../web/renderer/model-preview.js'),'utf8');
const ui=esbuild.transformSync(fs.readFileSync(path.join(__dirname,'../web/ui/app.ts'),'utf8'),{loader:'ts',target:'es2022'}).code;
const payload=(name,geometryKey=name,textureKey=name)=>({name,geometryKey,textureKey,player:{bodyFit:{}},meshes:[{sourceKind:'main',groups:[]}],textures:[]});
function renderer() {
  const requests=[],events=[],root={children:[]};let sequence=0,clears=0;
  const delegate=(kind,data)=>new Promise((resolve,reject)=>{
    const id=kind==='geometry'?++sequence:sequence;
    requests.push({kind,data,reject,finish(){
      if(id!==sequence)return reject(Error('Preview cancelled.'));
      if(kind==='geometry')root.children=data.meshes.map(()=>({userData:{}}));
      resolve({decodedTextures:0});
    }});
  });
  const context={api:{
    load:data=>delegate('geometry',data),loadArenaTextures:data=>delegate('textures',data),
    cancelLoad(){sequence++;},stats:()=>({decodedTextures:0}),setEyelashesVisible(){},stopCameraMovement(){},
  },disposeRoot(){clears++;root.children=[];},configurePreviewLighting(){}};
  const c=vm.createContext({context,modelRoot:root,viewerGlobalName:'test',transparentBackground:false,
    bodyEditor:{setBodyFit(){},initialize(){}},applyEditorDisplay(){},draw(){},setJerseyVisible(){},fitCamera(){},setGridVisible(){},
    window:{dispatchEvent:event=>events.push(event.type)},CustomEvent:class {constructor(type){this.type=type;}},performance,previewGarmentKind:()=>null,
  });
  vm.runInContext(rendererSource.slice(rendererSource.indexOf('let sourcePayload'),rendererSource.indexOf('function visibleHit')),c);
  vm.runInContext('function setStudioLighting(){window.lightingOwner=sourcePayload?.name;}',c);
  vm.runInContext(rendererSource.slice(rendererSource.indexOf('function preparePayload('),rendererSource.indexOf('window[viewerGlobalName] =')),c);
  return {c,requests,events,root,context,read:()=>vm.runInContext('({sourcePayload,pendingLoad})',c),clears:()=>clears,
    async warm(data){const loading=c.load(data);requests.at(-1).finish();return loading;}};
}
function frontend(loading=true) {
  const elements=new Map(),state={workingPlayer:{path:'A'},preview:{requestId:4,loading,dirty:false,loadedPath:'A',bodyFitTarget:'A',
    bodyFitDraft:{x:2},bodyFitInvalidInputs:new Map([['y','-']]),bodyFitBusy:true}};
  const queued=[];let clears=0,cancels=0,reloads=0;
  const element=id=>{if(!elements.has(id))elements.set(id,{hidden:false,querySelector(){return this;}});return elements.get(id);};
  const c=vm.createContext({state,$:element,basename:x=>x,resetPreviewDetails(){},renderWorkingPlayer(){},renderPreviewControls(){},
    queueMicrotask:callback=>queued.push(callback),loadStagedPreview(){reloads++;},
    window:{characterTool:{cancelStagedPreview(){cancels++;return Promise.resolve();}},modelPreview:{clear(){clears++;}}}});
  vm.runInContext(ui.slice(ui.indexOf('function samePlayerPath('),ui.indexOf('function currentPreviewBodyFit(')),c);
  return {c,state,element,queued,reloads:()=>reloads,counts:()=>({clears,cancels})};
}
(async()=>{
  const cold=renderer(),data=payload('first'),loading=cold.c.load(data);
  assert.equal(cold.read().sourcePayload,null,'Pending payload was advertised as committed');
  cold.requests[0].finish();await loading;assert.equal(cold.read().sourcePayload.name,'first');
  const reused=await cold.c.load(payload('caption','first','first'));
  assert.equal(cold.requests.length,1);assert.equal(reused.reusedGeometry,1);assert.equal(cold.read().sourcePayload.name,'caption');

  const pending=renderer();await pending.warm(payload('old'));
  const a=pending.c.load(payload('A','shared','same'));
  const b=pending.c.load(payload('B','shared','same'));
  assert.equal(pending.requests.length,3,'Pending geometry entered the completed reuse path');
  pending.requests[1].finish();assert.equal((await a).cancelled,true);
  pending.requests[2].finish();await b;assert.equal(pending.read().sourcePayload.name,'B');
  assert.equal(pending.events.filter(x=>x==='model-preview:loaded').length,2);

  const texture=renderer();await texture.warm(payload('base'));
  const update=texture.c.load(payload('update','base','new'));
  assert.equal(texture.requests[1].kind,'textures');assert.equal(texture.read().sourcePayload.name,'base');
  texture.requests[1].reject(Error('Controlled texture failure'));await assert.rejects(update,/Controlled texture failure/);
  assert.equal(texture.read().sourcePayload,null);assert.equal(texture.root.children.length,0);
  const retry=texture.c.load(payload('retry','base','new'));assert.equal(texture.requests.at(-1).kind,'geometry');
  texture.requests.at(-1).finish();await retry;

  const cleared=renderer(),old=cleared.c.load(payload('cancelled'));cleared.c.clear();
  cleared.requests[0].finish();assert.equal((await old).cancelled,true);assert.equal(cleared.read().sourcePayload,null);
  assert.equal(cleared.events.includes('model-preview:loaded'),false);
  const inner=renderer(),cancelled=inner.c.load(payload('inner'));inner.context.api.cancelLoad();
  inner.requests[0].finish();assert.equal((await cancelled).cancelled,true);assert.equal(inner.read().sourcePayload,null);
  assert(inner.clears()>0);
  const lighting=renderer();vm.runInContext('studioLighting=false',lighting.c);
  await lighting.warm(payload('lit-player'));assert.equal(lighting.c.window.lightingOwner,'lit-player');

  const active=frontend();assert(active.c.invalidatePlayerRender('a'));
  assert.deepEqual(active.counts(),{clears:1,cancels:1});assert.equal(active.state.preview.requestId,5);
  assert(!active.state.preview.loading && !active.state.preview.loadedPath && active.state.preview.dirty);
  assert.equal(active.state.preview.bodyFitTarget,'A');assert.equal(active.state.preview.bodyFitDraft.x,2);
  assert.equal(active.state.preview.bodyFitInvalidInputs.get('y'),'-');assert(active.state.preview.bodyFitBusy);
  const other=frontend();assert.equal(other.c.invalidatePlayerRender('B'),false);
  assert.equal(other.state.preview.requestId,4);assert.deepEqual(other.counts(),{clears:0,cancels:0});
  const idle=frontend(false);assert(idle.c.invalidatePlayerRender('A'));assert(idle.state.preview.dirty);
  assert.equal(idle.state.preview.loadedPath,'A');assert.deepEqual(idle.counts(),{clears:0,cancels:0});
  const visible=frontend();visible.state.activeView='preview';visible.state.workingGeneration=1;visible.state.preview.bodyFitBusy=false;
  visible.c.invalidatePlayerRender('A');assert.equal(visible.queued.length,1);visible.queued[0]();assert.equal(visible.reloads(),1);
  for(const change of ['stage','request','view','loading','reset']) {
    const stale=frontend();stale.state.activeView='preview';stale.state.workingGeneration=1;stale.state.preview.bodyFitBusy=false;
    stale.c.invalidatePlayerRender('A');
    if(change==='stage')stale.state.workingGeneration++;
    if(change==='request')stale.state.preview.requestId++;
    if(change==='view')stale.state.activeView='body';
    if(change==='loading')stale.state.preview.loading=true;
    if(change==='reset')stale.state.resetting=true;
    stale.queued[0]();assert.equal(stale.reloads(),0,'Obsolete automatic reload: '+change);
  }
  console.log('Renderer ownership: commit/reuse boundaries, latest generation, failures, cancellation, lighting and target-scoped UI draft preservation passed.');
})().catch(error=>{console.error(error);process.exitCode=1}).finally(finish);
