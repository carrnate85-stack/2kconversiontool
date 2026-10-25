const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawn, spawnSync } = require('node:child_process');
const { chromium } = require('C:/Users/carrn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright');
const { root, currentExe } = require('./rpc-client.cjs');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const digest = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const python = path.resolve(root, '../character-mod-tool-webview2/dist/Character-Mod-Tool-WebView2-0.1.31/App/Backend/python_runtime/python.exe');
function memberHashes(file) {
  const result = spawnSync(python, ['-B','-c','import hashlib,json,sys,zipfile; z=zipfile.ZipFile(sys.argv[1]); print(json.dumps({n:hashlib.sha256(z.read(n)).hexdigest() for n in z.namelist() if n.lower() != "appearance_info.json"}))',file], {windowsHide:true,encoding:'utf8',timeout:30000});
  assert.equal(result.status,0,result.stderr);return JSON.parse(result.stdout);
}
(async()=>{
  const source=path.resolve(root,'../character-mod-tool-rust/artifacts/conversion-output/png06827.iff');
  const sources=fs.readdirSync(path.dirname(source)).filter(n=>n==='png06827.iff'||/^png06827_.*\.iff$/i.test(n)).map(n=>path.join(path.dirname(source),n));
  const hashes=Object.fromEntries(sources.map(p=>[p,digest(p)]));
  const folder=fs.mkdtempSync(path.join(root,'artifacts/body-fit-persistence-'));
  const fixtures=path.join(folder,'fixtures');fs.mkdirSync(fixtures);
  for(const file of sources)fs.copyFileSync(file,path.join(fixtures,path.basename(file)));
  const target=path.join(fixtures,path.basename(source)),unchanged=memberHashes(target),exe=currentExe();
  const child=spawn(exe,['--ui-test'],{windowsHide:true,stdio:'ignore',env:{...process.env,CMT_TEST_DATA:path.join(folder,'data')}});
  const report={version:path.basename(exe),executableSha256:digest(exe),mutationsMocked:false,completed:false,errors:[]};let browser,page;
  try{
    for(let i=0;i<100;i++){try{browser=await chromium.connectOverCDP('http://127.0.0.1:9764');break;}catch{await delay(200);}}
    assert(browser);for(let i=0;i<100;i++){page=browser.contexts().flatMap(c=>c.pages()).find(p=>p.url().includes('tauri.localhost'));if(page)break;await delay(100);}
    assert(page);page.on('pageerror',e=>report.errors.push(e.message));await page.waitForFunction(()=>document.getElementById('startupLoading')?.hidden);
    await page.evaluate(async target=>{await setWorkingPlayer(target);},target);
    await page.locator('#previewTab').click();await page.waitForFunction(()=>state.preview.loadedPath&&!state.preview.loading,null,{timeout:180000});
    await page.locator('#bodyTab').click();await page.waitForFunction(()=>!state.body.loading);
    const baseline=await page.evaluate(()=>({...state.body.targetBodyFit}));
    const key=Object.keys(baseline).find(k=>/^spine2\.tx$/i.test(k));assert(key,'Representative body dimension is missing');
    const bodyValue=Number((baseline[key]+.25).toFixed(6));
    await page.getByLabel(`Target ${key}`,{exact:true}).fill(String(bodyValue));await page.locator('#saveBodyFit').click();
    await page.waitForFunction(({key,value})=>!state.body.loading&&state.body.targetBodyFit[key]===value&&!state.body.dirtyFields.size,{key,value:bodyValue},{timeout:90000});
    const first=await page.evaluate(async target=>(await window.characterTool.loadBodyFit({target})).targetBodyFit,target);
    assert.equal(first[key],bodyValue);for(const field of Object.keys(baseline))if(field!==key)assert.deepEqual(first[field],baseline[field],field);
    assert.deepEqual(memberHashes(target),unchanged,'Body Apply changed an unrelated archive member');
    report.bodySave={key,value:bodyValue,readback:true,unchangedFields:Object.keys(baseline).length-1,nonAppearanceMembersPreserved:true};
    await page.locator('#previewTab').click();await page.waitForFunction(()=>state.preview.loadedPath&&!state.preview.loading,null,{timeout:180000});
    await page.locator('#previewBodyFitToggle').click();
    const label=await page.evaluate(key=>{const group=previewBodyDimensionGroups().find(g=>g.x===key);return `${previewFitLabel(group.label)} X value`;},key);
    const previewValue=Number((baseline[key]-.25).toFixed(6));
    await page.getByLabel(label,{exact:true}).fill(String(previewValue));await page.locator('#previewApplyFit').click();
    await page.waitForFunction(({key,value})=>!state.preview.bodyFitBusy&&state.preview.bodyFit[key]===value&&!state.preview.bodyFitOpen,{key,value:previewValue},{timeout:90000});
    const second=await page.evaluate(async target=>(await window.characterTool.loadBodyFit({target})).targetBodyFit,target);
    assert.equal(second[key],previewValue);for(const field of Object.keys(baseline))if(field!==key)assert.deepEqual(second[field],baseline[field],field);
    assert.deepEqual(memberHashes(target),unchanged,'Preview Apply changed an unrelated archive member');
    await page.locator('#refreshPreview').click();await page.waitForFunction(({key,value})=>!state.preview.loading&&state.preview.loadedPath&&state.preview.bodyFit[key]===value,{key,value:previewValue},{timeout:180000});
    const final=await page.evaluate(()=>({body:{...state.body.targetBodyFit},preview:{...state.preview.bodyFit},dirty:state.preview.bodyFitDirtyFields.size,invalid:state.preview.bodyFitInvalidInputs.size,meshes:document.getElementById('previewMeshCount').textContent,textures:document.getElementById('previewTextureCount').textContent}));
    assert.equal(final.body[key],previewValue);assert.equal(final.preview[key],previewValue);assert.equal(final.dirty,0);assert.equal(final.invalid,0);assert(Number(final.meshes)>0);assert(Number(final.textures)>0);
    report.previewSave={key,value:previewValue,readback:true,minimized:true,refreshKeepsSavedValue:true,nonAppearanceMembersPreserved:true,meshes:final.meshes,textures:final.textures};
    for(const file of sources){assert.equal(digest(file),hashes[file]);if(file!==source)assert.equal(digest(path.join(fixtures,path.basename(file))),hashes[file]);}
    assert.notEqual(digest(target),hashes[source]);report.sourcesAndCompanionsUnchanged=true;assert.deepEqual(report.errors,[]);report.completed=true;console.log(JSON.stringify(report,null,2));
  }finally{
    if(page)await page.evaluate(()=>window.__TAURI__.core.invoke('engine_call',{command:'close-app',payload:{}})).catch(()=>{});await browser?.close().catch(()=>{});
    if(child.exitCode===null)await Promise.race([new Promise(r=>child.once('exit',r)),delay(5000)]);
    if(child.exitCode===null){child.kill();await new Promise(r=>child.once('exit',r));}
    fs.writeFileSync(path.join(root,'artifacts/body-fit-persistence-report.json'),JSON.stringify(report,null,2));
    assert.equal(path.dirname(fs.realpathSync(folder)),fs.realpathSync(path.join(root,'artifacts')));fs.rmSync(folder,{recursive:true,maxRetries:10,retryDelay:200});
  }
})().catch(e=>{console.error(e);process.exitCode=1;});
