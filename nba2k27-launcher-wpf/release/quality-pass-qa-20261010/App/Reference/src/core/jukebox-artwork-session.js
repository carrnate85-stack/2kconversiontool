'use strict';
const fs=require('node:fs/promises');
const path=require('node:path');
const crypto=require('node:crypto');
const digest=b=>crypto.createHash('sha256').update(b).digest('hex');
const logicalOk=s=>/^shared\/[a-f0-9]{2}\/[a-z0-9_]+\.[a-f0-9]{16}\.tld$/i.test(s);
async function readOptional(p){return fs.readFile(p).catch(e=>{if(e.code==='ENOENT')return null;throw e;});}
// Build private copies first; nothing in mods changes until the caller journals the plan.
async function prepareArtwork({libraryPath,gameRoot,modsRoot,sessionRoot,music,plain}){
  if(!libraryPath||!gameRoot)return {music,entries:[]};
  const raw=await readOptional(libraryPath);if(!raw)return {music,entries:[]};
  const state=JSON.parse(raw),root=path.resolve(modsRoot);
  const records=(state.artworkOverrides||[]).filter(r=>path.resolve(r.modsRoot||'')===root);
  const byBank=new Map(records.map(r=>[Number(r.bankIndex),r]));
  if(!music.Tracks.some(t=>byBank.get(Number(t['Bank Index']))?.storage==='library-v2'))return {music,entries:[]};
  const names=new Set((state.music?.Tracks||[]).filter(t=>Number(t.Collection)===1&&Number(t['Bank Index'])>=1&&Number(t['Bank Index'])<=71).map(t=>String(t['Cover Name']||'').toLowerCase()));
  const manifest=await fs.readFile(path.join(gameRoot,'manifest'),'utf8');
  const managed=new Set(records.filter(r=>r.storage!=='library-v2'&&logicalOk(r.logicalPath||'')&&path.resolve(r.absolutePath||'')===path.join(root,...r.logicalPath.split('/'))).map(r=>r.logicalPath.toLowerCase()));
  const slots=[],seen=new Set();
  for(const line of manifest.split(/\r?\n/)){
    const logical=line.split(',')[0].replaceAll('\\','/');if(!logicalOk(logical))continue;
    const cover=path.basename(logical).split('.')[0];if(!names.has(cover.toLowerCase())||seen.has(cover.toLowerCase()))continue;
    seen.add(cover.toLowerCase());
    const target=await plain(root,logical),original=await readOptional(target);
    if(original&&!managed.has(logical.toLowerCase()))continue; // Never borrow an unrelated mod's slot.
    slots.push({cover,logical,original});
  }
  const tracks=music.Tracks.map(t=>({...t})),wanted=tracks.filter(t=>byBank.has(Number(t['Bank Index'])));
  const preserved=new Set(tracks.filter(t=>!byBank.has(Number(t['Bank Index']))).map(t=>String(t['Cover Name']).toLowerCase()));
  const usable=slots.filter(s=>!preserved.has(s.cover.toLowerCase()));
  if(wanted.length>usable.length)throw new Error(`This playlist needs ${wanted.length} cover slots, but only ${usable.length} are available without replacing unrelated artwork mods. All artwork remains saved in the library.`);
  const cache=path.join(sessionRoot,'artwork-cache');await fs.mkdir(cache,{recursive:true});
  const entries=[];
  // Read every source before publishing: some old sources are slots about to be reused.
  for(let i=0;i<wanted.length;i++){
    const track=wanted[i],record=byBank.get(Number(track['Bank Index'])),slot=usable[i];
    let source;
    if(record.storage==='library-v2'){
      const profile=crypto.createHash('sha256').update(root).digest('hex').slice(0,16);
      source=path.join(path.dirname(libraryPath),'artwork-previews',profile,`${Number(track['Bank Index'])}.tld`);
    }else{if(!logicalOk(record.logicalPath||''))throw new Error('Invalid stored artwork resource path.');source=await plain(root,record.logicalPath);}
    const bytes=await fs.readFile(source);if(bytes.length<16||bytes.length>20*1048576)throw new Error('Stored cover artwork is missing or oversized.');
    const hash=digest(bytes),cachePath=path.join(cache,`${hash}.tld`);
    await fs.writeFile(cachePath,bytes);
    let backup=null,originalHash=null;
    if(slot.original){originalHash=digest(slot.original);backup=path.join(cache,`${originalHash}.tld`);await fs.writeFile(backup,slot.original);}
    entries.push({target:slot.logical,hash,cache:cachePath,originalHash,backup});
    track['Cover Name']=slot.cover;
  }
  return {music:{...music,Tracks:tracks},entries};
}
async function validateArtworkRestore(entries,{root,sessionRoot,plain}){
  if(!Array.isArray(entries)||entries.length>71)throw new Error('Invalid artwork recovery journal.');
  for(const e of entries){
    if(!logicalOk(e.target)||!/^[a-f0-9]{64}$/.test(e.hash)||
      (e.originalHash!==null&&!/^[a-f0-9]{64}$/.test(e.originalHash)))throw new Error('Invalid artwork recovery entry.');
    const target=await plain(root,e.target),current=await readOptional(target),currentHash=current?digest(current):null;
    if(currentHash!==null&&currentHash!==e.hash&&currentHash!==e.originalHash)throw new Error('Session artwork changed outside the launcher; it was left untouched.');
    if(e.originalHash){
      const backup=path.join(sessionRoot,'artwork-cache',`${e.originalHash}.tld`);
      if(digest(await fs.readFile(backup))!==e.originalHash)throw new Error('Artwork backup integrity check failed.');
    }
  }
}
async function restoreArtwork(entries,options){
  await validateArtworkRestore(entries,options);
  for(const e of entries){
    const target=await options.plain(options.root,e.target);
    if(e.originalHash)await fs.copyFile(path.join(options.sessionRoot,'artwork-cache',`${e.originalHash}.tld`),target);
    else await fs.unlink(target).catch(error=>{if(error.code!=='ENOENT')throw error;});
  }
}
module.exports={prepareArtwork,validateArtworkRestore,restoreArtwork};
