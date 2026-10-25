const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const esbuild = require('../build-tools/node_modules/esbuild');
const finish = require('./test-watchdog.cjs')('preview draft commit');
const source = esbuild.transformSync(fs.readFileSync(path.join(__dirname,'../web/ui/app.ts'),'utf8'), {loader:'ts',target:'es2022'}).code;
const baseline = process.argv.includes('--baseline'), failures=[];
const fit = {chestTHIN:.5,'Spine2.tx':0};

function setup({draft={},loadedPath='',otherTarget=false}={}) {
  const state={resetting:false,workingPlayer:{path:'A',name:'A'},body:{targetPlayer:{path:otherTarget?'B':'A'},
    targetBodyFit:{...fit},targetDraft:{...draft},dirtyFields:new Set(Object.keys(draft)),invalidFields:new Set()},preview:{loadedPath,bodyFitTarget:loadedPath,
    bodyFit:{...fit},bodyFitDraft:{},bodyFitDirtyFields:new Set(),bodyFitInvalidInputs:new Map(),requestId:0,dirty:true}};
  const payload={player:{bodyFit:{...fit}},meshes:[{}],stats:{modelsDecoded:1,companions:0},skipped:[]};
  let begin,resolve,reject,pending=true,visual=null;
  const started=new Promise(done=>begin=done),gpu=new Promise((done,fail)=>{resolve=done;reject=fail;});
  const calls=[],inputs=[],elements=new Map();
  const element=id=>{if(!elements.has(id))elements.set(id,{hidden:false,textContent:'',checked:false,
    querySelector(){return this;},querySelectorAll(){return [];},classList:{toggle(){}}});return elements.get(id);};
  const context=vm.createContext({state,$:element,samePlayerPath:(a,b)=>a===b,bodyFitValuesEqual:(a,b)=>a===b,
    currentTargetBodyFitValue:key=>Object.hasOwn(state.body.targetDraft,key)?state.body.targetDraft[key]:state.body.targetBodyFit[key],
    currentPreviewBodyFit:()=>({...state.preview.bodyFit,...state.preview.bodyFitDraft}),
    renderPreviewBodyFitControls(){},renderBodyFitRows(){},renderWorkingPlayer(){},renderPreviewControls(){},resetPreviewDetails(){},
    setPreviewStatus(){},setPreviewBodyFitStatus(){},renderPreviewWarnings(){},addLog(){},requestAnimationFrame:fn=>fn(),
    window:{configsEditor:{active:()=>''},characterTool:{loadStagedPreview:async()=>payload},modelPreview:{
      load:async value=>{const initial=structuredClone(value.player.bodyFit);inputs.push(initial);begin();
        try {const result=await gpu;visual=initial;return result;}finally{pending=false;}},
      setBodyFit:value=>{calls.push({pending,value:structuredClone(value)});visual=structuredClone(value);},
      resize(){},setJerseyVisible(){},setChestCorrection(){},
    }},
  });
  for(const [first,last] of [['function bodyFitEditBusy(','function updateBodyFitActions('], ['function previewBodyFitDraft(','async function loadStagedPreview('],
    ['async function loadStagedPreview(','function selectedSourceHeadband('],
    ['function syncBodyFitDraftFromPreview(','function syncLoadedPreviewBodyFit('],
    ['function revertPreviewBodyFit(','function calculatePreviewBodyFit(']]) {
    const start=source.indexOf(first);assert(start>=0,first);vm.runInContext(source.slice(start,source.indexOf(last,start)),context);
  }
  return {state,payload,calls,inputs,started,context,visual:()=>visual,
    start:()=>context.loadStagedPreview(true),
    finish:()=>resolve({decodedTextures:0}),fail:()=>reject(new Error('simulated GPU failure')),
    edit(key,value){
      if(typeof value==='number'&&Number.isFinite(Math.fround(value))) {
        state.body.invalidFields.delete(key);
        if(value===state.body.targetBodyFit[key])delete state.body.targetDraft[key];else state.body.targetDraft[key]=value;
      } else {state.body.targetDraft[key]=value;state.body.invalidFields.add(key);}
      context.syncPreviewBodyFitFromBody(key);
    },
  };
}
let cases=0;
async function check(label,run){cases++;try{await run();}catch(error){if(!baseline)throw new Error(label+': '+error.message,{cause:error});failures.push({label,error:error.message});}}

