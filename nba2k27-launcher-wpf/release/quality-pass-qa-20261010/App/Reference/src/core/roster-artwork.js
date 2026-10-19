'use strict';
const fs=require('node:fs/promises'),fsSync=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),readline=require('node:readline');
const run=require('node:util').promisify(require('node:child_process').execFile);
const {readZipArchive}=require('./jukebox-manager'),{PreviewQueue}=require('./preview-queue');
function candidates(kind,id){
 if(kind==='background'){const base=candidates('logo',id);return base[0]==='logos/logo000.iff'?['logos/logo000phi_1978.iff',...base]:base;}
 if(kind==='portrait'&&Number.isInteger(id)&&id>=0&&id<=65535){const n=String(id).padStart(4,'0');return [`player_images/chr_g${n}.iff`,`player_images/chr_r${n}_a1.iff`];}
 if(kind==='logo'&&typeof id==='string'&&/^logo[a-z0-9_]{1,90}$/i.test(id.replace(/\.iff$/i,'')))return ['logos/'+id.replace(/\.iff$/i,'').toLowerCase()+'.iff'];
 return [];
}
class RosterArtwork{
 constructor({cacheRoot,decoderPath,texconvPath}){Object.assign(this,{cacheRoot:path.resolve(cacheRoot),decoderPath,texconvPath});this.queue=new PreviewQueue(2);this.pending=new Map();this.memory=new Map();}
 async index(root){const stat=await fs.stat(path.join(root,'manifest'));const stamp=`${root}:${stat.size}:${stat.mtimeMs}`;if(this.stamp!==stamp){this.stamp=stamp;this.indexPromise=(async()=>{const entries=new Map(),stream=fsSync.createReadStream(path.join(root,'manifest'));const lines=readline.createInterface({input:stream,crlfDelay:Infinity});try{for await(const line of lines){const m=/^((?:player_images\/chr_[rg][a-z0-9_]+|logos\/logo[a-z0-9_]+)\.iff),([A-Z0-9]{2}),(\d+),(\d+)$/i.exec(line);if(m){const offset=Number(m[3]),size=Number(m[4]);if(Number.isSafeInteger(offset)&&offset>=0&&Number.isSafeInteger(size)&&size>0&&size<=16*1024*1024)entries.set(m[1].toLowerCase(),{archive:m[2],offset,size});}}}finally{lines.close();stream.destroy();}return entries;})();this.indexPromise.catch(()=>{if(this.stamp===stamp)this.stamp=null;});}return this.indexPromise;}
 async get(root,kind,id){
  const names=candidates(kind,id);if(!names.length||!root)return null;
  const entries=await this.index(root),name=names.find(n=>entries.has(n));if(!name)return null;
  const entry=entries.get(name),archive=path.join(root,entry.archive),stat=await fs.stat(archive);if(entry.offset+entry.size>stat.size)throw Error('Artwork archive range is invalid.');
  const key=crypto.createHash('sha256').update(`${this.stamp}:${name}:${stat.size}:${stat.mtimeMs}`).digest('hex');
  if(this.memory.has(key))return this.memory.get(key);if(this.pending.has(key))return this.pending.get(key);if(this.pending.size>=16)return null;
  const job=this.queue.run(async()=>{await fs.mkdir(this.cacheRoot,{recursive:true});if((await fs.lstat(this.cacheRoot)).isSymbolicLink())throw Error('Invalid artwork cache directory.');const target=path.join(this.cacheRoot,key+'.png');let data;try{const s=await fs.stat(target);if(s.size<=2*1024*1024)data=await fs.readFile(target);}catch(e){if(e.code!=='ENOENT')throw e;}
   if(data&&(data.length<24||data.subarray(0,8).toString('hex')!=='89504e470d0a1a0a'))data=null;
   if(!data){const tmp=await fs.mkdtemp(path.join(this.cacheRoot,'decode-'));try{
    const output=path.join(tmp,'image.iff');await run(this.decoderPath,[archive,String(entry.offset),String(entry.size),path.join(root,'data/oodle/oo2core_9_win64.dll'),output],{windowsHide:true,timeout:25000,maxBuffer:1024*1024});
    if((await fs.stat(output)).size>32*1024*1024)throw Error('Artwork is too large.');const textures=[...readZipArchive(await fs.readFile(output))].filter(([n])=>n.toLowerCase().endsWith('.dds'));if(textures.length!==1)return null;
    const dds=textures[0][1];if(dds.length<128||dds.length>24*1024*1024||dds.toString('ascii',0,4)!=='DDS '||dds.readUInt32LE(12)>4096||dds.readUInt32LE(16)>4096)throw Error('Unsupported artwork texture.');
    const input=path.join(tmp,'image.dds');await fs.writeFile(input,dds,{flag:'wx'});await run(this.texconvPath,['-nologo','-y','-ft','png','-f','R8G8B8A8_UNORM','-w','512','-h','512','-o',tmp,input],{windowsHide:true,timeout:20000,maxBuffer:1024*1024});data=await fs.readFile(path.join(tmp,'image.png'));if(data.length>2*1024*1024||data.subarray(0,8).toString('hex')!=='89504e470d0a1a0a')throw Error('Invalid artwork conversion');await fs.writeFile(target,data);
   }finally{if(path.dirname(tmp)===this.cacheRoot&&path.basename(tmp).startsWith('decode-'))await fs.rm(tmp,{recursive:true,force:true});}}
   const value='data:image/png;base64,'+data.toString('base64');this.memory.set(key,value);while(this.memory.size>32)this.memory.delete(this.memory.keys().next().value);await this.prune();return value;});
  this.pending.set(key,job);job.finally(()=>this.pending.delete(key)).catch(()=>{});return job;
 }
 async prune(){const files=[];for(const n of await fs.readdir(this.cacheRoot)){if(!/^[a-f0-9]{64}\.png$/.test(n))continue;const file=path.join(this.cacheRoot,n),s=await fs.lstat(file);if(s.isFile()&&!s.isSymbolicLink())files.push({file,size:s.size,time:s.mtimeMs});}let total=files.reduce((a,f)=>a+f.size,0);for(const f of files.sort((a,b)=>a.time-b.time)){if(total<=128*1024*1024)break;await fs.unlink(f.file).catch(()=>{});total-=f.size;}}
}
module.exports={RosterArtwork,candidates};
