const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawn } = require('node:child_process');
const { chromium } = require('C:/Users/carrn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright');
const { root, currentExe } = require('./rpc-client.cjs');
const baseline = process.argv.includes('--baseline'), delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const hash = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
(async () => {
  const exe = currentExe(), target = path.resolve(root, '../character-mod-tool-rust/artifacts/conversion-output/png06827.iff');
  const originalHash = hash(target), child = spawn(exe, ['--ui-test'], { windowsHide:true, stdio:'ignore' });
  const report = { version:path.basename(exe), executableSha256:hash(exe), baseline, mutationsMocked:true, geometryDecoded:true, completed:false, errors:[] };
  let browser, page;
  try {
    for (let i=0;i<100;i++) { try { browser=await chromium.connectOverCDP('http://127.0.0.1:9764'); break; } catch { await delay(200); } }
    assert(browser);
    for (let i=0;i<100;i++) { page=browser.contexts().flatMap(c=>c.pages()).find(p=>p.url().includes('tauri.localhost')); if(page)break; await delay(100); }
    assert(page); page.on('pageerror',error=>report.errors.push(error.message));
    await page.waitForFunction(()=>document.getElementById('startupLoading')?.hidden);
    await page.locator('#previewTab').click();
    await page.waitForFunction(()=>Boolean(window.modelPreview));
    report.result = await page.evaluate(async target => {
      setActiveView('full-swap'); await setWorkingPlayer(target); await refreshBodyFit();
      const api=window.characterTool, viewer=window.modelPreview;
      const load=api.loadStagedPreview, save=api.saveBodyFit, render=viewer.load;
      const payload=await load({target});
      let latest={...payload.player.bodyFit}, edit, progressCount=0, renderSalt='';
      const clockDescriptor=Object.getOwnPropertyDescriptor(performance,'now');
      let testTime=performance.now();
      const key='hips.tx', value=latest[key]+0.25;
      let nextValue=value;
      try {
        // Exercise the real renderer's time-sliced path even on a warm, fast GPU.
        Object.defineProperty(performance,'now',{configurable:true,value:()=>testTime+=10});
        api.loadStagedPreview=async()=>structuredClone({...payload,geometryKey:payload.geometryKey+renderSalt,
          meshes:payload.meshes.map(mesh=>({...mesh,geometryKey:mesh.geometryKey+renderSalt})),player:{...payload.player,bodyFit:latest}});
        api.saveBodyFit=async()=>{latest={...latest,[key]:nextValue};return {targetBodyFit:latest,changed:[key]};};
        const interrupted=(data,progress)=>render(data,item=>{
          progressCount++;
          if(!edit) {
            state.body.targetDraft={[key]:nextValue}; state.body.dirtyFields=new Set([key]);
            renderBodyFitRows(); edit=saveBodyFit();
          }
          progress(item);
        });
        viewer.load=interrupted;
        await loadStagedPreview(true); await edit;
        if(!progressCount || !edit) throw Error('The real renderer did not yield during preparation');
        const afterEdit={dirty:state.preview.dirty,loading:state.preview.loading,loadedPath:state.preview.loadedPath,
          staged:state.body.targetBodyFit[key],preview:state.preview.bodyFit[key],geometryIds:viewer.stats().geometryIds};
        viewer.load=render;
        await loadStagedPreview(true);
        const retry={dirty:state.preview.dirty,loading:state.preview.loading,loadedPath:state.preview.loadedPath,
          preview:state.preview.bodyFit[key],models:document.getElementById('previewMeshCount').textContent};
        renderSalt=':visible-edit';nextValue=value+0.25;edit=null;progressCount=0;viewer.load=interrupted;
        state.activeView='preview';
        await loadStagedPreview(true);await edit;
        await new Promise(resolve=>setTimeout(resolve,0));
        for(let i=0;i<300 && state.preview.loading;i++)await new Promise(resolve=>setTimeout(resolve,10));
        const automatic={loading:state.preview.loading,dirty:state.preview.dirty,loadedPath:state.preview.loadedPath,
          preview:state.preview.bodyFit[key],expected:nextValue,progressCount,models:document.getElementById('previewMeshCount').textContent};
        setActiveView('preview');
        return {key,value,afterEdit,retry,automatic};
      } finally {
        if(clockDescriptor)Object.defineProperty(performance,'now',clockDescriptor);else delete performance.now;
        api.loadStagedPreview=load;api.saveBodyFit=save;viewer.load=render;
      }
    },target);
    const r=report.result, stale=!r.afterEdit.dirty || Boolean(r.afterEdit.loadedPath);
    if(baseline) assert(stale,'Baseline did not reproduce the renderer commit race');
    else { assert(!stale,'Old renderer committed after a player edit');assert(!r.afterEdit.loading);assert.deepEqual(r.afterEdit.geometryIds,[]); }
    assert.equal(r.afterEdit.staged,r.value); assert.equal(r.retry.preview,r.value);
    assert(!r.retry.loading && !r.retry.dirty && r.retry.loadedPath); assert(Number(r.retry.models)>0);
    if(!baseline){assert(r.automatic.progressCount>0,'Automatic case never entered renderer preparation');assert(!r.automatic.loading && !r.automatic.dirty && r.automatic.loadedPath);assert.equal(r.automatic.preview,r.automatic.expected);assert(Number(r.automatic.models)>0);}
    assert.equal(hash(target),originalHash); assert.deepEqual(report.errors,[]);
    report.completed=true;console.log(JSON.stringify(report,null,2));
    await page.screenshot({path:path.join(root,'artifacts/preview-gpu-edit.png')});
  } finally {
    if(page)await page.evaluate(()=>window.__TAURI__.core.invoke('engine_call',{command:'close-app',payload:{}})).catch(()=>{});
    await browser?.close().catch(()=>{});
    if(child.exitCode===null)await Promise.race([new Promise(resolve=>child.once('exit',resolve)),delay(5000)]);
    if(child.exitCode===null){child.kill();await new Promise(resolve=>child.once('exit',resolve));}
    fs.writeFileSync(path.join(root,'artifacts',baseline?'preview-gpu-edit-baseline.json':'preview-gpu-edit-report.json'),JSON.stringify(report,null,2));
  }
})().catch(error=>{console.error(error);process.exitCode=1});
