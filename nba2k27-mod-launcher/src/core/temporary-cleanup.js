'use strict';
const fs=require('node:fs/promises');
const path=require('node:path');
// Startup only, before conversion workers start. Never sweep drafts, archives or backups.
async function cleanupTemporaryFiles(root,now=Date.now()){
  let removed=0;
  const list=async p=>fs.readdir(p,{withFileTypes:true}).catch(e=>{if(e.code==='ENOENT')return [];throw e;});
  for(const [relative,pattern] of [['blacktop/thumbnail-cache',/^decode-[a-zA-Z0-9]{6}$/],['blacktop/fit-preview-temp',/^fit-[a-zA-Z0-9]{6}$/],['blacktop/fit-preview-cache',/^fit-[a-zA-Z0-9]{6}$/],['jukebox',/^artwork-\d+-\d+$/]]){
    const parent=path.resolve(root,relative);
    const stat=await fs.lstat(parent).catch(()=>null);if(!stat||!stat.isDirectory()||stat.isSymbolicLink())continue;
    for(const entry of await list(parent)){
      if(!entry.isDirectory()||entry.isSymbolicLink()||!pattern.test(entry.name))continue;
      const target=path.resolve(parent,entry.name);
      if(path.dirname(target)!==parent)throw Error('Temporary cleanup path escaped storage');
      if(now-(await fs.lstat(target)).mtimeMs<86400000)continue;
      const children=await list(target);
      // Decode folders contain flat generated files. Refuse nested directories/links.
      if(children.some(x=>!x.isFile()||x.isSymbolicLink()))continue;
      for(const child of children){await fs.unlink(path.join(target,child.name));removed++;}
      await fs.rmdir(target);
    }
  }
  const pcm=path.resolve(root,'jukebox/game-session/native-pcm-v1');
  const stat=await fs.lstat(pcm).catch(()=>null);
  if(stat?.isDirectory()&&!stat.isSymbolicLink())for(const entry of await list(pcm)){
    if(!entry.isFile()||! /^[a-f0-9]{64}\.wav\.tmp$/.test(entry.name))continue;
    const target=path.join(pcm,entry.name);
    if(now-(await fs.lstat(target)).mtimeMs>=86400000){await fs.unlink(target);removed++;}
  }
  return {removed};
}
module.exports={cleanupTemporaryFiles};
