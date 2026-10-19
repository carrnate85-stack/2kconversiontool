'use strict';
// ConsolidatedClothingBinding: item rows, 12-byte property dictionary,
// and a u16 property-index stream. Palette bytes are stored RGBA, not floats.
function clothingBindings(buffer, ids){
 const fail=()=>{throw Error('Unsupported clothing color table.');};
 const u32=p=>{if(p<0||p+4>buffer.length)fail();return buffer.readUInt32LE(p);};
 const ref=p=>{if(p+8>buffer.length)fail();const n=buffer.readBigUInt64LE(p);if(n<1n||n>BigInt(buffer.length))fail();return p+Number(n)-1;};
 if(u32(0)!==0x20e04cc8)fail();
 const count=u32(28),propertyCount=u32(32),indexCount=u32(36),base=ref(40),properties=ref(48),indices=ref(56);
 if(count>200000||propertyCount>65536||indexCount>4000000||base<64||base+count*12>properties||properties+propertyCount*12>indices||indices+indexCount*2>buffer.length)fail();
 const wanted=new Set(ids),result=new Map();
 for(let i=0;i<count;i++){
  const row=base+i*12,id=u32(row);if(!wanted.has(id))continue;
  const n=u32(row+4),start=u32(row+8);if(n>512||start+n>indexCount||result.has(id))fail();
  const records=[];
  for(let j=0;j<n;j++){
   const key=buffer.readUInt16LE(indices+(start+j)*2);if(key>=propertyCount)fail();
   const p=properties+key*12;
   records.push({type:u32(p),parameter:u32(p+4),value:u32(p+8),rgba:[...buffer.subarray(p+8,p+12)]});
  }
  result.set(id,records);
 }
 return result;
}
function garmentPalette(records, materialHash,textureRange){
 // Only the verified two-region clothing shader is enabled. Do not apply
 // palette math to photographic/diffuse textures or guess unknown shaders.
 const twoRegion=records?.some(r=>r.type===5&&r.parameter===materialHash&&r.value===0x5b22e7c4);
 const flatMask=textureRange&&['min','max'].every(key=>Array.isArray(textureRange[key])&&textureRange[key].length>=3)
  &&textureRange.min.slice(0,3).every((value,i)=>value===textureRange.max[i])
  &&([[1,0,0],[0,1,0]].some(color=>color.every((value,i)=>value===textureRange.min[i])));
 const flatDecalBase=flatMask&&records?.some(r=>r.type===5&&r.parameter===materialHash&&r.value===0x1fe1f2b9);
 if(!twoRegion&&!flatDecalBase)return null;
 let active=null;const colors=[];
 for(const record of records){if(record.type===1)active=record.value;if(record.type===4&&active===materialHash)colors.push(record);}
 const primary=colors.find(r=>r.parameter===0xd965ece7),secondary=colors.find(r=>r.parameter===0xeed00af3);
 if(!primary&&!secondary)return null;
 if(flatDecalBase&&((textureRange.min[0]===1&&!primary)||(textureRange.min[1]===1&&!secondary)))return null;
 // Verified clothing_setup_defaults_clothing values are identity red/green.
 // Preserve untinted regions; never invent a white secondary color.
 return {primary:primary?.rgba.slice(0,3)||[255,0,0],secondary:secondary?.rgba.slice(0,3)||[0,255,0],partial:colors.some(r=>![0xd965ece7,0xeed00af3].includes(r.parameter)),schema:'clothing-two-region-v1'};
}
function garmentArtworkPlacement(records,materialHash,slot=0){
 if(!records?.length)return null;
 let active=null;const values=new Map();
 for(const record of records){
  if(record.type===1)active=record.value;
  if(active===materialHash&&record.type===3)values.set(record.parameter,record.value);
 }
 const parameters=[
  [0x3db07ada,0xa4b92b60,0x694e2875,0xf04779cf,0x2903633c],
  [0x0c586047,0x955131fd,0xe7c12f96,0x7ec87e2c,0x18eb79a1],
  [0xaa2f6bf3,0x33263a49,0x2b6b2f08,0xb2627eb2,0xbe9c7215],
  [0x6f88557d,0xf68104c7,0x21ae2611,0xb8a777ab,0x7b3b4c9b],
  [0xc9ff5ec9,0x50f60f73,0xed04268f,0x740d7735,0xdd4c472f],
  [0xf8174454,0x611e15ee,0x638b216c,0xfa8270d6,0xeca45db2],
 ][slot];
 if(!parameters)return null;
 if(parameters.slice(0,4).some(parameter=>!values.has(parameter)))return null;
 const floats=parameters.map((parameter,index)=>{if(index===4&&!values.has(parameter))return 0;const bytes=Buffer.allocUnsafe(4);bytes.writeUInt32LE(values.get(parameter));return bytes.readFloatLE(0);});
 const [scaleU,scaleV,offsetU,offsetV,rotation]=floats;
 if(!floats.every(Number.isFinite)||scaleU<=0||scaleV<=0||scaleU>64||scaleV>64||Math.abs(offsetU)>1||Math.abs(offsetV)>1||Math.abs(rotation)>360)return null;
 return {scale:[scaleU,scaleV],offset:[offsetU,offsetV],rotation};
}
module.exports={clothingBindings,garmentPalette,garmentArtworkPlacement};
