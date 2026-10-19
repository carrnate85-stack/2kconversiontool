'use strict';
const fs=require('node:fs/promises'),path=require('node:path'),crypto=require('node:crypto');
function digest(bytes){return crypto.createHash('sha256').update(bytes).digest('hex').toUpperCase();}
function relative(value){
 if(typeof value!=='string'||!value||value.includes('\\')||path.posix.isAbsolute(value)||value.split('/').some(part=>!part||part==='.'||part==='..'))throw Error('Bundled runtime resource path is invalid.');
 return value;
}
async function hashFile(file){return digest(await fs.readFile(file));}
async function writeAtomic(file,bytes){
 const temporary=`${file}.${crypto.randomUUID()}.tmp`;let handle;
 try{handle=await fs.open(temporary,'wx');await handle.writeFile(bytes);await handle.sync();await handle.close();handle=null;await fs.rename(temporary,file);}
 finally{if(handle)await handle.close().catch(()=>{});await fs.unlink(temporary).catch(error=>{if(error.code!=='ENOENT')throw error;});}
}
async function pruneRuntimeResourceVersions(targetRoot,keep=2){
 const parent=path.dirname(targetRoot),current=path.basename(targetRoot);let entries;
 try{entries=await fs.readdir(parent,{withFileTypes:true});}catch(error){if(error.code==='ENOENT')return [];throw error;}
 const candidates=[];
 for(const entry of entries){
  if(!entry.isDirectory()||entry.name===current)continue;
  const directory=path.join(parent,entry.name),manifestPath=path.join(directory,'manifest.json');
  try{const link=await fs.lstat(directory);if(link.isSymbolicLink())continue;const manifest=JSON.parse(await fs.readFile(manifestPath,'utf8'));if(manifest?.schema!==1||!Array.isArray(manifest.files))continue;const stat=await fs.stat(manifestPath);candidates.push({directory,modified:stat.mtimeMs});}catch{}
 }
 candidates.sort((a,b)=>b.modified-a.modified);
 const removed=[];
 for(const item of candidates.slice(Math.max(0,keep-1))){await fs.rm(item.directory,{recursive:true,force:true});removed.push(item.directory);}
 return removed;
}
async function ensureRuntimeResources({sourceRoot,targetRoot}){
 const manifest=JSON.parse(await fs.readFile(path.join(sourceRoot,'manifest.json'),'utf8'));
 if(manifest?.schema!==1||!Array.isArray(manifest.files)||manifest.files.length<1)throw Error('Bundled runtime resource manifest is invalid.');
 await fs.mkdir(targetRoot,{recursive:true});
 for(const record of manifest.files){
  const name=relative(record?.path);if(!Number.isSafeInteger(record.size)||record.size<1||!/^[A-F0-9]{64}$/.test(record.sha256||''))throw Error('Bundled runtime resource record is invalid.');
  const source=path.join(sourceRoot,...name.split('/')),target=path.join(targetRoot,...name.split('/'));let current=false;
  try{const stat=await fs.stat(target);current=stat.isFile()&&stat.size===record.size&&await hashFile(target)===record.sha256;}catch(error){if(error.code!=='ENOENT')throw error;}
  if(current)continue;
  const bytes=await fs.readFile(source);if(bytes.length!==record.size||digest(bytes)!==record.sha256)throw Error(`Bundled runtime resource failed verification: ${name}`);
  await fs.mkdir(path.dirname(target),{recursive:true});const temporary=target+'.'+crypto.randomUUID()+'.tmp';
  try{const handle=await fs.open(temporary,'wx');try{await handle.writeFile(bytes);await handle.sync();}finally{await handle.close();}await fs.unlink(target).catch(error=>{if(error.code!=='ENOENT')throw error;});await fs.rename(temporary,target);}finally{await fs.unlink(temporary).catch(error=>{if(error.code!=='ENOENT')throw error;});}
  if(await hashFile(target)!==record.sha256)throw Error(`Cached runtime resource failed verification: ${name}`);
 }
 const manifestPath=path.join(targetRoot,'manifest.json'),manifestBytes=Buffer.from(JSON.stringify(manifest));
 const existing=await fs.readFile(manifestPath).catch(()=>null);
 if(!existing?.equals(manifestBytes))await writeAtomic(manifestPath,manifestBytes);
 // Preserve the previous Electron runtime versions during WPF promotion.
 if(process.env.WPF_NATIVE_HOST!=='1')await pruneRuntimeResourceVersions(targetRoot);
 return targetRoot;
}
module.exports={ensureRuntimeResources,digest,relative,pruneRuntimeResourceVersions};
