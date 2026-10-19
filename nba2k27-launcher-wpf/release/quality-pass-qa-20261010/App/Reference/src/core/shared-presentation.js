'use strict';
const fs=require('node:fs');
const fsp=require('node:fs/promises');
const path=require('node:path');
const crypto=require('node:crypto');
// Legacy recovery/discovery only. New startup playback never stages game files.
const presentationPaths=root=>({directory:path.join(root,'presentation'),movie:path.join(root,'presentation','startup.mov')});

const same=(a,b)=>path.resolve(a).toLowerCase()===path.resolve(b).toLowerCase();
function resolvePresentationRoot(libraryRoot,marker){
  // While another profile is live, the original default folder is temporarily
  // parked at backupPath. Continue reading that library, not the live junction.
  if(marker?.backupPath && ['moved','active'].includes(marker.stage) && same(libraryRoot,marker.liveMods))return marker.backupPath;
  return libraryRoot;
}
async function findPresentationLibrary(settings,marker,onWarning=()=>{}){
  if(settings.presentationLibraryRoot)return settings.presentationLibraryRoot;
  const candidates=new Set([path.join(settings.gameRoot,'mods'),settings.modsRoot,...(settings.modsProfiles||[]).map(p=>p.path)]);
  let selected=settings.modsRoot,best=-1;
  for(const root of candidates){
    const physical=resolvePresentationRoot(root,marker),file=path.join(presentationPaths(physical).directory,'library.json');
    let count=0;
    try {const data=JSON.parse(await fsp.readFile(file,'utf8'));if(!Array.isArray(data.items))throw Error('Invalid startup video library: '+file);count=data.items.length;}
    catch(error){
      // A stale/inaccessible profile must not prevent the launcher from opening.
      // Never modify its library; surface the problem and inspect the next one.
      if(error.code!=='ENOENT'){onWarning(error);continue;}
      const movie=await fsp.stat(presentationPaths(physical).movie).catch(()=>null);count=movie?.isFile()?1:0;
    }
    if(count>best){best=count;selected=root;}
  }
  return selected;
}
async function digest(file){const hash=crypto.createHash('sha256');for await(const chunk of fs.createReadStream(file))hash.update(chunk);return hash.digest('hex');}
async function plainTarget(root,name){
  for(const file of [root,path.join(root,'presentation'),path.join(root,'presentation',name)]){
    const stat=await fsp.lstat(file).catch(error=>{if(error.code==='ENOENT')return null;throw error;});
    if(stat?.isSymbolicLink())throw Error('Shared startup video refuses linked target files or folders.');
  }
  return path.join(root,'presentation',name);
}
class SharedPresentationSession {
  constructor({directory,isGameRunning}){this.directory=path.resolve(directory);this.marker=path.join(this.directory,'shared-movie.json');this.isGameRunning=isGameRunning;}
  async restore(){
    if(await this.isGameRunning())return false;
    let marker;try{marker=JSON.parse(await fsp.readFile(this.marker,'utf8'));}catch(error){if(error.code==='ENOENT')return true;throw error;}
    if(marker.schema!==1||!path.isAbsolute(marker.root)||!/^launcher_shared_startup_[a-f0-9-]{36}\.mov$/.test(marker.name)||!/^[a-f0-9]{64}$/.test(marker.hash))throw Error('Invalid shared startup video recovery record.');
    const target=await plainTarget(marker.root,marker.name);
    if(!same(await fsp.realpath(marker.root),marker.realRoot))throw Error('The startup video staging folder changed; it was left untouched.');
    const stat=await fsp.lstat(target).catch(error=>{if(error.code==='ENOENT')return null;throw error;});
    if(stat){
      if(!stat.isFile()||await digest(target)!==marker.hash)throw Error('The temporary startup video changed; it was left untouched.');
      await fsp.unlink(target);
    }
    await fsp.unlink(this.marker);
    return true;
  }
  async prepare({source,modsRoot}){
    if(await this.isGameRunning())throw Error('Close NBA 2K27 before preparing the shared startup video.');
    await this.restore();
    const root=path.resolve(modsRoot);
    if(same(source,presentationPaths(root).movie))return 'presentation/startup.mov';
    const name=`launcher_shared_startup_${crypto.randomUUID()}.mov`,target=await plainTarget(root,name);
    const marker={schema:1,root,realRoot:await fsp.realpath(root),name,hash:await digest(source)};
    await fsp.mkdir(path.dirname(target),{recursive:true});await fsp.mkdir(this.directory,{recursive:true});
    // Journal before copying; never overwrite an existing movie or user file.
    await fsp.writeFile(this.marker,JSON.stringify(marker),{flag:'wx'});
    try {
      await fsp.copyFile(source,target,fs.constants.COPYFILE_EXCL);
      if(await digest(target)!==marker.hash)throw Error('The shared startup video changed while it was being prepared.');
      return `presentation/${name}`;
    } catch(error){await this.restore().catch(()=>{});throw error;}
  }
}
module.exports={resolvePresentationRoot,findPresentationLibrary,SharedPresentationSession};
