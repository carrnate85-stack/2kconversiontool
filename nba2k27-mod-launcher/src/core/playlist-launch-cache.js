'use strict';
const fs=require('node:fs/promises'),path=require('node:path'),crypto=require('node:crypto');
const {wrapPcmWave}=require('./native-pcm');
const hash=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const valid=value=>typeof value==='string'&&/^[a-f0-9]{64}$/.test(value);
class PlaylistLaunchCache{
 constructor(root){this.root=path.resolve(root);}
 async file(name){
  if(!/^[a-f0-9]{64}\.(wav|json)$/.test(name))throw Error('Invalid launch cache key.');
  let current=path.parse(this.root).root;
  for(const part of path.relative(current,this.root).split(path.sep).filter(Boolean)){
   current=path.join(current,part);const stat=await fs.lstat(current).catch(e=>{if(e.code==='ENOENT')return null;throw e;});
   if(stat?.isSymbolicLink())throw Error('Launch cache refuses linked directories.');
  }
  const file=path.join(this.root,name),stat=await fs.lstat(file).catch(e=>{if(e.code==='ENOENT')return null;throw e;});
  if(stat&&!stat.isFile())throw Error('Launch cache refuses linked or non-file entries.');
  return file;
 }
 async read(key){
  if(!valid(key))throw Error('Invalid launch cache source hash.');
  const file=await this.file(key+'.wav'),meta=await this.file(key+'.json');
  try{
   const metaStat=await fs.stat(meta);if(metaStat.size>4096)return null;
   const record=JSON.parse(await fs.readFile(meta,'utf8')),stat=await fs.stat(file);
   if(record.schema!==1||record.sourceHash!==key||!valid(record.hash)||record.size!==stat.size||stat.size<54||stat.size>256*1048576)return null;
   if(hash(await fs.readFile(file))!==record.hash)return null;
   return {path:file,hash:record.hash,size:stat.size};
  }catch(e){if(e.code==='ENOENT'||e instanceof SyntaxError)return null;throw e;}
 }
 async get(key,load){
  const cached=await this.read(key);if(cached)return {...cached,hit:true};
  const wave=await load();if(hash(wave)!==key)throw Error('Playlist source changed while caching.');
  const bytes=wrapPcmWave(wave),file=await this.file(key+'.wav'),meta=await this.file(key+'.json');
  await fs.mkdir(this.root,{recursive:true});
  // Only dedicated cache files are replaced; never source archives or mods.
  await fs.writeFile(file,bytes);const record={schema:1,sourceHash:key,hash:hash(bytes),size:bytes.length};
  await fs.writeFile(meta,JSON.stringify(record));return {path:file,hash:record.hash,size:record.size,hit:false};
 }
 async retain(keys){
  const keep=new Set(keys);if([...keep].some(key=>!valid(key)))throw Error('Invalid launch cache retention key.');
  for(const name of await fs.readdir(this.root).catch(e=>{if(e.code==='ENOENT')return [];throw e;})){
   if(!/^[a-f0-9]{64}\.json$/.test(name))continue;
   const key=name.slice(0,-5);if(keep.has(key))continue;
   const record=await this.read(key);if(!record)continue;
   await fs.unlink(record.path);await fs.unlink(await this.file(name));
  }
 }
}
module.exports={PlaylistLaunchCache};
