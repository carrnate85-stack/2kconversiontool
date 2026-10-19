'use strict';
const fs=require('node:fs');
const fsp=require('node:fs/promises');
const path=require('node:path');
const crypto=require('node:crypto');
const {pipeline}=require('node:stream/promises');
const {Transform}=require('node:stream');
const {legacyRoot,pointerFile,storageRoot,layout,readLocation,ensureNativeLocationHint}=require('./launcher-storage');

const chromiumNames=new Set(['blob_storage','Cache','Code Cache','DawnGraphiteCache','DawnWebGPUCache','GPUCache','Local Storage','Network','Session Storage','Shared Dictionary','declarative_performance_observer.db','declarative_performance_observer.db-journal','DIPS','DIPS-journal','Local State','Preferences']);
function destinationFor(relative,roots){
 const parts=relative.split(path.sep),first=parts[0],rest=parts.slice(1);
 if(first==='videos')return path.join(roots.videos,...rest);
 if(first==='jukebox')return path.join(roots.music,...rest);
 if(first==='diagnostics')return path.join(roots.logs,...rest);
 if(first==='runtime-resources')return path.join(roots.runtime,...rest);
 if(first==='blacktop'&&rest[0]==='preview-packs')return path.join(roots.previews,...rest.slice(1));
 if(first==='blacktop'&&rest[0]==='outfit-presets.json')return path.join(roots.outfits,'outfit-presets.json');
 if(first==='blacktop'&&rest[0]==='thumbnail-cache')return path.join(roots.cache,'Blacktop','thumbnails',...rest.slice(1));
 if(first==='blacktop'&&rest[0]==='fit-preview-cache')return path.join(roots.cache,'Blacktop','fit-previews',...rest.slice(1));
 if(first==='blacktop'&&rest[0]==='ball-thumbnails')return path.join(roots.cache,'Blacktop','ball-thumbnails',...rest.slice(1));
 if(first==='player-editor'&&['art-cache','badge-icons','header-labels'].includes(rest[0]))return path.join(roots.cache,'Player Editor',rest[0],...rest.slice(1));
 if(first==='provider-layouts')return path.join(roots.cache,'provider-layouts',...rest);
 if(chromiumNames.has(first))return path.join(roots.chromium,...parts);
 return path.join(roots.data,...parts);
}
async function digest(file){const hash=crypto.createHash('sha256');for await(const chunk of fs.createReadStream(file))hash.update(chunk);return hash.digest('hex');}
async function copyVerified(source,target,report){
 const before=await fsp.lstat(source);
 if(!before.isFile()||before.isSymbolicLink())throw Error(`Refusing non-regular source: ${source}`);
 await fsp.mkdir(path.dirname(target),{recursive:true});
 const hash=crypto.createHash('sha256');
 await pipeline(fs.createReadStream(source),new Transform({transform(chunk,_encoding,done){hash.update(chunk);done(null,chunk);}}),fs.createWriteStream(target,{flags:'wx'}));
 const after=await fsp.lstat(source),copied=await fsp.lstat(target);
 if(before.size!==after.size||before.mtimeMs!==after.mtimeMs||copied.size!==before.size||hash.digest('hex')!==await digest(target))throw Error(`Source changed or copy verification failed: ${source}`);
 report.files++;report.bytes+=before.size;
 report.onProgress({phase:'copying',files:report.files,bytes:report.bytes,source});
}
async function copyTree(source,relative,roots,report){
 const stat=await fsp.lstat(source);
 if(stat.isSymbolicLink())throw Error(`Refusing linked source: ${source}`);
 const target=destinationFor(relative,roots);
 if(stat.isFile())return copyVerified(source,target,report);
 if(!stat.isDirectory())throw Error(`Refusing unsupported source: ${source}`);
 await fsp.mkdir(target,{recursive:true});
 for(const name of await fsp.readdir(source))await copyTree(path.join(source,name),path.join(relative,name),roots,report);
}
function rebase(value,oldRoot,newRoot){
 if(typeof value==='string'){
  const old=path.resolve(oldRoot),candidate=path.resolve(value);
  if(path.isAbsolute(value)&&candidate.toLowerCase().startsWith((old+path.sep).toLowerCase()))return path.join(newRoot,path.relative(old,candidate));
  return value;
 }
 if(Array.isArray(value))return value.map(item=>rebase(item,oldRoot,newRoot));
 if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([key,item])=>[key,rebase(item,oldRoot,newRoot)]));
 return value;
}
async function rebaseMusicJson(root,oldRoot,newRoot){
 const visit=async directory=>{for(const name of await fsp.readdir(directory)){const file=path.join(directory,name),stat=await fsp.lstat(file);if(stat.isDirectory())await visit(file);else if(stat.isFile()&&name.toLowerCase().endsWith('.json')){
   const original=await fsp.readFile(file,'utf8'),updated=JSON.stringify(rebase(JSON.parse(original),oldRoot,newRoot),null,2)+'\n';
   if(JSON.stringify(JSON.parse(original))!==JSON.stringify(JSON.parse(updated)))await fsp.writeFile(file,updated,'utf8');
  }}};
 await visit(root);
}
async function migrate({appData,gameRoot,videoDirectory,onProgress=()=>{}}){
 const game=path.resolve(gameRoot),source=legacyRoot(appData),final=storageRoot(game);
 if(!(await fsp.stat(path.join(game,'NBA2K27.exe'))).isFile())throw Error('Selected game folder has no NBA2K27.exe.');
 if(!(await fsp.stat(path.join(source,'settings.json'))).isFile())throw Error('Existing launcher settings are missing.');
 if(await fsp.lstat(pointerFile(appData)).catch(()=>null))throw Error('A launcher location pointer already exists.');
 if(await fsp.lstat(final).catch(()=>null))throw Error('The destination 2k Launcher folder already exists.');
 const videoSource=path.resolve(videoDirectory||path.join(game,'mods','presentation'));
 if(!videoSource.toLowerCase().startsWith((game+path.sep).toLowerCase()))throw Error('Video source must be inside the selected game folder.');
 const videoStat=await fsp.lstat(videoSource).catch(error=>{if(error.code==='ENOENT')return null;throw error;});
 if(videoStat&&(!videoStat.isDirectory()||videoStat.isSymbolicLink()))throw Error('Video source must be a regular directory.');
 const stage=path.join(game,`2k Launcher.migrating-${crypto.randomUUID()}`),roots=layout(stage),report={files:0,bytes:0,onProgress};
 await fsp.mkdir(stage,{recursive:false});
 for(const folder of Object.values(roots).slice(1))await fsp.mkdir(folder,{recursive:true});
 onProgress({phase:'started',stage,source,videoSource});
 try{
  for(const name of await fsp.readdir(source))await copyTree(path.join(source,name),name,roots,report);
  if(videoStat)for(const name of await fsp.readdir(videoSource))await copyTree(path.join(videoSource,name),path.join('videos',name),roots,report);
  await rebaseMusicJson(roots.music,path.join(source,'jukebox'),path.join(final,'Library','Music'));
  const manifest={schema:1,migratedAt:new Date().toISOString(),source,videoSource,files:report.files,bytes:report.bytes,originalsPreserved:true};
  await fsp.writeFile(path.join(roots.data,'migration-manifest.json'),JSON.stringify(manifest,null,2)+'\n',{flag:'wx'});
  await fsp.rename(stage,final);
  const pointer={schema:1,gameRoot:game,root:final};
  const temporary=path.join(appData,`.launcher-location-${crypto.randomUUID()}.tmp`);
  try{await fsp.writeFile(temporary,JSON.stringify(pointer,null,2)+'\n',{flag:'wx'});await fsp.rename(temporary,pointerFile(appData));}
  finally{await fsp.rm(temporary,{force:true}).catch(()=>{});}
  readLocation(appData);
  ensureNativeLocationHint(appData,{mode:'organized',root:final});
  onProgress({phase:'complete',...manifest,root:final});
  return {root:final,...manifest};
 }catch(error){error.migrationStage=stage;throw error;}
}
module.exports={chromiumNames,destinationFor,rebase,migrate};
