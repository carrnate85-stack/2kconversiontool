'use strict';
let chunks=[],video=document.querySelector('video'),url,audio,canvas,ctx,total;
const b64=bytes=>{let s='';for(let i=0;i<bytes.length;i+=32768)s+=String.fromCharCode(...bytes.subarray(i,i+32768));return btoa(s);};
window.appendVideo=encoded=>{chunks.push(Uint8Array.from(atob(encoded),c=>c.charCodeAt(0)));};
function waitMedia(event,action){return new Promise((resolve,reject)=>{const timeout=setTimeout(()=>finish(Error('Video decoding timed out.')),30000);const ok=()=>finish(),bad=()=>finish(Error('This MP4 video codec could not be decoded.'));function finish(error){clearTimeout(timeout);video.removeEventListener(event,ok);video.removeEventListener('error',bad);error?reject(error):resolve();}video.addEventListener(event,ok,{once:true});video.addEventListener('error',bad,{once:true});action();});}
window.prepareVideo=async hasAudio=>{
 const blob=new Blob(chunks,{type:'video/mp4'});chunks=[];url=URL.createObjectURL(blob);
 await waitMedia('loadeddata',()=>{video.src=url;video.load();});
 if(!Number.isFinite(video.duration)||video.duration<=0||video.duration>600||!video.videoWidth||video.videoWidth>8192||video.videoHeight>8192)throw Error('Choose a video up to 10 minutes and 8192 pixels per side.');
 if(hasAudio){const ac=new AudioContext();try{const decoded=await ac.decodeAudioData(await blob.arrayBuffer());if(decoded.duration>600)throw Error('Audio exceeds the 10 minute limit.');const offline=new OfflineAudioContext(2,Math.ceil(decoded.duration*48000),48000),source=offline.createBufferSource();source.buffer=decoded;source.connect(offline.destination);source.start();audio=await offline.startRendering();}finally{await ac.close();}}
 total=Math.ceil(Math.max(video.duration,audio?.duration||0)*30);canvas=document.createElement('canvas');canvas.width=960;canvas.height=544;ctx=canvas.getContext('2d',{willReadFrequently:true});
 return {frames:total,seconds:total/30,audio:Boolean(audio),width:960,height:544};
};
window.convertVideoFrame=async index=>{
 if(!Number.isInteger(index)||index<0||index>=total)throw Error('Invalid frame request.');
 const time=Math.min(index/30,Math.max(0,video.duration-1/1000));
 if(Math.abs(video.currentTime-time)>1e-7)await waitMedia('seeked',()=>{video.currentTime=time;});
 ctx.fillStyle='#000';ctx.fillRect(0,0,960,544);const scale=Math.min(960/video.videoWidth,540/video.videoHeight),w=video.videoWidth*scale,h=video.videoHeight*scale;ctx.drawImage(video,(960-w)/2,(544-h)/2,w,h);
 const rgba=ctx.getImageData(0,0,960,544).data;let frame;
 for(const quality of [75,65,55,45,35,25,15]){frame=nativeMovieCodec.encode(rgba,960,544,quality);if(frame.length<=107168)break;}
 if(frame.length>107168)throw Error('This frame exceeds the native video size limit.');
 let pcm=null;if(audio){pcm=new Uint8Array(6400);const dv=new DataView(pcm.buffer),l=audio.getChannelData(0),r=audio.getChannelData(1);for(let i=0;i<1600;i++)for(let ch=0;ch<2;ch++){const v=Math.max(-1,Math.min(1,(ch?r:l)[index*1600+i]||0));dv.setInt16(i*4+ch*2,v<0?v*32768:v*32767,true);}}
 return {frame:b64(frame),pcm:pcm?b64(pcm):null};
};