(async()=>{
  await check('Unsaved dimensions are not renderer baselines',async()=>{
    const e=setup({draft:{'Spine2.tx':2}}),job=e.start();await e.started;e.finish();await job;
    assert.deepEqual(e.inputs[0],fit);assert.equal(e.visual()['Spine2.tx'],2);
    assert.deepEqual(e.payload.player.bodyFit,fit,'Cached metadata was mutated by a draft');
  });
  for(const loadedPath of ['', 'A'])await check('Latest body input survives '+(loadedPath?'rebuild':'first load'),async()=>{
    const e=setup({loadedPath}),job=e.start();await e.started;e.edit('Spine2.tx',.75);
    assert.equal(e.calls.length,0,'Input reached an editor whose geometry is being replaced');
    e.finish();await job;assert.equal(e.visual()['Spine2.tx'],.75);
    assert.equal(e.state.preview.bodyFitDraft['Spine2.tx'],.75);assert(e.state.preview.bodyFitDirtyFields.has('Spine2.tx'));
  });
  await check('Revert to baseline during preparation removes the old draft',async()=>{
    const e=setup({draft:{'Spine2.tx':2}}),job=e.start();await e.started;e.edit('Spine2.tx',0);e.finish();await job;
    assert.equal(e.visual()['Spine2.tx'],0);assert.equal(e.state.preview.bodyFitDraft['Spine2.tx'],undefined);
    assert.equal(e.state.preview.bodyFitDirtyFields.size,0);
  });
  await check('Incomplete input retains the latest finite visual value',async()=>{
    const e=setup(),job=e.start();await e.started;e.edit('Spine2.tx',.75);e.edit('Spine2.tx','');e.finish();await job;
    assert.equal(e.visual()['Spine2.tx'],.75);assert.equal(e.state.body.targetDraft['Spine2.tx'],'');
    assert.equal(e.state.preview.bodyFitInvalidInputs.get('Spine2.tx'),'');
    assert(Object.values(e.visual()).every(Number.isFinite));
  });
  await check('Revert clears an incomplete body input even without a numeric draft',async()=>{
    const e=setup(),job=e.start();await e.started;e.edit('Spine2.tx','');e.finish();await job;
    assert.equal(e.state.preview.bodyFitInvalidInputs.size,1);assert.equal(e.state.preview.bodyFitDirtyFields.size,0);
    e.context.revertPreviewBodyFit();
    assert.equal(e.state.body.invalidFields.size,0);assert.equal(e.state.body.targetDraft['Spine2.tx'],undefined);
    assert.equal(e.state.preview.bodyFitInvalidInputs.size,0);assert.deepEqual(e.visual(),fit);
  });
  await check('Overflow cannot reach committed geometry',async()=>{
    const e=setup(),job=e.start();await e.started;e.edit('Spine2.tx',Number.MAX_VALUE);e.finish();await job;
    assert.deepEqual(e.visual(),fit);assert.equal(e.state.body.targetDraft['Spine2.tx'],Number.MAX_VALUE);
  });
  await check('Other-player edits cannot influence this frame',async()=>{
    const e=setup({otherTarget:true}),job=e.start();await e.started;e.edit('Spine2.tx',2);e.finish();await job;
    assert.deepEqual(e.visual(),fit);assert.equal(e.calls.length,0);
  });
  await check('Superseded request never reapplies its old draft',async()=>{
    const e=setup({draft:{'Spine2.tx':2}}),job=e.start();await e.started;
    e.state.preview.requestId++;e.state.workingPlayer={path:'B',name:'B'};e.state.preview.loadedPath='';e.finish();await job;
    assert.equal(e.calls.length,0);assert.equal(e.state.preview.loadedPath,'');
  });
  await check('Failed renderer preserves input without committing it',async()=>{
    const e=setup({draft:{'Spine2.tx':2}}),job=e.start();await e.started;e.fail();await job;
    assert.equal(e.calls.length,0);assert.equal(e.state.preview.bodyFitDraft['Spine2.tx'],2);
    assert.equal(e.state.preview.loadedPath,'');assert.equal(e.state.preview.loading,false);
  });
  await check('Unedited load does not perform an extra deformation pass',async()=>{
    const e=setup(),job=e.start();await e.started;e.finish();await job;
    assert.deepEqual(e.visual(),fit);assert.equal(e.calls.length,0);
  });
  if(baseline){assert(failures.length);console.log(JSON.stringify({baseline:true,cases,reproduced:failures},null,2));}
  else console.log(`Preview draft commit: ${cases} baseline, live input, revert, invalid input, target, failure and no-op cases passed.`);
})().catch(error=>{console.error(error);process.exitCode=1;}).finally(finish);
