'use strict';
const path=require('node:path');
const exec=require('node:util').promisify(require('node:child_process').execFile);
const {fileTime}=require('./freestyle-unlocks');
const phases=new Set(['bound','waiting','no-game','ended','applied','restored','error','conflict','unsupported','write-failed']);
function options(value){
 if(!value||Object.keys(value).length!==3||!['drone','dj','mc'].every(k=>typeof value[k]==='boolean'))throw Error('Expected three Boolean removal options.');
 return {drone:value.drone,dj:value.dj,mc:value.mc};
}
function mask(value){return (value.drone?1:0)|(value.dj?2:0)|(value.mc?4:0);}
class BlacktopPresentation {
 constructor({directory,helperPath,publish=()=>{},invoke,now=Date.now,initialOptions={drone:false,dj:false,mc:false},persistOptions=async()=>{}}){
  Object.assign(this,{directory,helperPath,publish,now,persistOptions});this.options=options(initialOptions);this.state={phase:mask(this.options)?'configured':'disabled',message:'Changes apply on the next Play launch.'};this.queue=Promise.resolve();this.saveQueue=Promise.resolve();this.generation=0;this.running=false;
  this.invoke=invoke||(async(action,root,session,selection)=>{
   const args=action==='bind'?['debug-bind',root,path.join(directory,'presentation-bind.json'),'0','0','0']:['presentation-tick',root,path.join(directory,'blacktop-presentation.json'),String(session.pid),session.created,session.basis,String(selection)];
   let result;try{result=await exec(helperPath,args,{windowsHide:true,timeout:8000,maxBuffer:16384});}catch(e){if(!e.stdout)throw Error('Blacktop presentation helper failed. Check game/launcher permissions.');result=e;}
   return JSON.parse(result.stdout);
  });
 }
 view(){return {...this.state,options:{...this.options}};}
 emit(value){this.state={...value};this.publish(this.view());return this.view();}
 setOptions(value){this.options=options(value);return this.emit({phase:'configured',message:'Saved for the next Play launch. Remove MC also disables MC camera cuts.'});}
 saveOptions(value){
  const selection=options(value);
  // Only publish after durable storage succeeds. A failed save leaves the UI
  // and the next-launch selection unchanged; it never touches an active game.
  const job=this.saveQueue.then(async()=>{await this.persistOptions(selection);return this.setOptions(selection);});
  this.saveQueue=job.catch(()=>{});return job;
 }
 cancel(){this.running=false;this.generation++;clearTimeout(this.timer);}
 schedule(ms=500){clearTimeout(this.timer);if(this.running){this.timer=setTimeout(()=>this.tick(),ms);this.timer.unref?.();}}
 async arm(root){
  await this.saveQueue;await this.stop();this.root=root;this.session=null;this.blocked=false;this.selection=mask(this.options);
  if(!this.selection)return this.emit({phase:'disabled',message:'Blacktop removal options are off.'});
  this.startedAt=this.now();this.deadline=this.startedAt+120000;this.running=true;this.emit({phase:'waiting',message:'Waiting for the new Play session.'});this.schedule(0);
 }
 tick(){
  const generation=this.generation;
  const job=this.queue.then(async()=>{
   if(!this.running||generation!==this.generation)return this.view();
   if(!this.session&&this.now()>this.deadline){this.cancel();return this.emit({phase:'timeout',message:'No new game session found. No removal flags applied.'});}
   let value;
   try{
    value=await this.invoke(this.session?'tick':'bind',this.root,this.session,this.selection);
    if(!value||!phases.has(value.phase)||typeof value.message!=='string')throw Error('Invalid presentation response.');
    if(value.phase==='bound'){
     if(this.session||!Number.isSafeInteger(value.pid)||value.pid<=0||!/^\d+$/.test(value.created)||!/^\d+$/.test(value.basis)||BigInt(value.basis)<65536n||BigInt(value.created)<fileTime(this.startedAt)||BigInt(value.created)>fileTime(this.now()+1000))throw Error('Presentation requires a new game started with Play.');
     this.session={pid:value.pid,created:value.created,basis:value.basis};
    }
   }catch(e){value={phase:'error',message:e.message};}
   if(['error','conflict','unsupported','write-failed'].includes(value.phase)){this.blocked=true;this.cancel();}
   if(generation!==this.generation&&!this.blocked)return this.view();
   if(value.phase==='ended'||value.phase==='no-game'&&this.session){this.cancel();this.session=null;value={phase:'ended',message:'Game ended; memory-only removal settings cleared.'};}
   this.emit(value);this.schedule(value.phase==='bound'?0:500);return this.view();
  });this.queue=job.catch(()=>{});return job;
 }
 stop(){
  // Memory-only settings live until game exit, even if the launcher closes.
  this.cancel();const job=this.queue.then(()=>this.view());this.queue=job.catch(()=>{});return Promise.all([job,this.saveQueue]).then(()=>this.view());
 }
}
module.exports={BlacktopPresentation,options,mask};
