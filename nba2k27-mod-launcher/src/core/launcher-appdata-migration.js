'use strict';
const fs=require('node:fs');
const fsp=require('node:fs/promises');
const path=require('node:path');
const crypto=require('node:crypto');
const {pipeline}=require('node:stream/promises');
const {Transform}=require('node:stream');
const {appDataRoot,layout}=require('./launcher-storage');
const {destinationFor}=require('./launcher-storage-migration');

async function copyVerified(source,target,report){
 const before=await fsp.lstat(source);
 if(!before.isFile()||before.isSymbolicLink())throw Error(`Refusing non-regular source: ${source}`);
 await fsp.mkdir(path.dirname(target),{recursive:true});
 const hash=crypto.createHash('sha256');
 await pipeline(fs.createReadStream(source),new Transform({transform(chunk,_encoding,done){hash.update(chunk);done(null,chunk);}}),fs.createWriteStream(target,{flags:'wx'}));
 const after=await fsp.lstat(source),copied=await fsp.stat(target);
 if(before.size!==after.size||before.mtimeMs!==after.mtimeMs||copied.size!==before.size)throw Error(`Source changed during migration: ${source}`);
 const expected=hash.digest('hex'),check=crypto.createHash('sha256');
 for await(const chunk of fs.createReadStream(target))check.update(chunk);
 if(expected!==check.digest('hex'))throw Error(`Migration copy failed verification: ${source}`);
 report.files++;report.bytes+=before.size;report.onProgress({phase:'copying',files:report.files,bytes:report.bytes});
}

async function copyTree(source,target,report,skip=()=>false){
 const stat=await fsp.lstat(source);
 if(stat.isSymbolicLink())throw Error(`Refusing linked source: ${source}`);
 if(stat.isFile())return copyVerified(source,target,report);
 if(!stat.isDirectory())throw Error(`Refusing unsupported source: ${source}`);
 await fsp.mkdir(target,{recursive:true});
 for(const name of await fsp.readdir(source))if(!skip(path.join(source,name)))await copyTree(path.join(source,name),path.join(target,name),report,skip);
}

async function copyLegacyTree(source,relative,roots,report,skip=()=>false){
 const stat=await fsp.lstat(source);
 if(stat.isSymbolicLink())throw Error(`Refusing linked source: ${source}`);
 const target=destinationFor(relative,roots);
 if(stat.isFile())return copyVerified(source,target,report);
 if(!stat.isDirectory())throw Error(`Refusing unsupported source: ${source}`);
 await fsp.mkdir(target,{recursive:true});
 for(const name of await fsp.readdir(source))if(!skip(path.join(source,name)))await copyLegacyTree(path.join(source,name),path.join(relative,name),roots,report,skip);
}

function rebaseValue(value,sourceRoot,targetRoot,legacy,roots,videoSource=null){
 if(typeof value==='string'&&path.isAbsolute(value)){
  const candidate=path.resolve(value),prefix=sourceRoot+path.sep;
  if(videoSource&&(candidate.toLowerCase()===videoSource.toLowerCase()||candidate.toLowerCase().startsWith((videoSource+path.sep).toLowerCase())))
   return path.join(targetRoot,'Library','Videos',path.relative(videoSource,candidate));
  if(candidate.toLowerCase().startsWith(prefix.toLowerCase())){
   const relative=path.relative(sourceRoot,candidate);
   return legacy?destinationFor(relative,roots):path.join(targetRoot,relative);
  }
 }
 if(Array.isArray(value))return value.map(item=>rebaseValue(item,sourceRoot,targetRoot,legacy,roots,videoSource));
 if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([key,item])=>[key,rebaseValue(item,sourceRoot,targetRoot,legacy,roots,videoSource)]));
 return value;
}

async function rebaseJson(root,sourceRoot,targetRoot,legacy,roots,videoSource){
 const visit=async directory=>{
  for(const name of await fsp.readdir(directory)){
   const file=path.join(directory,name),stat=await fsp.lstat(file);
   if(stat.isDirectory())await visit(file);
   else if(stat.isFile()&&name.toLowerCase().endsWith('.json')){
    const original=await fsp.readFile(file,'utf8');let parsed;
    try{parsed=JSON.parse(original);}catch{continue;}
    const updated=rebaseValue(parsed,sourceRoot,targetRoot,legacy,roots,videoSource);
    if(JSON.stringify(parsed)!==JSON.stringify(updated))await fsp.writeFile(file,JSON.stringify(updated,null,2)+'\n','utf8');
   }
  }
 };
 for(const name of ['Data','Library']){
  const directory=path.join(root,name);
  if(await fsp.stat(directory).catch(()=>null))await visit(directory);
 }
}

