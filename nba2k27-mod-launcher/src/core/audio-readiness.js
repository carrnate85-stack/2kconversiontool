const path=require('node:path');
const {execFile}=require('node:child_process');
const {promisify}=require('node:util');
const exec=promisify(execFile);
async function waitAudioReadiness(helper,gameRoot,armedReport,execute=exec,options={}){
  if(typeof armedReport!=='string'||!path.isAbsolute(armedReport))throw new Error('Audio readiness requires the active session report path.');
  const stableMs=options.stableMs;
  if(stableMs!==undefined&&(!Number.isInteger(stableMs)||stableMs<50||stableMs>2000))throw new Error('Audio readiness stability interval is invalid.');
  const args=[path.join(gameRoot,'NBA2K27.exe'),'--armed-report',armedReport,...(stableMs===undefined?[]:['--stable-ms',String(stableMs)])];
  const {stdout}=await execute(helper,args,{windowsHide:true,timeout:65000,maxBuffer:32768});
  const report=JSON.parse(stdout.trim());
  if(report.ready!==true||report.memoryWrites!==0||report.gameCalls!==0||!Number.isInteger(report.pid))throw new Error('Audio startup readiness did not pass.');
  return report;
}
module.exports={waitAudioReadiness};
