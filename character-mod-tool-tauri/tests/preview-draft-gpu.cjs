const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawn } = require('node:child_process');
const { chromium } = require('C:/Users/carrn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright');
const { PNG } = require('C:/Users/carrn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/pngjs');
const { root, currentExe } = require('./rpc-client.cjs');
const hash = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const delay = ms => new Promise(resolve=>setTimeout(resolve,ms));
const baseline = process.argv.includes('--baseline');
function compare(left,right) {
  const decode = value => PNG.sync.read(Buffer.from(value.split(',')[1],'base64'));
  const a=decode(left),b=decode(right);assert.equal(a.width,b.width);assert.equal(a.height,b.height);
  let sum=0,changedPixels=0;
  for(let i=0;i<a.data.length;i+=4){let changed=false;for(let c=0;c<3;c++){const d=a.data[i+c]-b.data[i+c];sum+=d*d;if(Math.abs(d)>2)changed=true;}if(changed)changedPixels++;}
  return {rms:Math.sqrt(sum/(a.width*a.height*3)),changedPixels};
}
(async()=>{
  const exe=currentExe(),target=path.resolve(root,'../character-mod-tool-rust/artifacts/conversion-output/png06827.iff');
  const original=hash(target),child=spawn(exe,['--ui-test'],{windowsHide:true,stdio:'ignore'});
  const report={version:path.basename(exe),executableSha256:hash(exe),baseline,mutationsMocked:true,geometryDecoded:true,completed:false,errors:[]};
  let browser,page;
  try {
    for(let i=0;i<100;i++){try{browser=await chromium.connectOverCDP('http://127.0.0.1:9764');break;}catch{await delay(200);}}
    assert(browser);
    for(let i=0;i<100;i++){page=browser.contexts().flatMap(c=>c.pages()).find(p=>p.url().includes('tauri.localhost'));if(page)break;await delay(100);}
    assert(page);page.on('pageerror',error=>report.errors.push(error.message));
    await page.waitForFunction(()=>document.getElementById('startupLoading')?.hidden);
    await page.evaluate(async target=>{await setWorkingPlayer(target);await refreshBodyFit();},target);
    await page.locator('#previewTab').click();
    await page.waitForFunction(()=>state.preview.loadedPath&&!state.preview.loading,null,{timeout:180000});
    const result=await page.evaluate(async target=>{
      const api=window.characterTool,viewer=window.modelPreview,read=api.loadStagedPreview,save=api.saveBodyFit,render=viewer.load;
      const payload=await read({target}),fit={...payload.player.bodyFit},camera=viewer.cameraState();
      const key=Object.keys(fit).find(key=>/^spine2\.tx$/i.test(key))||Object.keys(fit).find(key=>/\.t[xyz]$/i.test(key));
      if(!key)throw Error('Representative player has no dimension values');
      $('bodyFitSearch').value='';$('bodyFitFilter').value='target';renderBodyFitRows();
      let salt='',saves=0,progress=0,injected=false,inject=null;
      const clockDescriptor=Object.getOwnPropertyDescriptor(performance,'now');let time=performance.now();
      const packet=()=>structuredClone({...payload,geometryKey:payload.geometryKey+salt,
        meshes:payload.meshes.map(mesh=>({...mesh,geometryKey:mesh.geometryKey+salt})),player:{...payload.player,bodyFit:{...fit}}});
      const input=value=>{const field=document.querySelector(`input[aria-label="Target ${key}"]`);if(!field||field.disabled)throw Error('Body input was not ready');
        field.value=String(value);field.dispatchEvent(new Event('input',{bubbles:true}));};
      const capture=()=>{viewer.setCameraState(camera);viewer.capturePng();return viewer.capturePng();};
      const cases=[];
      try {
        Object.defineProperty(performance,'now',{configurable:true,value:()=>time+=10});
        api.loadStagedPreview=async()=>packet();api.saveBodyFit=async()=>{saves++;throw Error('No player write is allowed in this preview-only check');};
        viewer.load=(value,callback)=>render(value,item=>{progress++;if(inject&&!injected){injected=true;inject();}callback(item);});
        for(const [index,mode] of ['before','during','incomplete','revert'].entries()) {
          revertPreviewBodyFit();state.body.targetDraft={};state.body.dirtyFields.clear();state.body.invalidFields.clear();renderBodyFitRows();
          const value=mode==='revert'?fit[key]:fit[key]+2;
          const expected={...fit,[key]:value};
          salt=':reference-'+index;await render(packet());viewer.setBodyFit(expected);const reference=capture();
          viewer.setBodyFit(fit);const neutral=capture();
          inject=null;injected=false;progress=0;salt=':draft-'+index;
          if(mode==='before'||mode==='revert')input(fit[key]+2);
          if(mode==='during')inject=()=>input(value);
          if(mode==='incomplete')inject=()=>{input(value);input('');};
          if(mode==='revert')inject=()=>input(fit[key]);
          await loadStagedPreview(true);
          const actual=capture(),metadata={...viewer.stats().payload.player.bodyFit};
          const stateAfter={draft:state.preview.bodyFitDraft[key],dirty:state.preview.bodyFitDirtyFields.has(key),
            invalid:state.preview.bodyFitInvalidInputs.has(key),applyBlocked:$('previewApplyFit').disabled};
          if(mode==='incomplete')await applyPreviewBodyFit();
          cases.push({mode,progress,injected,metadata,expected,stateAfter,actual,reference,neutral});
        }
        revertPreviewBodyFit();const ids=viewer.stats().geometryIds;input(fit[key]+1);
        await loadStagedPreview(true);
        const cached={before:ids,after:viewer.stats().geometryIds,metadata:{...viewer.stats().payload.player.bodyFit},value:state.preview.bodyFitDraft[key]};
        return {key,fit,cases,cached,saves};
      } finally {
        if(clockDescriptor)Object.defineProperty(performance,'now',clockDescriptor);else delete performance.now;
        api.loadStagedPreview=read;api.saveBodyFit=save;viewer.load=render;
      }
    },target);
    report.key=result.key;report.cases=[];
    const failures=[];
    for(const row of result.cases){
      const difference=compare(row.actual,row.reference),effect=compare(row.reference,row.neutral);
      const outcome={mode:row.mode,progress:row.progress,injected:row.injected,difference,effect,state:row.stateAfter,
        baselinePreserved:JSON.stringify(row.metadata)===JSON.stringify(result.fit)};
      report.cases.push(outcome);
      if(difference.rms>=.5||difference.changedPixels>80||!outcome.baselinePreserved)failures.push(row.mode);
      assert(row.progress>0,'Real renderer never yielded');
      if(row.mode!=='before')assert(row.injected);
      if(row.mode!=='revert')assert(effect.changedPixels>80,'Selected dimension had no meaningful visible effect');
      if(!baseline){
        assert(outcome.baselinePreserved,'Draft contaminated renderer metadata');
        if(row.mode==='incomplete'){assert(row.stateAfter.invalid);assert(row.stateAfter.applyBlocked);}
        if(row.mode==='revert')assert(!row.stateAfter.dirty);
      }
    }
    assert.deepEqual(result.cached.before,result.cached.after,'Cached draft refresh rebuilt geometry');
    if(!baseline)assert.deepEqual(result.cached.metadata,result.fit);
    report.saveAttempts=result.saves;
    if(baseline&&result.saves)failures.push('incomplete-save-guard');else assert.equal(result.saves,0);
    assert.equal(hash(target),original);assert.deepEqual(report.errors,[]);
    if(baseline)assert(failures.length,'Released build did not reproduce the draft problem');else assert.deepEqual(failures,[],'Visible draft differs from independently applied reference');
    report.reproduced=failures;report.cachedGeometryRetained=true;report.noPlayerWrites=true;report.sourceUnchanged=true;report.completed=true;
    await page.screenshot({path:path.join(root,'artifacts/preview-draft-gpu.png')});
    console.log(JSON.stringify(report,null,2));
  } finally {
    if(page)await page.evaluate(()=>window.__TAURI__.core.invoke('engine_call',{command:'close-app',payload:{}})).catch(()=>{});
    await browser?.close().catch(()=>{});
    if(child.exitCode===null)await Promise.race([new Promise(resolve=>child.once('exit',resolve)),delay(5000)]);
    if(child.exitCode===null){child.kill();await new Promise(resolve=>child.once('exit',resolve));}
    fs.writeFileSync(path.join(root,'artifacts',baseline?'preview-draft-gpu-baseline.json':'preview-draft-gpu-report.json'),JSON.stringify(report,null,2));
  }
})().catch(error=>{console.error(error);process.exitCode=1;});
