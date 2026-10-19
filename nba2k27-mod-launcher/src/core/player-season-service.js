'use strict';
const {execFile}=require('node:child_process');
const {promisify}=require('node:util');
const {decode,summary,plan,sharedWarning}=require('./player-season-stats');
const run=promisify(execFile);
const awards=require('../renderer/player-season-awards');
class PlayerSeasonService {
 constructor(helperPath,{backupRoot=null}={}){this.helperPath=helperPath;this.backupRoot=backupRoot;this.cache=new Map();this.busy=false;this.revision=0;}
 identity(player){return {recordIndex:player.recordIndex,firstName:player.firstName,lastName:player.lastName,cyberfaceId:player.cyberfaceId};}
 async invoke(request){const {stdout}=await run(this.helperPath,['--season-stats',Buffer.from(JSON.stringify(request)).toString('base64')],{windowsHide:true,timeout:30000,maxBuffer:1024*1024});const result=JSON.parse(stdout);if(!result.ok)throw Error(result.error||'Season stats could not be read.');return result;}
 async read(player){if(this.busy)throw Error('Wait for the season save to finish.');const revision=++this.revision,result=await this.invoke(this.identity(player));if(revision!==this.revision)throw Error('A newer season request replaced this one.');this.cache.clear();return result.rows.map(row=>{const totals=decode(Buffer.from(row.raw,'hex'));this.cache.set(row.token,{...row,identity:this.identity(player)});return {slot:row.slot,year:row.year,team:row.team,...(Object.hasOwn(row,'secondaryTeam')?{secondaryTeam:row.secondaryTeam}:{}),token:row.token,totals,awards:awards.decode(Buffer.from(row.raw,'hex')),averages:summary(totals),sharedWarning:sharedWarning(row.linkedPlayers)};});}
 async save(player,request,confirm){
  if(this.busy)throw Error('A season save is already in progress.');this.busy=true;
  try{
   const row=this.cache.get(request.token);if(!row||JSON.stringify(row.identity)!==JSON.stringify(this.identity(player)))throw Error('Refresh season stats before saving.');
   if(!request.changes||typeof request.changes!=='object'||Array.isArray(request.changes))throw Error('Invalid season changes.');
   const awardChanges={},statChanges={};for(const [key,value] of Object.entries(request.changes)){if(key.startsWith('award:'))awardChanges[key.slice(6)]=value;else statChanges[key]=value;}
   const result=plan(Buffer.from(row.raw,'hex'),statChanges,{linkedPlayers:row.linkedPlayers,confirmedShared:true});
   result.after=Buffer.from(awards.patch(result.after,awardChanges));if(result.after.equals(Buffer.from(row.raw,'hex')))return {saved:false};
   const warnings=[result.sharedWarning,...result.warnings].filter(Boolean);
   if(warnings.length&&!await confirm(warnings.join('\n\n')))return {canceled:true};
   const saved=await this.invoke({...row.identity,slot:row.slot,token:row.token,after:result.after.toString('hex'),confirmedShared:true,confirmedScoring:true});this.cache.clear();return saved;
  }finally{this.busy=false;}
 }
 async structure(player,request,confirm){
  if(this.busy)throw Error('A season save is already in progress.');
  if(!['add-older','remove-oldest'].includes(request.action))throw Error('Invalid season action.');
  this.busy=true;
  try{
   const row=this.cache.get(request.token);
   if(!row||JSON.stringify(row.identity)!==JSON.stringify(this.identity(player)))throw Error('Refresh season stats before continuing.');
   const rows=[...this.cache.values()];if(rows.some(other=>other.slot>row.slot))throw Error('Only the oldest season is supported.');
   const adding=request.action==='add-older';
   if(adding&&(row.slot>=24||row.year<=1900))throw Error('No older season slot is available.');
   if(!adding&&row.linkedPlayers.length!==1)throw Error('Shared-season removal is not supported yet.');
   const year=adding?row.year-1:row.year;
   const message=adding?`Add ${year-1}–${String(year).slice(-2)} with zero totals and the same team (${row.team}) as the oldest season? You can then edit its totals.`:`Remove ${year-1}–${String(year).slice(-2)} and its stats and awards from this player?`;
   if(!await confirm(message+'\n\nStay at the main menu or roster editor, outside gameplay. Use a backup roster. The game will pause briefly while this change is verified.'))return {canceled:true};
   const result=await this.invoke({...row.identity,slot:row.slot,token:row.token,structure:request.action,confirmedStructure:true,backupRoot:this.backupRoot});
   this.revision++;this.cache.clear();return result;
  }finally{this.busy=false;}
 }
}
module.exports={PlayerSeasonService};
