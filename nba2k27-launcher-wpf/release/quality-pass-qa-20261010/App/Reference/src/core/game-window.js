'use strict';
const {spawn}=require('node:child_process');
function waitForGameWindow({helper,gamePath,signal,spawnProcess=spawn,timeoutMs=121000}){
 return new Promise(resolve=>{
  if(signal?.aborted){resolve(false);return;}
  let child;try{child=spawnProcess(helper,['--wait',String(process.pid),gamePath],{windowsHide:true,stdio:['pipe','pipe','pipe']});}catch{resolve(false);return;}
  let settled=false,ready=false,pending='',timer,killTimer;
  const finish=value=>{if(settled)return;settled=true;clearTimeout(timer);signal?.removeEventListener('abort',cancel);resolve(value);};
  const cancel=()=>{if(settled)return;child.stdin.end();killTimer=setTimeout(()=>child.kill(),800);killTimer.unref();finish(false);};
  child.stdin.on('error',()=>{});child.stderr.resume();
  child.once('error',()=>finish(false));child.once('exit',code=>{clearTimeout(killTimer);finish(code===0&&ready&&!signal?.aborted);});
  child.stdout.on('data',bytes=>{pending+=bytes.toString();if(pending.length>8192){cancel();return;}let end;
   while((end=pending.indexOf('\n'))>=0){const line=pending.slice(0,end);pending=pending.slice(end+1);try{const row=JSON.parse(line);if(row.event==='window-ready'&&Number.isInteger(row.pid)&&row.pid>0)ready=true;}catch{}}
  });
  signal?.addEventListener('abort',cancel,{once:true});timer=setTimeout(cancel,timeoutMs);timer.unref();if(signal?.aborted)cancel();
 });
}
module.exports={waitForGameWindow};
