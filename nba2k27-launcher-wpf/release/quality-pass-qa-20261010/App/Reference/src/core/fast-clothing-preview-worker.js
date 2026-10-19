'use strict';
const {parentPort,workerData}=require('node:worker_threads');
const fs=require('node:fs/promises');
const path=require('node:path');
const os=require('node:os');
const crypto=require('node:crypto');
const v8=require('node:v8');
const {promisify}=require('node:util');
const execFile=promisify(require('node:child_process').execFile);
const {loadIff,readZipEntry,crc32}=require('../vendor/archive-preview/core/iff');
const {createIffPreview,createIffTextureGallery}=require('../vendor/archive-preview/core/preview');
const {resolveClothingItems,resolveClothingInventory}=require('./clothing-item-table');
const {clothingBindings,garmentPalette,garmentArtworkPlacement}=require('./clothing-color-bindings');
const accessorySlots=require('../shared/accessory-slots');
const accessorySupport=require('../shared/accessory-preview-support');
const accessoryRig=require('./accessory-rig-fit');
const shoeModels=require('../data/shoe-model-names.json').models;
const {shoePalette,colorizeShoeRegions,readShoeStyles,readShoeRegions,readShoePalettes}=require('./shoe-colorway-preview');

const MAX_CACHE_BYTES=256*1024*1024;
const memory=new Map();let memoryBytes=0;
const resourceJobs=new Map(),decodeWaiters=[];let decoderPermits=Math.min(6,Math.max(2,os.availableParallelism()-1));
let index=null,indexStamp='',tables=null,active=null,next=null;
let warmQueue=[],warmCursor=0,warmRunning=false,warmRequested=false;
let prefetchQueue=[],pendingPrefetch=new Set();const prefetched=new Set();
const stamp=s=>`${s.size}:${s.mtimeMs}:${s.ctimeMs}`;
async function decode(args,signal){
 if(decoderPermits>0)decoderPermits--;
 else await new Promise((resolve,reject)=>{
  const waiter={resolve:()=>{signal.removeEventListener('abort',cancel);resolve();}};
  const cancel=()=>{const index=decodeWaiters.indexOf(waiter);if(index>=0)decodeWaiters.splice(index,1);reject(signal.reason||Error('Preview cancelled.'));};
  decodeWaiters.push(waiter);signal.addEventListener('abort',cancel,{once:true});
  if(signal.aborted)cancel();
 });
 try{
  for(let attempt=0;attempt<2;attempt++){
   signal.throwIfAborted();
   try{return await execFile(workerData.decoderPath,args,{windowsHide:true,timeout:30000,maxBuffer:1024*1024,signal});}
   catch(error){
    if(signal.aborted)throw error;
    if(attempt===1)throw Error(`Game resource decoder failed (${error.code||'unknown'}). Retry the preview.`);
    await new Promise(resolve=>setTimeout(resolve,150));
   }
  }
 }
 finally{const waiter=decodeWaiters.shift();if(waiter)waiter.resolve();else decoderPermits++;}
}
function remember(key,value){
 const bytes=v8.serialize(value).byteLength;
 if(memory.has(key)){memoryBytes-=memory.get(key).bytes;memory.delete(key);}
 if(bytes>96*1024*1024)return value;
 memory.set(key,{value,bytes});memoryBytes+=bytes;
 while(memoryBytes>96*1024*1024){const oldest=memory.keys().next().value;memoryBytes-=memory.get(oldest).bytes;memory.delete(oldest);}
 return value;
}
async function manifest(gameRoot){
 const file=path.join(gameRoot,'manifest'),stat=await fs.stat(file);
 if(stat.size>128*1024*1024)throw Error('Unsupported game manifest.');
 const current=`${gameRoot}:${stamp(stat)}`;
 if(current===indexStamp)return index;
 const entries=new Map();
 for(const line of (await fs.readFile(file,'utf8')).split(/\r?\n/)){
  const m=/^([^,]+),([A-Z0-9]{2}),(\d+),(\d+)$/.exec(line);
  if(!m||(!m[1].startsWith('clothing/')&&m[1]!=='char/sig/png04090.iff'
   &&m[1]!=='shoes/shoe_global.iff'&&!/^shoes\/(?:shoe_style_|shoe_ed_region_)\d{4}\.iff$/.test(m[1])))continue;
  const offset=Number(m[3]),size=Number(m[4]);
  if(Number.isSafeInteger(offset)&&offset>=0&&Number.isSafeInteger(size)&&size>0&&size<=32*1024*1024)
   entries.set(m[1],{archive:m[2],offset,size});
 }
 index=entries;indexStamp=current;tables=null;memory.clear();memoryBytes=0;warmQueue=[];warmCursor=0;warmRequested=pendingPrefetch.size>0;prefetchQueue=[];prefetched.clear();
 return entries;
}
async function cacheFile(gameRoot,name){
 const entry=index.get(name);if(!entry)throw Error(`Preview resource unavailable: ${name}`);
 const archive=path.join(gameRoot,entry.archive),stat=await fs.stat(archive);
 if(entry.offset+entry.size>stat.size)throw Error('Preview archive range is invalid.');
 const key=crypto.createHash('sha256').update(`fit-v2:${indexStamp}:${name}:${entry.archive}:${entry.offset}:${entry.size}:${stamp(stat)}`).digest('hex');
 return {name,entry,archive,key};
}
async function cached(gameRoot,name,kind,build,signal){
 signal.throwIfAborted();
 const source=await cacheFile(gameRoot,name),key=`${kind}:${source.key}`;
 const hit=memory.get(key);if(hit){memory.delete(key);memory.set(key,hit);return {key:source.key,value:hit.value,source:'memory'};}
 if(resourceJobs.has(key)){const shared=await resourceJobs.get(key);return {key:source.key,value:shared.value,source:'memory'};}
 const job=loadCached(gameRoot,source,kind,key,build,signal);
 resourceJobs.set(key,job);
 try{return await job;}finally{if(resourceJobs.get(key)===job)resourceJobs.delete(key);}
}
async function loadCached(gameRoot,source,kind,key,build,signal){
 const file=path.join(workerData.cacheRoot,`${kind}-${source.key}.bin`);
 try{
  const stat=await fs.lstat(file);
  if(stat.isFile()&&!stat.isSymbolicLink()){
   if(stat.size>0&&stat.size<=32*1024*1024){
    try{const value=v8.deserialize(await fs.readFile(file));return {key:source.key,value:remember(key,value),source:'disk'};}
    catch{/* An interrupted app-owned cache write is rebuilt below. */}
   }
   await fs.unlink(file);
  }
 }catch(error){if(error.code!=='ENOENT')throw error;}
 const temp=await fs.mkdtemp(path.join(workerData.cacheRoot,'fit-'));
 try{
  const target=path.join(temp,'resource.iff');
  const started=performance.now();
  await decode([source.archive,String(source.entry.offset),String(source.entry.size),path.join(gameRoot,'data','oodle','oo2core_9_win64.dll'),target],signal);
  const decodedAt=performance.now();
  signal.throwIfAborted();
  if((await fs.stat(target)).size>64*1024*1024)throw Error('Preview resource is too large.');
  const value=await build(target);
  const builtAt=performance.now();
  const bytes=v8.serialize(value);
  if(bytes.length<=32*1024*1024){
   const staged=path.join(temp,'cache.bin');await fs.writeFile(staged,bytes);
   await fs.rename(staged,file).catch(async error=>{if(error.code!=='EEXIST')throw error;});
  }
  if(process.env.FAST_FIT_QA_PROFILE==='1')console.log(JSON.stringify({fitResource:source.name,kind,decodeMs:Math.round(decodedAt-started),buildMs:Math.round(builtAt-decodedAt)}));
  return {key:source.key,value:remember(key,value),source:'decoded'};
 }finally{
  if(path.dirname(temp)===workerData.cacheRoot&&path.basename(temp).startsWith('fit-'))await fs.rm(temp,{recursive:true,force:true});
 }
}
async function prune(){
 const files=[];for(const name of await fs.readdir(workerData.cacheRoot)){
  if(!/^(?:mesh|headmesh|texture|tables|rig)-[a-f0-9]{64}\.bin$/.test(name))continue;
  const file=path.join(workerData.cacheRoot,name),stat=await fs.lstat(file);
  if(stat.isFile()&&!stat.isSymbolicLink())files.push({file,size:stat.size,time:stat.mtimeMs});
 }
 let total=files.reduce((sum,item)=>sum+item.size,0);
 for(const item of files.sort((a,b)=>a.time-b.time)){if(total<=MAX_CACHE_BYTES)break;await fs.unlink(item.file);total-=item.size;}
}
async function clothingTables(gameRoot,signal){
 if(tables)return tables;
 const record=await cached(gameRoot,'clothing/clothing_itemtable.iff','tables',async target=>{
  const iff=await loadIff(target),member=name=>{const entry=iff.entries.find(e=>e.name===name);if(!entry)throw Error('Unsupported clothing item table.');return readZipEntry(iff.buffer,entry,true);};
  return {casuals:member('clothing_item_table.CasualsTable'),items:member('item_table.ConsolidatedClothingItems'),colors:member('binding_table_global.ConsolidatedClothingBinding')};
 },signal);
 const loaded=record.value;
 for(const name of ['casuals','items','colors'])if(!Buffer.isBuffer(loaded[name]))loaded[name]=Buffer.from(loaded[name]);
 loaded.inventory=resolveClothingInventory(loaded.casuals,loaded.items,workerData.previewNames);
 tables=loaded;
 return tables;
}
function commonWarmResources(inventory){
 const geos=new Map(),textures=new Map();
 for(const item of inventory.values()){
  const geo=item.dependencies.filter(dep=>dep.kind===2);
  if(geo.length===1&&(/^male_[a-zA-Z0-9_]+_geo$/.test(geo[0].name)||geo[0].name==='sweats_wide_geo')){
   const name=`clothing/geo/standard/${geo[0].name}.iff`;
   if(index.has(name))geos.set(name,(geos.get(name)||0)+1);
  }
  const base=colorTextureDependency(item);
  if(base){const name=`clothing/${base.name}.iff`;textures.set(name,(textures.get(name)||0)+1);}
 }
 const ranked=(frequency,kind,limit)=>[...frequency].sort((a,b)=>b[1]-a[1]).slice(0,limit).map(([name,score])=>({name,kind,score}));
 const chosen=[...ranked(geos,'mesh',10),...ranked(textures,'texture',6)];
 for(const name of ['male_jersey_basketball_classic_u_geo','male_jersey_basketball_classic_v_geo','male_jersey_basketball_nike_v_geo','male_jeans_geo','male_basketball_shorts_nike_geo']){
  const resource=`clothing/geo/standard/${name}.iff`;
  if(index.has(resource)&&!chosen.some(item=>item.name===resource))chosen.push({name:resource,kind:'mesh',score:name.includes('jersey')?700:300});
 }
 return chosen.sort((a,b)=>b.score-a.score);
}
function colorTextureDependency(item){
  const available=item.dependencies.filter(dep=>dep.kind===1&&index.has(`clothing/${dep.name}.iff`));
  // These inspected bundles each contain one named color DDS alongside normals
  // or RDMO. Their resource names include "mask", which the generic fallback
  // intentionally excludes to avoid selecting a texture/material mask map.
  const maskBundles={facemask_solid_gry:'variants_facemask_tex',mask_starchain_star_blk:'mask_starchain_star_tex',mask_gaiter_starchain_star_bge:'mask_gaiter_starchain_star_tex',mask_gaiter_starchain_wordmark_blk:'mask_gaiter_starchain_wordmark_tex'};
  if(maskBundles[item.asset])return available.find(dep=>dep.name===maskBundles[item.asset]);
  return available.find(dep=>/_j_untucked_tex$/.test(dep.name))
  ||available.find(dep=>/(?:^|_)colou?r(?:_tex)?$/.test(dep.name))
  ||available.find(dep=>!/normal|coneangle|rdmo|region|mask|wrinkle/.test(dep.name));
}
function queuePrefetch(){
 if(!tables||!index||!pendingPrefetch.size)return;
 for(const asset of pendingPrefetch){
  const item=tables.inventory.get(asset);if(!item)continue;
  const geo=item.dependencies.find(dep=>dep.kind===2&&(/^male_[a-zA-Z0-9_]+_geo$/.test(dep.name)||dep.name==='sweats_wide_geo'));
  const base=colorTextureDependency(item);
  const resources=[geo&&{name:`clothing/geo/standard/${geo.name}.iff`,kind:'mesh'},base&&{name:`clothing/${base.name}.iff`,kind:'texture'},
   ...item.dependencies.filter(dep=>dep.kind===1&&/_c_tex$/.test(dep.name)&&dep.name!==base?.name).slice(0,6).map(dep=>({name:`clothing/${dep.name}.iff`,kind:'texture'}))];
  for(const resource of resources){
   if(!resource||!index.has(resource.name)||prefetched.has(resource.name))continue;
   prefetched.add(resource.name);prefetchQueue.push(resource);
  }
 }
 pendingPrefetch.clear();
}
function kickWarmQueue(){
 queuePrefetch();
 if(warmRunning||!warmRequested||active||next||(!prefetchQueue.length&&warmCursor>=warmQueue.length))return;
 warmRunning=true;
 (async()=>{
  const gameRoot=path.resolve(workerData.gameRoot),signal=new AbortController().signal;
  while(warmRequested&&!active&&!next&&(prefetchQueue.length||warmCursor<warmQueue.length)){
   const item=prefetchQueue.shift()||warmQueue[warmCursor++];
   try{
    const loaded=await cached(gameRoot,item.name,item.kind,item.kind==='mesh'
     ?p=>createIffPreview(p,{skipTextures:true,maxTriangles:80000,maxModels:8})
     :p=>createIffTextureGallery(p,{maxTextures:8,maxBytes:16*1024*1024}),signal);
    if(loaded.source==='decoded')await prune().catch(()=>{});
   }catch{/* Optional warmup never blocks a selected outfit. */}
   if(warmRequested&&!active&&!next)await new Promise(resolve=>setTimeout(resolve,150));
  }
 })().catch(()=>{/* Background preparation is optional. */}).finally(()=>{
  warmRunning=false;
  if(warmRequested&&!active&&!next&&(prefetchQueue.length||warmCursor<warmQueue.length))kickWarmQueue();
 });
}
async function render(request,signal,emitProgress){
 const start=performance.now(),gameRoot=path.resolve(workerData.gameRoot);
 const profile=process.env.FAST_FIT_QA_PROFILE==='1',trace=(stage,extra={})=>{if(profile)console.log(JSON.stringify({fitStage:stage,ms:Math.round(performance.now()-start),...extra}));};
 await fs.mkdir(workerData.cacheRoot,{recursive:true});
 if((await fs.lstat(workerData.cacheRoot)).isSymbolicLink())throw Error('Preview cache cannot be a link.');
 await manifest(gameRoot);
 const assets=request.assets;
 if(!request.warm&&(!Array.isArray(assets)||assets.length<1||assets.length>7||assets.some(a=>typeof a!=='string'||!/^[a-zA-Z0-9_]{1,200}$/.test(a))))throw Error('Invalid outfit preview request.');
 const knownMeshes=new Set(request.knownMeshes||[]),knownTextures=new Set(request.knownTextures||[]);
 const prefetch=await Promise.allSettled([
  clothingTables(gameRoot,signal).then(value=>{trace('table');return value;}),
  cached(gameRoot,'char/sig/png04090.iff','mesh',p=>createIffPreview(p,{skipTextures:true,maxTriangles:100000,maxModels:8}),signal).then(value=>{
   trace('mannequin');
   if(request.showMannequinEarly&&!knownMeshes.has(value.key)){
    signal.throwIfAborted();
    const cache={memory:0,disk:0,decoded:0};cache[value.source]++;
    emitProgress({mannequin:{key:value.key,meshes:value.value.meshes},parts:[],warnings:[],stats:{cache,ms:Math.round(performance.now()-start)},progress:true,stage:'mannequin'});
   }
   return value;
  })
 ]);
 const prefetchFailure=prefetch.find(item=>item.status==='rejected');if(prefetchFailure)throw prefetchFailure.reason;
 const [t,body]=prefetch.map(item=>item.value);
 if(request.warm){
  if(!warmQueue.length)warmQueue=commonWarmResources(t.inventory);
  warmRequested=Boolean(prefetchQueue.length||warmCursor<warmQueue.length);
  return {warmed:true,backgroundResources:warmQueue.length,ms:Math.round(performance.now()-start)};
 }
 const resolved=new Map(assets.map(asset=>[asset,t.inventory.get(asset)]));
 const missing=assets.filter(asset=>!resolved.get(asset));
 if(missing.length)for(const [asset,item] of resolveClothingItems(t.casuals,t.items,missing))resolved.set(asset,item);
 const bindings=clothingBindings(t.colors,[...resolved.values()].filter(Boolean).map(item=>item.id));
 const result={mannequin:{key:body.key,meshes:knownMeshes.has(body.key)?null:body.value.meshes},parts:[],warnings:[],accessories:[],stats:{cache:{memory:0,disk:0,decoded:0},ms:0}};
 result.stats.cache[body.source]++;
 const jobs=assets.map(async asset=>{
  signal.throwIfAborted();
  const shoeId=/^shoe_(\d{4})_[a-zA-Z0-9_]+$/.exec(asset)?.[1];
  if(shoeId){
   // The stock default shoe has no numbered model entry or colorway resources.
   // Its actual game mesh is the unnumbered 2K generic shoe.
   const model=shoeId==='0000'?{slug:'special_twokgeneric'}:shoeModels[shoeId];
   if(!model)return {warning:`${asset}: no verified shoe model mapping.`};
   const meshPath=['dynamic','standard'].map(level=>`clothing/geo/${level}/shoes_${model.slug}_geo.iff`).find(name=>index.has(name));
   if(!meshPath)return {warning:`${asset}: shoe geometry unavailable.`};
   const mesh=await cached(gameRoot,meshPath,'mesh',p=>createIffPreview(p,{skipTextures:true,maxTriangles:80000,maxModels:8}),signal);
   if(!mesh.value.meshes.length)return {warning:`${asset}: shoe geometry could not be decoded.`,sources:[mesh.source]};
   const part={asset,meshKey:mesh.key,textureKey:'',meshes:knownMeshes.has(mesh.key)?null:mesh.value.meshes,palettes:null,texture:null};
   const sources=[mesh.source];
   try{
    const stylePath=`shoes/shoe_style_${shoeId}.iff`,regionPath=`shoes/shoe_ed_region_${shoeId}.iff`;
    if(!index.has(stylePath)||!index.has(regionPath)||!index.has('shoes/shoe_global.iff'))throw Error('shoe color resources are unavailable');
    const [style,global,regions]=await Promise.all([
     cached(gameRoot,stylePath,'tables',p=>readShoeStyles(p,shoeId,model.slug),signal),
     cached(gameRoot,'shoes/shoe_global.iff','tables',readShoePalettes,signal),
     cached(gameRoot,regionPath,'texture',readShoeRegions,signal)
    ]);
    sources.push(style.source,global.source,regions.source);
    const paletteBytes=global.value.get(asset);
    if(!paletteBytes)throw Error('selected colorway is unavailable');
    const palette=shoePalette(Buffer.from(paletteBytes),shoeId,style.value);
    if(palette.teamColors)throw Error('colorway depends on live team colors');
    const shoeTextureKeys={},shoeTextures={};
    for(const side of ['left','right']){
     const key=crypto.createHash('sha256').update(`${regions.key}:${asset}:${side}:regions-v1`).digest('hex');
     shoeTextureKeys[side]=key;
     if(!knownTextures.has(key)){
      const image=colorizeShoeRegions(Buffer.from(regions.value[side]),palette.colors);
      shoeTextures[side]={name:`${asset}_${side}_preview`,previewRgba:true,width:image.width,height:image.height,bytes:image.pixels.toString('base64')};
     }
    }
    part.shoeTextureKeys=shoeTextureKeys;part.shoeTextures=shoeTextures;
    return {part,artStates:[],sources};
   }catch(error){return {part,artStates:[],warning:`${asset}: ${error.message}; neutral shoe shape shown.`,sources};}
  }
  const slot=accessorySlots.slot(asset);
  const unavailable=reason=>({accessory:{asset,slot,state:'unavailable',reason}});
  const item=resolved.get(asset),geos=item?.dependencies.filter(d=>d.kind===2)||[];
  const staticEmission=slot&&accessorySupport.staticEmissionAsset(asset);
  if(slot&&(/emissive|hologra(?:phic|m)/i.test(asset)||item?.dependencies.some(d=>d.kind===1&&/emissiv|emission|animation/i.test(d.name)))&&!staticEmission)return unavailable(asset==='headband_2kcrew_outfit_primary_emissive'?'The emissive color depends on an unresolved dynamic color binding.':'Emissive or animated accessory materials are not supported yet.');
  if(slot&&accessorySupport.animated(asset))return unavailable('The source texture animation needs a verified playback controller and frame composition.');
  if(slot&&(geos.length!==1||!accessorySupport.geometry(geos[0].name)))return unavailable(accessorySupport.reason(asset));
  // The stock wide-sweatpants mesh is unprefixed even in the men's table.
  if(!slot&&(geos.length!==1||!(/^male_[a-zA-Z0-9_]+_geo$/.test(geos[0].name)||geos[0].name==='sweats_wide_geo')))return {warning:`${asset}: unsupported male preview geometry.`};
  // The inspected costume base mesh is authored in mannequin body coordinates,
   // with no guessed bone offset; facial morphs remain outside this static preview.
   const geo=slot&&geos[0].name.toLowerCase()==='mask_masquerade_geo'?'clothing/geo/headgearcap/mask_masquerade_geo.iff':`clothing/geo/standard/${slot?geos[0].name.toLowerCase():geos[0].name}.iff`;
  if(!index.has(geo))return slot?unavailable('Geometry is missing from this game installation.'):{warning:`${asset}: geometry missing from game manifest.`};
  const textureDependency=colorTextureDependency(item);
  const texPath=textureDependency?`clothing/${textureDependency.name}.iff`:geo;
  const tiledDependency=item.dependencies.find(d=>d.kind===1&&/_jersey_tiled$/.test(d.name)&&index.has(`clothing/${d.name}.iff`));
  const playerNumber=item.dependencies.find(d=>d.kind===5&&/^PLAYER_NUMBER_NAME:\d{1,2}:/.test(d.name));
  const fontDependency=playerNumber&&item.dependencies.find(d=>d.kind===3&&/_font$/.test(d.name)&&index.has(`clothing/${d.name}.iff`));
  const itemBindings=bindings.get(item.id);
  const jobsToLoad=[
   cached(gameRoot,geo,slot&&accessorySupport.headGeometry(geos[0].name)?'headmesh':'mesh',p=>createIffPreview(p,{skipTextures:true,maxTriangles:80000,maxModels:8,preferHighestDetail:Boolean(slot&&accessorySupport.headGeometry(geos[0].name))}),signal),
   cached(gameRoot,texPath,'texture',p=>createIffTextureGallery(p,{maxTextures:8,maxBytes:16*1024*1024}),signal)
  ];
  if(tiledDependency)jobsToLoad.push(cached(gameRoot,`clothing/${tiledDependency.name}.iff`,'texture',p=>createIffTextureGallery(p,{maxTextures:16,maxBytes:32*1024*1024}),signal));
  if(fontDependency)jobsToLoad.push(cached(gameRoot,`clothing/${fontDependency.name}.iff`,'texture',p=>createIffTextureGallery(p,{maxTextures:16,maxBytes:32*1024*1024}),signal));
  const loaded=await Promise.allSettled(jobsToLoad);
  const failed=loaded.find(entry=>entry.status==='rejected');if(failed)throw failed.reason;
  const [mesh,tex]=loaded.map(entry=>entry.value),sources=loaded.map(entry=>entry.value.source);
  let extra=2;
  const tiled=tiledDependency?loaded[extra++].value:null;
  const font=fontDependency?loaded[extra++].value:null;
  trace('garment-base',{asset,sources});
  if(!mesh.value.meshes.length)return slot?unavailable('Geometry could not be decoded.'):{warning:`${asset}: unreadable geometry.`,sources};
  if(slot&&mesh.value.stats?.truncated)return unavailable('The accessory mesh exceeds preview limits.');
  if(slot&&mesh.value.meshes.some(m=>!m.groups?.length))return unavailable('The accessory material groups could not be decoded.');
  if(slot&&mesh.value.meshes.some(m=>m.instanceMatrices?.length||m.instances?.some(i=>i.matrix?.some((n,j)=>Math.abs(n-([0,5,10,15].includes(j)?1:0))>0.00001))))return unavailable('This accessory needs a scene attachment transform.');
  if(slot&&mesh.value.meshes.some(m=>m.tangentOffsets?.length||!m.positions?.length||Array.from(m.positions).some(p=>!Number.isFinite(p))))return unavailable('This mesh needs body attachment projection.');
  const options=tex.value.textures||[];
  const selectedTexture=options.find(x=>/(?:^|[_\-.])(?:base_?)?colou?r(?:[_\-.]|$)/i.test(x.name))||options.find(x=>!/(?:normal|rough|metal|mask|rdmo)/i.test(x.name))||null;
  if(slot&&!selectedTexture)return unavailable('The accessory material could not be decoded.');
  if(slot&&selectedTexture){const bytes=Buffer.from(selectedTexture.bytes,'base64');if(bytes.length>=148&&bytes.toString('ascii',84,88)==='DX10'&&bytes.readUInt32LE(140)!==1)return unavailable('This texture array needs a verified animation or layer controller.');}
  const textureKey=selectedTexture?tex.key:'';
  const placements=[0,1,2,3,4,5].map(slot=>mesh.value.meshes.map(m=>m.groups.map(g=>garmentArtworkPlacement(itemBindings,crc32(Buffer.from(g.material)),slot))));
  // Hats declare their logo resources in ordered slots after color/normal/RDMO.
  // Their real names are *_tex, rather than the garment-only *_c_tex suffix.
  const hatArtwork=slot==='headwear';
  let artworkDependencies=selectedTexture?item.dependencies.filter(d=>d.kind===1&&(hatArtwork?!/normal|coneangle|rdmo|region|mask|wrinkle/.test(d.name):/_c_tex$/.test(d.name))&&d.name!==textureDependency?.name&&index.has(`clothing/${d.name}.iff`)).slice(0,8):[];
  const hatArtworkResources=new Map();let ignoredNonArtwork=0;
  if(hatArtwork){
   const resources=await Promise.all(artworkDependencies.map(async d=>{
    const value=await cached(gameRoot,`clothing/${d.name}.iff`,'texture',p=>createIffTextureGallery(p,{maxTextures:8,maxBytes:16*1024*1024}),signal);
    hatArtworkResources.set(d.name,value);
    const textures=value.value.textures||[];
    // A generic *_tex bundle can contain only normal/RDMO/cone-angle maps.
    // These occupy no artwork slot. Keep genuine logo order after filtering.
    if(textures.length&&textures.every(t=>/(?:normal|rough|metal|mask|rdmo|coneangle|_n[._])/i.test(t.name))){ignoredNonArtwork++;return null;}return d;
   }));
   artworkDependencies=resources.filter(Boolean);
  }
  artworkDependencies=artworkDependencies.slice(0,6);
  if(hatArtwork&&placements.some((groups,i)=>!artworkDependencies[i]&&groups.some(g=>g.some(p=>p&&(!ignoredNonArtwork||p.scale[0]!==1||p.scale[1]!==1||p.offset[0]!==0||p.offset[1]!==0||p.rotation!==0)))))return unavailable('A required hat artwork texture is missing.');
  const artStates=artworkDependencies.map((dependency,slot)=>{
   if(!placements[slot].some(groups=>groups.some(Boolean)))return null;
   const state={slot,settled:false,promise:null};
   state.promise=(hatArtworkResources.has(dependency.name)?Promise.resolve(hatArtworkResources.get(dependency.name)):cached(gameRoot,`clothing/${dependency.name}.iff`,'texture',p=>createIffTextureGallery(p,{maxTextures:8,maxBytes:16*1024*1024}),signal))
    .then(value=>{state.settled=true;return {value};},error=>{state.settled=true;return {error};});
   return state;
  });
  let materialTextures=null,materialTextureKeys=null;
  const separateSteampunk=slot&&geos[0].name.toLowerCase()==='goggles_steampunk_geo';
  if(separateSteampunk){
   // Use the matching named source texture for each material, never the frame
   // photograph for lenses or the transparent extra surface.
   if(!/^goggles_steampunk_solid_/.test(asset))return unavailable('This finish needs a verified metallic or iridescent material response.');
   const dep=item.dependencies.find(d=>d.kind===1&&d.name==='goggles_steampunk_tex_2');
   if(!dep)return unavailable('The separate lens and extra textures are missing.');
   const extraTextures=await cached(gameRoot,`clothing/${dep.name}.iff`,'texture',p=>createIffTextureGallery(p,{maxTextures:8,maxBytes:16*1024*1024}),signal);sources.push(extraTextures.source);
   const candidates=[...options.map(texture=>({texture,key:tex.key})),...extraTextures.value.textures.map(texture=>({texture,key:extraTextures.key}))];
   materialTextures=mesh.value.meshes.map(m=>m.groups.map(g=>{const stem=g.material.replace(/_mat$/,'_color');const matches=candidates.filter(x=>x.texture.name.replace(/\.[a-f0-9]{16}\.dds$/i,'')===stem);return matches.length===1?matches[0]:null;}));
   if(materialTextures.some(groups=>groups.some(x=>!x)))return unavailable('A matching material texture is missing or ambiguous.');
   materialTextureKeys=materialTextures.map(groups=>groups.map(x=>`${x.key}:${x.texture.name}`));
  }
  const palettes=mesh.value.meshes.map((m,i)=>m.groups.map((g,j)=>slot?accessorySupport.palette(itemBindings,crc32(Buffer.from(g.material)),materialTextures?.[i]?.[j]?.texture.range||selectedTexture?.range):garmentPalette(itemBindings,crc32(Buffer.from(g.material)),selectedTexture?.range)));
  if(slot&&palettes.some(groups=>groups.some(p=>!p)))return unavailable('The accessory color material is not supported yet.');
  let meshKey=mesh.key,previewMeshes=mesh.value.meshes;
  if(slot&&accessorySupport.headGeometry(geos[0].name)){
   try{
    const [sourceRig,targetRig]=await Promise.all([cached(gameRoot,geo,'rig',accessoryRig.readHeadRig,signal),cached(gameRoot,'char/sig/png04090.iff','rig',accessoryRig.readHeadRig,signal)]);
    const offset=accessoryRig.headOffset(sourceRig.value,targetRig.value);meshKey=mesh.key+':head-bind-v2:'+body.key;
    previewMeshes=accessoryRig.translateMeshes(mesh.value.meshes,offset);sources.push(sourceRig.source,targetRig.source);
   }catch(error){if(signal.aborted)throw error;return unavailable('The head attachment bind pose is incompatible.');}
  }
  const texture=selectedTexture&&knownTextures.has(tex.key)?null:selectedTexture;
  const part={asset,...(slot?{accessorySlot:slot,placement:'body-space'}:{}),meshKey,textureKey,meshes:knownMeshes.has(meshKey)?null:previewMeshes,palettes,texture};
  if(materialTextures){part.materialTextureKeys=materialTextureKeys;part.materialTextures=materialTextures.map((groups,i)=>groups.map((x,j)=>knownTextures.has(materialTextureKeys[i][j])?null:x.texture));}
  if(staticEmission){
   const dependencies=item.dependencies.filter(d=>d.kind===1&&/emissive/.test(d.name));
   if(dependencies.length!==1)return unavailable('The emission texture binding is missing or ambiguous.');
   const emissionResource=await cached(gameRoot,`clothing/${dependencies[0].name}.iff`,'texture',p=>createIffTextureGallery(p,{maxTextures:8,maxBytes:16*1024*1024}),signal);sources.push(emissionResource.source);
   const candidates=emissionResource.value.textures.filter(t=>/emissive/.test(t.name)&&/_c\.[a-f0-9]{16}\.dds$/i.test(t.name));if(candidates.length!==1)return unavailable('The emission texture could not be decoded.');
   const emissionTexture=candidates[0],emissionPalettes=mesh.value.meshes.map(m=>m.groups.map(g=>accessorySupport.emission(itemBindings,crc32(Buffer.from(g.material)),emissionTexture.range)));
   if(emissionPalettes.some(groups=>groups.some(x=>!x)))return unavailable('The emission color, intensity or alpha mask is not verified.');
   part.emissionTextureKey=`${emissionResource.key}:${emissionTexture.name}`;part.emissionTexture=knownTextures.has(part.emissionTextureKey)?null:emissionTexture;part.emissionPalettes=emissionPalettes;
  }
  const bigLogo=tiled?.value.textures?.find(x=>/^jersey_big_logo_color\./i.test(x.name));
  const patchLogo=tiled?.value.textures?.find(x=>/^jersey_logo_color\./i.test(x.name));
  if(bigLogo){part.jerseyBigLogoKey=`${tiled.key}:big`;part.jerseyBigLogo=bigLogo;}
  if(patchLogo){part.jerseyPatchKey=`${tiled.key}:patch`;part.jerseyPatch=patchLogo;}
  const numberAtlas=font?.value.textures?.find(x=>/^font_number_color\./i.test(x.name));
  if(numberAtlas){part.jerseyNumber=playerNumber.name.split(':')[1];part.jerseyNumberAtlasKey=`${font.key}:number`;part.jerseyNumberAtlas=numberAtlas;}
  placements.forEach((value,slot)=>{const suffix=slot?String(slot+1):'';part[`overlayKey${suffix}`]='';part[`overlay${suffix}`]=null;part[`overlayPlacements${suffix}`]=value;});
  return {part,...(slot?{accessory:{asset,slot,state:'shown',reason:staticEmission?'Static emissive preview on the default mannequin; game bloom is not reproduced.':/^mask_(gaiter|masquerade)_/.test(asset)?'Neutral static fit on the default mannequin; animated facial morphs are not reproduced.':'Static preview on the default mannequin.'}}:{}),
   artStates:artStates.filter(Boolean),
   warning:textureKey?'':`${asset}: color texture unavailable; neutral material shown.`,sources};
 });
 const artworkJobs=[];
 const outcomes=await Promise.allSettled(jobs);
 for(const [jobIndex,outcome] of outcomes.entries()){
  if(outcome.status==='rejected'){
   if(signal.aborted)throw outcome.reason;
   const failedAsset=assets[jobIndex],slot=accessorySlots.slot(failedAsset);
   if(slot)result.accessories.push({asset:failedAsset,slot,state:'unavailable',reason:'The accessory resource could not be decoded.'});
   else result.warnings.push(`A garment preview could not be prepared: ${outcome.reason.message}`);continue;
  }
  const value=outcome.value;
  if(value.accessory)result.accessories.push(value.accessory);
  if(value.part)result.parts.push(value.part);
  if(value.part)for(const state of value.artStates||[])artworkJobs.push({asset:value.part.asset,part:value.part,state});
  if(value.warning)result.warnings.push(value.warning);
  for(const source of value.sources||[])result.stats.cache[source]++;
 }
 if(artworkJobs.some(job=>!job.state.settled)){
  let timer;
  await Promise.race([Promise.all(artworkJobs.map(job=>job.state.promise)),new Promise(resolve=>{timer=setTimeout(resolve,80);})]);
  clearTimeout(timer);
  if(artworkJobs.some(job=>!job.state.settled)){
   signal.throwIfAborted();result.stats.ms=Math.round(performance.now()-start);
   trace('base-progress',{parts:result.parts.length});
   emitProgress({...result,progress:true});
  }
 }
 for(const job of artworkJobs){
  const art=await job.state.promise;
  if(art.error){if(signal.aborted)throw art.error;if(job.part.accessorySlot==='headwear'){result.parts=result.parts.filter(p=>p!==job.part);const status=result.accessories.find(a=>a.asset===job.asset);Object.assign(status,{state:'unavailable',reason:'Required hat artwork could not be decoded.'});}else result.warnings.push(`${job.asset}: artwork texture unavailable.`);continue;}
  result.stats.cache[art.value.source]++;
  // Resource names can hide bundles containing only a normal map. Never use
  // that map as photographic artwork simply because it was decoded first.
  const artworkTextures=art.value.value.textures||[];
  const colorArtwork=artworkTextures.filter(t=>!/(?:normal|rough|metal|mask|rdmo|coneangle|_n[._])/i.test(t.name));
  const selectedArtwork=colorArtwork.length===1?colorArtwork[0]:null;
  const placementGroups=job.part[`overlayPlacements${job.state.slot===0?'':String(job.state.slot+1)}`]||[];
  const defaultPlacement=placementGroups.flat().filter(Boolean).every(p=>p.scale[0]===1&&p.scale[1]===1&&p.offset[0]===0&&p.offset[1]===0&&p.rotation===0);
  if(!selectedArtwork&&artworkTextures.length&&colorArtwork.length===0&&defaultPlacement)continue;
  if(!selectedArtwork){if(job.part.accessorySlot==='headwear'){result.parts=result.parts.filter(p=>p!==job.part);const status=result.accessories.find(a=>a.asset===job.asset);Object.assign(status,{state:'unavailable',reason:'Required hat artwork could not be decoded.'});}continue;}
  const suffix=job.state.slot===0?'':String(job.state.slot+1);
  job.part[`overlayKey${suffix}`]=art.value.key;
  job.part[`overlay${suffix}`]=knownTextures.has(art.value.key)?null:selectedArtwork;
 }
 signal.throwIfAborted();
 if(result.stats.cache.decoded)await prune().catch(()=>{});
 result.stats.ms=Math.round(performance.now()-start);
 trace('complete',{cache:result.stats.cache});
 return result;
}
async function work(message){
 active={id:message.id,controller:new AbortController()};
 try{parentPort.postMessage({id:message.id,payload:await render(message.request,active.controller.signal,payload=>parentPort.postMessage({id:message.id,progress:true,payload}))});}
 catch(error){parentPort.postMessage({id:message.id,error:active.controller.signal.aborted?'Preview superseded.':error.message});}
 finally{active=null;if(next){const queued=next;next=null;work(queued);}else kickWarmQueue();}
}
parentPort.on('message',message=>{
 if(message?.cancel){active?.controller.abort();next=null;warmRequested=false;prefetchQueue=[];pendingPrefetch.clear();prefetched.clear();return;}
 if(typeof message?.prefetch==='string'){
  if(/^[a-zA-Z0-9_]{1,200}$/.test(message.prefetch)){
   prefetchQueue=[];pendingPrefetch.clear();prefetched.clear();pendingPrefetch.add(message.prefetch);
   warmRequested=true;kickWarmQueue();
  }
  return;
 }
 if(!Number.isSafeInteger(message?.id)||!message.request)return;
 if(active){active.controller.abort();if(next)parentPort.postMessage({id:next.id,error:'Preview superseded.'});next=message;}
 else work(message);
});
