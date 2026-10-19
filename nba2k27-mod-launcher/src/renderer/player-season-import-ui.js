(()=>{
 const panel=document.getElementById('studioDNA'),area=document.createElement('section');area.className='studio-subsection studio-season-import';area.id='playerSeasonImportReview';area.hidden=true;
 const title=document.createElement('h3');title.textContent='Review selected seasons';
 const note=document.createElement('p');note.className='studio-empty';note.textContent='Apply seasons separately from player ratings. Unselected history and other player versions are preserved. Keep your test roster loaded at the NBA 2K27 roster-edit player list (or inactive Home / Quick Play). Close in-game player-edit or save dialogs, then save the roster in-game after applying.';
 const content=document.createElement('div'),status=document.createElement('p');status.className='studio-empty';status.setAttribute('role','status');
 const button=text=>{const b=document.createElement('button');b.type='button';b.className='secondary';b.textContent=text;return b;};
 const refresh=button('Refresh review'),discard=button('Discard import'),apply=button('Apply selected seasons');apply.id='playerSeasonImportApply';apply.className='primary';
 const actions=document.createElement('div');actions.className='studio-dna-actions';actions.append(refresh,discard,apply);area.append(title,note,content,status,actions);(document.getElementById('player23Selection')||panel).append(area);
 let player=null,version=0,busy=false,pending=null;
 const identity=()=>({recordIndex:player.recordIndex,firstName:player.firstName,lastName:player.lastName,cyberfaceId:player.cyberfaceId});
 const api=request=>window.modLauncher.playerSeasons({...identity(),...request});
 const clean=error=>String(error.message||error).replace(/^Error invoking remote method '[^']+': (?:Error: )?/,'');
 const season=year=>`${year-1}–${String(year).slice(-2)}`;
 function lock(){refresh.disabled=discard.disabled=busy;apply.disabled=busy||!pending?.review?.ready;for(const select of content.querySelectorAll('select'))select.disabled=busy;window.dispatchEvent(new Event('player-editor:staged'));}
 function render(){
  content.replaceChildren();area.hidden=!pending;if(!pending){lock();return;}
  const review=pending.review;
  if(!review?.ready){
   if(review?.mappingRequired){
    const {year,role,sourceName}=review.mappingRequired,label=document.createElement('label'),text=document.createElement('span'),select=document.createElement('select');text.textContent=`${season(year)} · choose the destination ${role} team for ${sourceName}`;select.setAttribute('aria-label',text.textContent);select.add(new Option('Choose a current roster team…',''));
    for(const team of review.teams)select.add(new Option(`${team.displayName||team.name} · #${team.index}`,String(team.index)));
    select.addEventListener('change',()=>{if(select.value==='')return;pending.teamMappings[year]??={};pending.teamMappings[year][role==='primary'?'primaryTeamIndex':'secondaryTeamIndex']=Number(select.value);read(false);});label.append(text,select);content.append(label);
   }
   lock();return;
  }
  const table=document.createElement('table');table.className='studio-season-table';const head=document.createElement('tr');for(const text of ['Season','Destination team','Action','GP','PTS','Details']){const th=document.createElement('th');th.textContent=text;head.append(th);}table.append(head);
  const names={'create':'Create missing season','update':'Update this season','copy-shared':'Copy for this player','unchanged':'Already matches'};
  for(const row of review.rows){
   const tr=document.createElement('tr');for(const text of [season(row.year),(row.primaryTeam.displayName||row.primaryTeam.name)+(row.secondaryTeam?' / '+(row.secondaryTeam.displayName||row.secondaryTeam.name):''),names[row.operation],row.totals.gp,row.totals.points]){const td=document.createElement('td');td.textContent=String(text);tr.append(td);}
   const td=document.createElement('td'),details=document.createElement('details'),summary=document.createElement('summary'),values=document.createElement('pre');summary.textContent='Totals / awards';values.textContent=Object.entries(row.totals).map(([key,value])=>`${key}: ${value}`).join('\n')+'\n\n'+(row.awards?Object.entries(row.awards).map(([key,value])=>`${key}: ${value}`).join('\n'):'Existing awards preserved');details.append(summary,values);td.append(details);tr.append(td);table.append(tr);
  }
  content.append(table);for(const warning of review.warnings){const p=document.createElement('p');p.className='studio-empty';p.textContent=warning;content.append(p);}lock();
 }
 async function read(fresh){
  if(busy||!pending)return;const token=version,selection=pending;busy=true;lock();status.textContent='Reading and reviewing destination history…';
  try{const result=await api({action:'review-import',seasons:selection.seasons,teamMappings:selection.teamMappings,normalTeamsOnly:selection.normalTeamsOnly,preserveTotals:selection.preserveTotals,...(!fresh&&selection.review?{reviewId:selection.review.reviewId}:{})});if(token!==version||pending!==selection)return;selection.review=result;render();status.textContent=result.ready?(result.changed===false?'Selected stats and awards already match. No changes are needed.':`${result.allocated} missing or shared season records will be created. Review each year, then Apply selected seasons.`):'Choose the destination team to continue. Nothing has been written.';}
  catch(error){if(token===version&&pending===selection){selection.review=null;render();status.textContent=clean(error);}}finally{if(token===version){busy=false;lock();}}
 }
 window.isPlayerSeasonImportBusy=()=>busy;
 window.hasPendingSeasonImport=()=>!!pending;
 window.playerSeasonImportSummary=()=>pending?`${pending.seasons.length} season${pending.seasons.length===1?'':'s'}${pending.seasons.some(row=>row.awards)?' + awards':''} queued · ${busy?'Reading review':pending.review?.mappingRequired?'Choose destination team':pending.review?.ready?(pending.review.changed===false?'Already matches':'Review / Apply seasons'):'Review needed'}`:'';
 window.pendingImportedPlayerSeasonHistory=()=>{
  if(!pending)return undefined;
  if(busy||!pending.review?.ready)throw Error('Finish reviewing destination seasons and team choices before exporting.');
  return window.playerSeasonTransfer.validate(pending.review.rows.map(row=>({year:row.year,team:row.primaryTeam.name,secondaryTeam:row.secondaryTeam?.name??null,totals:row.totals,...(row.awards?{awards:row.awards}:{})})));
 };
 window.resetImportedPlayerSeasons=()=>{if(busy)return;version++;pending=null;status.textContent='';render();};
 window.prepareImportedPlayerSeasons=async(history,{normalTeamsOnly=false,teamMappings={},preserveTotals={}}={})=>{
  if(busy||player?.source!=='live-roster')throw Error('Open a live destination player and wait for the current season operation.');
  if(window.hasUnsavedSeasonChanges?.())throw Error('Save or reset existing season edits, or discard the previous import, before selecting new history.');
  const token=version,source=window.playerSeasonTransfer.validate(history);if(!source.length)throw Error('Choose at least one season to import.');
  // Validate first; staging never invokes the live write endpoint.
  return async()=>{if(token!==version||busy)throw Error('The player changed before season staging.');pending={seasons:source,teamMappings:structuredClone(teamMappings),preserveTotals:structuredClone(preserveTotals),normalTeamsOnly,review:null};render();area.scrollIntoView({block:'start',behavior:'smooth'});await read(true);};
 };
 refresh.addEventListener('click',()=>read(true));discard.addEventListener('click',window.resetImportedPlayerSeasons);
 async function applyPending(){
  if(busy||!pending?.review?.ready)return false;const token=version,selection=pending;let completed=false,message='';
  if(window.ImportReview){busy=true;lock();let accepted;try{accepted=await window.ImportReview.confirm({title:'Import season stats & awards',rows:window.ImportReview.seasonRows(selection.review),action:'Continue to apply',note:'Before values are from the verified destination snapshot. New seasons are marked unavailable / new. Unselected years remain unchanged. '+selection.review.warnings.join(' '),valid:()=>token===version&&pending===selection});}finally{if(token===version){busy=false;lock();}}if(!accepted||token!==version||pending!==selection)return false;}
  let confirmed;
  if(document.documentElement.classList.contains('roster-browser')){confirmed=window.confirm(`Apply ${selection.seasons.length} selected seasons?\n\nUnselected seasons and other player versions are preserved.\n${selection.review.warnings.join('\n')}\n\nKeep your test roster loaded at the NBA 2K27 roster-edit player list (or inactive Home / Quick Play). Close in-game player-edit or save dialogs. A recovery backup is saved before the brief verified edit. Save the roster in-game afterward.`);if(!confirmed)return;}
  busy=true;lock();status.textContent='Applying and verifying selected seasons…';
  try{const result=await api({action:'apply-import',reviewId:selection.review.reviewId,...(confirmed?{confirmed:true}:{})});if(token!==version||pending!==selection)return;if(result.canceled){status.textContent='Apply canceled. Your review is still here.';return;}
   pending=null;render();await window.refreshPlayerSeasonsAfterImport?.();if(token===version){message=result.saved?'Selected seasons applied and verified. Save the roster in-game to retain them.':'Selected seasons already match. No changes were needed.';status.textContent=message;completed=true;}area.hidden=false;
  }catch(error){if(token===version){if(pending===selection)selection.review=null;render();status.textContent=clean(error)+' Refresh the review before retrying.';}}finally{if(token===version){busy=false;lock();}}
  if(completed)window.dispatchEvent(new CustomEvent('player-seasons:imported',{detail:{...identity(),message}}));return completed;
 }
 apply.addEventListener('click',applyPending);
 window.importSelectedPlayerSeasons=async(history,{normalTeamsOnly=true,teamMappings={},preserveTotals={}}={})=>{
  if(busy||player?.source!=='live-roster')throw Error('Wait for the current season operation.');
  const source=window.playerSeasonTransfer.validate(history);
  if(pending){if(pending.normalTeamsOnly!==normalTeamsOnly||JSON.stringify(pending.preserveTotals)!==JSON.stringify(preserveTotals)||JSON.stringify(pending.seasons)!==JSON.stringify(source))throw Error('Finish or discard the current season review before changing the selected years.');}
  else await (await window.prepareImportedPlayerSeasons(source,{normalTeamsOnly,teamMappings,preserveTotals}))();
  if(pending?.review?.ready)return applyPending();return false;
 };
 window.addEventListener('player-editor:open',event=>{version++;player=event.detail;pending=null;busy=false;status.textContent='';render();});
 window.addEventListener('player-editor:committed',event=>{version++;player=event.detail.player;busy=false;if(pending){pending.review=null;render();status.textContent='Player values changed. Refresh the season review before applying.';}});
 render();
})();
