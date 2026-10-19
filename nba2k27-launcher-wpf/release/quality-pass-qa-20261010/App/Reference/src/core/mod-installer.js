'use strict';
const fs=require('node:fs'),fsp=require('node:fs/promises'),path=require('node:path'),crypto=require('node:crypto'),readline=require('node:readline');
const {classifyAsset}=require('./catalog');
const MAX_FILES=4096,MAX_FILE_BYTES=2*1024*1024*1024,MAX_TOTAL_BYTES=4*1024*1024*1024,MAX_MANIFEST_BYTES=512*1024*1024,MAX_MANIFEST_ENTRIES=2000000;
const missing=error=>error?.code==='ENOENT'?null:Promise.reject(error);
const same=(a,b)=>path.resolve(a).toLowerCase()===path.resolve(b).toLowerCase();
function manifestMatchForSource(matches,source){if(matches.length===1)return matches[0];if(matches.length<2)return null;const parent=path.basename(path.dirname(source)).toLowerCase();const narrowed=matches.filter(value=>path.posix.basename(path.posix.dirname(value)).toLowerCase()===parent);return narrowed.length===1?narrowed[0]:null;}
function loosePlayerLogical(name){return /^png\d{4,6}(?=[_.]|$).*\.iff$/i.test(name)?'char/sig/'+name:null;}
function safeLogical(value){const normalized=String(value||'').replaceAll('\\','/').replace(/^\/+|\/+$/g,'');const parts=normalized.split('/');if(!normalized||parts.some(part=>!part||part==='.'||part==='..'||part.includes(':')||/[\u0000-\u001f]/.test(part)))return null;return normalized;}
function rootedPackageLogical(relativePath,roots){const logical=safeLogical(relativePath);if(!logical)return null;const parts=logical.split('/');for(let index=0;index<parts.length;index++)if(roots?.has(parts[index].toLowerCase()))return safeLogical(parts.slice(index).join('/'));return null;}
async function plainChain(target,message){let current=path.parse(path.resolve(target)).root;for(const part of path.resolve(target).slice(current.length).split(path.sep).filter(Boolean)){current=path.join(current,part);const stat=await fsp.lstat(current).catch(missing);if(stat?.isSymbolicLink())throw Error(message);}return target;}
async function manifestLeafIndex(gameRoot){
 const manifest=path.join(path.resolve(gameRoot),'manifest'),stat=await fsp.lstat(manifest).catch(missing);if(!stat?.isFile()||stat.isSymbolicLink()||stat.size<1||stat.size>MAX_MANIFEST_BYTES)throw Error('The selected game folder has no usable stock manifest.');
 const index=new Map(),roots=new Set(),input=fs.createReadStream(manifest,{encoding:'utf8'}),lines=readline.createInterface({input,crlfDelay:Infinity});let entries=0;
 try{for await(const line of lines){const comma=line.indexOf(',');if(comma<=0)continue;if(++entries>MAX_MANIFEST_ENTRIES)throw Error('The stock manifest exceeds the installer safety limit.');const logical=safeLogical(line.slice(0,comma));if(!logical)continue;roots.add(logical.split('/')[0].toLowerCase());const key=path.posix.basename(logical).toLowerCase(),values=index.get(key)||[];if(!values.some(value=>value.toLowerCase()===logical.toLowerCase()))values.push(logical);index.set(key,values);}}
 finally{lines.close();input.destroy();}
 Object.defineProperty(index,'roots',{value:roots});
 return index;
}
async function planModInstall({gameRoot,modsRoot,sources}){
 const selected=[],seenSources=new Set();for(const value of Array.isArray(sources)?sources:[]){const source=path.resolve(String(value?.source||value||'')),relativePath=value&&typeof value==='object'?safeLogical(value.relativePath):null,key=source.toLowerCase()+'|'+String(relativePath||'').toLowerCase();if(!seenSources.has(key)){seenSources.add(key);selected.push({source,relativePath,package:value?.package===true});}}if(!selected.length||selected.length>MAX_FILES)throw Error(`Choose between 1 and ${MAX_FILES} mod files.`);
 const root=path.resolve(modsRoot),rootStat=await fsp.lstat(root).catch(missing);if(!rootStat?.isDirectory()||rootStat.isSymbolicLink()||!same(await fsp.realpath(root),root))throw Error('The active mods folder is not a plain local directory.');await plainChain(root,'Mod installation refuses linked folders.');
 const index=await manifestLeafIndex(gameRoot),items=[],issues=[],ignored=[],destinations=new Set();let totalBytes=0;
 for(const descriptor of selected){
  const source=descriptor.source;
  const name=path.basename(source);let stat;try{stat=await fsp.lstat(source);}catch(error){issues.push({name,reason:error.code==='ENOENT'?'File no longer exists.':error.message});continue;}
  if(!stat.isFile()||stat.isSymbolicLink()||!same(await fsp.realpath(source),source)){issues.push({name,reason:'Source must be a plain local file.'});continue;}
  if(stat.size<1||stat.size>MAX_FILE_BYTES||(totalBytes+=stat.size)>MAX_TOTAL_BYTES){issues.push({name,reason:'File selection exceeds the installer size limit.'});continue;}
  const matches=index.get(name.toLowerCase())||[],structuredPath=/\.(?:iff|bin|dds)$/i.test(name)?rootedPackageLogical(descriptor.relativePath,index.roots):null,manifestPath=!structuredPath?manifestMatchForSource(matches,source):null,playerPath=!structuredPath&&!manifestPath&&matches.length===0?loosePlayerLogical(name):null;
  const logicalPath=structuredPath||manifestPath||playerPath;if(!logicalPath){if(descriptor.package&&!/\.iff$/i.test(name)){ignored.push({name,reason:'Package documentation or preview file.'});continue;}issues.push({name,reason:matches.length?'Filename is ambiguous in the stock manifest.':'Filename is not listed in the stock manifest and is not a recognized PNG player file.'});continue;}
  const destination=path.resolve(root,...logicalPath.split('/')),relative=path.relative(root,destination);if(!relative||relative.startsWith('..')||path.isAbsolute(relative)){issues.push({name,reason:'Resolved destination is outside the mods folder.'});continue;}
  const destinationKey=destination.toLowerCase();if(destinations.has(destinationKey)){issues.push({name,reason:'Another selected file resolves to the same destination.'});continue;}destinations.add(destinationKey);
  const existing=await fsp.lstat(destination).catch(missing);if(existing&&(existing.isSymbolicLink()||!existing.isFile())){issues.push({name,reason:'Destination is not a plain file.'});continue;}
  items.push({source,name,logicalPath,destination,routing:structuredPath?'package-path':manifestPath?'manifest':'player-char-sig',category:classifyAsset(logicalPath),size:stat.size,modifiedMs:stat.mtimeMs,existing:existing?{size:existing.size,modifiedMs:existing.mtimeMs}:null});
 }
 if(!items.length&&!issues.length)issues.push({name:'Package',reason:'No installable mod files were found.'});
 return {root,items,issues,ignored,totalBytes:items.reduce((sum,item)=>sum+item.size,0),replacements:items.filter(item=>item.existing).length};
}
async function installModPlan(plan,{replace=false}={}){
 if(!plan||!Array.isArray(plan.items)||plan.issues?.length)throw Error('Resolve every mod routing issue before installing.');if(plan.replacements&&!replace)throw Error('Existing mod files require replacement confirmation.');
 const staged=[];try{for(const item of plan.items){const current=await fsp.lstat(item.source).catch(missing);if(!current?.isFile()||current.isSymbolicLink()||current.size!==item.size||current.mtimeMs!==item.modifiedMs)throw Error(`${item.name} changed after it was selected.`);await fsp.mkdir(path.dirname(item.destination),{recursive:true});await plainChain(path.dirname(item.destination),'Mod installation refuses linked destination folders.');const temporary=path.join(path.dirname(item.destination),`.mod-install-${crypto.randomUUID()}.tmp`);await fsp.copyFile(item.source,temporary,fs.constants.COPYFILE_EXCL);staged.push({...item,temporary,backup:null});}}catch(error){for(const item of staged)await fsp.unlink(item.temporary).catch(()=>{});throw error;}
 const committed=[];try{
  for(const item of staged){const current=await fsp.lstat(item.destination).catch(missing);if(Boolean(current)!==Boolean(item.existing)||current&&(current.isSymbolicLink()||!current.isFile()||current.size!==item.existing.size||current.mtimeMs!==item.existing.modifiedMs))throw Error(`${item.logicalPath} changed while the install was being prepared.`);if(current){item.backup=path.join(path.dirname(item.destination),`.mod-install-${crypto.randomUUID()}.bak`);await fsp.rename(item.destination,item.backup);}committed.push(item);await fsp.rename(item.temporary,item.destination);}
 }catch(error){for(const item of [...committed].reverse()){await fsp.unlink(item.destination).catch(()=>{});if(item.backup)await fsp.rename(item.backup,item.destination).catch(()=>{});}throw error;
 }finally{for(const item of staged)await fsp.unlink(item.temporary).catch(()=>{});}
 for(const item of committed)if(item.backup)await fsp.unlink(item.backup).catch(()=>{});
 return {installed:committed.length,replaced:committed.filter(item=>item.existing).length,items:committed.map(({name,logicalPath,category,size})=>({name,logicalPath,category,size}))};
}
async function resolveCatalogFile(modsRoot,catalog,fileId){
 const root=path.resolve(modsRoot),id=String(fileId||'').replaceAll('\\','/').toLowerCase();if(!catalog||!same(catalog.root,root))throw Error('Scan the active mod library again.');const file=catalog.files?.find(value=>value.id===id);if(!file)throw Error('That mod file is no longer in the scanned library.');const target=path.resolve(root,...file.relativePath.replaceAll('\\','/').split('/')),relative=path.relative(root,target);if(!relative||relative.startsWith('..')||path.isAbsolute(relative))throw Error('The mod file resolved outside the active library.');const stat=await fsp.lstat(target).catch(missing);if(!stat?.isFile()||stat.isSymbolicLink()||!same(await fsp.realpath(target),target))throw Error('That mod file is no longer a plain local file.');return target;
}
module.exports={manifestLeafIndex,manifestMatchForSource,loosePlayerLogical,rootedPackageLogical,planModInstall,installModPlan,resolveCatalogFile,safeLogical};
