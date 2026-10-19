'use strict';
const fs=require('node:fs/promises'),path=require('node:path');
const {redactText}=require('./support-bundle');
const labels={'session:status':'Game session','loader:status':'Session loader','camera:status':'Cameras','scaler:status':'Scaler','arena:status':'Arena','ball:status':'Ball','scoreboard:status':'Scoreboard','blacktop:carry-status':'Outfit carry-over','blacktop:crowd-status':'Crowd removal','blacktop:presentation-status':'Drone / DJ / MC','unlocks:freestyle-status':'Freestyle unlock','unlocks:blacktop-status':'Blacktop teams','unlocks:debug-status':'Debug menus'};
const metricNames=new Set(['startup','shutdown','catalog.ball','catalog.scoreboard','catalog.arena','catalog.outfit','catalog.rules']);
async function replaceFile(from,to){for(let attempt=0;;attempt++){try{return await fs.rename(from,to);}catch(error){if(!['EPERM','EACCES','EBUSY'].includes(error.code)||attempt===5)throw error;await new Promise(resolve=>setTimeout(resolve,25*(attempt+1)));}}}
const median=values=>{const sorted=[...values].sort((a,b)=>a-b);return sorted.length?sorted[Math.floor(sorted.length/2)]:null;};
function summarize(samples,version,lane){return [...metricNames].map(name=>{
 const valid=samples.filter(r=>r.name===name&&r.lane===lane),current=valid.filter(r=>r.version===version).slice(-20),priorVersion=[...valid].reverse().find(r=>r.version!==version)?.version;
 const previous=valid.filter(r=>r.version===priorVersion).slice(-20),ms=median(current.map(r=>r.ms)),baseline=median(previous.map(r=>r.ms));
 return {name,count:current.length,lastMs:current.at(-1)?.ms??null,medianMs:ms,maxMs:current.length?Math.max(...current.map(r=>r.ms)):null,baselineVersion:priorVersion||null,baselineMs:baseline,regression:current.length>=3&&previous.length>=3&&ms>baseline*1.3&&ms-baseline>100};
});}
class LauncherHealth{
 constructor({directory,version,lane='normal',now=Date.now}){Object.assign(this,{directory,version,lane,now});this.states=new Map();this.samples=[];this.tail=Promise.resolve();this.file=path.join(directory,'launcher-performance.json');this.loaded=this.load();}
 async load(){try{const stat=await fs.stat(this.file);if(stat.size>256*1024)return;const value=JSON.parse(await fs.readFile(this.file,'utf8'));if(value.schema===1&&Array.isArray(value.samples))this.samples=value.samples.filter(r=>metricNames.has(r.name)&&Number.isFinite(r.ms)&&r.ms>=0&&r.ms<=600000&&typeof r.version==='string').slice(-300);}catch{}}
 observe(channel,value){if(!labels[channel])return;const phase=String(value?.phase||value?.status||'unknown').slice(0,60),message=redactText(String(value?.message||value?.error||'').slice(0,1000));
  const status=/error|failed|unsupported|attention|blocked/.test(phase)?'Failed':/^(off|inactive|disabled|idle|closed|stopped|restored|configured|saved)$/.test(phase)?'Inactive':/pending|wait|prepar|arm|start|load|restor|monitor/.test(phase)?'Pending':/active|running|applied|ready/.test(phase)?'Active':'Unknown';
  this.states.set(channel,{name:labels[channel],phase,status,message,updatedAt:new Date(this.now()).toISOString()});
 }
 record(name,ms){if(!metricNames.has(name)||!Number.isFinite(ms)||ms<0||ms>600000)return Promise.resolve(false);
  const operation=this.tail.then(async()=>{await this.loaded;this.samples.push({name,ms:Math.round(ms),version:this.version,lane:this.lane,at:new Date(this.now()).toISOString()});this.samples=this.samples.slice(-300);await fs.mkdir(this.directory,{recursive:true});const temp=this.file+'.'+process.pid+'.tmp';try{await fs.writeFile(temp,JSON.stringify({schema:1,samples:this.samples}));await replaceFile(temp,this.file);}finally{await fs.rm(temp,{force:true}).catch(()=>{});}return true;});this.tail=operation.catch(()=>{});return operation;
 }
 async view(){await this.loaded;return {version:this.version,helpers:Object.entries(labels).map(([key,name])=>this.states.get(key)||{name,status:'Unknown',phase:'not-observed',message:'No status received this launcher session.',updatedAt:null}),performance:summarize(this.samples,this.version,this.lane)};}
}
module.exports={LauncherHealth,summarize,labels,metricNames};
