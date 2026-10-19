const fsp=require('node:fs/promises');
const path=require('node:path');
const {spawn}=require('node:child_process');
const crypto=require('node:crypto');

const MOVIE_LOGICAL_PATH='movies/era_intro_current.mov';
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));

function helperOutcome(receipt,code,signal){
  if(!receipt||receipt.schema!=='startup-presentation-v1'||!Number.isInteger(receipt.pid)||receipt.pid<0||
    typeof receipt.armed!=='boolean'||typeof receipt.restored!=='boolean'){
    return {ok:false,message:'Startup video helper exited without a valid restoration receipt. Close NBA 2K27 before retrying.'};
  }
  if(code===0&&!signal&&receipt.status==='restored'&&receipt.pid>0&&receipt.armed&&receipt.restored)
    return {ok:true,message:'Startup video trigger restored. Playback and gameplay stability are not verified by this result.'};
  if(code===0&&!signal&&receipt.status==='process-exited'&&receipt.pid>0&&receipt.armed)
    return {ok:true,message:'The startup video target exited; its process memory was released.'};
  return {ok:false,message:`Startup video helper did not complete safely (${receipt.status}; exit ${code ?? signal ?? 'unknown'}). Close NBA 2K27 before retrying.`};
}

class PresentationSession {
  constructor({helperPath,directory,onStatus=()=>{},spawnProcess=spawn,monitorMs=90000,pollMs=250}){
    this.helperPath=helperPath;this.directory=directory;this.onStatus=onStatus;
    this.waitingForMovie=false;this.monitorToken=0;
    this.children=new Set();
    this.spawnProcess=spawnProcess;this.monitorMs=monitorMs;this.pollMs=pollMs;
    this.active=null;this.arming=false;this.lastOutcome=null;
  }

  publish(state){
    // A closing renderer cannot prevent completion/evidence bookkeeping.
    try{this.onStatus(state);}catch(error){console.warn('Startup video status delivery failed:',error.message);}
  }

  async arm({movieEnabled,gameRoot=null,readinessMode=null}){
    if(this.active||this.arming)throw new Error('The previous startup video helper has not finished. Close NBA 2K27 before retrying.');
    this.waitingForMovie=movieEnabled===true;this.lastOutcome=null;this.monitorToken++;
    if(!movieEnabled)return;
    this.arming=true;
    try{
      const stat=await fsp.stat(this.helperPath).catch(()=>null);
      if(!stat?.isFile())throw new Error('The startup movie helper is missing.');
      const sessionId=`${Date.now()}-${crypto.randomUUID()}`;
      const sessionDirectory=path.join(this.directory,sessionId);
      await fsp.mkdir(sessionDirectory,{recursive:true});
      const triggerReceipt=path.join(sessionDirectory,'startup-presentation.json');
      const metadata={schema:'startup-presentation-session-v1',sessionId,startedAt:new Date().toISOString(),gameRoot,readinessMode};
      // Keep every attempt separate: a retry must never overwrite crash evidence.
      await fsp.writeFile(path.join(sessionDirectory,'launch.json'),JSON.stringify(metadata,null,2),{flag:'wx'});
      const child=this.spawnProcess(this.helperPath,['--wait-and-trigger',triggerReceipt],{cwd:path.dirname(this.helperPath),windowsHide:true,stdio:'ignore',detached:true});
      const attempt={child,sessionDirectory,triggerReceipt,metadata,finished:false};
      this.active=attempt;this.children.add(child);
      let complete;attempt.completion=new Promise(resolve=>{complete=resolve;});
      const finish=async(code,signal,error)=>{
        if(attempt.finished)return;
        attempt.finished=true;this.children.delete(child);
        let receipt=null;
        try{const stat=await fsp.lstat(triggerReceipt);if(stat.isFile()&&!stat.isSymbolicLink()&&stat.size<=8192)receipt=JSON.parse(await fsp.readFile(triggerReceipt,'utf8'));}catch{}
        let outcome=helperOutcome(receipt,code,signal);
        if(error)outcome={ok:false,message:`Startup video helper failed: ${error.message}`};
        const result={...metadata,finishedAt:new Date().toISOString(),exitCode:code,signal,receipt,...outcome};
        try{await fsp.writeFile(path.join(sessionDirectory,'result.json'),JSON.stringify(result,null,2),{flag:'wx'});}
        catch(e){outcome={ok:false,message:`Startup video evidence could not be saved: ${e.message}`};}
        this.lastOutcome=outcome;
        if(!outcome.ok){this.waitingForMovie=false;this.monitorToken++;}
        if(this.active===attempt)this.active=null;
        this.publish({phase:outcome.ok?'presentation-restored':'presentation-error',message:outcome.message,receiptPath:triggerReceipt,pid:receipt?.pid??null});
        complete(outcome);
      };
      child.once('exit',(code,signal)=>{void finish(code,signal);});
      child.on('error',error=>{
        // An error after spawn (e.g. failed termination) does not prove exit.
        if(child.pid){this.publish({phase:'presentation-error',message:`Startup video helper process error: ${error.message}. Close NBA 2K27 before retrying.`});return;}
        void finish(null,null,error);
      });
      // Await the spawn event, so a missing executable cannot silently launch the game.
      await new Promise((resolve,reject)=>{child.once('spawn',resolve);child.once('error',reject);});
      child.unref();
      return {sessionId,triggerReceipt};
    }catch(error){this.waitingForMovie=false;throw error;}
    finally{this.arming=false;}
  }

  monitorProvider(provider){
    const token=++this.monitorToken;
    return (async()=>{
      const deadline=Date.now()+this.monitorMs;
      while(token===this.monitorToken&&this.waitingForMovie&&Date.now()<deadline){
        try{
          const status=await provider.request({operation:'status'});
          if(token!==this.monitorToken||!this.waitingForMovie)return;
          if((status.recentActivity||[]).some(row=>row.ok&&String(row.logicalPath||'').toLowerCase()===MOVIE_LOGICAL_PATH&&['binary-read','read'].includes(row.operation))){
            this.waitingForMovie=false;this.publish({phase:'presentation-read',message:'The game requested startup video data; playback is not independently verified.'});return;
          }
        }catch{}
        await delay(this.pollMs);
      }
      if(token===this.monitorToken)this.waitingForMovie=false;
    })();
  }

  async stop({gameRunning=true}={}){
    this.monitorToken++;this.waitingForMovie=false;
    // Never kill a helper while it could still owe restoration to a live game.
    if(gameRunning)return;
    const attempt=this.active;
    for(const child of this.children)if(child.exitCode===null&&!child.signalCode)child.kill();
    if(attempt){
      let timer;
      try{await Promise.race([attempt.completion,new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error('Startup video helper did not exit; a new attempt is blocked.')),2000);})]);}
      finally{clearTimeout(timer);}
    }
  }
}

module.exports={MOVIE_LOGICAL_PATH,PresentationSession,helperOutcome};
