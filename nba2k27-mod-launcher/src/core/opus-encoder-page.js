'use strict';
// Trusted local worker page: no network, audio output, or Node access.
let source;
window.beginMusic=length=>{source=new Uint8Array(length);};
window.appendMusic=(base64,offset)=>{source.set(Uint8Array.from(atob(base64),c=>c.charCodeAt(0)),offset);};
window.encodeMusic=async function(){
 const raw=source,pcm=new DataView(raw.buffer),samples=raw.length/4;
 const config={codec:'opus',sampleRate:48000,numberOfChannels:2,bitrate:192000,opus:{frameDuration:20000}};
 if(!(await AudioEncoder.isConfigSupported(config)).supported)throw Error('Native Opus encoder is unavailable.');
 const packets=[];let failure;
 const encoder=new AudioEncoder({output:chunk=>{const b=new Uint8Array(chunk.byteLength);chunk.copyTo(b);packets.push(b);},error:e=>{failure=e.message;}});
 try{
  encoder.configure(config);
  for(let pos=0;pos<samples;pos+=960){
   const n=Math.min(960,samples-pos),data=new Float32Array(n*2);
   for(let i=0;i<n;i++){data[i]=pcm.getInt16((pos+i)*4,true)/32768;data[n+i]=pcm.getInt16((pos+i)*4+2,true)/32768;}
   const frame=new AudioData({format:'f32-planar',sampleRate:48000,numberOfFrames:n,numberOfChannels:2,timestamp:Math.round(pos/48000*1e6),data});
   encoder.encode(frame);frame.close();
   // Wait for actual encoder capacity, not thousands of throttled timer ticks
   // in a hidden window. No busy loop and only a small number of queued frames.
   if(encoder.encodeQueueSize>32)await new Promise(resolve=>encoder.addEventListener('dequeue',resolve,{once:true}));
   if(failure)throw Error(failure);
  }
  await encoder.flush();if(failure)throw Error(failure);
  const packed=new Uint8Array(packets.reduce((n,p)=>n+p.length+2,0));let offset=0;
  for(const p of packets){packed[offset++]=p.length&255;packed[offset++]=p.length>>8;packed.set(p,offset);offset+=p.length;}
  let text='';for(let i=0;i<packed.length;i+=16384)text+=String.fromCharCode(...packed.subarray(i,i+16384));
  return {samples,packed:btoa(text)};
 }finally{source=null;if(encoder.state!=='closed')encoder.close();}
};
window.verifyMusic=async function(base64,frames){
 const bytes=Uint8Array.from(atob(base64),c=>c.charCodeAt(0));
 const audio=await new OfflineAudioContext(2,1,48000).decodeAudioData(bytes.buffer);
 if(audio.length!==frames||audio.numberOfChannels!==2)throw Error('Compressed audio duration mismatch.');
 for(let c=0;c<2;c++)for(const s of audio.getChannelData(c))if(!Number.isFinite(s))throw Error('Invalid compressed audio sample.');
 return true;
};
