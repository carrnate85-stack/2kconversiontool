'use strict';
const fs=require('node:fs/promises'),path=require('node:path'),os=require('node:os');
const MAX_FILE_BYTES=256*1024,MAX_FILES=40;
const allowedExtensions=new Set(['.json','.jsonl','.log','.txt','.ini']);
const sensitiveKey=/(?:token|secret|password|credential|authorization|cookie|key)$/i;
const pathKey=/(?:path|root|directory|folder|file|target)$/i;

function leaf(value){const normalized=String(value||'').replaceAll('/','\\').replace(/[\\]+$/,'');return normalized.split('\\').pop()||'<redacted>';}
function replaceKnownPaths(value,knownPaths=[]){
 let text=String(value??'');
 const variants=[...new Set(knownPaths.filter(Boolean).flatMap(item=>{const value=String(item);return [value,value.replaceAll('\\','/'),value.replaceAll('\\','\\\\')];}))].sort((a,b)=>b.length-a.length);
 for(const candidate of variants)text=text.replaceAll(candidate,'<redacted-path>');
 return text;
}
function redactText(value,home=os.homedir(),knownPaths=[]){
 let text=replaceKnownPaths(value,knownPaths);
 if(home)text=text.replaceAll(home,'<user-profile>').replaceAll(home.replaceAll('\\','\\\\'),'<user-profile>');
 text=text.replace(/[A-Za-z]:\\Users\\[^\\\s"']+/gi,'<user-profile>');
 text=text.replace(/((?:token|secret|password|authorization|cookie|api[_-]?key)\s*[=:]\s*)[^\s,;]+/gi,'$1<redacted>');
 return text;
}
function redactValue(value,key='',knownPaths=[]){
 if(sensitiveKey.test(key))return '<redacted>';
 if(value===null||value===undefined||typeof value==='number'||typeof value==='boolean')return value;
 if(typeof value==='string')return pathKey.test(key)&&path.isAbsolute(value)?`<redacted-path>\\${leaf(value)}`:redactText(value,os.homedir(),knownPaths);
 if(Array.isArray(value))return value.map(item=>redactValue(item,key,knownPaths));
 if(typeof value==='object')return Object.fromEntries(Object.entries(value).map(([child,item])=>[child,redactValue(item,child,knownPaths)]));
 return String(value);
}
function safeSettings(settings={}){
 return {gamePathConfigured:Boolean(settings.gameRoot),modsPathConfigured:Boolean(settings.modsRoot),profileCount:Array.isArray(settings.modsProfiles)?settings.modsProfiles.length:0,
  provider:settings.provider||null,startCameras:settings.startCameras!==false,cachePreparedPlaylists:settings.cachePreparedPlaylists!==false,
  unlockWnbaFreestyle:Boolean(settings.unlockWnbaFreestyle),unlockWnbaBlacktop:Boolean(settings.unlockWnbaBlacktop),autoOpenBlacktop:Boolean(settings.autoOpenBlacktop)};
}
async function readDiagnostic(file,knownPaths=[]){
 const stat=await fs.stat(file),bytes=Math.min(stat.size,MAX_FILE_BYTES),handle=await fs.open(file,'r');let buffer;
 try{buffer=Buffer.alloc(bytes);await handle.read(buffer,0,bytes,Math.max(0,stat.size-bytes));}finally{await handle.close();}
 const raw=buffer.toString('utf8'),trimmed=raw.trim();let content;
 try{content=redactValue(JSON.parse(trimmed),'',knownPaths);}catch{content=redactText(raw,os.homedir(),knownPaths);}
 return {name:path.basename(file),modifiedUtc:stat.mtime.toISOString(),originalBytes:stat.size,truncated:stat.size>bytes,content};
}
async function createSupportBundle({userData,version,settings,validation,diagnosticDirectory=path.join(userData,'diagnostics')}){
 const knownPaths=[userData,settings?.gameRoot,settings?.modsRoot,...(Array.isArray(settings?.modsProfiles)?settings.modsProfiles.map(profile=>profile?.path):[])].filter(Boolean);
 let entries=[];try{entries=(await fs.readdir(diagnosticDirectory,{withFileTypes:true})).filter(entry=>entry.isFile()&&allowedExtensions.has(path.extname(entry.name).toLowerCase()));}catch(error){if(error.code!=='ENOENT')throw error;}
 const ranked=(await Promise.all(entries.map(async entry=>({entry,stat:await fs.stat(path.join(diagnosticDirectory,entry.name))})))).sort((a,b)=>b.stat.mtimeMs-a.stat.mtimeMs).slice(0,MAX_FILES);
 const diagnostics=[];for(const {entry} of ranked)diagnostics.push(await readDiagnostic(path.join(diagnosticDirectory,entry.name),knownPaths));
 const bundle={schema:'2k-launcher-support-v1',createdUtc:new Date().toISOString(),launcherVersion:String(version||'unknown'),privacy:{pathsRedacted:true,secretsRedacted:true,maxDiagnosticBytes:MAX_FILE_BYTES,maxFiles:MAX_FILES},system:{platform:process.platform,arch:process.arch,osRelease:os.release(),electron:process.versions.electron||null,node:process.versions.node},settings:safeSettings(settings),validation:redactValue(validation||null,'',knownPaths),diagnostics};
 return {bundle,text:JSON.stringify(bundle,null,2)+'\n',files:diagnostics.length,truncated:diagnostics.filter(row=>row.truncated).length};
}
module.exports={MAX_FILE_BYTES,MAX_FILES,createSupportBundle,redactText,redactValue,safeSettings};
