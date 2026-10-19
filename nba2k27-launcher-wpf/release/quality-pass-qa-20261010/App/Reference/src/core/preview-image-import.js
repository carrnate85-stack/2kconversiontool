'use strict';
const fs=require('node:fs');
const fsp=require('node:fs/promises');
const path=require('node:path');
const crypto=require('node:crypto');
const zlib=require('node:zlib');
const {Transform}=require('node:stream');
const {pipeline}=require('node:stream/promises');
const {openPreviewImagePack}=require('./preview-image-pack');
const {crc32}=require('./zip-replacement');
const assets=require('../data/preview-image-assets.json');

const expectedCounts=Object.freeze({top:12557,bottom:4007,accessory:11201,ball:468,sock:102,shoe:503});
const currentCounts=Object.freeze({...expectedCounts,accessory:assets.accessory.length});
const imageNames=accessories=>new Set(Object.keys(expectedCounts).flatMap(kind=>(kind==='accessory'?accessories:assets[kind]).map(id=>`blacktop-${kind}-previews/thumbnails/${id}.png`)));
const profiles=[{name:'legacy',counts:expectedCounts,names:imageNames(assets.legacyAccessory)},{name:'expanded-accessories',counts:currentCounts,names:imageNames(assets.accessory)}];
const managedName=/^preview-[a-f0-9]{64}\.zip$/;
const directory=(userData,managedRoot)=>managedRoot||path.join(userData,'blacktop','preview-packs');
function checkCatalog(pack,expected){
 const profile=profiles.find(p=>pack.count===p.names.size&&Object.keys(p.counts).every(kind=>pack.counts[kind]===p.counts[kind])&&(!expected||expected===expectedCounts||Object.keys(p.counts).every(kind=>expected[kind]===p.counts[kind])));
 if(!profile)throw Error('Choose a complete legacy preview ZIP or a complete expanded-accessory ZIP. An accessory-only delta is not a full preview pack.');
 for(const name of pack.names)if(!profile.names.has(name))throw Error('Preview ZIP contains an unknown image ID: '+name);
 return profile;
}
function checkPng(bytes){
 let offset=8,header=false,data=false,end=false,depth,color,interlace,palette=false;const idat=[];
 while(offset<bytes.length){
  if(offset+12>bytes.length)throw Error('Preview PNG has a truncated chunk.');
  const length=bytes.readUInt32BE(offset),type=bytes.toString('ascii',offset+4,offset+8),next=offset+12+length;
  if(next>bytes.length||crc32(bytes.subarray(offset+4,offset+8+length))!==bytes.readUInt32BE(offset+8+length))throw Error('Preview PNG chunk checksum failed.');
  if(!header){if(type!=='IHDR'||length!==13||bytes.readUInt32BE(offset+8)!==256||bytes.readUInt32BE(offset+12)!==256)throw Error('Preview thumbnails must be 256 by 256 PNGs.');depth=bytes[offset+16];color=bytes[offset+17];interlace=bytes[offset+20];if(!({0:[1,2,4,8,16],2:[8,16],3:[1,2,4,8],4:[8,16],6:[8,16]}[color]||[]).includes(depth)||bytes[offset+18]||bytes[offset+19]||interlace>1)throw Error('Preview PNG header is unsupported.');header=true;}
  else if(type==='IHDR')throw Error('Preview PNG has a duplicate header.');
  if(type==='PLTE'){if(data||length<3||length>768||length%3)throw Error('Preview PNG palette is invalid.');palette=true;}
  if(type==='IDAT'){data=true;idat.push(bytes.subarray(offset+8,offset+8+length));}
  if(type==='IEND'){if(length||next!==bytes.length)throw Error('Preview PNG end is invalid.');end=true;}
  offset=next;
 }
 if(!header||!data||!end)throw Error('Preview PNG is incomplete.');
 if(color===3&&!palette)throw Error('Indexed preview PNG needs a palette.');
 const decoded=zlib.inflateSync(Buffer.concat(idat),{maxOutputLength:2*1024*1024}),channels={0:1,2:3,3:1,4:2,6:4}[color];
 const passes=interlace?[[0,0,8,8],[4,0,8,8],[0,4,4,8],[2,0,4,4],[0,2,2,4],[1,0,2,2],[0,1,1,2]]:[[0,0,1,1]];let at=0;
 for(const [x,y,dx,dy] of passes){const width=Math.ceil((256-x)/dx),height=Math.ceil((256-y)/dy),stride=Math.ceil(width*channels*depth/8)+1;for(let row=0;row<height;row++){if(at+stride>decoded.length||decoded[at]>4)throw Error('Preview PNG pixel data is invalid.');at+=stride;}}
 if(at!==decoded.length)throw Error('Preview PNG pixel data length is invalid.');
}
async function validate(file,expected){
 const pack=await openPreviewImagePack(file);
 try{const profile=checkCatalog(pack,expected);for(const name of pack.names)checkPng(await pack.read(name));return {count:pack.count,counts:pack.counts,profile:profile.name};}
 finally{await pack.close();}
}
async function findImportedPreviewPack(userData,managedRoot){
 const root=directory(userData,managedRoot);let names;
 try{names=(await fsp.readdir(root)).filter(name=>managedName.test(name));}catch(error){if(error.code==='ENOENT')return null;throw error;}
 const files=await Promise.all(names.map(async name=>{const file=path.join(root,name);try{return {file,mtime:(await fsp.stat(file)).mtimeMs};}catch{return null;}}));
 files.sort((a,b)=>(b?.mtime||0)-(a?.mtime||0));return files.find(Boolean)?.file||null;
}
async function importPreviewImagePack({source,userData,managedRoot,onProgress=()=>{},expected=expectedCounts}){
 await validate(source,expected);
 const root=directory(userData,managedRoot);await fsp.mkdir(root,{recursive:true});
 const total=(await fsp.stat(source)).size,temporary=path.join(root,`.preview-${crypto.randomUUID()}.tmp`),hash=crypto.createHash('sha256');
 let copied=0,lastReport=0;
 try{
  await pipeline(fs.createReadStream(source),new Transform({transform(chunk,_encoding,done){hash.update(chunk);copied+=chunk.length;const now=Date.now();if(now-lastReport>200||copied===total){lastReport=now;onProgress({phase:'copying',copied,total});}done(null,chunk);}}),fs.createWriteStream(temporary,{flags:'wx'}));
  onProgress({phase:'validating',copied,total});
  const result=await validate(temporary,expected);
  const target=path.join(root,`preview-${hash.digest('hex')}.zip`);
  try{await fsp.access(target);await validate(target,expected);await fsp.unlink(temporary);await fsp.utimes(target,new Date(),new Date());}
  catch(error){if(error.code!=='ENOENT')throw error;await fsp.rename(temporary,target);}
  return {file:target,...result,sizeBytes:total};
 }catch(error){await fsp.unlink(temporary).catch(()=>{});throw error;}
}
module.exports={expectedCounts,currentCounts,directory,findImportedPreviewPack,importPreviewImagePack,validatePreviewImagePack:validate};
