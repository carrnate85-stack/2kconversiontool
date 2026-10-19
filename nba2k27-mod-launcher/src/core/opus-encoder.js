'use strict';
const path=require('node:path');
const {wrapPcmWave}=require('./native-pcm');
const {build,parse,toOgg,isNativeOpus,frames}=require('./native-opus');
let tail=Promise.resolve();
function encodeNativeOpus(wave){
 const job=tail.then(async()=>{
  if(isNativeOpus(wave)){parse(wave);return wave;}
  const pcm=wrapPcmWave(wave).subarray(54);
  const {app,BrowserWindow}=require('electron');await app.whenReady();
  const win=new BrowserWindow({show:false,webPreferences:{sandbox:true,nodeIntegration:false,contextIsolation:true,backgroundThrottling:false}});
  let timer;
  try{
   await win.loadFile(path.join(__dirname,'opus-encoder.html'));
   const task=(async()=>{
    await win.webContents.executeJavaScript(`window.beginMusic(${pcm.length});void 0`);
    for(let offset=0;offset<pcm.length;offset+=1024*1024)await win.webContents.executeJavaScript(`window.appendMusic('${pcm.subarray(offset,offset+1024*1024).toString('base64')}',${offset});void 0`);
    const result=await win.webContents.executeJavaScript('window.encodeMusic()');
    const payload=Buffer.from(result.packed,'base64'),packets=[];
    for(let i=0;i<payload.length;){if(i+2>payload.length)throw Error('Truncated encoder packet.');const n=payload.readUInt16LE(i);i+=2;if(!n||i+n>payload.length)throw Error('Invalid encoder packet.');packets.push(payload.subarray(i,i+n));i+=n;}
    // WebCodecs can flush a packet beyond the declared native duration.
    // Retain every packet needed to cover that duration, not encoder-only tail padding.
    let total=packets.reduce((n,p)=>n+frames(p),0);
    while(packets.length>1&&total-frames(packets.at(-1))>=result.samples)total-=frames(packets.pop());
    const native=build(packets,result.samples),parsed=parse(native);
    await win.webContents.executeJavaScript(`window.verifyMusic('${toOgg(native).toString('base64')}',${parsed.samples})`);
    return native;
   })();
   return await Promise.race([task,new Promise((_,reject)=>{
    win.webContents.once('render-process-gone',(_event,details)=>reject(Error('Audio worker stopped ('+details.reason+'); source retained.')));
    timer=setTimeout(()=>reject(Error('Music conversion timed out; source retained.')),180000);
   })]);
  }finally{clearTimeout(timer);if(!win.isDestroyed())win.destroy();}
 });tail=job.catch(()=>{});return job;
}
module.exports={encodeNativeOpus};
