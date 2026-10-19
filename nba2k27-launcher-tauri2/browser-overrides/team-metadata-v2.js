(function(root,factory){if(typeof module==='object'&&module.exports)module.exports=factory();else{root.NativeTeamMetadata=factory();root.NativeTeamMetadata.install();}})(typeof window==='object'?window:globalThis,()=>{
 'use strict';
 const names={"0":"NORMAL","1":"HISTORIC","2":"USER","3":"ONLINE","4":"ALLSTARS_EAST","5":"ALLSTARS_WEST","6":"STREET_HOME","7":"STREET_AWAY","8":"ROOKIES","9":"SOPHOMORES","10":"ROOKIE_SHOWCASE","11":"MYTEAM_TEAM","12":"EUROLEAGUE","13":"DOMINATION","14":"HIGH_SCHOOL","15":"COLLEGE_MENS","16":"COLLEGE_WOMENS","17":"AAU","18":"G_LEAGUE","19":"IGNITE","20":"SPORTS_2K","21":"HOOPS_SUMMIT","22":"FIBA","23":"FIBA_HIDDEN","24":"EXPANSION_SLOT","25":"EXPANSION_PREMADE","26":"HISTORIC_SNAPSHOT","27":"ALL_TIME","28":"E_LEAGUE","29":"WNBA","30":"DECADE","31":"JORDAN","32":"REBRAND_TEMPLATE","33":"HIDDEN","34":"RANDOMIZED","35":"COED"};
 const kindLabel=value=>Number.isInteger(value)&&value>=0&&value<=63?(names[value]||'Unknown / reserved'):'Unavailable';
 function yearLabel(value){if(!Number.isInteger(value)||value<0||value>255||value>=128)return 'Historic year unavailable (read only)';return value===0?'Historic year: roster default (read only)':'Season ending '+(value<60?2000+value:1900+value)+' (read only)';}
 function yearChoices(current){
  const years=new Map();for(let code=1;code<=127;code++){const year=code<60?2000+code:1900+code;if(!years.has(year))years.set(year,code);}
  if(Number.isInteger(current)&&current>0&&current<128)years.set(current<60?2000+current:1900+current,current);
  return [{value:0,label:'Roster default'},...[...years].sort((a,b)=>a[0]-b[0]).map(([year,value])=>({value,label:String(year-1)+'-'+String(year).slice(-2)}))];
 }
 function install(){
  if(typeof openTeamEditor!=='function')return;const original=openTeamEditor;let view=null;
  const nativeSave=window.modLauncher?.saveTeam;
  if(typeof nativeSave==='function')Object.defineProperty(window.modLauncher,'saveTeam',{value:request=>{
   const active=view;
   if(active&&request.recordIndex===active.team.recordIndex&&request.teamId===active.team.teamId&&request.expectedName===active.team.name&&!document.getElementById('teamEditorModal').classList.contains('hidden')){
    request={...request,arenaId:0};delete request.floorFile;
    if(!active.select.disabled){
     const value=Number(active.select.value);
     if(!Number.isInteger(value)||!yearChoices(active.raw).some(row=>row.value===value))throw Error('Choose a supported historic season.');
     if(value!==active.raw)request.classicTeamEndYearRaw=value;
    }
    if(!active.arena.disabled){
     const value=Number(active.arena.value);
     if(active.arena.value===''||!Number.isInteger(value)||active.arenas.filter(a=>a.arenaId===value).length!==1)throw Error('Choose a verified existing arena.');
     if(value!==active.team.nativeArenaId){request.nativeArenaId=value;request.nativeArenaCatalogId=active.team.nativeArenaCatalogId;}
    }
   }
   return nativeSave(request);
  }});
  openTeamEditor=function(team){view=null;const result=original.apply(this,arguments);if(team?.source!=='live-team'||!team.editCapabilities)return result;
   const types=document.getElementById('teamEditRosterType'),year=document.getElementById('teamEditHistoricYear');
   const raw=team.rosterTypeId;types.replaceChildren();
   for(let i=0;i<36;i++){const option=document.createElement('option');option.value=String(i);option.textContent=i+' - '+kindLabel(i);types.append(option);}
   if(Number.isInteger(raw)&&raw>=36&&raw<=63){const o=document.createElement('option');o.value=String(raw);o.textContent=raw+' - '+kindLabel(raw);types.append(o);}
   types.value=String(raw);types.disabled=team.editCapabilities.rosterTypeId!==true;
   // Keep the neutral historicYearId request contract; display the native raw
   // code separately so opening an editor never rewrites a nonunique encoding.
   year.disabled=true;year.hidden=true;
   if(year.parentElement?.firstChild?.nodeType===3)year.parentElement.firstChild.textContent='Historic season';
   let label=document.getElementById('teamNativeYearLabel');if(!label){label=document.createElement('span');label.id='teamNativeYearLabel';year.after(label);}label.textContent=yearLabel(team.classicTeamEndYearRaw);
   let select=document.getElementById('teamNativeYearChoice');if(!select){select=document.createElement('select');select.id='teamNativeYearChoice';select.setAttribute('aria-label','Historic season');year.after(select);}
   const yearRaw=team.classicTeamEndYearRaw;select.replaceChildren();
   for(const row of yearChoices(yearRaw)){const o=document.createElement('option');o.value=String(row.value);o.textContent=row.label;select.append(o);}
   select.disabled=team.editCapabilities.classicTeamEndYearRaw!==true;select.hidden=select.disabled;label.hidden=!select.disabled;select.value=String(yearRaw);
   const arena=document.getElementById('teamEditArenaModel');
   const arenas=typeof state==='object'&&Array.isArray(state.playerResearch?.arenas)?state.playerResearch.arenas:[];
   arena.disabled=team.editCapabilities.arena!==true;arena.value=Number.isInteger(team.nativeArenaId)?String(team.nativeArenaId):'';
   document.getElementById('teamEditArenaId').disabled=true;document.getElementById('teamEditArenaId').value=Number.isInteger(team.nativeArenaId)?String(team.nativeArenaId):'';
   document.getElementById('teamEditFloor').disabled=true;
   view={team,select,raw:yearRaw,arena,arenas};
   return result;
  };
  document.getElementById('teamEditArenaModel').addEventListener('change',()=>{
   if(!view||view.arena.disabled)return;const matches=view.arenas.filter(a=>a.arenaId===Number(view.arena.value));if(matches.length!==1)return;
   const arena=matches[0],floor=document.getElementById('teamEditFloor');floor.replaceChildren();const option=document.createElement('option');option.value=arena.floorFile;option.textContent=arena.floorFile;floor.append(option);floor.value=arena.floorFile;
   document.getElementById('teamEditFloorAsset').value=arena.floorFile;
  });
 }
 return {kindLabel,yearLabel,yearChoices,install};
});
