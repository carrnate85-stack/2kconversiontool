'use strict';
// One share file with bounded compressed segments, not one giant in-memory ZIP.
const fs=require('node:fs/promises');
const crypto=require('node:crypto');
const MAGIC=Buffer.from('2KPLAYLIST2\n');
const FOOTER=Buffer.from('2KPLIDX2');
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
const MAX_SEGMENT=384*1024*1024,MAX_INDEX=4*1024*1024,MAX_FILE=256*1024**3;
async function readExact(handle,length,offset){
  const b=Buffer.alloc(length);let done=0;
  while(done<length){const r=await handle.read(b,done,length-done,offset+done);if(!r.bytesRead)throw new Error('Truncated playlist container.');done+=r.bytesRead;}
  return b;
}
async function writeExact(handle,b){let done=0;while(done<b.length){const r=await handle.write(b,done,b.length-done);if(!r.bytesWritten)throw new Error('Playlist write stopped.');done+=r.bytesWritten;}}
async function writePlaylistContainer(destination,manifest,segments){
  const temporary=`${destination}.${process.pid}.${crypto.randomUUID()}.tmp`;
  const handle=await fs.open(temporary,'wx');let offset=MAGIC.length;const parts=[];
  try{
    await writeExact(handle,MAGIC);
    for await(const bytes of segments){
      if(!Buffer.isBuffer(bytes)||bytes.length<22||bytes.length>MAX_SEGMENT||parts.length>=4096||offset+bytes.length>MAX_FILE)throw new Error('Playlist container size limit exceeded.');
      parts.push({offset,length:bytes.length,sha256:hash(bytes)});await writeExact(handle,bytes);offset+=bytes.length;
    }
    if(!parts.length)throw new Error('Empty playlist container.');
    const index=Buffer.from(JSON.stringify({schema:'nba2k27-playlist-container-v2',manifest,parts}));
    if(index.length>MAX_INDEX)throw new Error('Playlist metadata is too large.');
    await writeExact(handle,index);
    const footer=Buffer.alloc(24);FOOTER.copy(footer);footer.writeBigUInt64LE(BigInt(offset),8);footer.writeBigUInt64LE(BigInt(index.length),16);
    await writeExact(handle,footer);await handle.sync();await handle.close();
    await fs.rename(temporary,destination);
  }catch(error){await handle.close().catch(()=>{});await fs.unlink(temporary).catch(()=>{});throw error;}
}
async function openPlaylistContainer(file){
  const handle=await fs.open(file,'r');
  try{
    const stat=await handle.stat();if(stat.size<MAGIC.length)return null;
    if(!(await readExact(handle,MAGIC.length,0)).equals(MAGIC))return null;
    if(stat.size<MAGIC.length+24||stat.size>MAX_FILE+MAX_INDEX+24)throw new Error('Invalid playlist container size.');
    const footer=await readExact(handle,24,stat.size-24);
    if(!footer.subarray(0,8).equals(FOOTER))throw new Error('Missing playlist container index.');
    const offset=Number(footer.readBigUInt64LE(8)),length=Number(footer.readBigUInt64LE(16));
    if(!Number.isSafeInteger(offset)||offset<MAGIC.length||length<1||length>MAX_INDEX||offset+length!==stat.size-24)throw new Error('Invalid playlist index bounds.');
    const index=JSON.parse((await readExact(handle,length,offset)).toString('utf8'));
    if(index.schema!=='nba2k27-playlist-container-v2'||!Array.isArray(index.parts)||!index.parts.length||index.parts.length>4096)throw new Error('Invalid playlist container index.');
    let end=MAGIC.length;
    for(const p of index.parts){
      if(p.offset!==end||!Number.isInteger(p.length)||p.length<22||p.length>MAX_SEGMENT||!/^[a-f0-9]{64}$/.test(p.sha256))throw new Error('Invalid playlist segment.');
      end+=p.length;if(end>offset)throw new Error('Playlist segment overlaps index.');
    }
    if(end!==offset||!index.manifest||!Array.isArray(index.manifest.tracks)||index.manifest.tracks.length>4096)throw new Error('Invalid combined playlist manifest.');
    return {...index,file,size:stat.size,mtimeMs:stat.mtimeMs};
  }finally{await handle.close();}
}
async function readPlaylistSegment(container,index){
  const p=container.parts[index];if(!p)throw new Error('Missing playlist segment.');
  const h=await fs.open(container.file,'r');
  try{
    const stat=await h.stat();if(stat.size!==container.size||stat.mtimeMs!==container.mtimeMs)throw new Error('Playlist changed during import.');
    const b=await readExact(h,p.length,p.offset);if(hash(b)!==p.sha256)throw new Error('Playlist segment integrity check failed.');return b;
  }finally{await h.close();}
}
module.exports={writePlaylistContainer,openPlaylistContainer,readPlaylistSegment};
