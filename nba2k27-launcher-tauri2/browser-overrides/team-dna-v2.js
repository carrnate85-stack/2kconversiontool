(function(root,factory){
 const api=factory();if(typeof module==='object'&&module.exports)module.exports=api;
 else{root.TeamDnaV2=api;api.install(root);}
})(typeof window==='object'?window:globalThis,()=>{
 'use strict';
 const INFO=['name','cityName','cityShort','stateShort','mainLogo'];
 const same=(a,b)=>JSON.stringify(a)===JSON.stringify(b);
 const FORMAT='nba2k27-native-team-metadata';
 function metadataPlan(source,before,caps,arenas,names,years){
  const values={},warnings=[];if(!source)return {values,warnings};
  if(source.format!==FORMAT||source.version!==1)throw Error('Unsupported native Team DNA metadata.');
  if(Object.hasOwn(source,'rosterTypeName')){
   const matches=names.map((name,id)=>({name,id})).filter(r=>r.name===source.rosterTypeName);
   if(caps?.rosterTypeId===true&&matches.length===1)values.type=String(matches[0].id);else warnings.push('Team type unavailable; left unchanged.');
  }
  if(Object.hasOwn(source,'classicSeasonEndYear')){
   const year=source.classicSeasonEndYear;
   const matches=years.filter(r=>(r.value===0?null:(r.value<60?2000+r.value:1900+r.value))===year);
   if(caps?.classicTeamEndYearRaw===true&&matches.length===1)values.year=String(matches[0].value);else warnings.push('Historic season unavailable; left unchanged.');
  }
  if(source.arena){
   const a=source.arena,matches=arenas.filter(r=>r.name===a.name&&r.arenaFile===a.arenaFile&&r.floorFile===a.floorFile&&!r.emptyCandidate);
   if(caps?.arena===true&&matches.length===1&&Number.isInteger(matches[0].arenaId)&&matches[0].arenaId>=0&&matches[0].arenaId<=65535&&arenas.filter(r=>r.arenaId===matches[0].arenaId).length===1)values.arena=String(matches[0].arenaId);else warnings.push('Arena match missing or ambiguous; left unchanged.');
  }
  return {values,warnings};
 }
 function infoPlan(source,before,capabilities){
  const allowed=new Set(capabilities?.infoFields||INFO),values={},skipped=[];
  for(const [key,value] of Object.entries(source||{})){
   if(!INFO.includes(key)||!allowed.has(key)){if(!same(value,before[key]))skipped.push(key);continue;}
   if(typeof value!=='string')throw Error('Invalid Team DNA text: '+key);
   if(value!==before[key])values[key]=value;
  }
  return {values,skipped};
 }
 function uniformPlan(source,destination){
  const rows=[],warnings=[],used=new Set();
  for(const row of source||[]){
   const matches=destination.filter(d=>same(d.match,row.match)||['typeId','sideId','designationId'].every(k=>d.match[k]===row.match[k]));
   if(matches.length!==1){warnings.push('A uniform match is missing or ambiguous; left unchanged.');continue;}
   const key=JSON.stringify(['typeId','sideId','designationId'].map(k=>row.match[k]));
   if(used.has(key))throw Error('Team DNA contains duplicate uniform matches.');used.add(key);
   rows.push(row);
  }
  return {rows,warnings};
 }
 function install(w){
  if(typeof openTeamEditor!=='function'||typeof transferTeamDna!=='function')return;
  let visit=0;const originalOpen=openTeamEditor;
  openTeamEditor=function(...args){visit++;return originalOpen.apply(this,args);};
  const inputs={name:'teamEditName',cityName:'teamEditCity',cityShort:'teamEditCityShort',stateShort:'teamEditStateShort',mainLogo:'teamEditLogo'};
  const readInfo=()=>Object.fromEntries(Object.entries(inputs).map(([k,id])=>[k,document.getElementById(id).value]));
  const control=id=>document.getElementById(id);
  const metadataState=team=>({type:control('teamEditRosterType')?.value,year:control('teamNativeYearChoice')?.value,arena:control('teamEditArenaModel')?.value,caps:{...team.editCapabilities,rosterTypeId:team.editCapabilities?.rosterTypeId===true&&control('teamEditRosterType')?.disabled===false,classicTeamEndYearRaw:team.editCapabilities?.classicTeamEndYearRaw===true&&control('teamNativeYearChoice')?.disabled===false,arena:team.editCapabilities?.arena===true&&control('teamEditArenaModel')?.disabled===false&&typeof team.nativeArenaCatalogId==='string'},catalog:team.nativeArenaCatalogId,arenas:state.playerResearch?.arenas||[]});
  const names=()=>Array.from({length:36},(_,i)=>w.NativeTeamMetadata?.kindLabel(i));
  function metadataPreview(value,arenas){const raw=Number(value.year),a=arenas.filter(r=>String(r.arenaId)===value.arena);return {type:names()[Number(value.type)],seasonEndYear:value.year===undefined?undefined:raw===0?'Roster default':raw<60?2000+raw:1900+raw,arena:a.length===1?{name:a[0].name,arenaFile:a[0].arenaFile,floorFile:a[0].floorFile}:undefined};}
  function exportMetadata(team){
   const m=metadataState(team),value={format:FORMAT,version:1},ns=names();
   if(m.caps?.rosterTypeId===true&&/^\d+$/.test(m.type||'')&&Number(m.type)<36&&typeof ns[Number(m.type)]==='string')value.rosterTypeName=ns[Number(m.type)];
   if(m.caps?.classicTeamEndYearRaw===true&&/^\d+$/.test(m.year||'')&&Number(m.year)<=127){const raw=Number(m.year);value.classicSeasonEndYear=raw===0?null:raw<60?2000+raw:1900+raw;}
   const matches=m.arenas.filter(a=>String(a.arenaId)===m.arena&&!a.emptyCandidate);
   if(m.caps?.arena===true&&typeof m.catalog==='string'&&matches.length===1){const a=matches[0];value.arena={name:a.name,arenaFile:a.arenaFile,floorFile:a.floorFile};}
   return value;
  }
  transferTeamDna=async function(action){
   const team=state.editingTeam,epoch=visit,status=document.getElementById('teamDnaStatus');
   if(!team||team.source!=='live-team'||teamDnaBusy||teamUniformBusy)return;
   const current=()=>visit===epoch&&state.editingTeam===team&&!document.getElementById('teamEditorModal').classList.contains('hidden');
   const check=()=>{if(!current())throw Error('The destination editor changed. Reopen Team DNA.');};
   const sections=[...document.getElementById('teamDnaSections').querySelectorAll('input:checked')].map(i=>i.value);
   if(!sections.length){status.textContent='Choose at least one Team DNA section.';return;}
   teamDnaBusy=true;document.getElementById('teamDnaImport').disabled=document.getElementById('teamDnaExport').disabled=true;
   try{
    let data;
    if(action==='export'){
     if(sections.includes('staff'))await teamStaffView.load();check();
     const info=readInfo(),staff=sections.includes('staff')?teamStaffView.snapshot():[];
     const rotations=sections.includes('rotations')?await teamRotationView.portable():null;check();
     data={format:'2kteam',version:1,game:'NBA2K27',name:info.name,info,arena:null,staff,rotations,uniforms:sections.includes('uniforms')?teamUniformView.snapshot():[]};
     data.nativeMetadata=exportMetadata(team);
    }
    const result=await w.modLauncher.teamDna({action,sections,...(data?{data}:{})});check();if(!result)return;
    if(action==='export'){status.textContent='Team DNA exported. Verified native type, historic season and logical arena metadata are included when available. Destination IDs and assignments stay excluded.';return;}
    const before=readInfo(),info=infoPlan(result.info,before,team.editCapabilities),warnings=[];
    if(info.skipped.length)warnings.push('Unavailable Team info left unchanged: '+info.skipped.join(', ')+'.');
    const nativeBefore=metadataState(team),nativeBeforeKey=JSON.stringify(nativeBefore),nativePlan=metadataPlan(result.nativeMetadata,nativeBefore,nativeBefore.caps,nativeBefore.arenas,names(),w.NativeTeamMetadata?.yearChoices(Number(nativeBefore.year))||[]);warnings.push(...nativePlan.warnings);
    if(result.arena&&!result.nativeMetadata?.arena)warnings.push('Legacy arena/floor metadata has no verified native semantics; left unchanged.');
    const uniformsBefore=teamUniformView?.snapshot()||[],uniforms=uniformPlan(result.uniforms,uniformsBefore);warnings.push(...uniforms.warnings);
    const staffPlan=result.staff?.length?await teamStaffView.prepareStage(result.staff):null;check();
    if(staffPlan?.missing?.length)warnings.push(staffPlan.missing.length+' unassigned staff roles were left unchanged.');
    const rotationPlan=result.rotations?await teamRotationView.prepareStage(result.rotations):null;check();
    const validate=()=>{check();if(!same(readInfo(),before)||!same(teamUniformView?.snapshot()||[],uniformsBefore)||JSON.stringify(metadataState(team))!==nativeBeforeKey)throw Error('Destination drafts changed during review; nothing was staged.');if(staffPlan)teamStaffView.validateStage(staffPlan);if(rotationPlan)teamRotationView.validateStage(rotationPlan);};
    validate();
    const rows=w.ImportReview?.diff({info:before,nativeMetadata:metadataPreview(nativeBefore,nativeBefore.arenas),uniforms:uniformsBefore,staff:staffPlan?.before||[],rotations:rotationPlan?.before||null},{info:{...before,...info.values},nativeMetadata:metadataPreview({...nativeBefore,...nativePlan.values},nativeBefore.arenas),uniforms:uniforms.rows,staff:staffPlan?.after||[],rotations:rotationPlan?.after||null})||[];
    if(w.ImportReview&&!await w.ImportReview.confirm({title:'Import Team DNA · '+result.name,rows,note:'Stages supported edits only. Save each affected tab separately. '+warnings.join(' '),valid:current})){if(current())status.textContent='Import canceled. Existing edits are unchanged.';return;}
    validate();
    // No await after full preflight: all section commits share this editor visit.
    if(staffPlan)teamStaffView.commitStage(staffPlan);
    if(rotationPlan)teamRotationView.commitStage(rotationPlan);
    if(uniforms.rows.length)warnings.push(...teamUniformView.stage(uniforms.rows));
    for(const [key,value] of Object.entries(info.values))document.getElementById(inputs[key]).value=value;
    for(const [key,value] of Object.entries(nativePlan.values)){const element=control({type:'teamEditRosterType',year:'teamNativeYearChoice',arena:'teamEditArenaModel'}[key]);element.value=value;if(key==='arena')element.dispatchEvent(new Event('change'));}
    status.textContent='Supported Team DNA edits staged locally. Save Team info, each staff member, rotations and each uniform in their own tabs. Nothing has been written to the game. '+warnings.join(' ');
   }catch(e){if(current())status.textContent=e.message;}
   finally{teamDnaBusy=false;document.getElementById('teamDnaImport').disabled=document.getElementById('teamDnaExport').disabled=false;}
  };
 }
 return {infoPlan,uniformPlan,metadataPlan,install};
});
