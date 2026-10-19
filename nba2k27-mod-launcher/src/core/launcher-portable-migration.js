'use strict';
const fsp=require('node:fs/promises');
const fs=require('node:fs');
const path=require('node:path');
const crypto=require('node:crypto');
const {layout}=require('./launcher-storage');
const {copyTree,copyLegacyTree,rebaseJson}=require('./launcher-appdata-migration');

const PENDING='.portable-import-pending';
const MANIFEST='portable-import-manifest.json';
const LEGACY_SKIP=new Set(['runtime-resources','Cache','Code Cache','GPUCache','DawnGraphiteCache','DawnWebGPUCache','blob_storage','Network','Session Storage','Local Storage','Shared Dictionary']);
const ORGANIZED_SKIP=new Set(['Cache','Runtime']);
const TRANSIENT=new Set(['SingletonLock','SingletonCookie','SingletonSocket']);
const inside=(parent,child)=>{const relative=path.relative(parent,child);return Boolean(relative)&&relative!=='..'&&!relative.startsWith('..'+path.sep)&&!path.isAbsolute(relative);};

async function digest(file){const hash=crypto.createHash('sha256');for await(const chunk of fs.createReadStream(file))hash.update(chunk);return hash.digest('hex');}
async function regularDirectory(file){const stat=await fsp.lstat(file);if(!stat.isDirectory()||stat.isSymbolicLink())throw Error(`Not a regular folder: ${file}`);}
async function commitFile(source,target){
 const sourceStat=await fsp.lstat(source);
 if(!sourceStat.isFile()||sourceStat.isSymbolicLink())throw Error(`Not a regular imported file: ${source}`);
 const existing=await fsp.lstat(target).catch(error=>{if(error.code==='ENOENT')return null;throw error;});
 if(existing){
  if(!existing.isFile()||existing.isSymbolicLink()||existing.size!==sourceStat.size||await digest(target)!==await digest(source))throw Error(`Portable import conflicts with an existing file: ${target}`);
  return;
 }
 await fsp.mkdir(path.dirname(target),{recursive:true});
 await fsp.copyFile(source,target,fs.constants.COPYFILE_EXCL);
 if(await digest(target)!==await digest(source))throw Error(`Portable import verification failed: ${target}`);
}
async function commitTree(source,target,omitSettings=false){
 const stat=await fsp.lstat(source);
 if(stat.isSymbolicLink())throw Error(`Linked import source: ${source}`);
 if(stat.isFile()){
  if(omitSettings&&path.basename(path.dirname(target)).toLowerCase()==='data'&&['settings.json',MANIFEST].includes(path.basename(target).toLowerCase()))return;
  return commitFile(source,target);
 }
 if(!stat.isDirectory())throw Error(`Unsupported import source: ${source}`);
 const existing=await fsp.lstat(target).catch(error=>{if(error.code==='ENOENT')return null;throw error;});
 if(existing)await regularDirectory(target);else await fsp.mkdir(target,{recursive:true});
 for(const name of await fsp.readdir(source))await commitTree(path.join(source,name),path.join(target,name),omitSettings);
}

