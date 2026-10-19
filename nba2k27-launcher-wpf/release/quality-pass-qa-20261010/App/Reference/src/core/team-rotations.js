'use strict';
const {promisify}=require('node:util'),{execFile}=require('node:child_process'),fs=require('node:fs/promises'),path=require('node:path'),crypto=require('node:crypto');
const run=promisify(execFile);
function unpack(raw){if(typeof raw!=='string'||!/^[A-Za-z0-9+/]{640}$/.test(raw))throw Error('Invalid rotation snapshot.');const b=Buffer.from(raw,'base64');return Array.from({length:48},(_,i)=>Array.from({length:5},(_,j)=>b.readUInt16LE(i*10+j*2)));}
function pack(rows){if(!Array.isArray(rows)||rows.length!==48||rows.some(r=>!Array.isArray(r)||r.length!==5||r.some(v=>!Number.isInteger(v)||v<0||v>65535)))throw Error('Rotations need 48 segments with five player IDs each.');const b=Buffer.alloc(480);rows.forEach((r,i)=>r.forEach((v,j)=>b.writeUInt16LE(v,i*10+j*2)));return b;}
function validate(before,rows,members){const current=unpack(before),after=pack(rows),valid=new Set(members.map(m=>m.id));for(let i=0;i<48;i++){if(current[i].every((v,j)=>v===rows[i][j]))continue;if(new Set(rows[i]).size!==5||rows[i].some(id=>!valid.has(id)))throw Error('Each edited segment needs five different players from this team.');}return after;}
class TeamRotationService{
 constructor(manager,{execute=run,backupRoot}={}){this.manager=manager;this.execute=execute;this.backupRoot=backupRoot;this.busy=false;this.snapshot=null;}
 async read(input){
  if(this.busy)throw Error('A rotation operation is already running.');const state=this.manager.state;
  if(state.status!=='ready'||!Number.isInteger(state.pid)||!state.teams?.some(t=>t.source==='live-team'&&t.recordIndex===input?.recordIndex&&t.teamId===input.teamId&&t.name===input.expectedName))throw Error('Refresh this live team before opening rotations.');
  this.busy=true;this.snapshot=null;try{
   const {stdout}=await this.execute(this.manager.helperPath,['--team-rotations','--pid',String(state.pid),'--record-index',String(input.recordIndex),'--team-id',String(input.teamId),'--expected-name',input.expectedName],{windowsHide:true,timeout:30000,maxBuffer:128*1024,encoding:'utf8'});
   if(this.manager.state!==state)throw Error('The roster changed. Refresh rotations.');const r=JSON.parse(stdout);
   if(r.ok!==true||r.schema!=='nba2k27-team-rotations-v1'||r.readOnly!==true||r.writes!==0||r.recordIndex!==input.recordIndex||r.teamId!==input.teamId||r.expectedName!==input.expectedName||!Array.isArray(r.members)||r.members.length<5||r.members.length>64)throw Error('Invalid guarded rotation report.');
   const ids=new Set();for(const m of r.members)if(!Number.isInteger(m.id)||m.id<0||m.id>65535||ids.has(m.id)||typeof m.name!=='string'||!m.name||m.name.length>39||/[\x00-\x1f\x7f]/.test(m.name))throw Error('Invalid rotation team members.');else ids.add(m.id);
   const rows=unpack(r.raw);if(!/^[a-f0-9]{64}$/.test(r.teamDigest)||!/^[a-f0-9]{64}$/.test(r.memberDigest))throw Error('Invalid rotation ownership proof.');this.snapshot={state,input:{...input},raw:r.raw,members:r.members,teamDigest:r.teamDigest,memberDigest:r.memberDigest,token:crypto.randomUUID()};return {rows,members:r.members,token:this.snapshot.token,readOnly:true,writes:0};
  }finally{this.busy=false;}
 }
 async save(input){
  if(this.busy)throw Error('A rotation operation is already running.');const s=this.snapshot,state=this.manager.state;
  if(state.status!=='ready'||!s||s.state!==state||s.token!==input?.token||s.input.recordIndex!==input.recordIndex||s.input.teamId!==input.teamId||s.input.expectedName!==input.expectedName)throw Error('Refresh rotations before saving.');
  const after=validate(s.raw,input.rows,s.members);if(after.toString('base64')===s.raw)return {saved:true,writes:0,readBackVerified:true};if(!this.backupRoot)throw Error('Rotation recovery storage is unavailable.');
  this.busy=true;try{
   await fs.mkdir(this.backupRoot,{recursive:true});const filename=path.join(this.backupRoot,'rotations-'+crypto.randomUUID()+'.json');const h=await fs.open(filename,'wx');try{await h.writeFile(JSON.stringify({...s.input,pid:state.pid,before:s.raw,after:after.toString('base64'),members:s.members}));await h.sync();}finally{await h.close();}
   if(this.manager.state!==state)throw Error('The roster changed before Save.');
   const {stdout}=await this.execute(this.manager.helperPath,['--team-rotations-write','--pid',String(state.pid),'--record-index',String(input.recordIndex),'--team-id',String(input.teamId),'--expected-name',input.expectedName,'--team-digest',s.teamDigest,'--member-digest',s.memberDigest,'--before',s.raw,'--after',after.toString('base64')],{windowsHide:true,timeout:30000,maxBuffer:128*1024,encoding:'utf8'});const r=JSON.parse(stdout);
   if(r.ok!==true||r.schema!=='nba2k27-team-rotations-write-v1'||r.readBackVerified!==true||r.teamId!==input.teamId||r.recordIndex!==input.recordIndex||!Number.isInteger(r.writes)||r.writes<1||r.writes>240)throw Error('The rotation save was not verified. Refresh before retrying.');
   if(!/^[a-f0-9]{64}$/.test(r.teamDigest))throw Error('Invalid saved rotation ownership proof.');s.teamDigest=r.teamDigest;s.raw=after.toString('base64');return {...r,saved:true,backup:filename};
  }finally{this.busy=false;}
 }
}
module.exports={TeamRotationService,unpack,pack,validate};
