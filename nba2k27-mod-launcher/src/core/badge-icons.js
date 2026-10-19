'use strict';
const fs=require('node:fs/promises'),fsSync=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),readline=require('node:readline');
const run=require('node:util').promisify(require('node:child_process').execFile);
const {readZipArchive}=require('./jukebox-manager'),{PreviewQueue}=require('./preview-queue');
const aliases=Object.freeze({
 deadeye:'deadeye_scoring',
 lightninglaunch:'lightning_laun',
 quicktrigger:'quickdraw'
});
const normalized=value=>String(value||'').toLowerCase().replace(/[^a-z0-9]/g,'');
function wantedSlug(label){const key=normalized(label);return aliases[key]||key;}
function memberSlug(name){const match=/^icon_mycareer_badge_(.+?)\.[a-f0-9]+\.dds$/i.exec(path.posix.basename(name));return match?normalized(match[1]):null;}
function chooseMember(names,label){const wanted=normalized(wantedSlug(label));return names.find(name=>memberSlug(name)===wanted)||null;}
class BadgeIcons{
 constructor({cacheRoot,decoderPath,texconvPath,execute=run}){Object.assign(this,{cacheRoot:path.resolve(cacheRoot),decoderPath,texconvPath,execute});this.queue=new PreviewQueue(1);this.memory=new Map();this.pending=null;}
 async index(root){
  const manifest=path.join(root,'manifest'),stat=await fs.stat(manifest),stamp=`${path.resolve(root)}:${stat.size}:${stat.mtimeMs}`;
  if(this.stamp===stamp&&this.entryPromise)return this.entryPromise;
  this.stamp=stamp;this.members=null;this.memory.clear();
  this.entryPromise=(async()=>{const stream=fsSync.createReadStream(manifest),lines=readline.createInterface({input:stream,crlfDelay:Infinity});try{for await(const line of lines){const match=/^badgeicons\.iff,([A-Z0-9]{2}),(\d+),(\d+)$/i.exec(line);if(!match)continue;const offset=Number(match[2]),size=Number(match[3]);if(Number.isSafeInteger(offset)&&offset>=0&&Number.isSafeInteger(size)&&size>0&&size<=8*1024*1024)return {archive:match[1],offset,size};}}finally{lines.close();stream.destroy();}return null;})();
  this.entryPromise.catch(()=>{if(this.stamp===stamp){this.stamp=null;this.entryPromise=null;}});return this.entryPromise;
 }
 async decoded(root,entry){
  if(this.members)return this.members;
  const archive=path.join(root,entry.archive),stat=await fs.stat(archive);if(entry.offset+entry.size>stat.size)throw Error('Badge icon archive range is invalid.');
  await fs.mkdir(this.cacheRoot,{recursive:true});const temporary=await fs.mkdtemp(path.join(this.cacheRoot,'decode-'));
  try{const output=path.join(temporary,'badgeicons.iff');await this.execute(this.decoderPath,[archive,String(entry.offset),String(entry.size),path.join(root,'data/oodle/oo2core_9_win64.dll'),output],{windowsHide:true,timeout:30000,maxBuffer:1024*1024});if((await fs.stat(output)).size>8*1024*1024)throw Error('Badge icon archive is too large.');const members=readZipArchive(await fs.readFile(output),{maxEntries:1000});this.members=new Map([...members].filter(([name,data])=>memberSlug(name)&&data.length>=128&&data.length<=4*1024*1024&&data.toString('ascii',0,4)==='DDS '));return this.members;}
  finally{await fs.rm(temporary,{recursive:true,force:true});}
 }
 async get(root,labels){
  if(!root||!Array.isArray(labels)||labels.length>64)return {};const clean=[...new Set(labels.filter(label=>typeof label==='string'&&label.length>0&&label.length<=60))];if(!clean.length)return {};
  const entry=await this.index(root);if(!entry)return {};
  const key=crypto.createHash('sha256').update(this.stamp).digest('hex').slice(0,20),result={},missing=[];
  await fs.mkdir(this.cacheRoot,{recursive:true});if((await fs.lstat(this.cacheRoot)).isSymbolicLink())throw Error('Invalid badge icon cache directory.');
  for(const label of clean){const cacheKey=key+'-'+normalized(label),target=path.join(this.cacheRoot,cacheKey+'.png');if(this.memory.has(cacheKey)){result[label]=this.memory.get(cacheKey);continue;}try{const data=await fs.readFile(target);if(data.length>=24&&data.length<=512*1024&&data.subarray(0,8).toString('hex')==='89504e470d0a1a0a'){const value='data:image/png;base64,'+data.toString('base64');this.memory.set(cacheKey,value);result[label]=value;continue;}}catch(error){if(error.code!=='ENOENT')throw error;}missing.push({label,cacheKey,target});}
  if(missing.length){const job=this.pending||(this.pending=this.queue.run(()=>this.convert(root,entry,key,missing)).finally(()=>{this.pending=null;}));await job;for(const item of missing)if(this.memory.has(item.cacheKey))result[item.label]=this.memory.get(item.cacheKey);}
  while(this.memory.size>96)this.memory.delete(this.memory.keys().next().value);return result;
 }
 async convert(root,entry,key,items){
  const members=await this.decoded(root,entry),names=[...members.keys()],selected=items.map(item=>({...item,member:chooseMember(names,item.label)})).filter(item=>item.member);if(!selected.length)return;
  const temporary=await fs.mkdtemp(path.join(this.cacheRoot,'convert-'));
  try{const inputs=[];for(const item of selected){const input=path.join(temporary,item.cacheKey+'.dds');await fs.writeFile(input,members.get(item.member),{flag:'wx'});inputs.push(input);}await this.execute(this.texconvPath,['-nologo','-y','-ft','png','-f','R8G8B8A8_UNORM','-w','96','-h','96','-o',temporary,...inputs],{windowsHide:true,timeout:45000,maxBuffer:1024*1024});for(const item of selected){const data=await fs.readFile(path.join(temporary,item.cacheKey+'.png'));if(data.length<24||data.length>512*1024||data.subarray(0,8).toString('hex')!=='89504e470d0a1a0a')continue;await fs.writeFile(item.target,data);this.memory.set(item.cacheKey,'data:image/png;base64,'+data.toString('base64'));}}
  finally{await fs.rm(temporary,{recursive:true,force:true});}
  await this.prune();
 }
 async prune(){const files=[];for(const name of await fs.readdir(this.cacheRoot)){if(!/^[a-f0-9]{20}-[a-z0-9]+\.png$/.test(name))continue;const file=path.join(this.cacheRoot,name),stat=await fs.lstat(file);if(stat.isFile()&&!stat.isSymbolicLink())files.push({file,time:stat.mtimeMs});}for(const item of files.sort((a,b)=>b.time-a.time).slice(256))await fs.unlink(item.file).catch(()=>{});}
}
module.exports={BadgeIcons,wantedSlug,memberSlug,chooseMember};
