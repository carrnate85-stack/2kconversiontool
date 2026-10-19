(function(root,factory){if(typeof module==='object'&&module.exports)module.exports=factory(require('./player-season-math'),require('./player-season-awards'));else root.playerSeasonTransfer=factory(root.playerSeasonMath,root.playerSeasonAwards);})(typeof window==='object'?window:globalThis,(math,awards)=>{
 const key=row=>JSON.stringify([row.year,row.team,row.secondaryTeam??null]);
 function validate(rows){
  if(!Array.isArray(rows)||rows.length>25)throw Error('Season history must contain at most 25 rows.');
  const seen=new Set();return rows.map(row=>{
   if(!row||!Number.isInteger(row.year)||row.year<1900||row.year>2155||typeof row.team!=='string'||!row.team.trim()||row.team.length>100||/[\u0000-\u001f]/.test(row.team)||!row.totals||typeof row.totals!=='object'||Array.isArray(row.totals))throw Error('Invalid season-history row.');
   if(seen.has(key(row)))throw Error('Duplicate season/team entries cannot be transferred safely.');seen.add(key(row));
   if(Object.keys(row.totals).some(k=>!Object.hasOwn(math.limits,k)))throw Error('Unsupported season total.');
   const totals={};for(const [name,max] of Object.entries(math.limits)){const value=row.totals[name];if(!Number.isInteger(value)||value<(name==='plusMinus'?-16384:0)||value>max)throw Error('Invalid season total: '+name);totals[name]=value;}
   if(totals.gs>totals.gp||totals.doubleDoubles>totals.gp||totals.tripleDoubles>totals.gp)throw Error('Season counts exceed games played.');
   if(totals.fgm>totals.fga||totals.threeMade>totals.threeAttempted||totals.ftm>totals.fta||totals.threeMade>totals.fgm||totals.threeAttempted>totals.fga)throw Error('Invalid season shooting totals.');
   const result={year:row.year,team:row.team,totals}; // No source process token or address.
   if(Object.hasOwn(row,'secondaryTeam')){if(row.secondaryTeam!==null&&(typeof row.secondaryTeam!=='string'||!row.secondaryTeam.trim()||row.secondaryTeam.length>100||/[\u0000-\u001f]/.test(row.secondaryTeam)))throw Error('Invalid secondary season team.');result.secondaryTeam=row.secondaryTeam;}
   if(row.awards!==undefined){if(!row.awards||typeof row.awards!=='object'||Array.isArray(row.awards)||Object.keys(row.awards).length!==awards.fields.length||awards.fields.some(f=>!Object.hasOwn(row.awards,f.key)))throw Error('Incomplete season awards.');awards.patch(new Uint8Array(64),row.awards);result.awards=Object.fromEntries(awards.fields.map(f=>[f.key,row.awards[f.key]]));}
   return result;
  });
 }
 function plan(source,target){
  const rows=validate(source),dest=new Map();for(const row of target){const k=key(row);if(dest.has(k))throw Error('Destination has ambiguous season/team entries.');dest.set(k,row);}
  return rows.map(row=>{const match=dest.get(key(row));if(!match||row.team==='—'||row.team==='Unknown team')throw Error('No unique existing season for '+row.team+' '+row.year+'. New seasons are not created.');
   const changes=Object.fromEntries(Object.entries(row.totals).filter(([k,v])=>v!==match.totals[k]));
   if(row.awards){if(!match.awards)throw Error('Destination season awards are unavailable. Refresh before importing.');for(const [k,v] of Object.entries(row.awards))if(v!==match.awards[k])changes['award:'+k]=v;}
   return {slot:match.slot,token:match.token,changes};
  });
 }
 function mergeReviewed(source,existing){
  const rows=validate(existing),selected=validate(source),byYear=new Map();
  for(const row of rows){if(byYear.has(row.year))throw Error('Destination has ambiguous season years.');byYear.set(row.year,row);}
  const seen=new Set();for(const row of selected){if(seen.has(row.year))throw Error('Choose one aggregate row per imported season.');seen.add(row.year);const old=byYear.get(row.year);byYear.set(row.year,{...row,awards:row.awards??old?.awards??Object.fromEntries(awards.fields.map(f=>[f.key,0]))});}
  return validate([...byYear.values()].sort((a,b)=>b.year-a.year));
 }
 return {validate,plan,mergeReviewed};
});
