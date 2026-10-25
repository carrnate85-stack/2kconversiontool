const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { spawn } = require('node:child_process');
const { chromium } = require('C:/Users/carrn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright');
const { root, currentExe } = require('./rpc-client.cjs');
const baseline = process.argv.includes('--baseline');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
(async () => {
  const data = fs.mkdtempSync(path.join(root, 'artifacts/body-fit-tabs-'));
  const child = spawn(currentExe(), ['--ui-test'], { windowsHide: true, stdio: 'ignore', env: { ...process.env, CMT_TEST_DATA: data } });
  const report = { version: path.basename(currentExe()), mutationsMocked: true, completed: false, errors: [] };
  let browser, page;
  try {
    for (let i=0;i<100;i++) { try { browser=await chromium.connectOverCDP('http://127.0.0.1:9764');break; } catch {await delay(200);} }
    assert(browser);
    for(let i=0;i<100;i++){page=browser.contexts().flatMap(c=>c.pages()).find(p=>p.url().includes('tauri.localhost'));if(page)break;await delay(100);}
    assert(page); page.on('pageerror',error=>report.errors.push(error.message));
    await page.waitForFunction(()=>document.getElementById('startupLoading')?.hidden);
    report.cases=await page.evaluate(async()=>{
      const api=window.characterTool,names=['inspectCharacter','listConfigs','loadBodyFit','saveBodyFit','applyChestFix','applyPreviewChestCorrection'];
      const original=Object.fromEntries(names.map(name=>[name,api[name]])),pending=[],results=[];
      const fit={chestTHIN:.5,chestTHICK:.25,'Spine2.tx':0}; let reads=0;
      const need=(value,message)=>{if(!value)throw Error(message);};
      const defer=(method,payload)=>new Promise((resolve,reject)=>pending.push({method,payload,resolve,reject}));
      const stage=async()=>{
        state.activeView='full-swap';await setWorkingPlayer('C:/BodyFitTabs.iff');
        state.body.sourceBodyFit={...fit};state.body.targetBodyFit={...fit};
        state.preview.loadedPath=state.workingPlayer.path;state.preview.bodyFitTarget=state.workingPlayer.path;
        state.preview.bodyFit={...fit};state.preview.bodyFitDraft={};state.preview.bodyFitDirtyFields.clear();
        state.preview.loading=false;state.preview.dirty=false;state.preview.bodyFitOpen=true;
        renderBodyFitRows();renderPreviewBodyFitControls();
      };
      const edit=()=>setPreviewFitValue('Spine2.tx',2);
      const reply=p=>p.resolve({targetBodyFit:{...fit,...p.payload.changes},changed:Object.keys(p.payload.changes||{})});
      const check=async(name,run)=>{await stage();const start=pending.length;try{await run();results.push({name,passed:true});}catch(e){results.push({name,passed:false,error:e.message});}finally{pending.slice(start).forEach(reply);await new Promise(r=>setTimeout(r,10));}};
      try{
        api.inspectCharacter=async path=>({path});api.listConfigs=async()=>({configs:[{name:'BASE',items:[],texturePresent:true}],defaultConfig:'BASE',activeConfig:'BASE',items:[]});
        api.loadBodyFit=async()=>{reads++;return{sourceBodyFit:{...fit},targetBodyFit:{...fit},targetEntry:'fixture'};};
        api.saveBodyFit=p=>defer('save',p);api.applyChestFix=p=>defer('chest',p);api.applyPreviewChestCorrection=p=>defer('previewChest',p);
        await check('Body save locks Preview edits and competing Apply',async()=>{
          edit();const task=saveBodyFit(),request=pending.at(-1),count=pending.length;
          const input=document.querySelector('#previewBodyFitControls input[type=number]');
          const blocked=input.disabled&&document.getElementById('previewApplyFit').disabled&&document.getElementById('previewRevertFit').disabled;
          const accepted=setPreviewFitValue('Spine2.tx',3),competitor=applyPreviewBodyFit();
          pending.slice(count).forEach(reply);await competitor;
          reply(request);await task;
          need(blocked,'Preview remained editable while Body was saving');need(!accepted,'Queued Preview input bypassed the save lock');
          need(pending.length===count,'Competing Preview save was admitted');need(state.preview.bodyFit['Spine2.tx']===2,'Submitted value was not synchronized');
        });
        await check('Preview save locks Body inputs, Apply and Refresh',async()=>{
          edit();const task=applyPreviewBodyFit(),request=pending.at(-1),count=pending.length,before=reads;
          const input=document.querySelector('#bodyFitRows input[aria-label="Target Spine2.tx"]');
          const blocked=input.disabled&&document.getElementById('saveBodyFit').disabled&&document.getElementById('refreshBodyFit').disabled;
          input.value='3';input.dispatchEvent(new Event('input',{bubbles:true}));
          await refreshBodyFit();const competitor=saveBodyFit();pending.slice(count).forEach(reply);await competitor;reply(request);await task;
          need(blocked,'Body remained editable while Preview was saving');need(reads===before,'Refresh read during the pending save');
          need(pending.length===count,'Competing Body save was admitted');need(state.body.targetBodyFit['Spine2.tx']===2,'Saved value was not retained');
        });
        for(const fromPreview of [false,true])await check((fromPreview?'Preview':'Body')+' save failure unlocks both views and keeps the draft',async()=>{
          edit();const task=fromPreview?applyPreviewBodyFit():saveBodyFit();pending.at(-1).reject(Error('fixture write failure'));await task;
          need(state.body.targetDraft['Spine2.tx']===2&&state.preview.bodyFitDraft['Spine2.tx']===2,'Failure discarded the draft');
          need(!document.querySelector('#bodyFitRows input').disabled&&!document.querySelector('#previewBodyFitControls input').disabled,'Failure retained a cross-view lock');
        });
        await check('Entering Body preserves unsaved Preview edits',async()=>{
          edit();setActiveView('body');while(state.body.loading)await new Promise(r=>setTimeout(r,5));
          need(state.body.targetDraft['Spine2.tx']===2&&state.preview.bodyFitDraft['Spine2.tx']===2,'Tab entry discarded unsaved edits');
        });
        await check('Incomplete Preview input blocks both views and survives tab entry',async()=>{
          edit();setPreviewFitValue('Spine2.tx','');const blocked=document.getElementById('saveBodyFit').disabled;
          setActiveView('body');while(state.body.loading)await new Promise(r=>setTimeout(r,5));
          need(blocked,'Incomplete Preview input did not block Body Apply');
          need(state.body.invalidFields.has('Spine2.tx')&&state.preview.bodyFitInvalidInputs.has('Spine2.tx'),'Tab entry lost the incomplete input');
          need(state.preview.bodyFitDraft['Spine2.tx']===2,'Incomplete input lost the last finite visual value');
          setPreviewFitValue('Spine2.tx',-1);need(!state.body.invalidFields.size&&!state.preview.bodyFitInvalidInputs.size,'Valid input did not recover both views');
        });
        await check('Explicit Refresh still discards edits intentionally',async()=>{
          edit();await refreshBodyFit();need(!state.body.dirtyFields.size&&!state.preview.bodyFitDirtyFields.size,'Refresh did not discard the draft');
        });
        await check('Preview chest operation excludes Body writes and reads',async()=>{
          state.preview.jerseyAvailable=true;const task=applyPreviewChestCorrection(),request=pending.at(-1),before=reads;
          const locked=document.querySelector('#bodyFitRows input').disabled;await refreshBodyFit();
          request.reject(Error('fixture correction failure'));await task;
          need(locked&&reads===before,'Chest correction did not hold the Body view');
          need(!document.querySelector('#bodyFitRows input').disabled,'Chest correction retained its lock after failure');
        });
        await check('Body chest operation excludes Preview edits',async()=>{
          const task=applyChestFix(),request=pending.at(-1);const locked=document.querySelector('#previewBodyFitControls input').disabled;
          const accepted=setPreviewFitValue('Spine2.tx',2);request.reject(Error('fixture correction failure'));await task;
          need(locked&&!accepted,'Body chest fix did not hold Preview edits');
          need(!document.querySelector('#previewBodyFitControls input').disabled,'Body chest fix retained its lock after failure');
        });
        return results;
      }finally{pending.forEach(reply);Object.assign(api,original);}
    });
    const failures=report.cases.filter(c=>!c.passed);
    if(baseline)assert(failures.length,'No baseline defect reproduced');else assert.deepEqual(failures,[]);
    assert.deepEqual(report.errors,[]);report.completed=true;console.log(JSON.stringify(report,null,2));
  }finally{
    if(page)await page.evaluate(()=>window.__TAURI__.core.invoke('engine_call',{command:'close-app',payload:{}})).catch(()=>{});
    await browser?.close().catch(()=>{});
    if(child.exitCode===null)await Promise.race([new Promise(r=>child.once('exit',r)),delay(5000)]);
    if(child.exitCode===null){child.kill();await new Promise(r=>child.once('exit',r));}
    fs.writeFileSync(path.join(root,'artifacts',baseline?'body-fit-tabs-baseline.json':'body-fit-tabs-report.json'),JSON.stringify(report,null,2));
    assert.equal(path.dirname(fs.realpathSync(data)),fs.realpathSync(path.join(root,'artifacts')));fs.rmSync(data,{recursive:true,maxRetries:10,retryDelay:200});
  }
})().catch(error=>{console.error(error);process.exitCode=1;});
