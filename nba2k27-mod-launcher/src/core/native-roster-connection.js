'use strict';
const path=require('node:path'),fs=require('node:fs'),crypto=require('node:crypto');
const {promisify}=require('node:util'),exec=promisify(require('node:child_process').execFile);
const EXPECTED='0ca3734d217d62197a2a88ea7485fc8f761c8b6862d3bce44616f5bf75e97c89';
async function verify(){
 const system=path.join(process.env.SystemRoot||'C:/Windows','System32');
 const {stdout}=await exec(path.join(system,'WindowsPowerShell/v1.0/powershell.exe'),['-NoProfile','-NonInteractive','-Command',
  '@(Get-Process NBA2K27 -ErrorAction SilentlyContinue | Select-Object Id,Path,StartTime) | ConvertTo-Json -Compress'],{windowsHide:true,timeout:10000});
 let rows;try{const parsed=JSON.parse(stdout);rows=Array.isArray(parsed)?parsed:[parsed];}catch{rows=[];}
 if(rows.length!==1||!Number.isInteger(rows[0].Id)||!rows[0].StartTime||typeof rows[0].Path!=='string'||path.basename(rows[0].Path).toLowerCase()!=='nba2k27.exe')
  throw Error('Open one NBA 2K27 game at the main menu before exporting a roster.');
 const game=rows[0];
 const {stdout:ports}=await exec(path.join(system,'netstat.exe'),['-ano'],{windowsHide:true,timeout:10000});
 if(!new RegExp(`TCP\\s+0\\.0\\.0\\.0:7928\\s+0\\.0\\.0\\.0:0\\s+LISTENING\\s+${game.Id}(?:\\s|$)`).test(ports))
  throw Error('The game’s native roster service is not ready. Stay at the main menu.');
 const digest=crypto.createHash('sha256');for await(const chunk of fs.createReadStream(game.Path))digest.update(chunk);
 if(digest.digest('hex')!==EXPECTED)throw Error('Native roster export has not been verified for this game build.');
 return {pid:game.Id,startTime:game.StartTime,gameBuildSha256:EXPECTED};
}
module.exports={verify};
