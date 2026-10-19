'use strict';
const path=require('node:path');
const exec=require('node:util').promisify(require('node:child_process').execFile);
const phases=new Set(['bound','no-game','ended','outside','waiting','applied','unsupported','conflict','error','write-failed']);
const fileTime=ms=>(BigInt(ms)+11644473600000n)*10000n;

class FreestyleUnlocks {
 constructor({directory,helperPath,publish=()=>{},invoke,now=Date.now}){
  Object.assign(this,{directory,helperPath,publish,now});this.queue=Promise.resolve();this.generation=0;this.running=false;this.timer=null;
  this.state={phase:'saved',message:'Saved — applies on your next launch.'};
  this.invoke=invoke|| (async(action,root,session)=>{
   const args=[action,root,path.join(directory,'team-categories-session.json'),String(session?.pid||0),session?.created||'0',session?.basis||'0'];
   let result;try{result=await exec(helperPath,args,{windowsHide:true,timeout:8000,maxBuffer:16384});}
   catch(e){if(!e.stdout)throw Error('Freestyle unlock helper could not complete. Check that the game and launcher use the same permission level.');result=e;}
   return JSON.parse(result.stdout);
  });
 }
 view(){return {...this.state};}
 emit(state){const changed=this.state.phase!==state.phase||this.state.message!==state.message;this.state={...state};if(changed)this.publish(this.view());return this.view();}
 // Only Play arms a session. Merely opening the launcher/settings never writes.
 arm(root,enabled){
  this.stop();this.gameRoot=root;this.session=null;this.applied=false;
  if(!enabled)return this.emit({phase:'disabled',message:'WNBA unlock is off for this launch.'});
  this.running=true;this.startedAt=this.now();this.deadline=this.startedAt+120000;
  this.emit({phase:'waiting',message:'Waiting for NBA 2K27.'});this.schedule(0);
 }
 schedule(ms=this.applied?3000:this.session?1000:500){clearTimeout(this.timer);if(this.running){this.timer=setTimeout(()=>this.tick(),ms);this.timer.unref?.();}}
 tick(){
  const generation=this.generation;
  const job=this.queue.then(async()=>{
   if(!this.running||generation!==this.generation)return this.view();
   if(!this.session&&this.now()>this.deadline){this.stop();return this.emit({phase:'timeout',message:'The game did not start in time. Unlocks will retry on your next launch.'});}
   let result;try{
    result=await this.invoke(this.session?(this.applied?'category-observe':'category-apply'):'category-bind',this.gameRoot,this.session&&{...this.session});
    if(!result||!phases.has(result.phase)||typeof result.message!=='string')throw Error('Invalid Freestyle unlock response.');
    if(result.phase==='bound'){
     if(this.session||!Number.isSafeInteger(result.pid)||result.pid<=0||!/^\d+$/.test(result.created)||!/^\d+$/.test(result.basis)||BigInt(result.basis)<65536n||BigInt(result.created)<fileTime(this.startedAt)||BigInt(result.created)>fileTime(this.now()+1000))throw Error('Unlocks require a new game session started with Play.');
    }
   }catch(e){result={phase:'error',message:e.message};}
   if(!this.running||generation!==this.generation)return this.view();
   if(result.phase==='bound')this.session={pid:result.pid,created:result.created,basis:result.basis};
   else if(result.phase==='applied')this.applied=true;
   else if(['no-game','ended'].includes(result.phase)&&this.session){this.stop();return this.emit({phase:'saved',message:'Game session ended. Unlocks are saved for your next launch.'});}
   else if(!['no-game','outside','waiting','applied'].includes(result.phase))this.stop();
   this.emit(result);this.schedule(result.phase==='bound'?0:undefined);return this.view();
  });
  this.queue=job.catch(()=>{});return job;
 }
 stop(){this.running=false;this.generation++;clearTimeout(this.timer);this.timer=null;}
}
module.exports={FreestyleUnlocks,fileTime};
