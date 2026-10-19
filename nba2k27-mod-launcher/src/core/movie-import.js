'use strict';
const fs=require('node:fs/promises'),path=require('node:path');
// Only inspect ISO media metadata; never scan encoded payload for codec strings.
function hasAudioTrack(bytes){
 let found=false,visited=0;
 function walk(start,end,depth){if(depth>8)throw Error('Invalid MP4 metadata.');for(let at=start;at<end;){if(++visited>100000||at+8>end)throw Error('Invalid MP4 box.');let n=bytes.readUInt32BE(at),head=8;const type=bytes.toString('ascii',at+4,at+8);if(n===1){if(at+16>end)throw Error('Truncated MP4 box.');const big=bytes.readBigUInt64BE(at+8);if(big>BigInt(Number.MAX_SAFE_INTEGER))throw Error('MP4 box too large.');n=Number(big);head=16;}if(n===0)n=end-at;if(n<head||at+n>end)throw Error('Invalid MP4 box size.');if(type==='hdlr'&&at+head+12<=at+n&&bytes.toString('ascii',at+head+8,at+head+12)==='soun')found=true;if(['moov','trak','mdia'].includes(type))walk(at+head,at+n,depth+1);at+=n;}}
 walk(0,bytes.length,0);return found;
}
function header(frames,audio){const b=Buffer.alloc(4096);b.writeUInt16LE(2);b.writeUInt16LE(1,2);[960,544,frames,30,1].forEach((v,i)=>b.writeUInt32LE(v,4+i*4));b.writeUInt32LE(262144,40);b.writeUInt32LE(107168,44);if(audio){b[24]=2;[48000,2,1].forEach((v,i)=>b.writeUInt32LE(v,28+i*4));b.writeUInt32LE(4,52);[0x063543BD,2,48000,frames*3200,18,frames*6400,4].forEach((v,i)=>b.writeUInt32LE(v,56+i*4));b.writeUInt16LE(1,84);b.writeUInt16LE(2,86);b.writeUInt32LE(48000,88);b.writeUInt32LE(192000,92);b.writeUInt16LE(4,96);b.writeUInt16LE(16,98);}return b;}
function block(rows){const pcm=Buffer.concat(rows.map(r=>r.pcm)),table=(8+rows.length*12+15)&~15,size=(table+rows.reduce((n,r)=>n+r.frame.length,0)+pcm.length+4095)&~4095;if(size>262144)throw Error('Native movie block exceeds capacity.');const b=Buffer.alloc(size);b.writeUInt32LE(rows.length,4);let at=table;rows.forEach((r,i)=>{b.writeUInt32LE(r.frame.length,8+i*12);b.writeUInt32LE(i===0?pcm.length:0,12+i*12);b.writeUInt32LE(at,16+i*12);r.frame.copy(b,at);at+=r.frame.length;if(i===0){pcm.copy(b,at);at+=pcm.length;}});return b;}
async function convertMp4({sourcePath,outputPath,onProgress=()=>{},signal}){
 const {app,BrowserWindow}=require('electron');await app.whenReady();
 const stat=await fs.stat(sourcePath);if(!stat.isFile()||stat.size>512*1024*1024)throw Error('Choose an MP4 smaller than 512 MB.');
 const bytes=await fs.readFile(sourcePath),audio=hasAudioTrack(bytes);
 const win=new BrowserWindow({show:false,webPreferences:{sandbox:true,nodeIntegration:false,contextIsolation:true,backgroundThrottling:false}});
 let handle,deadline;const abort=()=>{if(!win.isDestroyed())win.destroy();};signal?.addEventListener('abort',abort,{once:true});deadline=setTimeout(abort,60*60*1000);
 try{
  const check=()=>{if(signal?.aborted||win.isDestroyed())throw Error('Video import was cancelled.');};check();await win.loadFile(path.join(__dirname,'movie-import.html'));
  onProgress({percent:0,message:'Decoding video and audio…'});
  for(let at=0;at<bytes.length;at+=1024*1024){check();await win.webContents.executeJavaScript(`window.appendVideo('${bytes.subarray(at,at+1024*1024).toString('base64')}');void 0`);}
  const info=await win.webContents.executeJavaScript(`window.prepareVideo(${audio})`);if(!Number.isInteger(info.frames)||info.frames<1||info.frames>18000)throw Error('Invalid decoded movie duration.');
  handle=await fs.open(outputPath,'wx');await handle.write(header(info.frames,info.audio));let rows=[],payload=0,offset=4096,previous=0;
  async function flush(){if(!rows.length)return;const b=block(rows);if(offset+b.length>2*1024**3)throw Error('Converted movie exceeds 2 GB.');const pointer=Buffer.alloc(4);pointer.writeUInt32LE(b.length);await handle.write(pointer,0,4,previous||48);await handle.write(b,0,b.length,offset);previous=offset;offset+=b.length;rows=[];payload=0;}
  for(let i=0;i<info.frames;i++){check();const value=await win.webContents.executeJavaScript(`window.convertVideoFrame(${i})`),frame=Buffer.from(value.frame,'base64'),pcm=Buffer.from(value.pcm||'','base64');if(frame.length<32||frame.length>107168||pcm.length!==(info.audio?6400:0))throw Error('Invalid encoded video frame.');const next=payload+frame.length+pcm.length,needed=(((8+(rows.length+1)*12+15)&~15)+next+4095)&~4095;if(rows.length&&(rows.length===10||needed>262144))await flush();rows.push({frame,pcm});payload+=frame.length+pcm.length;if(i%10===0)onProgress({percent:Math.floor((i+1)*100/info.frames),message:`Converting video: ${i+1} / ${info.frames} frames`});}
  await flush();if(offset<16384)await handle.truncate(16384);await handle.sync();onProgress({percent:100,message:'Checking converted movie…'});
 }finally{clearTimeout(deadline);signal?.removeEventListener('abort',abort);if(handle)await handle.close();if(!win.isDestroyed())win.destroy();}
}
module.exports={convertMp4,hasAudioTrack,header,block};
