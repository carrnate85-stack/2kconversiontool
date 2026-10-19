'use strict';
const {spawn}=require('node:child_process');
const {stopChild}=require('./child-lifecycle');
class BlacktopSession {
 constructor({script,choices,diagnosticsDir,onStatus=()=>{},spawnProcess=spawn}){Object.assign(this,{script,choices,diagnosticsDir,onStatus,spawnProcess});this.child=null;this.started=false;this.state={phase:'idle',message:'Saved outfits apply once per Blacktop match.'};}
 emit(phase,message){this.state={phase,message};this.onStatus(this.state);}
 start(gameRoot){
  if(this.started)return;this.started=true;
  let child;
  try{if(!this.diagnosticsDir)throw Error('Outfit diagnostics directory is missing.');child=this.spawnProcess('powershell.exe',['-NoProfile','-ExecutionPolicy','Bypass','-File',this.script,'-GameRoot',gameRoot,'-ChoicesPath',this.choices,'-DiagnosticsDir',this.diagnosticsDir],{windowsHide:true,stdio:['ignore','pipe','pipe']});}
  catch(error){this.emit('error',error.message);return;}
  this.child=child;let buffer='';let failure='';
  const messages={waiting:'Waiting for your saved outfits and Blacktop match…',applying:'Loading your saved outfits…',applied:'Outfit data applied. Clothing may take a moment to appear.',stopped:'Outfit session ended.'};
  child.stdout.on('data',data=>{buffer+=data.toString();if(buffer.length>32768)buffer=buffer.slice(-32768);let at;
   while((at=buffer.indexOf('\n'))>=0){const line=buffer.slice(0,at).trim();buffer=buffer.slice(at+1);if(!line.startsWith('OUTFITS:'))continue;
    const value=line.slice(8);if(value.startsWith('error:'))this.emit('error',value.slice(6));else if(value.startsWith('warning:'))this.emit('warning',value.slice(8));else if(messages[value])this.emit(value,messages[value]);}
  });
  child.stderr.on('data',data=>{failure=(failure+data.toString()).slice(-2048);});
  child.on('error',error=>{if(this.child===child)this.emit('error',error.message);});
  child.on('exit',code=>{if(this.child!==child)return;this.child=null;if(code&&this.state.phase!=='error')this.emit('error',failure||'Outfit session stopped before confirmation.');});
 }
 async stop(){const child=this.child;this.child=null;this.started=false;this.emit('stopped','Outfit session ended.');return stopChild(child,{graceful:value=>value.kill()});}
}
module.exports={BlacktopSession};
