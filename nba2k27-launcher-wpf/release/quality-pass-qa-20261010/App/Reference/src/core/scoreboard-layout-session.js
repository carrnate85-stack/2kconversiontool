'use strict';
const fs=require('node:fs/promises'),path=require('node:path'),crypto=require('node:crypto');
const exec=require('node:util').promisify(require('node:child_process').execFile);
const {findManifestEntries}=require('./jukebox-manager');
const {buildScoreboardArchive,verifyScoreboardTargets}=require('./scoreboard-layout-archive');
const {sha}=require('./scoreboard-layout-patch');
const catalog=require('../data/scoreboard-text-adapters.json');
const optional=async p=>fs.readFile(p).catch(e=>{if(e.code==='ENOENT')return null;throw e;});
async function plain(p){
 const resolved=path.resolve(p);let current=path.parse(resolved).root;
 for(const part of resolved.slice(current.length).split(path.sep).filter(Boolean)){
  current=path.join(current,part);const stat=await fs.lstat(current).catch(e=>{if(e.code==='ENOENT')return null;throw e;});
  if(stat?.isSymbolicLink())throw Error('Scoreboard preparation refuses linked files or folders.');
 }
}
async function atomic(p,data){
 await plain(p);await fs.mkdir(path.dirname(p),{recursive:true});const temp=p+'.'+crypto.randomUUID()+'.tmp';
 try{await fs.writeFile(temp,data,{flag:'wx'});await fs.rename(temp,p);}finally{await fs.unlink(temp).catch(e=>{if(e.code!=='ENOENT')throw e;});}
}
class ScoreboardLayoutSession{
 constructor({root,decoderPath,isGameRunning=async()=>false,build=null,layouts=catalog.layouts,targets=catalog.targets,patch=buildScoreboardArchive}){
  this.layouts=layouts;this.targets=targets;this.patch=patch;
  this.root=path.resolve(root);this.marker=path.join(this.root,'session.json');this.decoderPath=decoderPath;this.isGameRunning=isGameRunning;
  this.build=build||this.buildStock.bind(this);this.ready=false;
 }
 async decode(gameRoot,entry,output){
  await exec(this.decoderPath,[path.join(gameRoot,entry.archive),String(entry.offset),String(entry.size),path.join(gameRoot,'data/oodle/oo2core_9_win64.dll'),output],{windowsHide:true,timeout:120000,maxBuffer:1048576});
 }
 async sourceIdentity(gameRoot,entries){
  const archives=[];
  for(const archive of [...new Set([...entries.values()].map(entry=>entry.archive))].sort()){
   const file=path.join(gameRoot,archive);await plain(file);const stat=await fs.stat(file);
   if(!stat.isFile())throw Error('A scoreboard source archive is unavailable.');
   archives.push({archive,size:stat.size,mtimeMs:stat.mtimeMs});
  }
  const decoder=await fs.stat(this.decoderPath);
  if(!decoder.isFile())throw Error('The scoreboard decoder is unavailable.');
  return sha(JSON.stringify({schema:2,entries:[...entries].sort(([a],[b])=>a.localeCompare(b)),archives,decoder:{size:decoder.size,mtimeMs:decoder.mtimeMs},layouts:this.layouts,targets:this.targets}));
 }
 async readBundleCache(identity){
  const metaRaw=await optional(path.join(this.root,'cache','bundle.json'));let meta;
  try{meta=metaRaw&&JSON.parse(metaRaw);}catch{return null;}
  if(meta?.schema!==2||meta.identity!==identity||!Array.isArray(meta.files)||meta.files.length!==this.layouts.length)return null;
  const byAsset=new Map(meta.files.map(file=>[file.asset,file]));if(byAsset.size!==this.layouts.length)return null;
  const result=[];
  for(const layout of this.layouts){
   const saved=byAsset.get(layout.asset);if(!saved||saved.id!==layout.id||!/^[a-f0-9]{64}$/.test(saved.hash))return null;
   const bytes=await optional(path.join(this.root,'cache',layout.id+'.iff'));if(!bytes||sha(bytes)!==saved.hash)return null;
   result.push({asset:layout.asset,bytes});
  }
  return result;
 }
 async writeBundleCache(identity,result){
  for(const file of result){const layout=this.layouts.find(candidate=>candidate.asset===file.asset);if(!layout)throw Error('Invalid scoreboard cache asset.');await atomic(path.join(this.root,'cache',layout.id+'.iff'),file.bytes);}
  await atomic(path.join(this.root,'cache','bundle.json'),JSON.stringify({schema:2,identity,files:result.map(file=>({id:this.layouts.find(layout=>layout.asset===file.asset).id,asset:file.asset,hash:sha(file.bytes)}))}));
 }
 async buildStock(gameRoot){
  await plain(this.root);await fs.mkdir(this.root,{recursive:true});
  const temp=await fs.mkdtemp(path.join(this.root,'prepare-')),result=[];
  try{
   const requested=['loc/english.iff',...this.layouts.map(layout=>layout.asset)],entries=await findManifestEntries(gameRoot,requested);
   const identity=await this.sourceIdentity(gameRoot,entries),warm=await this.readBundleCache(identity);if(warm)return warm;
   if(this.targets.length){const english=path.join(temp,'english.iff');await this.decode(gameRoot,entries.get('loc/english.iff'),english);
   verifyScoreboardTargets(await fs.readFile(english),this.targets);}
   for(const layout of this.layouts){
    const entry=entries.get(layout.asset.toLowerCase()),handle=await fs.open(path.join(gameRoot,entry.archive),'r');let packed;
    try{
     if(!Number.isSafeInteger(entry.size)||entry.size<1||entry.size>64*1024*1024)throw Error('Unsupported scoreboard archive size.');
     packed=Buffer.alloc(entry.size);let n=0;while(n<packed.length){const r=await handle.read(packed,n,packed.length-n,entry.offset+n);if(!r.bytesRead)throw Error('Truncated stock scoreboard.');n+=r.bytesRead;}
    }finally{await handle.close();}
    const identity=sha(packed)+sha(JSON.stringify(layout)),cache=path.join(this.root,'cache',layout.id+'.iff'),meta=cache+'.json';
    const cached=await optional(cache),rawMeta=await optional(meta);let saved;
    try{saved=rawMeta&&JSON.parse(rawMeta);}catch{saved=null;}
    let bytes;
    if(cached&&saved?.identity===identity&&saved.hash===sha(cached))bytes=cached;
    else{
     const raw=path.join(temp,layout.id+'.iff');await this.decode(gameRoot,entry,raw);bytes=this.patch(await fs.readFile(raw),layout);
     await atomic(cache,bytes);await atomic(meta,JSON.stringify({identity,hash:sha(bytes)}));
    }
    result.push({asset:layout.asset,bytes});
   }
   await this.writeBundleCache(identity,result);
   return result;
  }finally{await fs.rm(temp,{recursive:true,force:true});}
 }
 async restore(){
  this.ready=false;await plain(this.marker);const raw=await optional(this.marker);if(!raw)return;
  if(await this.isGameRunning())throw Error('Close NBA 2K27 before restoring scoreboard files.');
  const state=JSON.parse(raw),allowed=new Set(this.layouts.map(l=>l.asset));
  if(state.schema!==1||!path.isAbsolute(state.modsRoot||'')||!Array.isArray(state.files)||state.files.length!==allowed.size)throw Error('Invalid scoreboard recovery journal; files left untouched.');
  const seen=new Set(),remove=[];
  // Validate the ENTIRE journal and every destination before removing anything.
  for(const file of state.files){
   if(!allowed.has(file.asset)||seen.has(file.asset)||!/^[a-f0-9]{64}$/.test(file.hash))throw Error('Invalid scoreboard recovery target; files left untouched.');
   seen.add(file.asset);const target=path.join(state.modsRoot,...file.asset.split('/'));await plain(target);
   const current=await optional(target);if(current&&sha(current)!==file.hash)throw Error('A scoreboard file changed outside the launcher; it was left untouched.');
   if(current)remove.push(target);
  }
  for(const target of remove)await fs.unlink(target);
  await fs.unlink(this.marker);
 }
 async prepare({gameRoot,modsRoot}){
  if(await this.isGameRunning())throw Error('Close NBA 2K27 before preparing scoreboard layouts.');
  await this.restore();const resolved=path.resolve(modsRoot);
  for(const layout of this.layouts){const target=path.join(resolved,...layout.asset.split('/'));await plain(target);if(await optional(target))throw Error('A custom scoreboard mod is installed; it was left untouched.');}
  const built=await this.build(gameRoot),allowed=new Set(this.layouts.map(l=>l.asset));
  if(!Array.isArray(built)||built.length!==allowed.size||new Set(built.map(f=>f.asset)).size!==allowed.size||built.some(f=>!allowed.has(f.asset)||!Buffer.isBuffer(f.bytes)||!f.bytes.length))throw Error('Scoreboard preparation returned an incomplete set.');
  if(await this.isGameRunning())throw Error('NBA 2K27 started during scoreboard preparation. Close it and retry.');
  await atomic(this.marker,JSON.stringify({schema:1,modsRoot:resolved,files:built.map(f=>({asset:f.asset,hash:sha(f.bytes)}))}));
  // Exclusive creation: never overwrite a mod that appeared after preflight.
  for(const file of built){const target=path.join(resolved,...file.asset.split('/'));await plain(target);await fs.mkdir(path.dirname(target),{recursive:true});await fs.writeFile(target,file.bytes,{flag:'wx'});}
  this.ready=true;return {installed:true,count:built.length};
 }
}
module.exports={ScoreboardLayoutSession};
