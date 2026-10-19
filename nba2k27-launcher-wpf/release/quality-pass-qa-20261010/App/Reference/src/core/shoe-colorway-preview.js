'use strict';
const {loadIff,readZipEntry}=require('../vendor/archive-preview/core/iff');

function shoeStyle(iff,id,slug){
 const entry=iff.entries.find(item=>item.name===`${id}_style.NikeIdStyle`);
 if(!entry)throw Error('Shoe style is missing.');
 const bytes=readZipEntry(iff.buffer,entry,true);
 if(bytes.length<272||bytes.readUInt32LE(16)!==0xf86ea2b6||bytes.readUInt32LE(20)!==Number(id)
  ||bytes.subarray(216,272).toString('ascii').split('\0')[0]!==slug)throw Error('Shoe style does not match its model.');
 const count=bytes.readUInt32LE(84);
 if(count<1||count>32)throw Error('Unsupported shoe region count.');
 return count;
}
function shoePalette(bytes,id,count){
 if(bytes.length!==848||bytes.readUInt32LE(0)!==0xfae98e2b||bytes.readUInt32LE(72)!==Number(id))throw Error('Unsupported shoe palette.');
 const colors=[];let teamColors=false;
 for(let index=0;index<count;index++){
  const offset=80+index*24;
  if(bytes.readUInt16LE(offset)!==index||bytes[offset+7]!==255)throw Error('Unsupported shoe region.');
  if(bytes[offset+8]!==0xc0)teamColors=true;
  colors.push([...bytes.subarray(offset+4,offset+7)]);
 }
 return {colors,teamColors};
}
function shoeRegionMap(iff,side){
 const stem=side==='right'?'shoe_regions_r.':'shoe_regions.';
 const entry=iff.entries.find(item=>item.name.startsWith(stem)&&item.name.endsWith('.dds'));
 if(!entry||entry.size>20*1024*1024)throw Error(`Shoe ${side} region map is unavailable.`);
 return readZipEntry(iff.buffer,entry,true);
}
function colorizeShoeRegions(bytes,colors,maxSize=512){
 if(bytes.length<148||bytes.toString('ascii',0,4)!=='DDS '||bytes.toString('ascii',84,88)!=='DX10'||bytes.readUInt32LE(128)!==28)throw Error('Unsupported shoe region map.');
 const sourceWidth=bytes.readUInt32LE(16),sourceHeight=bytes.readUInt32LE(12);
 if(!sourceWidth||!sourceHeight||sourceWidth>2048||sourceHeight>2048||bytes.readUInt32LE(20)!==sourceWidth*4||bytes.length<148+sourceWidth*sourceHeight*4)throw Error('Invalid shoe region dimensions.');
 const scale=Math.max(1,Math.ceil(Math.max(sourceWidth,sourceHeight)/maxSize));
 const width=Math.ceil(sourceWidth/scale),height=Math.ceil(sourceHeight/scale),pixels=Buffer.alloc(width*height*4);
 for(let y=0;y<height;y++)for(let x=0;x<width;x++){
  const sourceX=Math.min(sourceWidth-1,Math.floor((x+.5)*sourceWidth/width));
  const sourceY=Math.min(sourceHeight-1,Math.floor((y+.5)*sourceHeight/height));
  const color=colors[bytes[148+(sourceY*sourceWidth+sourceX)*4]>>>3];
  if(!color)throw Error('Shoe region index outside its palette.');
  const offset=(y*width+x)*4;pixels[offset]=color[0];pixels[offset+1]=color[1];pixels[offset+2]=color[2];pixels[offset+3]=255;
 }
 return {width,height,pixels};
}
async function readShoeStyles(file,id,slug){return shoeStyle(await loadIff(file),id,slug);}
async function readShoeRegions(file){const iff=await loadIff(file);return {left:shoeRegionMap(iff,'left'),right:shoeRegionMap(iff,'right')};}
async function readShoePalettes(file){
 const iff=await loadIff(file),entries=new Map();
 for(const entry of iff.entries){
  if(!/^shoe_\d{4}_[a-zA-Z0-9_]+\.CustomShoeData$/.test(entry.name))continue;
  entries.set(entry.name.slice(0,-'.CustomShoeData'.length),readZipEntry(iff.buffer,entry,true));
 }
 return entries;
}
module.exports={shoeStyle,shoePalette,colorizeShoeRegions,readShoeStyles,readShoeRegions,readShoePalettes};
