'use strict';
const {spawn}=require('node:child_process');
function watchController({helper,onSkip,log=()=>{},spawnProcess=spawn}){
  const child=spawnProcess(helper,['--watch',String(process.pid)],{windowsHide:true,stdio:['pipe','pipe','pipe']});
  let stopped=false,pending='',exited=false,complete;
  const done=new Promise(resolve=>complete=resolve);
  child.stdin.on('error',()=>{});
  child.on('error',error=>{log({event:'controller-error',message:error.message});if(!child.pid){exited=true;complete();}});
  child.once('exit',code=>{exited=true;log({event:'controller-exit',code});complete();});
  child.stderr.on('data',bytes=>log({event:'controller-error',message:bytes.toString().slice(0,512)}));
  child.stdout.on('data',bytes=>{
    pending+=bytes.toString();if(pending.length>8192){pending='';return;}
    let end;while((end=pending.indexOf('\n'))>=0){const line=pending.slice(0,end);pending=pending.slice(end+1);
      try{const row=JSON.parse(line);if(!['controller','skip'].includes(row.event))continue;log(row);if(row.event==='skip'&&!stopped){stopped=true;void onSkip();}}catch{}
    }
  });
  return {stop:async()=>{
    stopped=true;if(exited)return;
    child.stdin.end();let timer;
    try{await Promise.race([done,new Promise(resolve=>{timer=setTimeout(resolve,600);})]);}finally{clearTimeout(timer);}
    if(!exited)child.kill();
  }};
}
module.exports={watchController};
