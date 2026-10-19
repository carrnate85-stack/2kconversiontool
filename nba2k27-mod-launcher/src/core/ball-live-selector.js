'use strict';
const path=require('node:path');
const fs=require('node:fs');
const {spawn,execFile}=require('node:child_process');
const exec=require('node:util').promisify(execFile);
const {waitForExit}=require('./child-lifecycle');
const {fileTime}=require('./freestyle-unlocks');
const phases=new Set(['pending','ready','loading','applied','current','restoring','restored','ended','unavailable','incompatible','error']);
const validChoice=value=>value==='current'||typeof value==='string'&&value.length<=160&&!value.includes('..')&&/^balls\/ball_[a-z0-9_.-]+\.iff$/i.test(value);
function savedState(selected){return {phase:'saved',message:selected==='current'?'Current installed ball will be used.':'Saved for the next Play session. Choose a ball at Blacktop player selection and wait for Ready before starting.'};}
class BallLiveSelector{
 constructor({directory,helperPath,publish=()=>{},invoke,spawnProcess=spawn,exists=fs.existsSync,now=Date.now,waitExit=waitForExit}){
  Object.assign(this,{directory,helperPath,publish,spawnProcess,exists,now,waitExit});this.selected='current';this.active=false;this.running=false;this.child=null;this.stopping=false;this.blocked=false;
  this.journal=path.join(directory,'ball-asset-session.json');this.state=savedState(this.selected);
  // Legacy recovery only. Never reactivate the rejected bridges as a fallback.
  this.invoke=invoke||(async(action,root)=>{
   if(action!=='ball-recover')throw Error('Only legacy ball recovery is allowed.');
   let result;try{result=await exec(this.executable(),[action,root,path.join(directory,'ball-live-session.json'),'0','0','0','current'],{windowsHide:true,timeout:8000,maxBuffer:16384});}
   catch(error){if(!error.stdout)throw Error('Ball recovery could not complete. Close the game before retrying.');result=error;}
   const value=JSON.parse(result.stdout);
   if(!value||typeof value.phase!=='string'||typeof value.message!=='string')throw Error('Invalid ball recovery response.');return value;
  });
 }
 executable(){return typeof this.helperPath==='function'?this.helperPath():this.helperPath;}
 view(){return {...this.state,active:this.active,selected:this.selected};}
 emit(value){this.state={...value};this.publish(this.view());return this.view();}
 async recover(root){
  let result;try{result=await this.invoke('ball-recover',root);}catch(error){result={phase:'error',message:error.message};}
  if(!['restored','ended','no-game'].includes(result.phase)){this.blocked=true;return this.emit(result);}
  if(this.exists(this.journal))return this.start(root,'recover');return result;
 }
 start(root,mode='session'){
  if(this.child)throw Error('Ball restoration is still running. Close the game before starting another session.');
  this.stopping=false;this.running=true;this.blocked=false;
  this.emit({phase:mode==='recover'?'restoring':'pending',message:mode==='recover'?'Checking the previous ball substitution…':'Waiting for the new Play session.'});
  let child;
  try{child=this.spawnProcess(this.executable(),['ball-assets',root,this.journal,mode==='recover'?'0':String(fileTime(this.now())),mode==='recover'?'current':this.selected,mode],{windowsHide:true,stdio:['pipe','pipe','pipe']});}
  catch(error){this.running=false;this.blocked=true;return this.emit({phase:'error',message:`Ball helper could not start: ${error.message}`});}
  this.child=child;let buffer='',terminal=false;
  const fail=message=>{if(this.child!==child)return;this.blocked=true;this.emit({phase:'error',message});};
  child.stdout.on('data',data=>{
   if(this.child!==child)return;buffer+=data.toString();
   if(buffer.length>65536){buffer='';fail('Invalid ball helper response. Close the game before retrying.');child.stdin.end();return;}
   let at;while((at=buffer.indexOf('\n'))>=0){const line=buffer.slice(0,at);buffer=buffer.slice(at+1);
    try{const value=JSON.parse(line);if(!phases.has(value.phase)||typeof value.message!=='string'||!validChoice(value.selected))throw Error('Invalid response');
     terminal=value.phase==='ended'||(mode==='recover'||this.stopping)&&value.phase==='restored';
     // Old-choice responses cannot turn a newly selected ball green.
     if(value.phase==='error')fail(value.message);
     else if(value.phase==='unavailable'&&!this.blocked&&value.selected===this.selected)this.emit({phase:'unavailable',message:value.message,applied:null});
     else if(!this.blocked&&(value.selected===this.selected||mode==='recover'||value.phase==='ended'))this.emit({phase:value.phase,message:value.message,applied:value.applied||null});
    }catch{fail('Invalid ball helper response. Close the game before retrying.');child.stdin.end();}
   }
  });
  child.stderr.on('data',()=>{});
  child.stdin.on('error',()=>fail('Ball helper disconnected. Close the game before retrying.'));
  child.on('error',error=>{fail(`Ball helper could not start${error?.code?` (${error.code})`:''}. No new ball selection can be applied.`);if(this.child===child){this.child=null;this.running=false;}});
  child.on('exit',code=>{
   if(this.child!==child)return;
   if(!terminal&&!this.blocked)fail(`Ball observer stopped unexpectedly${Number.isInteger(code)?` (exit code ${code})`:''}. Close the game before retrying.`);
   this.child=null;this.running=false;
  });
  return this.view();
 }
 async arm(root,logicalPath='current',active=false){
  if(!validChoice(logicalPath))throw Error('Invalid ball selection.');
  await this.stop();if(this.child)throw Error('Ball restoration is still running. Close the game before starting another session.');
  this.gameRoot=root;this.selected=logicalPath;this.active=active;return this.start(root);
 }
 set(logicalPath,active){
  if(!validChoice(logicalPath))throw Error('Invalid ball selection.');
  const changed=logicalPath!==this.selected;this.selected=logicalPath;this.active=active===true;
  if(this.blocked)return this.view();
  if(!this.child||this.stopping)return this.emit(savedState(this.selected));
  if(changed){this.emit({phase:'pending',message:'Choice saved. It will prepare at Blacktop player selection once the previous ball is unloaded.'});this.child.stdin.write(logicalPath+'\n');}
  // Selection observations are advisory, not permission to restore on loading.
  return this.view();
 }
 async tick(){return this.view();}
 async stop(){
  this.active=false;const child=this.child;if(!child)return this.view();
  this.stopping=true;this.running=false;
  try{child.stdin.end();}catch{}
  // Never force-kill the owner of an in-use substitution on a stop timeout.
  if(!await this.waitExit(child,1200)){
   child.unref?.();child.stdout?.unref?.();child.stderr?.unref?.();
   if(!this.blocked)this.emit({phase:'restoring',message:'Ball cleanup continues until a safe main menu or game exit.'});
  }
  return this.view();
 }
}
module.exports={BallLiveSelector,savedState,validChoice};
