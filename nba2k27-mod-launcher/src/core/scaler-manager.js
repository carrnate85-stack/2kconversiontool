'use strict';
const fs=require('node:fs/promises'),path=require('node:path');
const exec=require('node:util').promisify(require('node:child_process').execFile);
const phases=new Set(['no-game','new-session','ended','waiting','unsupported','conflict','error','write-failed','applied','reload','reset']);
const categoryKeys=['coaches','cheerleaders','arenaPersonnel'];
function preference(value){
 if(!value||Object.keys(value).some(k=>!['enabled','percent',...categoryKeys].includes(k))||typeof value.enabled!=='boolean'||!Number.isInteger(value.percent)||value.percent<95||value.percent>100||categoryKeys.some(k=>value[k]!==undefined&&typeof value[k]!=='boolean'))throw Error('Choose a whole percentage from 95 to 100 and valid character groups.');
 return {enabled:value.enabled,percent:value.percent,coaches:value.coaches??true,cheerleaders:value.cheerleaders??true,arenaPersonnel:value.arenaPersonnel??true};
}
function categoryMask(value){return (value.coaches?1:0)|(value.cheerleaders?2:0)|(value.arenaPersonnel?4:0);}
class ScalerManager {
 constructor({directory,helperPath,publish=()=>{},invoke,now=Date.now}){
  Object.assign(this,{directory,helperPath,publish,now});this.preferences=preference({enabled:false,percent:100});
  this.state={phase:'disabled',message:'Scaling is off. Arena characters use their original size.',...this.preferences};this.queue=Promise.resolve();this.running=false;this.applying=false;this.deadline=0;this.gameRoot=null;this.timer=null;
  this.observationErrors=0;this.maxObservationErrors=3;
  this.invoke=invoke|| (async (action,root,percent,categories)=>{
   const args=[action,root,path.join(directory,'session.json'),String(percent),String(categories)];let output;
   try{output=await exec(helperPath,args,{windowsHide:true,timeout:8000,maxBuffer:16384});}catch(e){if(!e.stdout)throw Error('Scaler helper could not complete. Check that the game and launcher use the same permission level.');output=e;}
   return JSON.parse(output.stdout);
  });
 }
 view(){return {...this.state,...this.preferences};}
 emit(value){this.state={...this.state,...value};this.publish(this.view());return this.view();}
 serial(fn){const job=this.queue.then(fn);this.queue=job.catch(()=>{});return job;}
 async load(){
  try{this.preferences=preference(JSON.parse(await fs.readFile(path.join(this.directory,'preferences.json'),'utf8')));if(this.preferences.enabled)this.emit({phase:'saved',message:'Saved — applies on your next launch.'});}
  catch(e){if(e.code!=='ENOENT'){this.preferences=preference({enabled:false,percent:100});this.emit({phase:'error',message:'Scaler preferences could not be read. Automatic scaling is off; save your preference to continue.'});}}
  return this.view();
 }
 async result(action){
  try{const r=await this.invoke(action,this.gameRoot,this.preferences.percent,categoryMask(this.preferences));if(!r||!phases.has(r.phase)||typeof r.message!=='string')throw Error('Invalid scaler response.');return r;}
  catch(e){return {phase:'error',message:e.message};}
 }
 async recover(root){
  this.gameRoot=root;if(!root)return this.view();
  // No game writes on launcher startup. Only resume observing an owned session.
  const r=await this.result('observe');
  if(['applied','reload','reset'].includes(r.phase)){this.running=true;this.applying=false;this.emit(r);this.schedule();}
  else if(!['no-game','new-session','ended'].includes(r.phase))this.emit(r);
  return this.view();
 }
 arm(root){
  this.stop();this.observationErrors=0;this.gameRoot=root;if(!this.preferences.enabled)return this.emit({phase:'disabled',message:'Automatic character scaling is off.'});
  this.authorized=true;this.restoring=false;this.running=true;this.applying=true;this.deadline=this.now()+120000;
  this.emit({phase:'waiting',message:'Saved — waiting for game initialization.'});this.schedule(0);
 }
 schedule(ms=this.applying?500:3000){clearTimeout(this.timer);if(this.running){this.timer=setTimeout(()=>this.tick(),ms);this.timer.unref?.();}}
 tick(){return this.serial(async()=>{
  if(!this.running)return this.view();
  if(this.applying&&this.now()>this.deadline){this.running=false;return this.emit({phase:'timeout',message:'Player data was not ready in time. Retry apply is available here.'});}
  const observing=!this.applying,r=await this.result(this.applying?(this.restoring?'restore':'apply'):'observe');
  if(observing&&r.phase==='error'){
   this.observationErrors++;
   if(this.observationErrors<this.maxObservationErrors){this.emit({phase:'waiting',message:'Game is closing; verifying scaler cleanup.'});this.schedule(1000);return this.view();}
  }else this.observationErrors=0;
  if(['applied','reload'].includes(r.phase))this.applying=false;
  else if(['no-game','new-session','ended'].includes(r.phase)){if(!this.applying||this.restoring){this.authorized=false;this.running=false;r.phase='saved';r.message='Game session ended. Your size is saved for the next launch.';}}
  else if(r.phase!=='waiting'&&r.phase!=='reset')this.running=false;
  this.emit(r);this.schedule();return this.view();
 });}
 save(value){return this.serial(async()=>{
  const next=preference(value);await fs.mkdir(this.directory,{recursive:true});
  const file=path.join(this.directory,'preferences.json'),temp=file+'.tmp';await fs.writeFile(temp,JSON.stringify(next));await fs.rename(temp,file);this.preferences=next;
  if(!this.gameRoot)return this.emit(next.enabled?{phase:'saved',message:'Saved — applies on your next launch.'}:{phase:'disabled',message:'Scaling is off. Arena characters use their original size.'});
  if(!next.enabled){
   this.stop();const r=await this.result('restore');
   if(r.phase==='waiting'){this.restoring=true;this.running=true;this.applying=true;this.deadline=this.now()+120000;this.schedule();}
   return this.emit(['no-game','ended'].includes(r.phase)?{phase:'disabled',message:'Automatic character scaling is off.'}:r);
  }
  if(this.running){this.restoring=false;this.applying=true;this.deadline=this.now()+120000;this.emit({phase:'waiting',message:'Saved — waiting to apply player size.'});this.schedule(0);return this.view();}
  return this.emit({phase:'saved',message:'Saved — applies on your next launch.'});
 });}
 applySaved(){return this.serial(async()=>{
  if(!this.preferences.enabled)return this.emit({phase:'disabled',message:'Enable automatic character scaling first.'});
  if(!this.gameRoot)return this.emit({phase:'saved',message:'Launch NBA 2K27 through Play first.'});
  // Explicit retry may update only an existing owned session or one armed by Play.
  const r=await this.result('observe');
  if(['new-session','no-game','ended'].includes(r.phase)&&!this.authorized)return this.emit({phase:'saved',message:'Launch NBA 2K27 through Play to start a scaling session.'});
  if(['unsupported','conflict','error','write-failed'].includes(r.phase))return this.emit(r);
  this.restoring=false;this.running=true;this.applying=true;this.deadline=this.now()+120000;this.schedule(0);return this.emit({phase:'waiting',message:'Waiting to apply the saved player size.'});
 });}
 stop(){this.running=false;clearTimeout(this.timer);this.timer=null;}
}
module.exports={ScalerManager,preference,categoryMask};
