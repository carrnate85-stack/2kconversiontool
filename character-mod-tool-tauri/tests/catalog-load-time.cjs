const assert = require('assert/strict');
const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');
const { chromium } = require('C:/Users/carrn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright');
const { root, currentExe } = require('./rpc-client.cjs');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
async function measure(exe) {
  const child = spawn(exe, ['--ui-test'], { windowsHide:true, stdio:'ignore' });
  let browser;
  try {
    for (let i=0; i<100; i++) { try { browser=await chromium.connectOverCDP('http://127.0.0.1:9764'); break; } catch { await delay(200); } }
    assert(browser);
    let page;
    for (let i=0; i<100; i++) { page=browser.contexts().flatMap(c=>c.pages()).find(p=>p.url().includes('tauri.localhost')); if(page) break; await delay(100); }
    assert(page);
    await page.waitForFunction(()=>document.getElementById('engineStatus')?.textContent.includes('Engine '));
    return await page.evaluate(async()=>{
      const rows=[];
      for (const request of [
        {assetType:'hair',assetId:'char/sig/png06827_geo_hair_dreads.iff'},
        {assetType:'hair',assetId:'char/sig/png06827_geo_hair_dreads_tangentspace.iff'},
        {assetType:'facialhair',assetId:'char/sig/png06827_geo_facialhair_beard_tangentspace.iff'},
        {assetType:'hair',assetId:'char/sig/png06572_geo_hair_curly_afro_tangentspace.iff'},
      ]) {
        const samples=[];
        for (let repeat=0; repeat<3; repeat++) {
          const start=performance.now();
          const result=await window.characterTool.previewNativeHair({...request,includeBase:repeat===0});
          samples.push({ms:performance.now()-start,headMeshes:result.base?.meshes.length||0,assetMeshes:result.asset.meshes.length,stats:result.stats});
        }
        rows.push({assetId:request.assetId,samples});
      }
      return rows;
    });
  } finally {
    await browser?.close(); child.kill();
    if(child.exitCode===null) await new Promise(resolve=>child.once('exit',resolve));
  }
}
(async()=>{
  const before=await measure(path.join(root,'dist/Character-Mod-Tool-Tauri-Preview-0.2.5.exe'));
  const after=await measure(currentExe());
  const report={before,after};
  fs.writeFileSync(path.join(root,'artifacts/catalog-load-time-report.json'),JSON.stringify(report,null,2));
  console.log(JSON.stringify(after.map((row,i)=>({asset:row.assetId,
    beforeMs:before[i].samples.map(s=>Math.round(s.ms)),afterMs:row.samples.map(s=>Math.round(s.ms))})),null,2));
})().catch(error=>{console.error(error);process.exitCode=1;});
