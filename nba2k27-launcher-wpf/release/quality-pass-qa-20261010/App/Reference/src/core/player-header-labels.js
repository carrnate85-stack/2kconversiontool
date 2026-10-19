'use strict';
const fs=require('node:fs/promises'),path=require('node:path');
const run=require('node:util').promisify(require('node:child_process').execFile);
const {findManifestEntry,readZipArchive}=require('./jukebox-manager');
const {resolve}=require('./camera-localization');

function archetypeLabel(text,key){
 if(typeof key!=='string'||!/^[a-f0-9]{16}$/i.test(key)||/^(0{16}|f{16})$/i.test(key))return null;
 try{const value=resolve(text,key).value;return value.length>0&&value.length<=120&&!/[\x00-\x1f\x7f]/.test(value)?value:null;}catch{return null;}
}

// One decoded English table in memory, loaded only when a valid live key is requested.
// Temporary extraction stays in the launcher's configured cache and is removed
// even on decode failure.
class PlayerHeaderLabels{
 constructor({cacheRoot,decoderPath,decode=run}){this.cacheRoot=path.resolve(cacheRoot);this.decoderPath=decoderPath;this.decode=decode;this.pending=null;this.cache=null;this.entry=null;}
 async get(root,key){
  if(!root||typeof key!=='string'||!/^[a-f0-9]{16}$/i.test(key)||/^(0{16}|f{16})$/i.test(key))return null;
  root=path.resolve(root);
  if(this.pending){if(this.pending.root!==root)return null;return archetypeLabel(await this.pending.promise,key);}
  const promise=this.load(root);this.pending={root,promise};
  try{return archetypeLabel(await promise,key);}finally{if(this.pending?.promise===promise)this.pending=null;}
 }
 async load(root){
  const manifest=await fs.stat(path.join(root,'manifest')),stamp=`${root}:${manifest.size}:${manifest.mtimeMs}`;
  if(this.entry?.stamp!==stamp)this.entry={stamp,value:await findManifestEntry(root,'loc/english.iff')};
  const entry=this.entry.value,archive=path.join(root,entry.archive),stat=await fs.stat(archive);
  if(entry.size>16*1024*1024||entry.offset+entry.size>stat.size)throw Error('Invalid English archive range.');
  const identity=`${stamp}:${stat.size}:${stat.mtimeMs}`;
  if(this.cache?.identity===identity)return this.cache.text;
  this.cache=null;
  await fs.mkdir(this.cacheRoot,{recursive:true});
  if((await fs.lstat(this.cacheRoot)).isSymbolicLink())throw Error('Invalid player header cache directory.');
  const tmp=await fs.mkdtemp(path.join(this.cacheRoot,'decode-'));
  try{
   const output=path.join(tmp,'english.iff');
   await this.decode(this.decoderPath,[archive,String(entry.offset),String(entry.size),path.join(root,'data/oodle/oo2core_9_win64.dll'),output],{windowsHide:true,timeout:30000,maxBuffer:1024*1024});
   if((await fs.stat(output)).size>32*1024*1024)throw Error('English archive exceeds its size limit.');
   const text=readZipArchive(await fs.readFile(output)).get('TEXT.VCLOCALIZEDATA');
   if(!text||text.length>32*1024*1024)throw Error('English player labels are unavailable.');
   const after=await fs.stat(archive),afterManifest=await fs.stat(path.join(root,'manifest'));
   if(after.size!==stat.size||after.mtimeMs!==stat.mtimeMs||afterManifest.size!==manifest.size||afterManifest.mtimeMs!==manifest.mtimeMs)throw Error('Game data changed during label loading.');
   this.cache={identity,text};return text;
  }finally{if(path.dirname(tmp)===this.cacheRoot&&path.basename(tmp).startsWith('decode-'))await fs.rm(tmp,{recursive:true,force:true});}
 }
}
module.exports={PlayerHeaderLabels,archetypeLabel};
