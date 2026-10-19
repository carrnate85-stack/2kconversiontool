'use strict';
const fs=require('node:fs/promises'),path=require('node:path');
const {Worker,isMainThread,parentPort,workerData}=require('node:worker_threads');
const mask=(1n<<64n)-1n;
function archiveKey(name){let crc=mask;for(const b of Buffer.from(name.toUpperCase(),'ascii')){crc^=BigInt(b);for(let i=0;i<8;i++)crc=(crc>>1n)^((crc&1n)?0xc96c5795d7870f42n:0n);}return crc^mask;}
function page(packet,sequence,granule,flags){
 const lacing=[...Array(Math.floor(packet.length/255)).fill(255),packet.length%255];
 const out=Buffer.alloc(27+lacing.length+packet.length);out.write('OggS');out[5]=flags;out.writeBigUInt64LE(BigInt(granule),6);out.writeUInt32LE(1,14);out.writeUInt32LE(sequence,18);out[26]=lacing.length;Buffer.from(lacing).copy(out,27);packet.copy(out,27+lacing.length);
 let crc=0;for(const b of out){crc^=b<<24;for(let i=0;i<8;i++)crc=((crc<<1)^((crc&0x80000000)?0x04c11db7:0))>>>0;}out.writeUInt32LE(crc,22);return out;
}
function toOgg(raw){
 if(raw.length<44||raw.readUInt32LE(0)!==0xbbcfa17e||raw.readUInt32LE(4)!==36||raw.readUInt32LE(8)!==0x758195f9||raw.readUInt32LE(12)!==1||raw.readUInt32LE(16)!==48000||raw.readUInt32LE(28)!==raw.length-44)throw Error('This audio format is not supported by preview yet.');
 const samples=raw.readUInt32LE(20);if(!samples||samples>48000*20)throw Error('Name clip exceeds the preview limit.');
 const packets=[];let cursor=44,total=0;
 while(cursor<raw.length){if(cursor+2>raw.length)throw Error('Truncated audio packet.');const n=raw.readUInt16LE(cursor);cursor+=2;if(!n||n>1275||cursor+n>raw.length)throw Error('Invalid audio packet.');const p=raw.subarray(cursor,cursor+n);cursor+=n;
  // Guard the two mono Opus frame configurations validated against stock clips.
  if(p[0]!==0x78&&p[0]!==0x70)throw Error('This voice variant is not supported by preview yet.');
  total+=p[0]===0x78?960:480;packets.push({p,total});if(packets.length>2000)throw Error('Too many audio packets.');
 }
 if(samples>total||samples<=total-960)throw Error('Audio duration is inconsistent.');
 const head=Buffer.alloc(19);head.write('OpusHead');head[8]=1;head[9]=1;head.writeUInt32LE(48000,12);
 // Preview uses zero pre-skip; native decoder delay is not yet mapped.
 const tags=Buffer.alloc(16);tags.write('OpusTags');
 return Buffer.concat([page(head,0,0,2),page(tags,1,0,0),...packets.map(({p,total},i)=>page(p,i+2,i===packets.length-1?samples:total,i===packets.length-1?4:0))]);
}
function validateRequest(request){if(!request||!['auto','commentary','pa'].includes(request.bank)||!Number.isInteger(request.id)||request.id<0||request.id>32767)throw Error('Choose an assigned sound ID from 0 to 32767.');}
async function selectPreview(request,readBank){
 validateRequest(request);
 const banks=request.bank==='auto'?['commentary','pa']:[request.bank];
 const failures=[];
 for(const bank of banks){try{return {...await readBank(bank),bank};}catch(error){failures.push(error);}}
 const missing=failures.every(error=>error.code==='NO_CLIP');
 throw Error(missing?'No clip available for this sound ID in '+(request.bank==='auto'?'stock English commentary or PA.':request.bank==='pa'?'stock English PA.':'stock English commentary.'):'No supported stock preview could be loaded for this ID.');
}
async function readPreview(gameRoot,request){return selectPreview(request,bank=>readBankPreview(gameRoot,{...request,bank}));}
async function readBankPreview(gameRoot,request){
 validateRequest(request);const root=path.resolve(gameRoot);const indexPath=path.join(root,'0A');const stat=await fs.stat(indexPath);if(stat.size<4096||stat.size>128*1024*1024)throw Error('Unsupported game archive index.');
 const index=await fs.readFile(indexPath);if(index.readUInt32LE(0)!==0xaa00b3bf||index.readUInt32LE(8)!==1)throw Error('Unsupported game archive index version.');
 const files=(await fs.readdir(root)).filter(n=>/^[0-9][A-Z]$/.test(n)).sort();const ranges=[];let end=0;
 for(const name of files){const file=path.join(root,name),s=await fs.stat(file);if(!s.isFile())continue;ranges.push({file,start:end,end:end+s.size});end+=s.size;}
 async function read(name,max){const needle=Buffer.alloc(8);needle.writeBigUInt64LE(archiveKey(name));const at=index.indexOf(needle);if(at<0||at+24>index.length||index.indexOf(needle,at+1)>=0)throw Error('Audio resource is missing or ambiguous.');const size=index.readUInt32LE(at+8),position=Number(index.readBigUInt64LE(at+16)>>23n);const range=ranges.find(r=>position>=r.start&&position<r.end);if(!size||size>max||!range||position+size>range.end)throw Error('Audio archive range is invalid.');const h=await fs.open(range.file,'r');try{const data=Buffer.alloc(size);if((await h.read(data,0,size,position-range.start)).bytesRead!==size)throw Error('Incomplete audio read.');return data;}finally{await h.close();}}
 const folder=request.bank==='pa'?'newaudio/paplayers':'newaudio/players';const directory=await read(folder+'/directory_listing.adb2',16*1024*1024);if(directory.length%28)throw Error('Unsupported audio directory.');
 const matches=[];for(let at=0;at<directory.length;at+=28)if(directory.readUInt32LE(at+4)===request.id){const keys=Array.from({length:6},(_,i)=>directory.readUInt32LE(at+i*4));const seconds=directory.readFloatLE(at+24);if(keys.every(k=>k<=999999)&&Number.isFinite(seconds)&&seconds>0&&seconds<=20)matches.push({keys,seconds});}
 matches.sort((a,b)=>a.keys[4]-b.keys[4]||a.keys[5]-b.keys[5]);if(!matches.length)throw Object.assign(Error('No stock clip is indexed for this sound ID.'),{code:'NO_CLIP'});
 let lastError;for(const candidate of matches.slice(0,8)){try{const raw=await read(folder+'/'+candidate.keys.map(k=>String(k).padStart(6,'0')).join('-')+'.wav',1024*1024);const ogg=toOgg(raw);if(Math.abs(raw.readUInt32LE(20)/48000-candidate.seconds)>0.025)throw Error('Audio and directory durations differ.');const after=await fs.stat(indexPath);if(after.size!==stat.size||after.mtimeMs!==stat.mtimeMs)throw Error('Game archives changed; retry preview.');return {base64:ogg.toString('base64'),variants:matches.length,context:candidate.keys[4],seconds:candidate.seconds};}catch(error){lastError=error;}}
 throw lastError;
}
let active=false;
function preview(gameRoot,request){validateRequest(request);if(active)throw Error('A preview is already loading.');active=true;return new Promise((resolve,reject)=>{const worker=new Worker(__filename,{workerData:{gameRoot,request}});let done=false;const finish=(error,value)=>{if(done)return;done=true;clearTimeout(timer);active=false;void worker.terminate();error?reject(error):resolve(value);};const timer=setTimeout(()=>finish(Error('Audio preview timed out.')),20000);worker.once('message',m=>finish(m.error?Error(m.error):null,m.value));worker.once('error',e=>finish(e));worker.once('exit',()=>{if(!done)finish(Error('Audio preview stopped.'));});});}
if(!isMainThread)readPreview(workerData.gameRoot,workerData.request).then(value=>parentPort.postMessage({value}),error=>parentPort.postMessage({error:error.message}));
module.exports={archiveKey,toOgg,validateRequest,selectPreview,readPreview,preview};
