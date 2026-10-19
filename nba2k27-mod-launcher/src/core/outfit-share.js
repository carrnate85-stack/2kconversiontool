'use strict';
const fs=require('node:fs/promises');
const catalog=require('../data/streetwear-components.json');
const {selection}=require('./outfit-presets');
const SCHEMA='nba2k27-shared-outfit-v1',MAX_BYTES=16384;
function nameOf(value){
 if(typeof value!=='string'||!value.trim()||value.trim().length>64||/[\u0000-\u001f\u007f]/.test(value))throw Error('Enter an outfit name (1–64 characters).');
 return value.trim();
}
function encodeOutfit(value){
 const name=nameOf(value?.name),pieces=selection(value);
 const base=catalog.presets.find(p=>p.id===pieces.streetId);
 // Exact original pieces identify the top variant without exporting a local
 // library UUID, transient preset index, user path, player or memory address.
 const document={schema:pieces.accessories!==undefined?'nba2k27-shared-outfit-v3':pieces.accessory?'nba2k27-shared-outfit-v2':SCHEMA,name,top:{asset:base.top,original:[...base.original]},pieces:{bottom:pieces.bottom}};
 for(const key of ['shoe','sock','layer','accessory','accessories'])if(pieces[key]!==undefined)document.pieces[key]=pieces[key];
 return JSON.stringify(document,null,2)+'\n';
}
function decodeOutfit(bytes){
 if(!(typeof bytes==='string'||Buffer.isBuffer(bytes))||Buffer.byteLength(bytes)>MAX_BYTES)throw Error('Outfit file is too large (16 KB maximum).');
 let value;try{value=JSON.parse(bytes.toString());}catch{throw Error('This is not a valid .2koutfit JSON file.');}
 if(![SCHEMA,'nba2k27-shared-outfit-v2','nba2k27-shared-outfit-v3'].includes(value?.schema)||!value.top||typeof value.top.asset!=='string'||!Array.isArray(value.top.original)
   ||value.top.original.length<2||value.top.original.length>7||!value.top.original.every(a=>typeof a==='string'&&a.length<=256)
   ||!value.pieces||typeof value.pieces!=='object'||Array.isArray(value.pieces))throw Error('Unsupported or invalid .2koutfit file.');
 const name=nameOf(value.name);
 const base=catalog.presets.find(p=>['male','female'].includes(p.sourceGroup.pool)&&p.top===value.top.asset&&JSON.stringify(p.original)===JSON.stringify(value.top.original));
 if(!base)throw Error('This outfit’s top variant is not available in this catalog. Nothing was imported.');
 const pieces=selection({streetId:base.id,bottom:value.pieces.bottom,shoe:value.pieces.shoe,sock:value.pieces.sock,layer:value.pieces.layer,accessory:value.pieces.accessory,accessories:value.pieces.accessories});
 return {name,...pieces};
}
async function readOutfitFile(file){
 const handle=await fs.open(file,'r');
 try{
  const stat=await handle.stat();if(!stat.isFile()||stat.size>MAX_BYTES)throw Error('Choose a .2koutfit file no larger than 16 KB.');
  const buffer=Buffer.alloc(MAX_BYTES+1);let total=0;
  while(total<buffer.length){const {bytesRead}=await handle.read(buffer,total,buffer.length-total,null);if(!bytesRead)break;total+=bytesRead;}
  return decodeOutfit(buffer.subarray(0,total));
 }finally{await handle.close();}
}
module.exports={encodeOutfit,decodeOutfit,readOutfitFile,MAX_BYTES};
