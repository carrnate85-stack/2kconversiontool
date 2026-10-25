const fs = require('fs');
const assert = require('assert/strict');
const crypto = require('crypto');
const path = require('path');
const { spawn } = require('child_process');
const { chromium } = require('C:/Users/carrn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright');
const { root, currentExe } = require('./rpc-client.cjs');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const target = process.env.CMT_TEST_TARGET || 'C:/Users/carrn/AppData/Local/CharacterModToolTauri/outputs/png2882.iff';
const outputHashes = () => Object.fromEntries(fs.readdirSync(path.dirname(target))
  .filter(name => /\.iff$/i.test(name)).map(name => [name, crypto.createHash('sha256')
    .update(fs.readFileSync(path.join(path.dirname(target), name))).digest('hex')]));
(async () => {
  const beforeHashes = outputHashes();
  const child = spawn(currentExe(), ['--ui-test'], { windowsHide:true, stdio:'ignore' });
  const report = { errors:[] };
  let browser, page;
  try {
    for(let i=0;i<100;i++) { try { browser=await chromium.connectOverCDP('http://127.0.0.1:9764'); break; } catch { await delay(200); } }
    if(!browser) throw new Error('Test window unavailable');
    for(let i=0;i<100;i++) { page=browser.contexts().flatMap(c=>c.pages()).find(p=>p.url().includes('tauri.localhost')); if(page)break;await delay(100); }
    page.on('pageerror',error=>report.errors.push(error.message));
    page.on('console',msg=>{ if(msg.type()==='error') report.errors.push(msg.text()); });
    await page.waitForFunction(()=>document.getElementById('engineStatus')?.textContent.includes('Engine '));
    report.asset=await page.evaluate(async target=>{
      const preview=await window.characterTool.loadStagedPreview({target});
      window.testHairPayload=preview;
      const invoke=(command,payload)=>window.__TAURI__.core.invoke('engine_call',{command,payload});
      const resources=await invoke('native-preview-resources',{target});
      const main=await invoke('read-archive',{path:resources.filePath});
      const mainScenes=main.members.filter(x=>/\.scne$/i.test(x.name)).map(x=>({name:x.name,value:JSON.parse(atob(x.data))}));
      const scenes=[];
      window.testHairFrames=[];
      for(const item of resources.options.companionPaths.filter(x=>/hair/i.test(x.name))) {
        const archive=await invoke('read-archive',{path:item.path});
        scenes.push({name:item.name,metadata:archive.members.filter(x=>/\.(scne|txtr)$/i.test(x.name)).map(x=>({name:x.name,value:JSON.parse(atob(x.data))}))});
        if(!/geo_hair/.test(item.name)||/tangentspace/.test(item.name))continue;
        const mesh=preview.meshes.find(m=>m.sourceName===item.name&&!m.tangentSpaceAttachment);
        if(!mesh)continue;
        const visit=value=>{
          if(!value||typeof value!=='object')return;
          const descriptor=value.VertexFormat?.TANGENTFRAME0;
          if(descriptor?.Format==='R10G10B10A2_UINT') {
            const stream=value.VertexStream[descriptor.Stream||0];
            const member=archive.members.find(m=>m.name===stream.Binary);
            const bytes=Uint8Array.from(atob(member.data),c=>c.charCodeAt(0));
            const view=new DataView(bytes.buffer);
            const frames=Array.from(mesh.sourceVertexIds,id=>view.getUint32(id*stream.Stride+(descriptor.ByteOffset||0),true));
            window.testHairFrames.push({geometryKey:mesh.geometryKey,frames});
          }
          for(const child of Object.values(value))visit(child);
        };
        for(const member of archive.members.filter(m=>/\.scne$/i.test(m.name)))visit(JSON.parse(atob(member.data)));
      }
      return {target,mainScenes,items:preview.activeItems,meshes:preview.meshes.filter(m=>/hair/i.test(m.sourceName)).map(m=>({
        source:m.sourceName,vertices:m.positions.length/3,groups:m.groups,tangent:m.tangentSpaceAttachment,projected:m.tangentProjected,
      })),textures:preview.textures.filter(t=>/hair/i.test(t.name)).map(t=>({...t,bytes:undefined})),scenes,skipped:preview.skipped};
    },target);
    report.render=await page.evaluate(async()=>{
      const {createExplorerRenderer}=await import('./explorer/model-preview.js');
      const canvas=document.createElement('canvas');canvas.width=640;canvas.height=640;document.body.append(canvas);
      const context=createExplorerRenderer({canvas,picker:true});
      window.hairTestContext=context;
      await context.api.load(window.testHairPayload);
      const materials=[];
      context.modelRoot.traverse(object=>{
        const list=Array.isArray(object.material)?object.material:[object.material];
        for(const material of list.filter(m=>m&&/hair/i.test(m.name))) materials.push({
          name:material.name,source:object.userData.previewModel,map:material.map?.name,normal:material.normalMap?.name,
          lightmap:material.userData.strandLightmap,anisotropyMap:material.anisotropyMap?.name,anisotropy:material.anisotropy,
          alphaTest:material.alphaTest,transparent:material.transparent,depthWrite:material.depthWrite,
          strandSpecularResponse:material.userData.strandSpecularResponse,
          uniforms:material.userData.strandLightmapUniforms?Object.keys(material.userData.strandLightmapUniforms):[],
        });
      });
      return {materials};
    }).catch(async error=>({error:error.message}));
    report.maps=await page.evaluate(()=>{
      const context=window.hairTestContext;
      return ['color','tangent','lightmap'].map(kind=>{
        const texture=context.boundTextureMatching('hair',kind==='color'?'color':'auxiliary',new RegExp(kind));
        const decoded=texture&&context.decodedTexturePixels(texture);
        if(!decoded)return {kind};
        const canvas=document.createElement('canvas');canvas.width=canvas.height=decoded.size;
        canvas.getContext('2d').putImageData(new ImageData(new Uint8ClampedArray(decoded.pixels),decoded.size,decoded.size),0,0);
        let transparent=0,opaque=0;
        for(let i=3;i<decoded.pixels.length;i+=4){if(decoded.pixels[i]<32)transparent++;if(decoded.pixels[i]>223)opaque++;}
        return {kind,transparent,opaque,png:canvas.toDataURL()};
      });
    });
    for(const map of report.maps)if(map.png){fs.writeFileSync(path.join(root,'artifacts/hair-'+map.kind+'.png'),Buffer.from(map.png.split(',')[1],'base64'));delete map.png;}
    await page.evaluate(async target=>{await setWorkingPlayer(target);setActiveView('preview');},target);
    await page.waitForFunction(()=>state.preview.loadedPath&&!state.preview.loading,null,{timeout:120000});
    await page.evaluate(yaw=>{window.modelPreview.focusHead();window.modelPreview.setCameraState({...window.modelPreview.cameraState(),yaw,pitch:0});},Number(process.env.CMT_CAMERA_YAW||Math.PI/2));
    report.main=await page.evaluate(()=>window.modelPreview.stats().materialTextures.filter(m=>/hair/i.test(m.name)));
    await page.locator('#previewCanvas').screenshot({path:path.join(root,'artifacts/hair-materials.png')});
    report.variants=await page.evaluate(()=>{
      const context=window.hairTestContext;
      context.setCameraState(window.modelPreview.cameraState());
      const variants=[];
      const hairMaterials=[];
      context.modelRoot.traverse(object=>{
        for(const material of (Array.isArray(object.material)?object.material:[object.material]).filter(m=>m&&/^hair/i.test(m.name)))hairMaterials.push(material);
      });
      for(const mode of ['baseline','no-anisotropy','no-specular','rougher','stored-normals','fiber-specular','blend-no-depth','cutout','coverage']) {
        context.applyAutomaticTextures();
        context.modelRoot.traverse(object=>{
          if(!object.geometry)return;
          object.geometry.computeVertexNormals();
          if(mode!=='stored-normals')return;
          const data=window.testHairFrames.find(x=>object.userData.geometryKey?.endsWith('|'+x.geometryKey));
          if(!data||data.frames.some(v=>v>>>20))return;
          const attribute=object.geometry.getAttribute('normal');
          for(let i=0;i<data.frames.length;i++) {
            const value=data.frames[i];
            let x=(value&1023)/511.5-1,y=((value>>>10)&1023)/511.5-1,z=1-Math.abs(x)-Math.abs(y);
            if(z<0){const oldX=x;x=(1-Math.abs(y))*(oldX>=0?1:-1);y=(1-Math.abs(oldX))*(y>=0?1:-1);}
            const length=Math.hypot(x,y,z);
            attribute.setXYZ(i,x/length,-z/length,y/length);
          }
          attribute.needsUpdate=true;
        });
        for(const material of hairMaterials) {
          material.specularIntensity=1;
          material.alphaToCoverage=material.userData.strandAlphaMode==='coverage';
          if(mode==='blend-no-depth'){material.transparent=true;material.depthWrite=false;material.alphaToCoverage=false;}
          if(mode==='cutout'||mode==='coverage') {
            material.transparent=false;
            material.alphaToCoverage=mode==='coverage';
          }
          if(mode==='no-anisotropy') material.anisotropy=0;
          if(mode==='no-specular') material.specularIntensity=0;
          if(mode==='rougher') material.roughness=.85;
          if(mode==='fiber-specular'&&!material.userData.strandSpecularResponse) {
            const previous=material.onBeforeCompile;
            const key=material.customProgramCacheKey();
            material.onBeforeCompile=shader=>{
              previous(shader);
              shader.fragmentShader=shader.fragmentShader.replace('#include <lights_fragment_end>', '#include <lights_fragment_end>\nvec3 fiberTint = sqrt(clamp(diffuseColor.rgb, 0.0, 1.0));\nreflectedLight.directSpecular *= fiberTint;\nreflectedLight.indirectSpecular *= fiberTint;');
            };
            material.customProgramCacheKey=()=>key+':fiber-specular-test';
          }
          material.needsUpdate=true;
        }
        context.renderer.render(context.scene,context.camera);
        variants.push({mode,png:context.renderer.domElement.toDataURL()});
      }
      return variants;
    });
    report.frames=await page.evaluate(()=>window.testHairFrames.map(x=>({vertices:x.frames.length,orientationNonzero:x.frames.filter(v=>v>>>20).length})));
    for(const variant of report.variants){fs.writeFileSync(path.join(root,'artifacts/hair-'+variant.mode+'.png'),Buffer.from(variant.png.split(',')[1],'base64'));delete variant.png;}
    assert.deepEqual(report.errors, []);
    assert(report.main.filter(m=>m.map&&/^hair|^facialHair/i.test(m.name)).every(m=>m.strandSpecularResponse));
    assert(report.main.filter(m=>m.strandSpecularResponse).every(m=>!m.transparent&&m.depthWrite&&['coverage','cutout'].includes(m.strandAlphaMode)));
    assert.deepEqual(outputHashes(), beforeHashes);
    report.exportFilesUnchanged = true;
    console.log(JSON.stringify({...report,asset:{...report.asset,scenes:undefined,mainScenes:undefined}},null,2));
  } finally {
    fs.writeFileSync(path.join(root,'artifacts/hair-materials-report.json'),JSON.stringify(report,null,2));
    await browser?.close();child.kill();if(child.exitCode===null)await new Promise(resolve=>child.once('exit',resolve));
  }
})().catch(error=>{console.error(error);process.exitCode=1;});
