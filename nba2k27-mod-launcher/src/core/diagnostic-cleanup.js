'use strict';
const fs=require('node:fs/promises');
const path=require('node:path');
const {processIsRunning}=require('./session');
const owned=/^nba2k27_(?:saved_outfits|character_queue_noop|character_queue_thread|description_clone|description_edit|same_outfit_rebuild|change_top_rebuild|expand_copy|full_outfit)_\d+(?:_match\d+)?(?:_player\d+)?\.(?:log|plan|trigger)$/;
const proxy=/^nba2k27_version_proxy(?:_armed|_audit)?\.(?:log|trace)$/;
async function files(directory){
  try{const s=await fs.lstat(directory);if(!s.isDirectory()||s.isSymbolicLink())return [];
    const result=[];for(const name of await fs.readdir(directory)){
      const file=path.join(directory,name),stat=await fs.lstat(file);
      if(stat.isFile()&&!stat.isSymbolicLink())result.push({name,file,stat});
    }return result;
  }catch(e){if(e.code==='ENOENT')return [];throw e;}
}
// No recursive deletions. Only explicitly named generated files; never saved assets.
async function cleanupDiagnostics({directory,gameRoot,isGameRunning=()=>processIsRunning('NBA2K27.exe'),now=Date.now()}){
  if(await isGameRunning())return {skipped:true,removed:0};
  // Keep successful helper reports too: the game may have crashed afterward.
  // If preserving evidence fails, leave the originals untouched.
  await require('./session-evidence').archiveSessionLogs({directory,gameRoot});
  const storage=path.join(directory,'game-session');
  for(let current=path.resolve(storage);;current=path.dirname(current)){
    const stat=await fs.lstat(current).catch(e=>{if(e.code==='ENOENT')return null;throw e;});
    if(stat?.isSymbolicLink())throw Error('Diagnostic cleanup refuses linked storage');
    if(path.dirname(current)===current)break;
  }
  await fs.mkdir(storage,{recursive:true});
  let removed=0;
  for(const location of [gameRoot,storage].filter(Boolean))for(const entry of await files(location)){
    if(!owned.test(entry.name)&&!proxy.test(entry.name))continue;
    const failure=entry.name.endsWith('.log')&&entry.stat.size<=1024*1024&&
      /(?:^|\r?\n)STATE=(?:FAILED|INCONCLUSIVE)\b/.test(await fs.readFile(entry.file,'utf8'));
    if(failure&&now-entry.stat.mtimeMs<7*86400000){
      if(path.resolve(location)!==path.resolve(storage)){
        const destination=path.join(storage,entry.name);
        try{await fs.copyFile(entry.file,destination,require('node:fs').constants.COPYFILE_EXCL);}
        catch(e){if(e.code==='EEXIST'){
          const existing=await fs.lstat(destination);
          if(!existing.isFile()||existing.isSymbolicLink()||existing.size!==entry.stat.size||!(await fs.readFile(destination)).equals(await fs.readFile(entry.file)))continue;
        }else throw e;}
        await fs.unlink(entry.file);removed++;
      }
    }else{await fs.unlink(entry.file);removed++;}
  }
  for(const [location,pattern,maxCount,maxBytes] of [[storage,owned,20,5*1024*1024],[directory,/^blacktop-panel-[\d.]+-\d+\.jsonl$/,5,5*1024*1024]]){
    let count=0,bytes=0;
    for(const entry of (await files(location)).filter(x=>pattern.test(x.name)).sort((a,b)=>b.stat.mtimeMs-a.stat.mtimeMs)){
      count++;bytes+=entry.stat.size;
      if(count>maxCount||bytes>maxBytes||now-entry.stat.mtimeMs>7*86400000){await fs.unlink(entry.file);removed++;}
    }
  }
  return {skipped:false,removed};
}
module.exports={cleanupDiagnostics};
