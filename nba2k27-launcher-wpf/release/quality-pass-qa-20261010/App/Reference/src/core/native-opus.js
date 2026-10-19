// Native stereo Opus container. Stock round-trip verified; custom playback tested in game.
const MAGIC=0xbbcfa17e,CODEC=0x758195f9;
function frames(packet){
 const toc=packet[0],config=toc>>3,code=toc&3;
 if(code!==0||config<16)throw Error('Research supports single-frame CELT packets only.');
 return [120,240,480,960][config&3];
}
function build(packets,samples){
 if(!packets.length||packets.length>60000||!Number.isInteger(samples)||samples<=0)throw Error('Invalid music sample count.');
 let offset=0,total=0;const entries=[],payload=[];
 for(const packet of packets){if(!Buffer.isBuffer(packet)||!packet.length||packet.length>1275||(packet[0]&4)!==4)throw Error('Invalid stereo packet');entries.push({offset,time:total/48000});total+=frames(packet);const size=Buffer.alloc(2);size.writeUInt16LE(packet.length);payload.push(size,packet);offset+=2+packet.length;}
 if(total%960===0)entries.push({offset,time:total/48000});
 if(samples>total||total-samples>960)throw Error('Unexpected final packet padding.');
 const descriptor=28+entries.length*8,header=Buffer.alloc(8+descriptor);
 [MAGIC,descriptor,CODEC,2,48000,samples*2,entries.length*8,offset,2048].forEach((v,i)=>header.writeUInt32LE(v,i*4));
 entries.forEach((entry,i)=>{header.writeUInt32LE(i?entry.offset:entries.length-1,36+i*8);header.writeFloatLE(entry.time,40+i*8);});
 return Buffer.concat([header,...payload]);
}
function parse(bytes){
 if(!Buffer.isBuffer(bytes)||bytes.length>80*1024*1024||bytes.length<52||bytes.readUInt32LE(0)!==MAGIC||bytes.readUInt32LE(8)!==CODEC||bytes.readUInt32LE(12)!==2||bytes.readUInt32LE(16)!==48000)throw Error('Not stock stereo Opus');
 const descriptor=bytes.readUInt32LE(4),start=8+descriptor,count=bytes.readUInt32LE(36)+1;
 if(count>60000||descriptor!==28+count*8||bytes.readUInt32LE(24)!==count*8||bytes.readUInt32LE(28)!==bytes.length-start||bytes.readUInt32LE(32)!==2048)throw Error('Invalid seek descriptor');
 let cursor=start,total=0;const packets=[];
 while(cursor<bytes.length){const i=packets.length;if(i>=count||cursor+2>bytes.length)throw Error('Packet count mismatch');
  if((i&&bytes.readUInt32LE(36+i*8)!==cursor-start)||Math.abs(bytes.readFloatLE(40+i*8)-total/48000)>0.0001)throw Error('Seek offset or time mismatch');
  const n=bytes.readUInt16LE(cursor);cursor+=2;if(!n||n>1275||cursor+n>bytes.length)throw Error('Invalid packet length');const packet=bytes.subarray(cursor,cursor+n);if(!(packet[0]&4))throw Error('Not stereo');total+=frames(packet);packets.push(packet);cursor+=n;
 }
 if(count===packets.length+1){if(total%960!==0||bytes.readUInt32LE(36+packets.length*8)!==bytes.length-start||Math.abs(bytes.readFloatLE(40+packets.length*8)-total/48000)>.0001)throw Error('Invalid final seek entry');}
 else if(count!==packets.length)throw Error('Truncated seek table');const samples=bytes.readUInt32LE(20)/2;
 if(!Number.isInteger(samples)||samples<=0||samples>57600000||samples>total||total-samples>960)throw Error('Invalid duration');
 return {packets,samples,total,seconds:samples/48000};
}
function page(packet,sequence,granule,flags){
 const lacing=[...Array(Math.floor(packet.length/255)).fill(255),packet.length%255],b=Buffer.alloc(27+lacing.length+packet.length);b.write('OggS');b[5]=flags;b.writeBigUInt64LE(BigInt(granule),6);b.writeUInt32LE(1,14);b.writeUInt32LE(sequence,18);b[26]=lacing.length;Buffer.from(lacing).copy(b,27);packet.copy(b,27+lacing.length);let crc=0;for(const value of b){crc^=value<<24;for(let i=0;i<8;i++)crc=((crc<<1)^((crc&0x80000000)?0x04c11db7:0))>>>0;}b.writeUInt32LE(crc,22);return b;
}
function toOgg(native,preSkip=0){
 const {packets,samples,total}=parse(native);if(samples+preSkip>total)throw Error('Not enough packets for decoder delay');
 const head=Buffer.alloc(19);head.write('OpusHead');head[8]=1;head[9]=2;head.writeUInt16LE(preSkip,10);head.writeUInt32LE(48000,12);const tags=Buffer.alloc(16);tags.write('OpusTags');let decoded=0;
 return Buffer.concat([page(head,0,0,2),page(tags,1,0,0),...packets.map((p,i)=>{decoded+=frames(p);return page(p,i+2,i===packets.length-1?samples+preSkip:decoded,i===packets.length-1?4:0);})]);
}
function isNativeOpus(b){return Buffer.isBuffer(b)&&b.length>=12&&b.readUInt32LE(0)===MAGIC&&b.readUInt32LE(8)===CODEC;}
module.exports={build,parse,frames,toOgg,isNativeOpus};
