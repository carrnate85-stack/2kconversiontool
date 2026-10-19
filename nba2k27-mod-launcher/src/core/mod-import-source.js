'use strict';
const fsp=require('node:fs/promises'),path=require('node:path'),crypto=require('node:crypto');
const {execFile}=require('node:child_process'),{promisify}=require('node:util');
const run=promisify(execFile);
const ARCHIVE_EXTENSIONS=new Set(['.zip','.rar']);
const MAX_ENTRIES=4096,MAX_BYTES=4*1024*1024*1024,MAX_ARCHIVE_BYTES=4*1024*1024*1024;
const missing=error=>error?.code==='ENOENT'?null:Promise.reject(error);
const same=(a,b)=>path.resolve(a).toLowerCase()===path.resolve(b).toLowerCase();
function archiveType(file){const extension=path.extname(String(file||'')).toLowerCase();return ARCHIVE_EXTENSIONS.has(extension)?extension.slice(1):null;}
function safeEntry(value){const normalized=String(value||'').replaceAll('\\','/').replace(/^\.\//,'').replace(/\/+$/,'');if(!normalized||normalized.startsWith('/')||/^[a-z]:/i.test(normalized))return null;const parts=normalized.split('/');if(parts.some(part=>!part||part==='.'||part==='..'||part.includes(':')||/[\u0000-\u001f]/.test(part)))return null;return normalized;}
function tarExecutable(){return path.join(process.env.SystemRoot||'C:\\Windows','System32','tar.exe');}
async function executeTar(executable,args){try{return await run(executable,args,{windowsHide:true,timeout:120000,maxBuffer:32*1024*1024,encoding:'utf8'});}catch(error){const detail=String(error.stderr||error.message||'Archive extraction failed.').trim().split(/\r?\n/).at(-1);throw Error('Could not read this archive: '+detail);}}
function inspectVerboseListing(text){
 let total=0,records=0;
 for(const line of String(text||'').split(/\r?\n/).filter(Boolean)){
  const type=line[0];if(type!=='-'&&type!=='d')throw Error('Archive links and special entries are not supported.');
  const match=/^\S+\s+\d+\s+\d+\s+\d+\s+(\d+)\s+/.exec(line);if(!match)throw Error('The archive file list could not be verified.');
  const size=Number(match[1]);if(!Number.isSafeInteger(size)||size<0||(total+=size)>MAX_BYTES)throw Error('The archive expands beyond the 4 GB safety limit.');records++;
 }
 return {records,totalBytes:total};
}
async function inspectArchive(file,{executable=tarExecutable()}={}){
 const resolved=path.resolve(file),type=archiveType(resolved),stat=await fsp.lstat(resolved).catch(missing);
 if(!type)throw Error('Only ZIP and RAR archives are supported.');
 if(!stat?.isFile()||stat.isSymbolicLink()||!same(await fsp.realpath(resolved),resolved))throw Error('The selected archive must be a plain local file.');
 if(stat.size<1||stat.size>MAX_ARCHIVE_BYTES)throw Error('The selected archive exceeds the 4 GB safety limit.');
 const [{stdout:list},{stdout:verbose}]=await Promise.all([executeTar(executable,['-tf',resolved]),executeTar(executable,['-tvf',resolved])]);
 const entries=String(list||'').split(/\r?\n/).filter(Boolean),seen=new Set();
 if(!entries.length)throw Error('The selected archive is empty.');
 if(entries.length>MAX_ENTRIES)throw Error('The archive exceeds the '+MAX_ENTRIES.toLocaleString()+' entry safety limit.');
 for(const entry of entries){const safe=safeEntry(entry);if(!safe)throw Error('The archive contains an unsafe path.');const key=safe.toLowerCase();if(seen.has(key))throw Error('The archive contains duplicate Windows paths.');seen.add(key);}
 const verified=inspectVerboseListing(verbose);if(verified.records!==entries.length)throw Error('The archive file list changed while it was being checked.');
 return {file:resolved,type,entries,totalBytes:verified.totalBytes};
}
async function collectFolder(directory,{includeRootName=false}={}){
 const root=path.resolve(directory),rootStat=await fsp.lstat(root).catch(missing);
 if(!rootStat?.isDirectory()||rootStat.isSymbolicLink()||!same(await fsp.realpath(root),root))throw Error('The selected mod folder must be a plain local directory.');
 const prefix=includeRootName?path.basename(root):'',pending=[{absolute:root,relative:''}],sources=[];let totalBytes=0,fileCount=0;
 while(pending.length){
  const current=pending.pop(),entries=await fsp.readdir(current.absolute,{withFileTypes:true});entries.sort((a,b)=>a.name.localeCompare(b.name));
  for(const entry of entries){
   const absolute=path.join(current.absolute,entry.name),relative=path.join(current.relative,entry.name),stat=await fsp.lstat(absolute);
   if(entry.isSymbolicLink()||stat.isSymbolicLink())throw Error('Mod packages cannot contain linked files or folders.');
   if(entry.isDirectory()){if(!same(await fsp.realpath(absolute),absolute))throw Error('Mod packages cannot contain linked folders.');pending.push({absolute,relative});continue;}
   if(!entry.isFile()||!stat.isFile()||!same(await fsp.realpath(absolute),absolute))throw Error('Mod packages may contain only plain files and folders.');
   if(stat.size===0)continue;
   if(++fileCount>MAX_ENTRIES)throw Error('The mod package exceeds the '+MAX_ENTRIES.toLocaleString()+' file safety limit.');
   if((totalBytes+=stat.size)>MAX_BYTES)throw Error('The mod package exceeds the 4 GB safety limit.');
   const packagePath=safeEntry(path.join(prefix,relative));if(!packagePath)throw Error('The mod package contains an unsafe path.');
   sources.push({source:absolute,relativePath:packagePath,package:true});
  }
 }
 return {sources,totalBytes};
}
async function prepareModSources({paths,mode='files',temporaryRoot,executable=tarExecutable()}){
 const selected=[...new Set((Array.isArray(paths)?paths:[]).map(value=>path.resolve(String(value||''))))];
 if(!selected.length)throw Error('Choose a mod folder, archive, or file.');
 const temporary=await fsp.mkdtemp(path.join(path.resolve(temporaryRoot),'2k-mod-import-'+crypto.randomUUID()+'-')),sources=[];let archiveCount=0;
 const cleanup=()=>fsp.rm(temporary,{recursive:true,force:true,maxRetries:10,retryDelay:100});
 try{
  if(mode==='folder'){
   if(selected.length!==1)throw Error('Choose one mod folder at a time.');
   sources.push(...(await collectFolder(selected[0],{includeRootName:true})).sources);
  }else{
   for(const selectedPath of selected){
    if(archiveType(selectedPath)){
     await inspectArchive(selectedPath,{executable});const output=path.join(temporary,'archive-'+archiveCount++);await fsp.mkdir(output);
     await executeTar(executable,['--no-same-owner','--no-same-permissions','-xf',selectedPath,'-C',output]);
     sources.push(...(await collectFolder(output)).sources);
    }else sources.push({source:selectedPath,relativePath:null,package:false});
   }
  }
  if(!sources.length)throw Error('No files were found in this mod package.');
  return {sources,archiveCount,folderCount:mode==='folder'?1:0,cleanup};
 }catch(error){await cleanup();throw error;}
}
module.exports={ARCHIVE_EXTENSIONS,MAX_ENTRIES,archiveType,safeEntry,inspectVerboseListing,inspectArchive,collectFolder,prepareModSources,tarExecutable};
