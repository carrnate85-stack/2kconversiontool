'use strict';
// Read-only protocol allowlist. No generic opcode API and no write commands.
const net=require('node:net');
function request(slot,blob){
 if(!Number.isInteger(slot)||slot<1||slot>100||typeof blob!=='boolean')throw Error('Invalid roster read.');
 const b=Buffer.alloc(12);b.writeUInt32BE(blob?0x3f683266:0x9198dc77);b.writeUInt32BE(4,4);b.writeUInt32LE(slot,8);return b;
}
function decode(body,slot,blob){
 if(blob){
  if(body.length<16||body.readUInt32LE(0)!==slot||body.readUInt32LE(4)!==body.length-16||body.length===16)throw Error('Invalid or empty roster blob.');
  return {slot,stamp:body.subarray(8,16).toString('hex'),bundle:Buffer.from(body.subarray(16))};
 }
 if(body.length<25||body.readUInt32LE(0)!==slot||body.readUInt32LE(4)>9)throw Error('Invalid slot metadata.');
 const n=body.readUInt32LE(9);if(n>128||n%2||25+n>body.length)throw Error('Invalid roster name length.');
 const d=body.readUInt32LE(13+n);if(d>256||25+n+d!==body.length)throw Error('Invalid description length.');
 const name=body.subarray(13,13+n).toString('utf16le');if(/[\x00-\x1f\x7f]/u.test(name))throw Error('Invalid roster name.');
 return {slot,type:body.readUInt32LE(4),flagByte:body[8],name,descriptionHex:body.subarray(17+n,17+n+d).toString('hex'),stamp:body.subarray(-8).toString('hex')};
}
function read(slot,blob=false,connect=net.createConnection){
 const packet=request(slot,blob);
 return new Promise((resolve,reject)=>{
  const socket=connect({host:'127.0.0.1',port:7928});let phase=0,parts=[],size=0,done=false,controls=0;
  const limit=blob?16*1024*1024+24:4104;
  const timer=setTimeout(()=>finish(Error('Roster read timed out. Stay at the main menu and try again.')),15000);
  function finish(error,value){if(done)return;done=true;clearTimeout(timer);socket.destroy();error?reject(error):resolve(value);}
  socket.on('connect',()=>socket.write(Buffer.from('ROSTERTOOL')));
  socket.on('error',()=>finish(Error('Cannot reach the game’s roster service. Open NBA 2K27 at the main menu.')));
  socket.on('close',()=>{if(!done)finish(Error('Roster connection closed before the read completed.'));});
  socket.on('data',chunk=>{
   if(done)return;
   try{
    size+=chunk.length;if(size>limit+4096)throw Error('Roster response is too large.');parts.push(chunk);
    if(phase===0){
     if(size<8)return;const ack=Buffer.concat(parts,size);
     if(ack.subarray(0,8).toString('hex')!=='8d921c1500000000')throw Error('Roster handshake rejected.');
     phase=1;const rest=ack.subarray(8);parts=rest.length?[rest]:[];size=rest.length;socket.write(packet);
    }
    while(size>=8){
    // Header may span chunks. Inspect only eight bytes until the full body arrives.
    const header=parts[0].length>=8?parts[0]:Buffer.concat(parts,Math.min(size,8));
    const length=header.readUInt32BE(4);
    // Native roster transport's default empty packet (constructor 21201F9).
    // It can arrive before/after replies, coalesced or fragmented across TCP reads.
    if(header.readUInt32BE(0)===0xff1bebf9){
     if(length!==0||++controls>128)throw Error('Invalid roster transport message.');
     const rest=Buffer.concat(parts,size).subarray(8);parts=rest.length?[rest]:[];size=rest.length;continue;
    }
    if(header.readUInt32BE(0)!==(blob?0xeb553eb0:0x90a3aa2b)||length<(blob?16:25)||length+8>limit)throw Error('Unexpected roster response.');
    if(size<length+8)return;
    // Decode only this framed response. Subsequent transport packets are not
    // part of its body; this one-request connection closes after the reply.
    finish(null,decode(Buffer.concat(parts,size).subarray(8,8+length),slot,blob));return;
    }
   }catch(error){finish(error);}
  });
 });
}
module.exports={read,request,decode};
