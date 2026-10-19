'use strict';
const {execFile}=require('node:child_process'),{promisify}=require('node:util'),crypto=require('node:crypto'),path=require('node:path');
const model=require('./player-season-import-plan'),transfer=require('../renderer/player-season-transfer');
const run=promisify(execFile);
const same=(a,b)=>JSON.stringify(a)===JSON.stringify(b);
function identity(player){
 if(!player||!Number.isInteger(player.recordIndex)||player.recordIndex<0||player.recordIndex>=8192||typeof player.firstName!=='string'||typeof player.lastName!=='string'||player.firstName.length>19||player.lastName.length>19||!Number.isInteger(player.cyberfaceId)||player.cyberfaceId<0||player.cyberfaceId>65535)throw Error('Refresh the destination player before importing seasons.');
 return {recordIndex:player.recordIndex,firstName:player.firstName,lastName:player.lastName,cyberfaceId:player.cyberfaceId};
}
function hex(value,minimum,maximum,multiple){
 if(typeof value!=='string'||value.length<minimum*2||value.length>maximum*2||value.length%(multiple*2)||!/^[0-9a-f]+$/i.test(value))throw Error('Invalid native season import snapshot.');
 return Buffer.from(value,'hex');
}
function decodeSnapshot(result,player){
 if(!result||result.ok!==true||result.writes!==0||result.recordIndex!==player.recordIndex||typeof result.snapshotId!=='string'||!/^[0-9a-f]{64}$/.test(result.snapshotId)||!Array.isArray(result.teams))throw Error('The native season snapshot was not verified read-only.');
 const snapshot={recordIndex:result.recordIndex,players:hex(result.players,model.PLAYER_STRIDE,8192*model.PLAYER_STRIDE,model.PLAYER_STRIDE),stats:hex(result.stats,64,32767*64,64),pool:hex(result.pool,64,64,64),teams:result.teams.map(t=>({index:t?.index,name:t?.name,pointer:t?.pointer,...(t?.displayName!==undefined?{displayName:t.displayName}:{})}))};
 model.snapshot(snapshot);return snapshot;
}
class PlayerSeasonImportService {
 constructor(helperPath,{backupRoot=null}={}){this.helperPath=helperPath;this.backupRoot=backupRoot;this.pending=null;this.busy=false;this.revision=0;}
 clear(){this.revision++;this.pending=null;}
 async teams(player){
  if(this.busy)throw Error('Wait for the season import operation to finish.');
  this.busy=true;const revision=this.revision;
  try{const destination=identity(player),value=decodeSnapshot(await this.snapshot(destination),destination);if(revision!==this.revision)throw Error('The destination player changed while reading teams.');
   const teams=value.teams.filter(team=>team.displayName?.endsWith(' · Normal')).map(({index,name,displayName})=>({index,name,displayName}));if(!teams.length)throw Error('No verified normal destination teams are available. Refresh the roster.');return {teams};
  }finally{this.busy=false;}
 }
 async snapshot(player){
  const {stdout}=await run(this.helperPath,['--season-import-snapshot',Buffer.from(JSON.stringify(player)).toString('base64')],{windowsHide:true,timeout:30000,maxBuffer:32*1024*1024});return JSON.parse(stdout);
 }
 async saveNative(request){
  const payload=JSON.stringify(request);if(Buffer.byteLength(payload,'utf8')>65536)throw Error('Season import request is too large.');
  return new Promise((resolve,reject)=>{
   const child=execFile(this.helperPath,['--season-import'],{windowsHide:true,timeout:30000,maxBuffer:1024*1024},(error,stdout,stderr)=>{
    if(error)return reject(Error(String(stderr||'Season import could not be verified. Refresh the review before retrying.').trim()));
    try{resolve(JSON.parse(stdout));}catch{reject(Error('Native season import returned an invalid verification report.'));}
   });
   // EPIPE may accompany an early native rejection; the completed process
   // supplies its bounded diagnostic. Do not terminate a paused transaction.
   child.stdin.on('error',()=>{});child.stdin.end(payload);
  });
 }
 async review(player,request){
  if(this.busy)throw Error('Wait for the season import review to finish.');
  this.busy=true;const revision=this.revision;
  try{
   if(this.pending)this.pending.ready=false;
   const destination=identity(player),source=transfer.validate(request?.seasons),normalTeamsOnly=request?.normalTeamsOnly===true;if(!source.length)throw Error('Choose at least one season to import.');
   const preserveTotals=request?.preserveTotals??{};
   let pending=this.pending;
   if(request?.reviewId!==undefined){
    if(!pending||pending.reviewId!==request.reviewId||!same(destination,pending.identity)||!same(source,pending.source)||normalTeamsOnly!==pending.normalTeamsOnly||!same(preserveTotals,pending.preserveTotals))throw Error('The destination or selected seasons changed. Start a fresh import review.');
   }else{
    this.pending=null;const native=await this.snapshot(destination);if(revision!==this.revision)throw Error('A newer player selection replaced this import review.');
    const snapshot=decodeSnapshot(native,destination);pending={reviewId:crypto.randomUUID(),nativeToken:native.snapshotId,snapshot,source,identity:destination,normalTeamsOnly,preserveTotals:structuredClone(preserveTotals),ready:false};this.pending=pending;
   }
   pending.ready=false;const mappings=request?.teamMappings??{};let plan;
   try{plan=model.plan(pending.snapshot,source,{teamMappings:mappings,normalTeamsOnly,preserveTotals});}
   catch(error){
    if(error.code!=='TEAM_MAPPING_REQUIRED')throw error;
    return {reviewId:pending.reviewId,ready:false,mappingRequired:{year:error.year,role:error.role,sourceName:error.sourceName},teams:error.choices};
   }
   pending.plan=plan;pending.teamMappings=structuredClone(mappings);pending.ready=true;
   return {reviewId:pending.reviewId,ready:true,originYear:plan.originYear,allocated:plan.allocated,rows:plan.review,warnings:plan.warnings,changed:plan.changes.length>0};
  }finally{this.busy=false;}
 }
 // Internal hand-off to the native commit adapter. The renderer receives
 // only a review ID and human-readable metadata, never tables or addresses.
 reviewed(player,reviewId){
  const p=this.pending;if(this.busy||!p?.ready||p.reviewId!==reviewId||!same(p.identity,identity(player)))throw Error('Refresh and review selected seasons before applying.');return p;
 }
 async apply(player,request,confirm){
  const pending=this.reviewed(player,request?.reviewId),revision=this.revision;
  if(!this.backupRoot||!path.isAbsolute(this.backupRoot))throw Error('A contained season recovery folder is required before applying.');
  this.busy=true;
  try{
   const counts=Object.fromEntries(['create','update','copy-shared','unchanged'].map(key=>[key,pending.plan.review.filter(r=>r.operation===key).length]));
   const message=`Import ${pending.source.length} selected seasons into ${player.firstName} ${player.lastName}?\n\n${counts.create} new, ${counts.update} updated, ${counts['copy-shared']} shared seasons copied for this player, ${counts.unchanged} unchanged. Unselected seasons and other player versions are preserved.\n\n${pending.plan.warnings.join('\n')}\n\nKeep your test roster loaded at the NBA 2K27 roster-edit player list (or inactive Home / Quick Play). Close in-game player-edit or save dialogs. A recovery backup is saved before the brief verified edit. Save the roster in-game to retain it.`;
   if(!await confirm(message))return {canceled:true};
   if(this.revision!==revision||this.pending!==pending)throw Error('The player selection changed. Review the import again.');
   pending.ready=false;
   const expectedDigest=crypto.createHash('sha256').update(pending.plan.after.players).update(pending.plan.after.stats).update(pending.plan.after.pool).digest('hex');
   const seasons=pending.source.map((r,i)=>({year:r.year,primaryTeamIndex:pending.plan.review[i].primaryTeam.index,secondaryTeamIndex:pending.plan.review[i].secondaryTeam?.index??-1,totals:pending.plan.review[i].totals,awards:r.awards??null}));
   const result=await this.saveNative({...pending.identity,token:pending.nativeToken,expectedDigest,backupRoot:this.backupRoot,confirmed:true,seasons});
   const changed=pending.plan.changes.length>0;
   if(!result||result.ok!==true||result.saved!==changed||result.afterDigest!==expectedDigest||result.allocated!==pending.plan.allocated||result.originYear!==pending.plan.originYear)throw Error('Native season import did not match the reviewed verification report. Refresh before retrying.');
   if(changed&&(typeof result.backup!=='string'||!path.isAbsolute(result.backup)||!path.resolve(result.backup).startsWith(path.resolve(this.backupRoot)+path.sep)))throw Error('Season recovery backup was not verified inside the launcher recovery folder.');
   return {saved:changed,allocated:result.allocated,originYear:result.originYear,...(changed?{backup:result.backup}:{})};
  }finally{if(!pending.ready&&this.pending===pending)this.pending=null;this.busy=false;}
 }
}
module.exports={PlayerSeasonImportService,decodeSnapshot};
