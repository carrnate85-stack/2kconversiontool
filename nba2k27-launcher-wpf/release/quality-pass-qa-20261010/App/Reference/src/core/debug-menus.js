'use strict';
const path=require('node:path');
const exec=require('node:util').promisify(require('node:child_process').execFile);
const {fileTime}=require('./freestyle-unlocks');
const phases=new Set(['bound','no-game','ended','waiting','applied','restored','unsupported','conflict','error','write-failed']);
class DebugMenus {
 constructor({directory,helperPath,publish=()=>{},invoke,now=Date.now}){
  Object.assign(this,{directory,helperPath,publish,now});this.enabled=false;this.running=false;this.session=null;this.applied=false;this.queue=Promise.resolve();this.generation=0;
  this.state={phase:'disabled',message:'Debug menus are off. Resets to off whenever the launcher opens.'};
  this.invoke=invoke||(async(action,root,session)=>{
   let result;try{result=await exec(helperPath,[action,root,path.join(directory,'debug-menu-session.json'),String(session?.pid||0),session?.created||'0',session?.basis||'0'],{windowsHide:true,timeout:8000,maxBuffer:16384});}
   catch(error){if(!error.stdout)throw Error('Debug menu helper could not complete. Check game/launcher permissions.');result=error;}
   const value=JSON.parse(result.stdout);if(!value||!phases.has(value.phase)||typeof value.message!=='string')throw Error('Invalid debug menu response.');return value;
  });
 }
 view(){return {...this.state,enabled:this.enabled};}
 emit(value){this.state={...value};this.publish(this.view());return this.view();}
 cancel(){this.running=false;this.generation++;clearTimeout(this.timer);this.timer=null;}
 // Volatile only: no settings file can enable this at application startup.
 async setEnabled(enabled){
  if(typeof enabled!=='boolean')throw Error('Expected a Boolean debug menu selection.');
  this.enabled=enabled;
  if(!enabled)return this.stop();
  if(this.session&&this.state.phase==='restored'){
   this.running=true;this.applied=false;this.deadline=this.now()+120000;this.schedule(0);
   return this.emit({phase:'waiting',message:'Re-enabling debug menus in the owned Play session.'});
  }
  return this.emit({phase:'armed',message:'Experimental — enabled for the next Play launch. Steam must be in Offline Mode.'});
 }
 async arm(root){
  await this.stop();this.gameRoot=root;this.session=null;this.applied=false;this.blocked=false;
  if(!this.enabled)return this.emit({phase:'disabled',message:'Debug menus are off.'});
  this.startedAt=this.now();this.deadline=this.startedAt+120000;this.running=true;
  this.emit({phase:'waiting',message:'Waiting for a new offline Play session.'});this.schedule(0);
 }
 schedule(ms=this.applied?3000:500){clearTimeout(this.timer);if(this.running){this.timer=setTimeout(()=>this.tick(),ms);this.timer.unref?.();}}
 tick(){
  const generation=this.generation;
  const job=this.queue.then(async()=>{
   if(!this.running||generation!==this.generation)return this.view();
   if(!this.applied&&this.now()>this.deadline){this.cancel();return this.emit({phase:'timeout',message:'Debug features did not initialize in time. No further debug writes will be attempted.'});}
   let result;try{
    result=await this.invoke(!this.session?'debug-bind':this.applied?'debug-observe':'debug-apply',this.gameRoot,this.session);
    if(!result||!phases.has(result.phase)||typeof result.message!=='string')throw Error('Invalid debug menu response.');
    if(result.phase==='bound'){
     if(this.session||!Number.isSafeInteger(result.pid)||result.pid<=0||!/^\d+$/.test(result.created)||!/^\d+$/.test(result.basis)||BigInt(result.basis)<65536n||BigInt(result.created)<fileTime(this.startedAt)||BigInt(result.created)>fileTime(this.now()+1000))throw Error('Debug menus require a new game process started with Play.');
     this.session={pid:result.pid,created:result.created,basis:result.basis};
    }
   }catch(error){result={phase:'error',message:error.message};}
   // Preserve ownership even when disable arrives during a helper operation.
   if(result.phase==='applied')this.applied=true;
   if(['conflict','write-failed','error','unsupported'].includes(result.phase))this.blocked=true;
   if(!this.running||generation!==this.generation)return this.view();
   if(['no-game','ended'].includes(result.phase)&&this.session){this.cancel();this.session=null;this.applied=false;return this.emit({phase:'ended',message:'Game ended; memory-only debug changes are gone.'});}
   if(!['bound','waiting','no-game','applied'].includes(result.phase))this.cancel();
   this.emit(result);this.schedule(result.phase==='bound'?0:undefined);return this.view();
  });this.queue=job.catch(()=>{});return job;
 }
 stop(){
  this.cancel();
  const job=this.queue.then(async()=>{
   if(this.blocked)return this.view();
   if(!this.session)return this.emit({phase:'disabled',message:'Debug menus are off.'});
   let result;try{result=await this.invoke('debug-restore',this.gameRoot,this.session);}catch(error){result={phase:'error',message:error.message};}
   if(['conflict','write-failed','error','unsupported'].includes(result.phase))this.blocked=true;
   if(['no-game','ended'].includes(result.phase)){this.session=null;this.applied=false;result={phase:'ended',message:'Game ended; memory-only debug changes are gone.'};}
   if(result.phase==='restored')this.applied=false;
   return this.emit(result);
  });this.queue=job.catch(()=>{});return job;
 }
}
module.exports={DebugMenus};
