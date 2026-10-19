'use strict';
const {execFile}=require('node:child_process');
const {optionsFor}=require('../shared/game-rule-values');

function execute(file,args){
 return new Promise((resolve,reject)=>execFile(file,args,{windowsHide:true,timeout:8000,maxBuffer:65536},(error,stdout)=>{
  if(error&&!stdout)return reject(error);
  try{resolve(JSON.parse(stdout));}catch{reject(Error('Game Rules helper returned an invalid response.'));}
 }));
}
class DirectGameRules {
 constructor({helperPath,settings,run=execute}){Object.assign(this,{helperPath,settings,run});this.pending=false;}
 async invoke(args){
  if(this.pending)throw Error('Another Game Rules operation is running.');
  this.pending=true;
  try{
   const response=await this.run(this.helperPath,args);
   if(!response||typeof response.phase!=='string')throw Error('Invalid Game Rules response.');
   if(response.phase!=='ready')throw Error(response.message||'Game Rules are unavailable.');
   if(!Array.isArray(response.values)||response.values.length!==39)throw Error('Incomplete Game Rules snapshot.');
   return response;
  }finally{this.pending=false;}
 }
 async read(){const {gameRoot}=await this.settings();return this.invoke(['rules-read',gameRoot]);}
 async set(snapshot,field,target){
  if(!snapshot||!['pid','created','basis','owner'].every(k=>/^\d+$/.test(String(snapshot[k])))||typeof field!=='string'||!Number.isInteger(target))throw Error('Invalid Game Rules selection.');
  const row=snapshot.values?.find(row=>row.field===field);
  const allowed=optionsFor(field,row?.width).map(option=>option.value);
  if(!row||!allowed.includes(row.value)||!allowed.includes(target)||row.value===target)throw Error('This rule value is unavailable or unchanged.');
  if(snapshot.values.length!==39||snapshot.values.some(value=>!Number.isSafeInteger(value.value)||value.value<0||value.value>0xffffffff))throw Error('Invalid Game Rules snapshot.');
  const {gameRoot}=await this.settings();
  return this.invoke(['rules-write',gameRoot,String(snapshot.pid),String(snapshot.created),String(snapshot.basis),String(snapshot.owner),field,String(row.value),String(target),snapshot.values.map(value=>value.value).join(',')]);
 }
}
module.exports={DirectGameRules};
