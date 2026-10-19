'use strict';
const path=require('node:path');
const exec=require('node:util').promisify(require('node:child_process').execFile);
const {fileTime}=require('./freestyle-unlocks');
const phases=new Set(['bound','waiting','ended','applied','restored','unchanged','error','conflict','unsupported','write-failed']);
const failures=new Set(['error','conflict','unsupported','write-failed']);
class BlacktopCrowd {
 constructor({directory,helperPath,publish=()=>{},invoke,now=Date.now,initialEnabled=true,persistEnabled=async()=>{}}){
  if(typeof initialEnabled!=='boolean')throw Error('Expected a Boolean crowd selection.');
  Object.assign(this,{directory,helperPath,publish,now,persistEnabled});this.enabled=initialEnabled;
  this.running=false;this.session=null;this.applied=false;this.blocked=false;this.stopping=false;this.generation=0;this.queue=Promise.resolve();this.saveQueue=Promise.resolve();
  this.state={phase:'configured',message:initialEnabled?'Crowd control leaves the original setting unchanged.':'Crowd removal saved for the next Play launch.'};
  this.invoke=invoke||(async(action,root,session)=>{
   let result;try{result=await exec(helperPath,[action,root,path.join(directory,'blacktop-crowd-session.json'),String(session?.pid||0),session?.created||'0',session?.basis||'0'],{windowsHide:true,timeout:8000,maxBuffer:16384});}
   catch(e){if(!e.stdout)throw Error('Crowd helper could not complete. Check game/launcher permissions.');result=e;}
   return JSON.parse(result.stdout);
  });
 }
 view(){return {...this.state,enabled:this.enabled,canRestore:!!this.session&&this.applied&&!this.blocked};}
 emit(value){this.state={...value};this.publish(this.view());return this.view();}
 validate(value){if(!value||!phases.has(value.phase)||typeof value.message!=='string'||(value.writes!==undefined&&(!Number.isInteger(value.writes)||value.writes<0||value.writes>6))||(value.phase==='applied'&&!Number.isInteger(value.writes))||(value.ownsChanges!==undefined&&(typeof value.ownsChanges!=='boolean'||value.ownsChanges&&(!['waiting','applied'].includes(value.phase)||!Number.isInteger(value.writes)))))throw Error('Invalid crowd helper response.');return value;}
 cancel(){this.running=false;this.generation++;clearTimeout(this.timer);this.timer=null;}
 schedule(ms=500){clearTimeout(this.timer);if(this.running){this.timer=setTimeout(()=>this.tick(),ms);this.timer.unref?.();}}
 saveEnabled(enabled){
  if(typeof enabled!=='boolean')return Promise.reject(Error('Expected a Boolean crowd selection.'));
  const job=this.saveQueue.then(async()=>{
   await this.persistEnabled(enabled);this.enabled=enabled;
   if(enabled)return this.stop();
   // A newly disabled choice never attaches to or writes an existing game.
   if(this.running||this.applied)return this.emit(this.state);
   return this.emit({phase:'configured',message:'Crowd removal saved for the next Play launch.'});
  });this.saveQueue=job.catch(()=>{});return job;
 }
 async arm(root){
  await this.saveQueue;await this.stop();
  if(this.applied&&!this.blocked)throw Error('The previous Blacktop crowd visit is still awaiting safe restoration.');
  this.root=root;this.session=null;this.applied=false;this.blocked=false;this.stopping=false;
  if(this.enabled)return this.emit({phase:'configured',message:'Crowd control leaves the original setting unchanged.'});
  this.startedAt=this.now();this.deadline=this.startedAt+120000;this.running=true;this.emit({phase:'waiting',message:'Waiting for a new Play session and Blacktop player selection.'});this.schedule(0);
 }
 tick(){
  const generation=this.generation;
  const job=this.queue.then(async()=>{
   if(!this.running||generation!==this.generation)return this.view();
   // The deadline belongs to finding a new game, not to how long the user
   // spends at startup videos or menus after that exact game has been bound.
   if(!this.session&&this.now()>this.deadline){this.cancel();return this.emit({phase:'timeout',message:'No new game session found. No crowd change attempted.'});}
   let value;try{
    value=this.validate(await this.invoke(this.stopping?'crowd-restore':this.session?'crowd-tick':'crowd-bind',this.root,this.session));
    if(value.phase==='bound'){
     if(this.session||value.writes!==0||!Number.isSafeInteger(value.pid)||value.pid<=0||!/^\d+$/.test(value.created)||!/^\d+$/.test(value.basis)||BigInt(value.basis)<65536n||BigInt(value.created)<fileTime(this.startedAt)||BigInt(value.created)>fileTime(this.now()+1000))throw Error('Crowd control requires a new game started with Play.');
     this.session={pid:value.pid,created:value.created,basis:value.basis};
    }else if((value.phase==='applied'||value.ownsChanges===true)&&!this.session)throw Error('Crowd control has no owned game session.');
   }catch(e){value={phase:'error',message:e.message};}
   // Keep write ownership when restoration/shutdown arrives during a helper call.
   if(value.phase==='applied'||value.ownsChanges===true)this.applied=true;
   if(failures.has(value.phase)){this.blocked=true;this.cancel();}
   if(generation!==this.generation&&!this.blocked)return this.view();
   if(value.phase==='ended'){this.cancel();this.session=null;this.applied=false;this.stopping=false;}
   if(value.phase==='restored'||value.phase==='unchanged'){
    this.applied=false;
    // Auto-restoration ends a Blacktop visit, not the Play-session monitor.
    // A later Blacktop selection in this same exact game can apply again.
    if(this.stopping){this.cancel();this.stopping=false;}
   }
   this.emit(value);this.schedule(value.phase==='bound'?0:this.applied?250:500);return this.view();
  });this.queue=job.catch(()=>{});return job;
 }
 stop(){
  this.cancel();this.stopping=true;
  const job=this.queue.then(async()=>{
   if(this.blocked)return this.view();
   if(!this.session){this.stopping=false;return this.view();}
   let value;try{value=this.validate(await this.invoke('crowd-restore',this.root,this.session));}catch(e){value={phase:'error',message:e.message};}
   if(failures.has(value.phase))this.blocked=true;
   if(value.phase==='ended'){this.session=null;this.applied=false;this.stopping=false;}
   if(value.phase==='restored'){this.applied=false;this.stopping=false;}
   // Turning removal off in a match must not write into gameplay. Keep a
   // restore-only monitor until the native helper verifies the main menu.
   if(value.phase==='waiting'&&this.applied&&!this.blocked){this.running=true;this.schedule();}
   return this.emit(value);
  });this.queue=job.catch(()=>{});return job;
 }
}
module.exports={BlacktopCrowd};
