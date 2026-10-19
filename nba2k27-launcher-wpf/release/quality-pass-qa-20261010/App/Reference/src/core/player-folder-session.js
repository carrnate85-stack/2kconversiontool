const fs=require('node:fs/promises');
const path=require('node:path');
const identity=s=>`${s.dev}:${s.ino}`;
// Windows may return the on-disk casing (Players) for a requested path (players).
// Case is not an ownership boundary there; junctions are still rejected below.
const samePath=(a,b)=>typeof a==='string'&&typeof b==='string'&&(process.platform==='win32'?path.resolve(a).toLowerCase()===path.resolve(b).toLowerCase():path.resolve(a)===path.resolve(b));
async function stat(file){try{return await fs.lstat(file,{bigint:true});}catch(error){if(error.code==='ENOENT')return null;throw error;}}
async function directoryExists(directory){
 const value=await stat(directory);if(!value)return false;
 if(!value.isDirectory()||value.isSymbolicLink()||!samePath(await fs.realpath(directory),directory))throw Error(`Unsafe player-folder directory: ${directory}`);
 return true;
}
class PlayerFolderSession {
 constructor({markerPath,isGameRunning}){this.markerPath=markerPath;this.isGameRunning=isGameRunning;this.queue=Promise.resolve();}
 exclusive(fn){const result=this.queue.then(fn);this.queue=result.catch(()=>{});return result;}
 async journal(value){await fs.mkdir(path.dirname(this.markerPath),{recursive:true});await fs.writeFile(this.markerPath+'.tmp',JSON.stringify(value));await fs.rename(this.markerPath+'.tmp',this.markerPath);}
 restore(){return this.exclusive(()=>this.restoreUnlocked());}
 async restoreUnlocked(){
  if(await this.isGameRunning())return false;
  let data;try{data=JSON.parse(await fs.readFile(this.markerPath,'utf8'));}catch(error){if(error.code==='ENOENT')return true;throw error;}
  if(data.schema!==1||!path.isAbsolute(data.root)||!Array.isArray(data.links)||data.links.length>10000)throw Error('Invalid player-folder recovery record.');
  const sig=path.join(data.root,'char','sig');
  for(const entry of data.links){
   if(typeof entry.name!=='string'||path.basename(entry.name)!==entry.name||!/^.+\.iff$/i.test(entry.name)||typeof entry.identity!=='string')throw Error('Invalid player-folder recovery path.');
  }
  // Never follow a replaced directory/junction during recovery.
  if(!await directoryExists(path.join(data.root,'char'))||!await directoryExists(sig))throw Error('Player-folder path changed; recovery paused.');
  for(const entry of data.links){
   const target=path.join(sig,entry.name),current=await stat(target);
   if(current?.isFile()&&!current.isSymbolicLink()&&identity(current)===entry.identity)await fs.unlink(target);
  }
  await fs.unlink(this.markerPath);return true;
 }
 prepare(modsRoot){return this.exclusive(async()=>{
  if(await this.isGameRunning())throw Error('Close NBA 2K27 before preparing player folders.');
  await this.restoreUnlocked();
  const root=await fs.realpath(modsRoot),sig=path.join(root,'char','sig');
  const char=path.join(root,'char'),players=path.join(root,'players');
  await directoryExists(char);
  const hasSig=await directoryExists(sig),hasPlayers=await directoryExists(players);
  const entries=hasSig?await fs.readdir(sig,{withFileTypes:true}):[],loose=new Set(entries.filter(e=>!e.isDirectory()).map(e=>e.name.toLowerCase())),candidates=new Map();let loosePriority=0;
  for(const sourceRoot of [...(hasSig?[sig]:[]),...(hasPlayers?[players]:[])]){
  const folders=sourceRoot===sig?entries:await fs.readdir(sourceRoot,{withFileTypes:true});
  for(const folder of folders.filter(e=>e.isDirectory()&&!e.isSymbolicLink()).sort((a,b)=>a.name.localeCompare(b.name))){
   const directory=path.join(sourceRoot,folder.name);if(!samePath(await fs.realpath(directory),directory))continue;
   const label=path.relative(root,directory);
   for(const file of await fs.readdir(directory,{withFileTypes:true})){
    if(!file.isFile()||file.isSymbolicLink()||!file.name.toLowerCase().endsWith('.iff'))continue;
    const key=file.name.toLowerCase();if(loose.has(key)){loosePriority++;continue;}
    if(candidates.has(key))throw Error(`Player-folder conflict: ${file.name} is in both "${candidates.get(key).folder}" and "${label}". Keep one copy or put your preferred copy loose in char/sig.`);
    if(candidates.size>=10000)throw Error('Player folders exceed the 10,000-file safety limit.');
    candidates.set(key,{name:file.name,folder:label,source:path.join(directory,file.name)});
   }
  }
  }
  if(!candidates.size)return {linked:0,loosePriority};
  // Create only the canonical destination, never through a junction/symlink.
  if(!await directoryExists(char))await fs.mkdir(char);
  if(!await directoryExists(sig))await fs.mkdir(sig);
  const journal={schema:1,root,links:[]};
  try{
   for(const item of candidates.values()){
    const sourceStat=await stat(item.source);if(!sourceStat?.isFile()||sourceStat.isSymbolicLink()||!samePath(await fs.realpath(item.source),item.source))throw Error('A player-folder file changed while preparing. Please retry.');
    journal.links.push({name:item.name,identity:identity(sourceStat)});
   }
   // Journal first, so even an interrupted launch can recover its exact links.
   await this.journal(journal);
   for(const item of candidates.values())await fs.link(item.source,path.join(sig,item.name));
   return {linked:candidates.size,loosePriority};
  }catch(error){await this.restoreUnlocked();throw Error(`Could not prepare player folders: ${error.message}`);}
 });}
}
module.exports={PlayerFolderSession};
