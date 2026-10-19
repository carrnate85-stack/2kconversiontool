'use strict';
const fs=require('node:fs/promises'),path=require('node:path'),crypto=require('node:crypto'),zlib=require('node:zlib');
const compress=require('node:util').promisify(zlib.gzip);
const native=/^nba2k27_(?:saved_outfits|character_queue_noop|character_queue_thread|description_clone|description_edit|same_outfit_rebuild|change_top_rebuild|expand_copy|full_outfit)_\d+(?:_match\d+)?(?:_player\d+)?\.(?:log|plan)$/;
const proxy=/^nba2k27_version_proxy(?:_armed|_audit)?\.(?:log|trace)$/;
const app=/^(?:blacktop-(?:panel|selections)-[\d.]+-\d+\.jsonl|jukebox-audio-route\.json|last-loader-failure\.json)$/;
const archived=/^[a-f0-9]{64}--[a-zA-Z0-9_.-]+\.gz$/;
async function plain(directory){
 for(let current=path.resolve(directory);;current=path.dirname(current)){
  const stat=await fs.lstat(current).catch(e=>{if(e.code==='ENOENT')return null;throw e;});
  if(stat?.isSymbolicLink())throw Error('Diagnostic storage refuses linked paths.');
  if(path.dirname(current)===current)break;
 }
}
async function archiveSessionLogs({directory,gameRoot}){
 if(!directory)return {saved:0};
 const destination=path.join(directory,'session-history');await plain(destination);await fs.mkdir(destination,{recursive:true});
 let saved=0;
 for(const folder of [gameRoot,path.join(directory,'game-session'),directory].filter(Boolean)){
  await plain(folder);
  const entries=await fs.readdir(folder,{withFileTypes:true}).catch(e=>{if(e.code==='ENOENT')return [];throw e;});
  for(const entry of entries){
   if(!entry.isFile()||!(native.test(entry.name)||proxy.test(entry.name)||app.test(entry.name)))continue;
   const handle=await fs.open(path.join(folder,entry.name),'r');let bytes;
   try{
    const stat=await handle.stat(),limit=4*1024*1024,length=Math.min(stat.size,limit),buffer=Buffer.alloc(length);
    let offset=0;while(offset<length){const part=await handle.read(buffer,offset,length-offset,Math.max(0,stat.size-length)+offset);if(!part.bytesRead)break;offset+=part.bytesRead;}
    bytes=buffer.subarray(0,offset);
    if(stat.size>limit)bytes=Buffer.concat([Buffer.from('[TRUNCATED: last 4 MiB of '+stat.size+' bytes]\n'),bytes]);
   }finally{await handle.close();}
   const name=crypto.createHash('sha256').update(bytes).digest('hex')+'--'+entry.name+'.gz';
   const target=path.join(destination,name),temporary=target+'.'+crypto.randomUUID()+'.tmp';
   if(await fs.lstat(target).catch(e=>{if(e.code==='ENOENT')return null;throw e;}))continue;
   try{await fs.writeFile(temporary,await compress(bytes),{flag:'wx'});await fs.rename(temporary,target);saved++;}
   finally{await fs.unlink(temporary).catch(e=>{if(e.code!=='ENOENT')throw e;});}
  }
 }
 const rows=[];
 for(const entry of await fs.readdir(destination,{withFileTypes:true}))if(entry.isFile()&&archived.test(entry.name)){
  const file=path.join(destination,entry.name),stat=await fs.lstat(file);if(!stat.isSymbolicLink())rows.push({file,size:stat.size,time:stat.mtimeMs});
 }
 let size=0,count=0;
 for(const row of rows.sort((a,b)=>b.time-a.time)){size+=row.size;count++;if(count>200||size>32*1024*1024)await fs.unlink(row.file);}
 return {saved,directory:destination};
}
module.exports={archiveSessionLogs,plain};
