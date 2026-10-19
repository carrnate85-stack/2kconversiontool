'use strict';
const fs=require('node:fs/promises'),path=require('node:path'),crypto=require('node:crypto');
const exec=require('node:util').promisify(require('node:child_process').execFile);
const {findManifestEntry}=require('./jukebox-manager');
const {buildScaledBall}=require('./ball-scale-archive');
const DEFAULT_BALL='balls/ball_gameplay_official_nba_nba_official.iff';
// Geometry-only fingerprint of the user's verified 3%-smaller NBA ball.
// Texture-only edits keep this identity, preventing a second reduction.
const KNOWN_SCALED=['1ba2f5d75a7958a3938550d849006ded7184c134c16141efbc3a91971386bd36'];
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const optional=p=>fs.readFile(p).catch(e=>{if(e.code==='ENOENT')return null;throw e;});
async function plain(p){let current=path.parse(path.resolve(p)).root;for(const part of path.resolve(p).slice(current.length).split(path.sep).filter(Boolean)){
 current=path.join(current,part);const s=await fs.lstat(current).catch(e=>{if(e.code==='ENOENT')return null;throw e;});if(s?.isSymbolicLink())throw Error('Ball preparation refuses linked files or folders.');}}
async function write(p,data){await plain(p);await fs.mkdir(path.dirname(p),{recursive:true});const t=p+'.'+crypto.randomUUID()+'.tmp';try{await fs.writeFile(t,data,{flag:'wx'});await fs.rename(t,p);}finally{await fs.unlink(t).catch(e=>{if(e.code!=='ENOENT')throw e;});}}
const safeAsset=a=>typeof a==='string'&&/^balls\/ball[a-z0-9_.-]*\.iff$/i.test(a)&&!a.includes('..');
class BallScaleSession{
 constructor({root,decoderPath,isGameRunning=async()=>false,decode=null}){this.root=path.resolve(root);this.marker=path.join(this.root,'session.json');this.decoderPath=decoderPath;this.isGameRunning=isGameRunning;this.decode=decode||this.stock.bind(this);}
 async stock(gameRoot){
  const e=await findManifestEntry(gameRoot,DEFAULT_BALL);
  if(!Number.isSafeInteger(e.size)||e.size<1||e.size>64*1024*1024)throw Error('Unsupported stock ball size.');
  await plain(this.root);await fs.mkdir(this.root,{recursive:true});const out=path.join(this.root,'stock-'+crypto.randomUUID()+'.iff');
  try{await exec(this.decoderPath,[path.join(gameRoot,e.archive),String(e.offset),String(e.size),path.join(gameRoot,'data/oodle/oo2core_9_win64.dll'),out],{windowsHide:true,timeout:120000,maxBuffer:1048576});return await fs.readFile(out);}
  finally{await fs.unlink(out).catch(e=>{if(e.code!=='ENOENT')throw e;});}
 }
 async restore(){
  await plain(this.marker);const raw=await optional(this.marker);if(!raw)return;
  if(await this.isGameRunning())throw Error('Close NBA 2K27 before restoring ball mods.');
  const s=JSON.parse(raw);
  if(s.schema!=='ball-scale-session-v1'||!path.isAbsolute(s.modsRoot||'')||!Array.isArray(s.files)||s.files.length>64||typeof s.createdBalls!=='boolean')throw Error('Invalid ball recovery journal. Originals retained.');
  const seen=new Set(),actions=[];
  for(const f of s.files){
   if(!safeAsset(f.asset)||seen.has(f.asset.toLowerCase())||!/^[a-f0-9]{64}$/.test(f.after)||!(f.before===null||/^[a-f0-9]{64}$/.test(f.before)))throw Error('Invalid ball recovery target.');
   seen.add(f.asset.toLowerCase());const target=path.join(s.modsRoot,...f.asset.split('/'));await plain(target);const current=await optional(target);
   if(current&&sha(current)!==f.before&&sha(current)!==f.after)throw Error('A ball mod changed outside the launcher; it and its backup were left untouched.');
   let original=null;if(f.before){const backup=path.join(this.root,f.before+'.original');await plain(backup);original=await optional(backup);if(!original||sha(original)!==f.before)throw Error('Ball backup failed verification; no files restored.');}
   actions.push({target,current,original});
  }
  // Validate every destination and backup before the first restoration.
  for(const a of actions){if(a.original){if(!a.current?.equals(a.original))await write(a.target,a.original);}else if(a.current)await fs.unlink(a.target);}
  if(s.createdBalls)await fs.rmdir(path.join(s.modsRoot,'balls')).catch(e=>{if(!['ENOENT','ENOTEMPTY','EEXIST'].includes(e.code))throw e;});
  await fs.unlink(this.marker);
  for(const before of new Set(s.files.map(f=>f.before).filter(Boolean)))await fs.unlink(path.join(this.root,before+'.original')).catch(e=>{if(e.code!=='ENOENT')throw e;});
 }
 async prepare({gameRoot,modsRoot,enabled,percent}){
  if(typeof enabled!=='boolean'||!Number.isInteger(percent)||percent<95||percent>100)throw Error('Invalid ball scaler setting.');
  if(await this.isGameRunning())throw Error('Close NBA 2K27 before preparing ball mods.');
  await this.restore();if(!enabled||percent===100)return {installed:0,alreadyScaled:0,skipped:[],enabled:false};
  const root=path.resolve(modsRoot),balls=path.join(root,'balls');await plain(balls);
  const listed=await fs.readdir(balls,{withFileTypes:true}).catch(e=>{if(e.code==='ENOENT')return [];throw e;});
  const candidates=listed.filter(e=>/^ball.*\.iff$/i.test(e.name)).sort((a,b)=>a.name.localeCompare(b.name));
  if(candidates.length>64)throw Error('Too many ball mods for one safe preparation pass.');
  const built=[],skipped=[];let alreadyScaled=0,totalBytes=0;
  for(const entry of candidates){const asset='balls/'+entry.name,target=path.join(balls,entry.name);
   if(!safeAsset(asset)||!entry.isFile()||entry.isSymbolicLink()){skipped.push(entry.name);continue;}
   await plain(target);const stat=await fs.stat(target);if(stat.size>64*1024*1024||totalBytes+stat.size>256*1024*1024){skipped.push(entry.name);continue;}
   totalBytes+=stat.size;const original=await fs.readFile(target);
   let result;try{result=buildScaledBall(original,{alreadyScaledShapes:KNOWN_SCALED});}catch{skipped.push(entry.name);continue;}
   if(result.alreadyScaled){alreadyScaled++;continue;}
   built.push({asset,original,bytes:result.bytes,before:sha(original),after:sha(result.bytes)});
  }
  // Existing unsupported mods are left alone, never replaced by the fallback.
  if(!candidates.length){const result=buildScaledBall(await this.decode(gameRoot));built.push({asset:DEFAULT_BALL,original:null,bytes:result.bytes,before:null,after:sha(result.bytes)});}
  if(!built.length)return {installed:0,alreadyScaled,skipped,enabled:true};
  await plain(this.root);await fs.mkdir(this.root,{recursive:true});
  for(const f of built)if(f.original){const backup=path.join(this.root,f.before+'.original'),existing=await optional(backup);if(existing&&sha(existing)!==f.before)throw Error('Ball backup conflict.');if(!existing)await fs.writeFile(backup,f.original,{flag:'wx'});}
  if(await this.isGameRunning())throw Error('NBA 2K27 started during ball preparation.');
  const createdBalls=!(await fs.stat(balls).catch(e=>{if(e.code==='ENOENT')return null;throw e;}));
  await write(this.marker,JSON.stringify({schema:'ball-scale-session-v1',modsRoot:root,createdBalls,files:built.map(({asset,before,after})=>({asset,before,after}))}));
  for(const f of built){const target=path.join(root,...f.asset.split('/'));await plain(target);const current=await optional(target);
   if((current?sha(current):null)!==f.before)throw Error('A ball mod changed during preparation; backup retained.');
   if(f.before)await write(target,f.bytes);else{await fs.mkdir(path.dirname(target),{recursive:true});await fs.writeFile(target,f.bytes,{flag:'wx'});}
  }
  return {installed:built.length,alreadyScaled,skipped,enabled:true};
 }
}
module.exports={BallScaleSession,KNOWN_SCALED,DEFAULT_BALL};
