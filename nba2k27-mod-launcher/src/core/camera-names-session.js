'use strict';
const fs=require('node:fs/promises'),path=require('node:path'),crypto=require('node:crypto');
const {execFile}=require('node:child_process');const {promisify}=require('node:util');
const {findManifestEntry}=require('./jukebox-manager');
const {buildCameraNames,verifyCameraNames,namesIni,cameraNames}=require('./camera-localization');
const hash=b=>crypto.createHash('sha256').update(b).digest('hex').toUpperCase();
const optional=p=>p?fs.readFile(p).catch(e=>{if(e.code==='ENOENT')return null;throw e;}):Promise.resolve(null);
const knownLegacy=new Set(['19A8F875EDC3F27CA7DE4177D2064F5E8AF97D97110B73A25865DD4D21F27EF2','C8D190AD8B53DC2C7174C4D2E99E3B735579DFF3B75A0AB6FF1FFD46EA1C2E5A','06E0C33B9E2D0A006B45FE79391998732CD3D18C099AE1D03B6060E3EDD7D4AB']);
async function plain(p){let current=path.parse(path.resolve(p)).root;for(const part of path.resolve(p).slice(current.length).split(path.sep).filter(Boolean)){current=path.join(current,part);const s=await fs.lstat(current).catch(e=>{if(e.code==='ENOENT')return null;throw e;});if(s?.isSymbolicLink())throw new Error('Camera names refuse linked files or folders.');}return p;}
async function write(p,b){await plain(p);await fs.mkdir(path.dirname(p),{recursive:true});const temp=p+'.'+crypto.randomUUID()+'.tmp';try{await fs.writeFile(temp,b,{flag:'wx'});await fs.rename(temp,p);}finally{await fs.unlink(temp).catch(e=>{if(e.code!=='ENOENT')throw e;});}}
class CameraNamesSession{
 constructor({root,template,decoderPath,isGameRunning=async()=>false,decode=null}){this.root=path.resolve(root);this.template=template;this.decoderPath=decoderPath;this.isGameRunning=isGameRunning;this.marker=path.join(this.root,'names-session.json');this.cache=path.join(this.root,'names-cache');this.decode=decode|| (async({gameRoot,entry,output})=>promisify(execFile)(this.decoderPath,[path.join(gameRoot,entry.archive),String(entry.offset),String(entry.size),path.join(gameRoot,'data','oodle','oo2core_9_win64.dll'),output],{windowsHide:true,timeout:120000,maxBuffer:1048576}));}
 async seedKeys(force=false){const destination=path.join(this.root,'CameraNames.ini');if(this.seeded&&!force&&await optional(destination))return;await write(destination,namesIni());this.seeded=true;}
 async status(modsRoot){const target=path.join(modsRoot||'','loc','english.iff'),current=await optional(target),source=await optional(this.template);return {available:true,appManaged:true,installed:Boolean(await optional(this.marker)),conflict:Boolean(current&&!knownLegacy.has(hash(current))&&(!source||hash(source)!==hash(current))&&!await optional(this.marker)),target,message:'Camera names are prepared from the installed game for Play; no bundled or permanent English mod is required.'};}
 async build(gameRoot,overrides={}){
   const names=cameraNames(overrides);
   const entry=await findManifestEntry(gameRoot,'loc/english.iff');
   const h=await fs.open(path.join(gameRoot,entry.archive),'r');let packed;
   try{packed=Buffer.alloc(entry.size);let n=0;while(n<packed.length){const r=await h.read(packed,n,packed.length-n,entry.offset+n);if(!r.bytesRead)throw new Error('Stock English archive is truncated.');n+=r.bytesRead;}}finally{await h.close();}
   const identity=hash(packed)+'-names-v3-'+hash(JSON.stringify(names)).slice(0,16),output=path.join(this.cache,identity+'.iff');const existing=await optional(output);
   if(existing){verifyCameraNames(existing,names);return {output,bytes:existing};}
   await fs.mkdir(this.cache,{recursive:true});const raw=path.join(this.cache,identity+'.stock.iff');
   await this.decode({gameRoot,entry,output:raw});
   const bytes=buildCameraNames(await fs.readFile(raw),names);await write(output,bytes);
   await fs.unlink(raw);return {output,bytes};
 }
 async restore(){
   const raw=await optional(this.marker);if(!raw)return;
   if(await this.isGameRunning())throw new Error('Close NBA 2K27 before restoring camera name files.');
   const s=JSON.parse(raw);
   if(s.schema!=='camera-names-session-v1'||!path.isAbsolute(s.modsRoot)||s.target!==path.join(s.modsRoot,'loc','english.iff')||!/^[A-F0-9]{64}$/.test(s.hash)||!(s.before===null||/^[A-F0-9]{64}$/.test(s.before)))throw new Error('Invalid camera-name recovery journal; files left untouched.');
   await plain(s.target);const current=await optional(s.target);
   if(current&&hash(current)!==s.hash&&hash(current)!==s.before)throw new Error('Camera name file changed outside the launcher; it was left untouched.');
   // A recognized old permanent camera mod was migrated to the app backup.
   if(current)await fs.unlink(s.target);
   await fs.unlink(this.marker);
 }
 async prepare({gameRoot,modsRoot,names={}}){
   if(await this.isGameRunning())throw new Error('Restart NBA 2K27 to apply its camera names before language data loads.');
   await this.restore();await this.seedKeys(true);
   const root=path.resolve(modsRoot),target=path.join(root,'loc','english.iff');await plain(target);
   const original=await optional(target),before=original?hash(original):null;
   const template=await optional(this.template),templateHash=template?hash(template):null;
   if(original&&!knownLegacy.has(before)&&before!==templateHash)throw new Error('A different loc/english.iff text mod is installed. It was left untouched; remove it from this profile to use camera names.');
   const built=await this.build(gameRoot,names);
   if(original)await write(path.join(this.root,'legacy-camera-names.iff'),original);
   const state={schema:'camera-names-session-v1',modsRoot:root,target,before,hash:hash(built.bytes)};
   await write(this.marker,JSON.stringify(state));
   const latest=await optional(target);if((latest?hash(latest):null)!==before)throw new Error('English mod changed during camera preparation.');
   await write(target,built.bytes);return {installed:true,appManaged:true,available:true,target};
 }
}
module.exports={CameraNamesSession};
