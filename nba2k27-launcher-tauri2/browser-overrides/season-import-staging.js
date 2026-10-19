(function(root,factory){if(typeof module==='object'&&module.exports)module.exports=factory();else root.VersionTwoSeasonStaging=factory();})(typeof window==='object'?window:globalThis,()=>{
 'use strict';
 const supported=c=>c?.existingRowsOnly===true&&c?.nonsharedOnly===true&&typeof c?.structure==='boolean'&&c?.bulkApply===false&&c?.maxStagedRows===1;
 function create({api,identity,hasUnsaved,stage,onCapabilities}){
  let generation=0,busy=false;
  return {invalidate(){generation++;busy=false;},async prepare(history,options={}){
   if(busy)throw Error('Wait for the season review to finish.');
   const current=generation,who=identity();if(!who)throw Error('Open a live roster player first.');
   const valid=()=>current===generation&&JSON.stringify(identity())===JSON.stringify(who);
   if(hasUnsaved())throw Error('Save or reset existing season edits before staging an import.');
   busy=true;
   try{
    const directory=await api({...who,action:'teams'});if(!valid())throw Error('The player changed during season review.');
    if(!supported(directory?.capabilities))return null;
    onCapabilities(directory.capabilities);
    if(!Array.isArray(history)||history.length!==1)throw Error('Choose one existing, unshared season at a time. Save that season before staging another.');
    if(hasUnsaved())throw Error('Save or reset existing season edits before staging an import.');
    const result=await api({...who,action:'review-existing-import',seasons:history,teamMappings:options.teamMappings||{},preserveTotals:options.preserveTotals||{},normalTeamsOnly:options.normalTeamsOnly===true});
    if(!valid())throw Error('The player changed during season review.');
    if(!supported(result?.capabilities)||!Array.isArray(result.rows)||!Array.isArray(result.plans)||result.plans.length!==1)throw Error('Refresh the verified destination season review.');
    const plan=result.plans[0],matches=result.rows.filter(row=>row.slot===plan.slot&&row.token===plan.token);
    if(matches.length!==1||!plan.changes||typeof plan.changes!=='object'||Array.isArray(plan.changes))throw Error('The season review no longer matches its destination.');
    const destination=matches[0],noChanges=Object.entries(plan.changes).every(([key,value])=>value===(key.startsWith('award:')?destination.awards?.[key.slice(6)]:destination.totals?.[key]));
    const commit=()=>{if(!valid()||hasUnsaved())throw Error('The destination changed before season staging. Refresh the review.');if(noChanges)return {staged:false,noChanges:true};stage(result);return {staged:true,noChanges:false};};
    Object.defineProperties(commit,{existingRowsOnly:{value:true},noChanges:{value:noChanges}});return commit;
   }finally{if(current===generation)busy=false;}
  }};
 }
 const normalTeams=teams=>Array.isArray(teams)?teams.filter(t=>t.normal===true||(t.normal===undefined&&t.displayName?.endsWith(' · Normal'))):[];
 const capabilitiesForDirectory=directory=>{
  if(directory?.capabilities!==undefined)return directory.capabilities;
  const teams=directory?.teams;
  const legacy=directory&&Object.keys(directory).length===1&&Array.isArray(teams)&&teams.length>0&&teams.length<=1024&&teams.every(t=>Number.isInteger(t.index)&&t.index>=0&&t.index<1024&&typeof t.name==='string'&&typeof t.displayName==='string'&&t.normal===undefined)&&teams.some(t=>t.displayName.endsWith(' · Normal'));
  return legacy?{legacy:true,structure:true,existingRowsOnly:false,bulkApply:true}:null;
 };
 return {supported,create,normalTeams,capabilitiesForDirectory};
});
