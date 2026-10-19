'use strict';
const {spawn}=require('node:child_process');
const {fileTime}=require('./freestyle-unlocks');
const {stopChild}=require('./child-lifecycle');
class BlacktopUnlocks {
 constructor({helperPath,publish=()=>{},start=spawn,now=Date.now,stopTimeoutMs=1500}){Object.assign(this,{helperPath,publish,start,now,stopTimeoutMs});this.child=null;this.operation=Promise.resolve();this.stopping=false;}
 enqueue(action){const operation=this.operation.then(action);this.operation=operation.catch(()=>{});return operation;}
 arm(root,enabled){return this.enqueue(async()=>{
  if(!await this.stopOwned())throw new Error('Blacktop unlock restoration is still pending. Close NBA 2K27 before starting another session.');
  if(!enabled)return;
  const child=this.start(this.helperPath,['blacktop-watch',root,String(fileTime(this.now()))],{windowsHide:true,stdio:['pipe','pipe','pipe']});
  this.child=child;this.stopping=false;let pending='',lastPhase='',lastError='';
  child.stdin.on('error',()=>{});
  child.stdout.on('data',chunk=>{
   pending+=chunk.toString();if(pending.length>16384){child.stdin.end();return;}
   const lines=pending.split('\n');pending=lines.pop();
   for(const line of lines){try{const value=JSON.parse(line);if(this.child===child&&typeof value.message==='string'){lastPhase=value.phase;if(value.phase==='error'||value.phase==='conflict')lastError=value.message;if(!this.stopping)this.publish(value);}}catch{}}
  });
  child.stderr.on('data',()=>{});
  child.on('error',error=>{if(this.child===child){lastError=error.message;this.publish({phase:'error',message:`Blacktop unlock helper failed: ${lastError}`});if(!child.pid){this.child=null;this.stopping=false;}}});
  child.on('exit',(code,signal)=>{if(this.child!==child)return;const stopping=this.stopping;this.child=null;this.stopping=false;
   const failed=Boolean(signal)||code!=null&&code!==0||lastPhase==='error'||lastPhase==='conflict';
   if(stopping)this.publish(failed?{phase:'error',message:`Blacktop unlock restoration could not be confirmed.${lastError?' '+lastError:' Close NBA 2K27 before retrying.'}`}:{phase:'ended',message:'Blacktop unlock helper finished restoration.'});
   else if((failed||lastPhase!=='ended')&&lastPhase!=='error'&&lastPhase!=='unavailable'&&lastPhase!=='conflict')this.publish({phase:'error',message:`Blacktop unlock helper stopped unexpectedly${signal?` (${signal})`:code!=null?` (exit ${code})`:''}.${lastError?' '+lastError:' Restart the game before retrying.'}`});
  });
 });}
 // EOF requests orderly native restoration; never terminate an armed worker.
 stop(){return this.enqueue(()=>this.stopOwned());}
 async stopOwned(){
  const child=this.child;if(!child)return true;
  const first=!this.stopping;this.stopping=true;
  const exited=await stopChild(child,{timeoutMs:this.stopTimeoutMs,forceKill:false,graceful:value=>{if(first)value.stdin?.end();}});
  if(exited){if(this.child===child){this.child=null;this.stopping=false;this.publish({phase:'ended',message:'Blacktop unlock helper finished restoration.'});}}
  else this.publish({phase:'pending',message:'Blacktop unlock restoration is still pending; close NBA 2K27 before retrying.'});
  return exited;
 }
}
module.exports={BlacktopUnlocks};
