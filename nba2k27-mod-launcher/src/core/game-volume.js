'use strict';
const fs=require('node:fs/promises'),path=require('node:path'),crypto=require('node:crypto');
const {spawn}=require('node:child_process');
const valid=value=>value&&Number.isFinite(value.volume)&&value.volume>=0&&value.volume<=1&&typeof value.muted==='boolean';
const samePath=(a,b)=>typeof a==='string'&&typeof b==='string'&&path.resolve(a).toLowerCase()===path.resolve(b).toLowerCase();
class GameVolume {
  constructor({helper,cacheFile,spawnProcess=spawn,log=()=>{}}){Object.assign(this,{helper,cacheFile,spawnProcess,log});this.active=null;this.saving=Promise.resolve();}
  async start(gamePath,onVolume,{muteGame=false,onFailure=()=>{}}={}){
    await this.stop();
    if(this.restoring)throw Error('Previous intro audio restoration is still pending.');
    const token={stopped:false,child:null};this.active=token;
    let current={volume:0.25,muted:false,source:'fallback'};
    try{const saved=JSON.parse(await fs.readFile(this.cacheFile,'utf8'));if(saved.schema===1&&samePath(saved.gamePath,gamePath)&&valid(saved))current={volume:saved.volume,muted:saved.muted,source:'cached'};}catch{}
    if(token.stopped||this.active!==token)return;
    onVolume(current);
    if(!gamePath)return;
    let child;
    // The restoring helper must survive Windows tearing down the launcher's
    // child-process job. Its own parent/EOF watchdog controls its short lifetime.
    try{child=this.spawnProcess(this.helper,[muteGame?'--duck':'--watch',String(process.pid),gamePath],{windowsHide:true,detached:muteGame,stdio:['pipe','pipe','pipe']});token.child=child;}
    catch(error){this.log({event:'volume-helper-error',message:error.message});if(muteGame)onFailure();return;}
    let pending='',exited=false,complete,first;const done=new Promise(resolve=>complete=resolve),initial=new Promise(resolve=>first=resolve);
    const ended=()=>{if(exited)return;exited=true;complete();first();if(muteGame&&!token.stopped){this.log({event:'game-audio-helper-ended'});onFailure();}};
    child.stdin.on('error',()=>{});child.stderr.on('data',bytes=>this.log({event:'volume-helper-error',message:bytes.toString().slice(0,256)}));
    child.once('error',error=>{this.log({event:'volume-helper-error',message:error.message});if(!child.pid)ended();});child.once('exit',ended);
    child.stdout.on('data',bytes=>{
      pending+=bytes.toString();if(pending.length>8192){pending='';return;}
      let end;while((end=pending.indexOf('\n'))>=0){const line=pending.slice(0,end);pending=pending.slice(end+1);
        let row;try{row=JSON.parse(line);}catch{continue;}
        if(['game-audio-held','game-audio-restored','game-audio-restore-failed'].includes(row.event)){this.log({event:row.event});continue;}
        if(token.stopped||this.active!==token)continue;
        if(row.event!=='volume')continue;
        if(row.found===false){first();continue;}
        if(row.found!==true||!valid(row))continue;
        current={volume:row.volume,muted:row.muted,source:'live'};onVolume(current);first();
        this.log({event:'video-volume',...current});
        const saved={schema:1,gamePath,volume:row.volume,muted:row.muted};
        this.saving=this.saving.then(async()=>{
          await fs.mkdir(path.dirname(this.cacheFile),{recursive:true});const temp=this.cacheFile+'.'+crypto.randomUUID()+'.tmp';
          try{await fs.writeFile(temp,JSON.stringify(saved),{flag:'wx'});await fs.rename(temp,this.cacheFile);}finally{await fs.rm(temp,{force:true});}
        }).catch(error=>this.log({event:'volume-cache-error',message:error.message}));
      }
    });
    token.stop=async()=>{token.stopped=true;if(exited)return;child.stdin.end();let timer;
      try{await Promise.race([done,new Promise(resolve=>{timer=setTimeout(resolve,muteGame?2000:800);})]);}finally{clearTimeout(timer);}
      // Never hard-kill a helper that owns mute restoration. EOF/parent exit
      // leaves its finally block running even if an audio device is slow.
      if(!exited){if(muteGame){this.log({event:'game-audio-restoration-pending'});this.restoring=done.finally(()=>{this.restoring=null;});}else child.kill();}
    };
    let timer;try{await Promise.race([initial,new Promise(resolve=>{timer=setTimeout(resolve,750);})]);}finally{clearTimeout(timer);}
  }
  async stop(){const token=this.active;this.active=null;if(token){token.stopped=true;await token.stop?.();}await this.saving;}
}
module.exports={GameVolume,valid};