async function migrateToPortable({targetRoot,sourceStorage,isGameRunning=async()=>false,onProgress=()=>{}}){
 if(!sourceStorage||!['legacy','organized'].includes(sourceStorage.mode))throw Error('No previous launcher library was selected for portable import.');
 if(await isGameRunning())throw Error('Close NBA 2K27 before importing the launcher library.');
 const target=path.resolve(targetRoot),legacy=sourceStorage.mode==='legacy';
 const source=path.resolve(legacy?sourceStorage.data:sourceStorage.root);
 await regularDirectory(target);await regularDirectory(source);
 if(source===target||inside(source,target)||inside(target,source))throw Error('Portable library must be separate from the previous library.');
 const sourceSettings=path.join(source,legacy?'settings.json':path.join('Data','settings.json'));
 if(!(await fsp.stat(sourceSettings)).isFile())throw Error('Previous launcher settings are missing.');
 const pending=path.join(target,'Data',PENDING);
 const existingManifest=path.join(target,'Data',MANIFEST);
 if(await fsp.lstat(pending).catch(()=>null)&&await fsp.lstat(existingManifest).catch(()=>null)){
  const manifest=JSON.parse(await fsp.readFile(existingManifest,'utf8'));
  if(manifest.schema!==1||path.resolve(manifest.source)!==source||!(await fsp.stat(path.join(target,'Data','settings.json')).catch(()=>null))?.isFile())throw Error('An incomplete portable import needs inspection. The previous library is untouched.');
  await fsp.rm(pending,{force:true});
  return {root:target,...manifest};
 }
 const stage=path.join(target,`.portable-import-${crypto.randomUUID()}`),stageRoots=layout(stage);
 const report={files:0,bytes:0,onProgress};
 await fsp.mkdir(stage,{recursive:false});
 try{
  const skip=file=>TRANSIENT.has(path.basename(file));
  for(const name of await fsp.readdir(source)){
   if((legacy?LEGACY_SKIP:ORGANIZED_SKIP).has(name)||TRANSIENT.has(name))continue;
   const from=path.join(source,name);
   if(legacy)await copyLegacyTree(from,name,stageRoots,report,skip);
   else await copyTree(from,path.join(stage,name),report,skip);
  }
  let videoSource=null;
  if(legacy){
   const settings=JSON.parse(await fsp.readFile(sourceSettings,'utf8'));
   if(typeof settings.gameRoot==='string'&&path.isAbsolute(settings.gameRoot)){
    const candidate=path.join(path.resolve(settings.gameRoot),'mods','presentation');
    const stat=await fsp.lstat(candidate).catch(error=>{if(error.code==='ENOENT')return null;throw error;});
    if(stat){
     if(!stat.isDirectory()||stat.isSymbolicLink())throw Error('The old startup-video folder is not a regular directory.');
     if(!await fsp.lstat(stageRoots.videos).catch(()=>null)){await copyTree(candidate,stageRoots.videos,report,skip);videoSource=candidate;}
    }
   }
  }
  await rebaseJson(stage,source,target,legacy,layout(target),videoSource);
  if(!(await fsp.stat(path.join(stage,'Data','settings.json')).catch(()=>null))?.isFile())throw Error('Imported settings are missing.');
  if(await isGameRunning())throw Error('NBA 2K27 opened during import; the previous library is unchanged.');
  const manifest={schema:1,source,sourcePreserved:true,importedAt:new Date().toISOString(),files:report.files,bytes:report.bytes,rebuilt:['Cache','Runtime']};
  await fsp.writeFile(path.join(stage,'Data',MANIFEST),JSON.stringify(manifest,null,2)+'\n',{flag:'wx'});
  await fsp.mkdir(path.join(target,'Data'),{recursive:true});
  await fsp.writeFile(pending,JSON.stringify({schema:1,source})+'\n');
  for(const name of await fsp.readdir(stage))await commitTree(path.join(stage,name),path.join(target,name),true);
  await commitFile(path.join(stage,'Data','settings.json'),path.join(target,'Data','settings.json'));
  await commitFile(path.join(stage,'Data',MANIFEST),existingManifest);
  if(await digest(path.join(target,'Data',MANIFEST))!==await digest(path.join(stage,'Data',MANIFEST)))throw Error('Portable import manifest verification failed.');
  await fsp.rm(pending,{force:true});
  return {root:target,...manifest};
 }finally{
  if(!inside(target,stage)||!path.basename(stage).startsWith('.portable-import-'))throw Error('Unsafe portable staging path.');
  await fsp.rm(stage,{recursive:true,force:true});
 }
}
module.exports={PENDING,MANIFEST,migrateToPortable};
