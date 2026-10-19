'use strict';
const {spawn}=require('node:child_process');
const {stopChild}=require('./child-lifecycle');
const {choices}=require('../data/blacktop-arenas.json');
function processAlive(pid){try{process.kill(pid,0);return true;}catch(error){return error.code==='EPERM';}}
class ArenaManager {
 constructor({helperPath,catalogPath,settings,publish=()=>{},spawnProcess=spawn,isAlive=processAlive,healthIntervalMs=1000,stopTimeoutMs=1500}){
  Object.assign(this,{helperPath,catalogPath,settings,publish,spawnProcess,isAlive,healthIntervalMs,stopTimeoutMs});this.child=null;this.pending=null;this.generation=0;this.healthTimer=null;this.stopping=false;this.stopPending=null;
  this.state={selected:'default',phase:'idle',message:'Uses the game’s Day / Night choice.',choices};
 }
 view(){return {...this.state};}
 emit(value){this.state={...this.state,...value};this.publish(this.view());}
 clearHealth(){if(this.healthTimer){clearInterval(this.healthTimer);this.healthTimer=null;}}
 checkHealth(child){
  if(this.child!==child||!child.pid||this.isAlive(child.pid))return;
  this.child=null;this.stopping=false;this.clearHealth();this.emit({phase:'error',message:'Arena helper disappeared before applying the selection. Select the arena again.'});
 }
 async start(){
  if(this.stopping)throw new Error('Arena restoration is still pending. Close NBA 2K27 before changing arenas.');
  if(this.child){this.checkHealth(this.child);if(this.child)return;}
  if(this.pending)return this.pending;
  const generation=this.generation;
  this.pending=(async()=>{
   const {gameRoot}=await this.settings();
   if(generation!==this.generation)return;
   const helperPath=typeof this.helperPath==='function'?this.helperPath():this.helperPath;
   const catalogPath=typeof this.catalogPath==='function'?this.catalogPath():this.catalogPath;
   const child=this.spawnProcess(helperPath,[gameRoot,catalogPath],{windowsHide:true,stdio:['pipe','pipe','pipe']});
   this.child=child;let buffer='',lastError='',stderr='';
   child.stdout.on('data',data=>{buffer+=data.toString();if(buffer.length>65536)buffer=buffer.slice(-65536);let i;
    while((i=buffer.indexOf('\n'))>=0){const line=buffer.slice(0,i);buffer=buffer.slice(i+1);try{const value=JSON.parse(line);
     if(this.child===child&&value.selected===this.state.selected&&['applied','pending','error'].includes(value.phase)&&typeof value.message==='string'){
      if(value.phase==='error')lastError=value.message;
      if(!this.stopping)this.emit({phase:value.phase,message:value.message});
     }
    }catch{}}
   });
   child.stderr.on('data',data=>{stderr=(stderr+data.toString()).slice(-500);});
   child.stdin.on('error',()=>{if(this.child===child)this.emit({phase:'error',message:'Arena helper disconnected. Restart the launcher before changing arenas.'});});
   child.on('error',error=>{if(this.child===child){if(!child.pid){this.child=null;this.stopping=false;this.clearHealth();}this.emit({phase:'error',message:`Arena helper failed: ${error.message}`});}});
   child.on('exit',(code,signal)=>{if(this.child===child){this.child=null;this.clearHealth();const detail=lastError||stderr.trim()||'Select the arena again; if restoration failed, close the game.';if(this.stopping){this.stopping=false;this.emit(signal||code!=null&&code!==0||lastError?{phase:'error',message:`Arena restoration could not be confirmed. ${detail}`}:{phase:'idle',message:'Arena helper finished restoration.'});return;}this.emit({phase:'error',message:`Arena helper stopped${code!==null&&code!==undefined?` (exit ${code})`:signal?` (${signal})`:''}. ${detail}`});}});
   await new Promise((resolve,reject)=>{child.once('spawn',resolve);child.once('error',reject);});
   if(this.child===child&&child.pid){this.healthTimer=setInterval(()=>this.checkHealth(child),this.healthIntervalMs);this.healthTimer.unref?.();}
  })();
  try{await this.pending;}finally{this.pending=null;}
 }
 async select(id){
  const choice=choices.find(c=>c.id===id);
  if(!choice||choice.disabled)throw new Error('That arena is not available for testing yet.');
  await this.start();
  const child=this.child;
  if(this.stopping||!child||child.exitCode!==null&&child.exitCode!==undefined)throw new Error('Arena helper stopped before the selection was sent. Select the arena again.');
  const previous=this.state.selected;
  this.emit({selected:id,phase:'pending',message:'Preparing arena for the next match…'});
  try{await new Promise((resolve,reject)=>{child.stdin.write(id+'\n',error=>error?reject(error):resolve());});}
  catch(error){this.emit({selected:previous,phase:'error',message:`Arena selection could not reach the helper: ${error.message}`});throw error;}
  return this.view();
 }
 stop(){
  if(this.stopPending)return this.stopPending;
  this.generation++;const first=!this.stopping;this.stopping=true;
  this.stopPending=(async()=>{
   await this.pending?.catch(()=>{});const child=this.child;this.clearHealth();
   const exited=await stopChild(child,{timeoutMs:this.stopTimeoutMs,forceKill:false,graceful:value=>{if(first)value.stdin?.end();}});
   if(exited){if(this.child===child)this.child=null;this.stopping=false;}
   else this.emit({phase:'pending',message:'Arena restoration is still pending; close NBA 2K27 before changing arenas.'});
   return exited;
  })().finally(()=>{this.stopPending=null;});return this.stopPending;
 }
}
module.exports={ArenaManager};
