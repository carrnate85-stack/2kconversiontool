'use strict';
const path=require('node:path'),crypto=require('node:crypto'),{spawn}=require('node:child_process');
const MAX=256*1024*1024;
const activeChildren=new Set();
const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
function defaultFlacPath(){return process.resourcesPath?path.join(process.resourcesPath,'flac','flac.exe'):path.join(__dirname,'../../vendor/flac-1.5.0/unpacked/flac-1.5.0-win/Win64/flac.exe');}
function waveFromPcm(pcm){
  const header=Buffer.alloc(44);header.write('RIFF');header.writeUInt32LE(pcm.length+36,4);header.write('WAVEfmt ',8);header.writeUInt32LE(16,16);header.writeUInt16LE(1,20);header.writeUInt16LE(2,22);header.writeUInt32LE(48000,24);header.writeUInt32LE(192000,28);header.writeUInt16LE(4,32);header.writeUInt16LE(16,34);header.write('data',36);header.writeUInt32LE(pcm.length,40);return Buffer.concat([header,pcm]);
}
function canonicalWave(wave){return Buffer.isBuffer(wave)&&wave.length>44&&wave.length<=MAX&&(wave.length-44)%4===0&&wave.equals(waveFromPcm(wave.subarray(44)));}
function run(exe,args,input,limit){return new Promise((resolve,reject)=>{
  const child=spawn(exe,args,{windowsHide:true,stdio:['pipe','pipe','pipe']});let length=0,failure,stderr='';const chunks=[];
  activeChildren.add(child);
  const fail=error=>{failure ||= error;child.kill();};
  const timer=setTimeout(()=>fail(Error('FLAC conversion timed out. Original audio was retained.')),120000);
  child.on('error',error=>{failure=error;});
  child.stdout.on('data',chunk=>{length+=chunk.length;if(length>limit)fail(Error('FLAC output exceeds the audio size limit.'));else chunks.push(chunk);});
  child.stderr.on('data',chunk=>{if(stderr.length<2048)stderr+=chunk.toString();});
  child.stdin.on('error',error=>{if(error.code!=='EPIPE')fail(error);});
  child.on('close',code=>{activeChildren.delete(child);clearTimeout(timer);if(failure||code!==0)reject(failure||Error('FLAC conversion failed; original audio retained. '+stderr.trim()));else resolve(Buffer.concat(chunks));});
  child.stdin.end(input);
});}
class FlacAudio {
  constructor(executable=defaultFlacPath()){this.executable=executable;}
  async encode(wave){
    // Preserve unusual legacy RIFF layouts verbatim as WAV instead of discarding metadata.
    if(!canonicalWave(wave))return null;
    const data=await run(this.executable,['--silent','--verify','-8','--no-padding','--no-seektable','--stdout','-'],wave,MAX);
    const metadata={audioCodec:'flac',audioWaveSha256:sha(wave),audioPcmBytes:wave.length-44};
    const restored=await this.decode(data,metadata);
    if(!restored.equals(wave))throw Error('FLAC round-trip verification failed. Original WAV was retained.');
    return {data,metadata};
  }
  async decode(data,record){
    if(record.audioCodec==='native-opus'){
      const {parse}=require('./native-opus');parse(data);
      if(!/^[a-f0-9]{64}$/.test(record.audioNativeSha256||'')||sha(data)!==record.audioNativeSha256)throw Error('Opus audio integrity check failed.');
      return data;
    }
    if(!record.audioCodec||record.audioCodec==='wav')return data;
    if(record.audioCodec!=='flac')throw Error('Unsupported playlist audio codec.');
    const bytes=record.audioPcmBytes;
    if(!Buffer.isBuffer(data)||data.length<42||data.length>MAX||data.toString('ascii',0,4)!=='fLaC'||(data[4]&127)!==0||data.readUIntBE(5,3)!==34
      ||!Number.isInteger(bytes)||bytes<4||bytes>MAX-44||bytes%4||!/^[a-f0-9]{64}$/.test(record.audioWaveSha256||''))throw Error('Invalid FLAC playlist audio metadata.');
    const info=data.readBigUInt64BE(18),rate=Number(info>>44n),channels=Number((info>>41n)&7n)+1,bits=Number((info>>36n)&31n)+1,samples=Number(info&0xfffffffffn);
    if(rate!==48000||channels!==2||bits!==16||(samples&&samples*4!==bytes))throw Error('FLAC audio format or sample count is invalid.');
    const pcm=await run(this.executable,['--silent','--decode','--force-raw-format','--endian=little','--sign=signed','--stdout','-'],data,bytes);
    if(pcm.length!==bytes)throw Error('FLAC decoded length mismatch.');
    const wave=waveFromPcm(pcm);if(sha(wave)!==record.audioWaveSha256)throw Error('FLAC decoded audio integrity check failed.');return wave;
  }
}
async function stopFlacChildren(){const children=[...activeChildren];for(const child of children)try{child.kill();}catch{}await Promise.all(children.map(child=>new Promise(resolve=>{if(child.exitCode!=null)return resolve();child.once('close',resolve);setTimeout(resolve,1000).unref?.();})));return children.length;}
module.exports={FlacAudio,defaultFlacPath,waveFromPcm,canonicalWave,stopFlacChildren};