async function migrateToAppData({appData,sourceStorage,onProgress=()=>{},isGameRunning=async()=>false}){
 if(sourceStorage?.needsAppDataMigration!==true)throw Error('No AppData migration is pending.');
 if(await isGameRunning())throw Error('Close NBA 2K27 before moving the launcher library.');
 const legacy=sourceStorage.mode==='legacy',sourceRoot=path.resolve(legacy?sourceStorage.data:sourceStorage.root);
 const final=appDataRoot(appData),stage=path.join(appData,`2k Launcher.migrating-${crypto.randomUUID()}`);
 const sourceStat=await fsp.lstat(sourceRoot);
 if(!sourceStat.isDirectory()||sourceStat.isSymbolicLink())throw Error('Launcher source is not a regular folder.');
 if(await fsp.lstat(final).catch(error=>{if(error.code==='ENOENT')return null;throw error;}))throw Error('AppData launcher folder already exists; refusing to merge libraries.');
 const settingsSource=path.join(sourceRoot,legacy?'settings.json':path.join('Data','settings.json'));
 if(!(await fsp.stat(settingsSource)).isFile())throw Error('Launcher settings are missing from the old library.');
 const report={files:0,bytes:0,onProgress};
 await fsp.mkdir(stage,{recursive:false});
 try{
  const roots=layout(stage);
  for(const name of await fsp.readdir(sourceRoot)){
   // Cache and helper binaries are reproducible; migrating old versions would
   // add gigabytes and preserve stale executable code in the new library.
   if(!legacy&&['Cache','Runtime'].includes(name))continue;
   if(legacy&&['runtime-resources','Cache','Code Cache','GPUCache','DawnGraphiteCache','DawnWebGPUCache','blob_storage','Network','Session Storage','Local Storage','Shared Dictionary'].includes(name))continue;
   const source=path.join(sourceRoot,name);
   if(legacy)await copyLegacyTree(source,name,roots,report);
   else await copyTree(source,path.join(stage,name),report);
  }
  let videoSource=null;
  if(legacy){
   const settings=JSON.parse(await fsp.readFile(settingsSource,'utf8'));
   if(typeof settings.gameRoot==='string'&&path.isAbsolute(settings.gameRoot)){
    const gameRoot=path.resolve(settings.gameRoot),candidate=path.join(gameRoot,'mods','presentation');
    const videoStat=await fsp.lstat(candidate).catch(error=>{if(error.code==='ENOENT')return null;throw error;});
    if(videoStat){
     if(!videoStat.isDirectory()||videoStat.isSymbolicLink())throw Error('The old startup-video folder is not a regular directory.');
     if(!(await fsp.stat(roots.videos).catch(()=>null))){
      await copyTree(candidate,roots.videos,report);
      videoSource=candidate;
     }
    }
   }
  }
  await rebaseJson(stage,sourceRoot,final,legacy,layout(final),videoSource);
  if(await isGameRunning())throw Error('NBA 2K27 opened during migration; the old library was preserved. Close the game and retry.');
  if(!(await fsp.stat(path.join(stage,'Data','settings.json')).catch(()=>null))?.isFile())throw Error('Migrated settings are missing.');
  await fsp.mkdir(path.join(stage,'Data'),{recursive:true});
  const manifest={schema:1,source:sourceRoot,videoSource,migratedAt:new Date().toISOString(),files:report.files,bytes:report.bytes,sourcePreserved:true,rebuilt:['Cache','Runtime']};
  await fsp.writeFile(path.join(stage,'Data','appdata-migration-manifest.json'),JSON.stringify(manifest,null,2)+'\n',{flag:'wx'});
  await fsp.rename(stage,final);
  onProgress({phase:'complete',...manifest});
  return {root:final,...manifest};
 }catch(error){
  await fsp.rm(stage,{recursive:true,force:true}).catch(()=>{});
  throw error;
 }
}
module.exports={migrateToAppData,rebaseValue,copyTree,copyLegacyTree,rebaseJson};
