'use strict';
// Recovery only for 0.35.115-test.1. No scanning, generation or new ball sessions.
const fs=require('node:fs/promises'),path=require('node:path'),crypto=require('node:crypto');
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const optional=p=>fs.readFile(p).catch(e=>{if(e.code==='ENOENT')return null;throw e;});
async function plain(p){let current=path.parse(path.resolve(p)).root;for(const part of path.resolve(p).slice(current.length).split(path.sep).filter(Boolean)){
 current=path.join(current,part);const s=await fs.lstat(current).catch(e=>{if(e.code==='ENOENT')return null;throw e;});if(s?.isSymbolicLink())throw Error('Ball recovery refuses linked files or folders.');}}
async function write(p,data){await plain(p);await fs.mkdir(path.dirname(p),{recursive:true});const t=p+'.'+crypto.randomUUID()+'.tmp';try{await fs.writeFile(t,data,{flag:'wx'});await fs.rename(t,p);}finally{await fs.unlink(t).catch(e=>{if(e.code!=='ENOENT')throw e;});}}
const safeAsset=a=>typeof a==='string'&&/^balls\/ball[a-z0-9_.-]*\.iff$/i.test(a)&&!a.includes('..');
class BallScaleRecovery{
 constructor({root,isGameRunning=async()=>false}){this.root=path.resolve(root);this.marker=path.join(this.root,'session.json');this.isGameRunning=isGameRunning;}
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
  // Verify the entire journal before touching any destination.
  for(const a of actions){if(a.original){if(!a.current?.equals(a.original))await write(a.target,a.original);}else if(a.current)await fs.unlink(a.target);}
  if(s.createdBalls)await fs.rmdir(path.join(s.modsRoot,'balls')).catch(e=>{if(!['ENOENT','ENOTEMPTY','EEXIST'].includes(e.code))throw e;});
  await fs.unlink(this.marker);
  for(const before of new Set(s.files.map(f=>f.before).filter(Boolean)))await fs.unlink(path.join(this.root,before+'.original')).catch(e=>{if(e.code!=='ENOENT')throw e;});
 }
}
module.exports={BallScaleRecovery};
