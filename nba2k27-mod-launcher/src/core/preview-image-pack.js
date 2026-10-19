'use strict';
const fs=require('node:fs/promises');
const path=require('node:path');
const {crc32}=require('./zip-replacement');

const EOCD=0x06054b50,CENTRAL=0x02014b50,LOCAL=0x04034b50;
const MAX_ENTRIES=40000,MAX_CENTRAL=12*1024*1024,MAX_IMAGE=4*1024*1024;
const PNG=Buffer.from([137,80,78,71,13,10,26,10]);
const imageName=name=>/^blacktop-(?:top|bottom|accessory|ball|sock|shoe)-previews\/thumbnails\/[a-zA-Z0-9_.-]+\.png$/.test(name)&&!name.includes('..');

async function readAt(handle,length,position){
 const bytes=Buffer.alloc(length);let offset=0;
 while(offset<length){const result=await handle.read(bytes,offset,length-offset,position+offset);if(!result.bytesRead)throw Error('Preview ZIP ended unexpectedly.');offset+=result.bytesRead;}
 return bytes;
}

async function openPreviewImagePack(file){
 const handle=await fs.open(file,'r');
 try{
  const stat=await handle.stat();
  if(!stat.isFile()||stat.size<22)throw Error('Choose a valid launcher preview ZIP file.');
  if(stat.size>2*1024*1024*1024)throw Error('This ZIP exceeds the launcher preview limit of 2 GiB. Renderer exports can include 512 px images and catalogs; import the complete launcher 256 px thumbnail ZIP instead.');
  const tailSize=Math.min(stat.size,65557),tail=await readAt(handle,tailSize,stat.size-tailSize);
  let eocd=-1;
  for(let at=tail.length-22;at>=0;at--)if(tail.readUInt32LE(at)===EOCD&&at+22+tail.readUInt16LE(at+20)===tail.length){eocd=at;break;}
  if(eocd<0)throw Error('Preview ZIP end record is missing.');
  const count=tail.readUInt16LE(eocd+10),size=tail.readUInt32LE(eocd+12),offset=tail.readUInt32LE(eocd+16);
  if(tail.readUInt16LE(eocd+4)||tail.readUInt16LE(eocd+6)||tail.readUInt16LE(eocd+8)!==count||!count||count>MAX_ENTRIES||size>MAX_CENTRAL||offset+size>stat.size-tailSize+eocd)
   throw Error('Preview ZIP directory is invalid or uses ZIP64.');
  const central=await readAt(handle,size,offset),entries=new Map(),counts={},samples={};let cursor=0;
  for(let i=0;i<count;i++){
   if(cursor+46>size||central.readUInt32LE(cursor)!==CENTRAL)throw Error('Preview ZIP directory entry is invalid.');
   const flags=central.readUInt16LE(cursor+8),method=central.readUInt16LE(cursor+10);
   const packed=central.readUInt32LE(cursor+20),unpacked=central.readUInt32LE(cursor+24);
   const nameLength=central.readUInt16LE(cursor+28),extraLength=central.readUInt16LE(cursor+30),commentLength=central.readUInt16LE(cursor+32);
   const end=cursor+46+nameLength+extraLength+commentLength;
   if(end>size||flags&1||method!==0||packed!==unpacked||packed<PNG.length||packed>MAX_IMAGE)throw Error('Preview ZIP contains an unsupported image entry.');
   const name=central.subarray(cursor+46,cursor+46+nameLength).toString('utf8');
   if(!imageName(name)||entries.has(name))throw Error('Preview ZIP contains an unsafe or duplicate image name.');
   const kind=name.match(/^blacktop-([^-]+)-previews\//)[1];counts[kind]=(counts[kind]||0)+1;samples[kind]??=name;
   const localOffset=central.readUInt32LE(cursor+42);
   if(localOffset+30+packed>offset)throw Error('Preview ZIP image position is invalid.');
   entries.set(name,{offset:localOffset,size:packed,crc:central.readUInt32LE(cursor+16)});
   cursor=end;
  }
  if(cursor!==size)throw Error('Preview ZIP directory length is invalid.');
  return {file,count:entries.size,counts,samples,names:Object.freeze([...entries.keys()]),async read(name){
   const entry=entries.get(name);if(!entry)return null;
   const header=await readAt(handle,30,entry.offset);
   if(header.readUInt32LE(0)!==LOCAL||header.readUInt16LE(8)!==0)throw Error('Preview ZIP image header is invalid.');
   const localName=await readAt(handle,header.readUInt16LE(26),entry.offset+30);
   if(localName.toString('utf8')!==name||header.readUInt16LE(6)&1)throw Error('Preview ZIP image name or flags do not match its directory.');
   if(!(header.readUInt16LE(6)&8)&&(header.readUInt32LE(14)!==entry.crc||header.readUInt32LE(18)!==entry.size||header.readUInt32LE(22)!==entry.size))throw Error('Preview ZIP image header does not match its directory.');
   const at=entry.offset+30+header.readUInt16LE(26)+header.readUInt16LE(28);
   if(at+entry.size>offset)throw Error('Preview ZIP image exceeds its data section.');
   const bytes=await readAt(handle,entry.size,at);
   if(!bytes.subarray(0,8).equals(PNG)||crc32(bytes)!==entry.crc)throw Error('Preview ZIP image checksum failed.');
   return bytes;
  },close:()=>handle.close()};
 }catch(error){await handle.close();throw error;}
}

function createPreviewImageResolver({candidates,bundledRoot}){
 let opened,opening;
 const open=()=>opening ||= (async()=>{for(const file of candidates){try{opened=await openPreviewImagePack(file);break;}catch(error){if(error.code!=='ENOENT')console.warn('Optional preview ZIP ignored:',error.message);}}})();
 return {async activate(file){
  await open();
  const next=await openPreviewImagePack(file);
  const previous=opened;opened=next;opening=Promise.resolve();
  await previous?.close();return {file:next.file,count:next.count,counts:next.counts};
 },async read(name){
  if(!imageName(name))return null;
  await open();
  if(opened){try{const bytes=await opened.read(name);if(bytes)return bytes;}catch(error){console.warn('Optional preview image unavailable:',error.message);}}
  try{return await fs.readFile(path.join(bundledRoot,...name.split('/')));}catch(error){if(error.code==='ENOENT')return null;throw error;}
 },get status(){return opened?{file:opened.file,count:opened.count,counts:opened.counts}:null;},close:()=>opened?.close()};
}
module.exports={imageName,openPreviewImagePack,createPreviewImageResolver};
