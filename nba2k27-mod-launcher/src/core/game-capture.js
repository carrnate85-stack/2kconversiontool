'use strict';
const fs=require('node:fs/promises'),path=require('node:path'),crypto=require('node:crypto');
const {spawn}=require('node:child_process');
const {stopChild}=require('./child-lifecycle');
const {archiveSessionLogs,plain}=require('./session-evidence');
const sessionName=/^session-\d{13}-[a-f0-9-]{36}$/;
const owned=/^(?:capture\.json|watch\.jsonl|exit\.json|hang-[1-3]\.(?:dmp|partial))$/;
async function retainCaptures(root,keep=3){
 await plain(root);const entries=await fs.readdir(root,{withFileTypes:true}).catch(e=>{if(e.code==='ENOENT')return [];throw e;});
 const sessions=entries.filter(e=>e.isDirectory()&&sessionName.test(e.name)).map(e=>e.name).sort().reverse();
 for(const name of sessions.slice(keep)){
  const folder=path.join(root,name);await plain(folder);
  let marker;try{marker=JSON.parse(await fs.readFile(path.join(folder,'capture.json'),'utf8'));}catch{continue;}
  if(marker.schema!=='2k-game-capture-v1')continue;
  const children=await fs.readdir(folder,{withFileTypes:true});
  if(children.some(c=>!c.isFile()||!owned.test(c.name)))continue;
  for(const child of children)await fs.unlink(path.join(folder,child.name));
  await fs.rmdir(folder);
 }
}
class GameCapture {
 constructor({directory,helperPath,spawnProcess=spawn,onStatus=()=>{}}){
  this.directory=directory;this.helperPath=helperPath;this.spawnProcess=spawnProcess;this.onStatus=onStatus;
  this.child=null;this.pending=null;this.session=null;this.tail=Promise.resolve();this.presence=null;this.state={phase:'idle',message:'Play starts the local game monitor. Ctrl+Shift+F12 captures a stuck loading screen.'};
 }
 status(){return {...this.state};}
 // Separate from UI phases: the native monitor holds the exact game process
 // handle. A capture or monitor failure is not evidence that the game exited.
 gamePresence(){return this.presence;}
 emit(phase,message){this.state={phase,message,directory:this.session};this.onStatus(this.status());}
 async start(gameRoot){
  if(!await this.stop())throw Error('The previous diagnostic monitor is still stopping.');
  this.presence=null;
  const root=path.join(this.directory,'game-captures');await plain(root);await fs.mkdir(root,{recursive:true});await retainCaptures(root,2);
  const folder=path.join(root,`session-${Date.now()}-${crypto.randomUUID()}`);await fs.mkdir(folder);this.session=folder;
  await fs.writeFile(path.join(folder,'capture.json'),JSON.stringify({schema:'2k-game-capture-v1',createdAt:new Date().toISOString(),game:path.join(gameRoot,'NBA2K27.exe'),maxDumps:3,maxDumpBytes:64*1024*1024,privacy:'Local memory snapshots; may contain private data. Not included in support exports. No debugger attachment or automatic upload.'},null,2),{flag:'wx'});
  const child=this.spawnProcess(this.helperPath(),['--watch',path.join(gameRoot,'NBA2K27.exe'),folder],{windowsHide:true,stdio:['pipe','pipe','pipe']});this.child=child;
  let buffer='',written=0,attachedPid=null;
  const record=value=>{const line=JSON.stringify({...value,time:new Date().toISOString()})+'\n';
   this.tail=this.tail.then(async()=>{
    if(written+Buffer.byteLength(line)<=1024*1024){await fs.appendFile(path.join(folder,'watch.jsonl'),line);written+=Buffer.byteLength(line);}
    if(value.event==='game-exit')await fs.writeFile(path.join(folder,'exit.json'),JSON.stringify({...value,exitCodeHex:'0x'+value.value.toString(16).padStart(8,'0'),time:new Date().toISOString()},null,2),{flag:'wx'});
   }).catch(error=>this.emit('error',`Could not save game diagnostics: ${error.message}`));
  };
  child.stdout.on('data',chunk=>{
   buffer+=chunk.toString();if(buffer.length>65536){buffer='';this.emit('error','Diagnostic monitor output exceeded its limit.');return;}
   let newline;while((newline=buffer.indexOf('\n'))>=0){const line=buffer.slice(0,newline);buffer=buffer.slice(newline+1);let value;try{value=JSON.parse(line);}catch{continue;}
    if(this.child!==child)continue;
    if(value.event==='attached-read-only'){
     if(!Number.isInteger(value.pid)||value.pid<=0||attachedPid!==null)continue;
     attachedPid=value.pid;this.presence=true;
    }
    if(value.event==='game-exit'){
     if(attachedPid===null||value.pid!==attachedPid||!Number.isInteger(value.value)||value.value<0||value.value>0xffffffff)continue;
     this.presence=false;
    }
    record(value);
    if(value.event==='attached-read-only')this.emit('watching','Game monitor ready. Press Ctrl+Shift+F12 if loading gets stuck.');
    if(value.event==='game-exit'){
     this.emit('exited',`Game exited (0x${value.value.toString(16).padStart(8,'0')}). Local diagnostics saved.`);
     void archiveSessionLogs({directory:this.directory,gameRoot}).catch(error=>record({event:'archive-error',message:error.message}));
    }
    if(value.event==='game-not-found')this.emit('error','No matching game process appeared within 90 seconds.');
    if(value.event==='dump-complete'||value.event==='auto-dump-complete'){
     const file=path.join(folder,`hang-${value.value}.dmp`);this.emit('captured','Game snapshot saved locally.');
     if(value.event==='dump-complete'&&this.pending){this.pending.resolve({filePath:file});this.pending=null;}
    }
    if(value.event==='dump-error'||value.event==='auto-dump-error'){
     const error=Error(`Game snapshot unavailable (Windows error ${value.value}). Limit: 3 snapshots per Play session.`);this.emit('error',error.message);
     if(value.event==='dump-error'&&this.pending){this.pending.reject(error);this.pending=null;}
    }
   }
  });
  child.stderr.on('data',chunk=>record({event:'monitor-stderr',message:chunk.toString().slice(0,2048)}));
  child.stdin.on('error',()=>{});
  child.on('error',error=>{record({event:'monitor-error',message:error.message});if(this.child!==child)return;if(this.presence!==false)this.presence=null;this.emit('error',error.message);});
  child.once('close',(code)=>{
   record({event:'monitor-exit',code});if(this.child!==child)return;this.child=null;if(this.presence!==false)this.presence=null;
   if(this.pending){this.pending.reject(Error('The game or diagnostic monitor exited before the snapshot completed.'));this.pending=null;}
   if(!['exited','error','idle'].includes(this.state.phase))this.emit('idle','Diagnostic monitor ended; launch through Play to start a new session.');
  });
  this.emit('waiting','Waiting for this Play session’s game process.');
 }
 async capture(){
  if(!this.child||this.child.exitCode!=null)throw Error('Start NBA 2K27 through Play first; a live game is required for a snapshot.');
  if(this.pending)throw Error('A game snapshot is already in progress.');
  const result=new Promise((resolve,reject)=>{const timer=setTimeout(()=>{if(this.pending){this.pending=null;reject(Error('Snapshot is still taking too long; check the local capture folder.'));}},20000);
   this.pending={resolve:value=>{clearTimeout(timer);resolve(value);},reject:error=>{clearTimeout(timer);reject(error);}};});
  this.child.stdin.write('capture\n');this.emit('capturing','Capturing a small game snapshot; the game may pause briefly.');return result;
 }
 async stop(){
  const child=this.child;if(!child)return true;
  const stopped=await stopChild(child,{timeoutMs:17000,graceful:process=>process.stdin.end('stop\n')});
  if(stopped&&this.child===child){this.child=null;if(this.presence!==false)this.presence=null;}await this.tail;return stopped;
 }
}
module.exports={GameCapture,retainCaptures};
