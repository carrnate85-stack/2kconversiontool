'use strict';
const {parentPort,workerData}=require('node:worker_threads');
const fs=require('node:fs/promises'),path=require('node:path'),{promisify}=require('node:util');
const exec=promisify(require('node:child_process').execFile);
const {loadIff,readZipEntry,crc32}=require('../vendor/archive-preview/core/iff');
const {createIffPreview}=require('../vendor/archive-preview/core/preview');
const {resolveClothingItems}=require('./clothing-item-table');
const {clothingBindings,garmentPalette}=require('./clothing-color-bindings');
const {FitMemoryCache}=require('./fit-memory-cache');
const memory=new FitMemoryCache(),sources=new Map();let manifestCache=null,signature='',abort=null,busy=false;
const stamp=async p=>{const s=await fs.stat(p);return `${s.size}:${s.mtimeMs}:${s.ctimeMs}`;};
async function main(data){
 const {gameRoot,cacheRoot,decoderPath,assets}=data;
 const started=performance.now(),timings=[];let hits=0;
 const time=async(label,fn)=>{const start=performance.now();const result=await fn();timings.push({stage:label,ms:Math.round(performance.now()-start)});return result;};
 await fs.mkdir(cacheRoot,{recursive:true});if((await fs.lstat(cacheRoot)).isSymbolicLink())throw Error('Preview storage cannot be a link.');
 const temp=await fs.mkdtemp(path.join(cacheRoot,'fit-'));
 try{
  const manifestPath=path.join(gameRoot,'manifest');if((await fs.stat(manifestPath)).size>128*1024*1024)throw Error('Unsupported manifest size.');
  const nextSignature=gameRoot+':'+await stamp(manifestPath)+':'+await stamp(decoderPath);
  let changed=signature!==nextSignature;
  if(!changed)for(const [file,old]of sources){if(await stamp(file)!==old){changed=true;break;}}
  if(changed){memory.clear();sources.clear();manifestCache=null;signature=nextSignature;}
  const entries=manifestCache||new Map();
  if(!manifestCache){
  for(const line of (await fs.readFile(manifestPath,'utf8')).split(/\r?\n/)){
   const m=/^([^,]+),([A-Z0-9]{2}),(\d+),(\d+)$/.exec(line);if(!m)continue;
   if(!m[1].startsWith('clothing/')&&m[1]!=='char/sig/png04090.iff')continue;
   const offset=Number(m[3]),size=Number(m[4]);if(Number.isSafeInteger(offset)&&offset>=0&&Number.isSafeInteger(size)&&size>0)entries.set(m[1],{archive:m[2],offset,size});
  }
  manifestCache=entries;}
  const decoded=new Map();let decodedBytes=0;
  async function decode(name){
   abort.signal.throwIfAborted();if(decoded.has(name))return decoded.get(name);
   const e=entries.get(name);if(!e||e.size>32*1024*1024)throw Error('A required preview resource is unavailable.');
   const archive=path.join(gameRoot,e.archive);if(e.offset+e.size>(await fs.stat(archive)).size)throw Error('Preview archive range is invalid.');sources.set(archive,await stamp(archive));
   const target=path.join(temp,String(decoded.size)+'.iff');
   const task=(async()=>{
   try{await time('extract '+name,()=>exec(decoderPath,[archive,String(e.offset),String(e.size),path.join(gameRoot,'data','oodle','oo2core_9_win64.dll'),target],{windowsHide:true,timeout:30000,maxBuffer:1024*1024,signal:abort.signal}));}
   catch(error){if(abort.signal.aborted)throw error;throw Error('Could not decode preview resource '+name+'.');}
   const bytes=(await fs.stat(target)).size;decodedBytes+=bytes;
   if(bytes>64*1024*1024||decodedBytes>192*1024*1024)throw Error('This outfit exceeds the first-pass preview size limit.');
   return target;})();decoded.set(name,task);return task;
  }
  let tables=memory.get('tables'),body=memory.get('body');if(tables)hits++;if(body)hits++;
  // Two independent native extractions at most; wait for both before cleanup,
  // including failures/cancellation so no helper can write into a removed folder.
  const prefetch=await Promise.allSettled([tables?null:decode('clothing/clothing_itemtable.iff'),body?null:decode('char/sig/png04090.iff')]);
  const failure=prefetch.find(r=>r.status==='rejected');if(failure)throw failure.reason;
  if(!tables){const table=await loadIff(await decode('clothing/clothing_itemtable.iff'));
   const member=name=>{const e=table.entries.find(e=>e.name===name);if(!e)throw Error('Unsupported clothing table.');return readZipEntry(table.buffer,e,true);};
   tables={casuals:member('clothing_item_table.CasualsTable'),items:member('item_table.ConsolidatedClothingItems'),colors:member('binding_table_global.ConsolidatedClothingBinding')};memory.set('tables',tables);
  }
  // structuredClone represents cached Buffers as Uint8Array.
  for(const key of Object.keys(tables))if(!Buffer.isBuffer(tables[key]))tables[key]=Buffer.from(tables[key]);
  const resolved=resolveClothingItems(tables.casuals,tables.items,assets);
  const bindings=clothingBindings(tables.colors,[...resolved.values()].map(item=>item.id));
  const payload={meshes:[],textures:[],materials:[],warnings:[],stats:{triangles:0,modelsDecoded:0},prototype:true};
  for(const [i,asset] of assets.entries()){
   abort.signal.throwIfAborted();
   const item=resolved.get(asset),geometries=item?.dependencies.filter(d=>d.kind===2)||[];
   // Only explicitly male geometry is accepted in this first pass. Unresolved
   // footwear/accessories are reported, never guessed or silently substituted.
   if(geometries.length!==1||!/^male_[a-zA-Z0-9_]+_geo$/.test(geometries[0].name)){
    payload.warnings.push(asset.replaceAll('_',' ')+': preview geometry not supported yet.');continue;
   }
   const geo='clothing/geo/standard/'+geometries[0].name+'.iff';
   if(!entries.has(geo)){payload.warnings.push(asset.replaceAll('_',' ')+': standard mesh unavailable.');continue;}
   const textureDependencies=item.dependencies.filter(d=>d.kind===1&&!/normal|coneangle|rdmo|region|mask|wrinkle/.test(d.name)).slice(0,1);
   const exactColor=textureDependencies.some(d=>[asset+'_color_tex',asset+'_tex'].includes(d.name));
   const key=JSON.stringify([geo,textureDependencies.map(d=>d.name)]);
   let part=memory.get(key);
   if(part)hits++;else{
   const names=textureDependencies.map(d=>'clothing/'+d.name+'.iff').filter(name=>entries.has(name));
   const extracted=await Promise.allSettled([decode(geo),...names.map(decode)]);
   const failed=extracted.find(r=>r.status==='rejected');if(failed)throw failed.reason;
   const geoPath=extracted[0].value;
   const companions=names.map((name,i)=>({name,path:extracted[i+1].value,kind:'texture'}));
   part=await time('mesh '+asset,()=>createIffPreview(geoPath,{companionPaths:companions,skipTextures:!companions.length,maxTriangles:80000,maxModels:8}));
   memory.set(key,part);
   }
   if(!part.meshes.length){payload.warnings.push(asset.replaceAll('_',' ')+': no readable mesh.');continue;}
   const color=part.textures.find(t=>t.binding?.channel==='color');
   const materials=[];
   let paletteApplied=false;
   for(const mesh of part.meshes){mesh.name=asset;for(const group of mesh.groups){group.originalMaterial=group.material;group.garmentPalette=garmentPalette(bindings.get(item.id),crc32(Buffer.from(group.material)));if(group.garmentPalette)paletteApplied=true;group.material='fitpiece'+i+'material'+materials.length;materials.push(group.material);}}
   if(color){color.binding=undefined;color.assignments=materials.map(material=>({material,channel:'color'}));payload.textures.push(color);}
   else payload.warnings.push(asset.replaceAll('_',' ')+': shown in neutral colors; tint recipe pending.');
   if(color&&!exactColor&&!paletteApplied)payload.warnings.push(asset.replaceAll('_',' ')+': base texture only; game tint and decals may differ.');
   payload.meshes.push(...part.meshes);payload.stats.triangles+=part.stats.triangles;
  }
  if(!payload.meshes.length)throw Error('None of these garments have supported male preview geometry yet.');
  if(!body){const bodyPath=await decode('char/sig/png04090.iff');body=await time('mannequin mesh',()=>createIffPreview(bodyPath,{skipTextures:true,maxTriangles:100000,maxModels:8}));memory.set('body',body);}
  if(!body.meshes.length)throw Error('The stock male preview model could not be read.');
  for(const mesh of body.meshes){mesh.name='Male mannequin';mesh.sourceKind='fit-base';for(const g of mesh.groups){g.originalMaterial=g.material;g.material='fit_mannequin';}}
  payload.meshes.push(...body.meshes);payload.stats.triangles+=body.stats.triangles;payload.stats.modelsDecoded=payload.meshes.length;
  payload.warnings.push('Static fit approximation: body morphs, clipping rules and some shoes are not supported yet.');
  if(payload.stats.triangles>500000)throw Error('This outfit is too complex for the first-pass preview.');
  payload.stats.preparationMs=Math.round(performance.now()-started);payload.stats.cacheHits=hits;payload.stats.cacheBytes=memory.bytes;payload.stats.timings=timings;
  return payload;
 }finally{await fs.rm(temp,{recursive:true,force:true});}
}
async function run(data){
 if(busy)return;busy=true;abort=new AbortController();const timer=setTimeout(()=>abort.abort(),90000);
 try{const payload=await main(data);abort.signal.throwIfAborted();parentPort.postMessage({payload});}
 catch(error){parentPort.postMessage({error:abort.signal.aborted?'Preview cancelled.':error.code==='ENOENT'?'A required game or preview file is missing. Check your game folder in Settings.':error.message});}
 finally{clearTimeout(timer);busy=false;abort=null;}
}
parentPort.on('message',m=>{if(m==='cancel')abort?.abort();else if(m?.render)run(m.render);});
run(workerData);
