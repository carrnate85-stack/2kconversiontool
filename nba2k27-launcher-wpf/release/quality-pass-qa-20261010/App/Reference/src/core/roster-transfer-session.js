'use strict';
const fs=require('node:fs/promises'),path=require('node:path'),crypto=require('node:crypto');
const transfer=require('./roster-transfer');
const WRITES_DISABLED='Whole-roster writes are disabled pending identity, ownership and recovery validation.';
// A batch is intentionally not described as atomic. Individual native edits
// verify/rollback; completed records remain applied if a later record fails.
class RosterTransferSession {
 constructor({scan,manager,backupRoot}){Object.assign(this,{scan,manager,backupRoot});this.busy=false;this.pending=null;this.report={status:'idle'};}
 async prepare(data,sections,includeTeams){
  if(this.busy)throw Error('A roster operation is already running.');
  this.busy=true;this.pending=null;
  try{const state=await this.scan();const plan=transfer.plan(data,state,sections,includeTeams);this.pending={plan,id:crypto.randomUUID(),pid:state.pid};return {id:this.pending.id,...transfer.preview(plan)};}finally{this.busy=false;}
 }
 start(id){
  // Defense in depth: the IPC gate alone must not enable this draft coordinator.
  throw Error(WRITES_DISABLED);
  /* Draft execution path retained for further offline review, not callable.
  if(this.busy||!this.pending||id!==this.pending.id)throw Error('Preview the roster import again.');
  const pending=this.pending;this.pending=null;this.busy=true;this.cancelled=false;
  this.report={status:'preflight',completed:0,total:pending.plan.entries.filter(e=>e.status==='change').length};
  this.task=this.run(pending).catch(error=>{this.report={...this.report,status:'failed',message:error.message};}).finally(()=>{this.busy=false;});
  return this.status();
  */
 }
 status(){return {...this.report};}
 cancel(){this.cancelled=true;return this.status();}
 async run(pending){
  throw Error(WRITES_DISABLED);
  /* Draft execution path retained for further offline review, not callable.
  const state=await this.scan();
  if(state.pid!==pending.pid||transfer.fingerprint(state)!==pending.plan.fingerprint)throw Error('The loaded roster changed after preview. Nothing was applied. Preview again.');
  const entries=pending.plan.entries.filter(e=>e.status==='change');
  if(!entries.length)throw Error('No changes to apply.');
  if(this.cancelled){this.report.status='cancelled';return;}
  const backup=transfer.snapshot(state),folder=path.join(this.backupRoot,new Date().toISOString().replace(/[:.]/g,'-')+'-'+crypto.randomBytes(3).toString('hex'));
  await fs.mkdir(folder,{recursive:true});
  await fs.writeFile(path.join(folder,'before.2kroster'),JSON.stringify(backup),'utf8');
  this.report={...this.report,status:'applying',backup:folder};
  const recovery=structuredClone(backup),journal=[];
  const persist=async()=>{
   // Both original and post-import identity are retained in the journal. Recovery
   // uses the verified post-write identity so renamed players can be found again.
   await fs.writeFile(path.join(folder,'recovery.2kroster'),JSON.stringify(recovery),'utf8');
   await fs.writeFile(path.join(folder,'journal.json'),JSON.stringify({status:this.report,journal},null,2),'utf8');
  };
  await persist();
  for(const entry of entries){
   if(this.cancelled){this.report.status='cancelled';await persist();return;}
   this.report.current=entry.name;
   journal.push({kind:entry.kind,name:entry.name,status:'pending',...(entry.before?{before:entry.before,recovery:entry.recovery}:{})});
   await persist(); // A recovery record must be durable before any game write.
   try{
    if(entry.kind==='player')await this.manager.editPlayer(entry.edit);else await this.manager.editTeam(entry.edit);
    journal.at(-1).status='verified';
    if(entry.kind==='player'){
     const old=recovery.players.find(p=>JSON.stringify(p.match)===JSON.stringify(entry.before.match));
     if(old)old.match=entry.recovery.match;
    }
    this.report.completed++;await persist();
   }catch(error){journal.at(-1).status='failed-or-unconfirmed';this.report.status='failed';this.report.message='Stopped at '+entry.name+': '+error.message;await persist();return;}
  }
  this.report.status='complete';delete this.report.current;await persist();
  */
 }
}
module.exports={RosterTransferSession};
