(()=>{
 const panel=document.getElementById('studioStats'),awardsPanel=document.getElementById('studioAwards');
 const section=document.createElement('section');section.className='studio-seasons';
 const heading=document.createElement('h3');heading.textContent='Season history';
 const note=document.createElement('p');note.className='studio-empty';note.textContent='Open Stats to load season history.';
 const refresh=document.createElement('button');refresh.type='button';refresh.className='secondary';refresh.textContent='Refresh seasons';
 const addOlder=document.createElement('button');addOlder.type='button';addOlder.className='secondary';addOlder.textContent='Add older season';addOlder.disabled=true;
 const removeOldest=document.createElement('button');removeOldest.type='button';removeOldest.className='secondary';removeOldest.textContent='Remove oldest season';removeOldest.disabled=true;
 const wrap=document.createElement('div');wrap.className='studio-season-scroll';
 wrap.tabIndex=0;wrap.setAttribute('role','region');wrap.setAttribute('aria-label','Season statistics, scroll horizontally for more columns');
 const navigation=document.createElement('div');navigation.className='studio-season-navigation';
 const left=document.createElement('button'),right=document.createElement('button'),scrollLabel=document.createElement('span');
 left.type=right.type='button';left.className=right.className='secondary';left.textContent='←';right.textContent='→';left.title='Earlier columns';right.title='More columns';left.setAttribute('aria-label','Earlier season-stat columns');right.setAttribute('aria-label','More season-stat columns');scrollLabel.textContent='Columns';
 const position=document.createElement('input');position.type='range';position.min='0';position.max='1000';position.value='0';position.setAttribute('aria-label','Horizontal position in season statistics');
 navigation.append(scrollLabel,left,position,right);
 function syncScroll(){const max=Math.max(0,wrap.scrollWidth-wrap.clientWidth);position.value=String(max?Math.round(wrap.scrollLeft/max*1000):0);position.disabled=max<1;left.disabled=wrap.scrollLeft<1;right.disabled=wrap.scrollLeft>=max-1;}
 left.addEventListener('click',()=>{wrap.scrollLeft-=Math.max(200,wrap.clientWidth*.7);syncScroll();});right.addEventListener('click',()=>{wrap.scrollLeft+=Math.max(200,wrap.clientWidth*.7);syncScroll();});
 position.addEventListener('input',()=>{wrap.scrollLeft=Number(position.value)/1000*(wrap.scrollWidth-wrap.clientWidth);syncScroll();});
 wrap.addEventListener('scroll',syncScroll);new ResizeObserver(syncScroll).observe(wrap);
 const importStats=document.createElement('button');importStats.type='button';importStats.className='secondary';importStats.id='playerImportStats';importStats.textContent='Import stats';importStats.addEventListener('click',()=>window.openPlayerStatsImport?window.openPlayerStatsImport():window.openPlayer23Import?.({statsOnly:true}));
 const actions=document.createElement('div');actions.style.cssText='display:flex;flex-wrap:wrap;gap:8px';actions.append(refresh,addOlder,removeOldest,importStats);
 section.append(heading,actions,note,navigation,wrap);panel.prepend(section);
 awardsPanel.querySelector('.studio-empty')?.remove();
 const awardNote=document.createElement('p');awardNote.className='studio-empty';awardNote.textContent='Open Awards to load season awards.';
 const awardsWrap=document.createElement('div');awardsWrap.className='studio-awards-list';
 awardsPanel.append(awardNote,awardsWrap);
 const highTitle=panel.querySelector('.studio-section-title');if(highTitle)highTitle.textContent='Single-game highs';
 let player=null,generation=0,loaded=false,busy=false,drafts=new Map();
 window.isSavingPlayerSeasons=()=>busy||window.isPlayerSeasonImportBusy?.()===true;
 let lastRows=[];
 window.resetPlayerSeasonDrafts=()=>{if(window.isSavingPlayerSeasons())return;drafts.clear();window.resetImportedPlayerSeasons?.();render(lastRows);window.dispatchEvent(new Event('player-editor:staged'));};
 window.hasUnsavedSeasonChanges=()=>window.isSavingPlayerSeasons()||window.hasPendingSeasonImport?.()===true||[...drafts.values()].some(d=>Object.keys(d.changes).length>0)||[...wrap.querySelectorAll('input')].some(input=>!input.validity.valid);
 window.exportPlayerSeasonHistory=async()=>{
  const token=generation;if(window.isSavingPlayerSeasons())throw Error('Wait for season loading/saving to finish.');if(!loaded)await load();
  if(token!==generation||!loaded)throw Error('Load season history before exporting it.');
  if([...wrap.querySelectorAll('input')].some(input=>!input.validity.valid))throw Error('Correct invalid season fields before exporting.');
  const rows=lastRows.map(row=>{const changes=drafts.get(row.slot)?.changes||{};return {year:row.year,team:row.team,...(Object.hasOwn(row,'secondaryTeam')?{secondaryTeam:row.secondaryTeam}:{}),totals:{...row.totals,...Object.fromEntries(Object.entries(changes).filter(([k])=>!k.startsWith('award:')))},awards:{...row.awards,...Object.fromEntries(Object.entries(changes).filter(([k])=>k.startsWith('award:')).map(([k,v])=>[k.slice(6),v]))}};});
  const pending=window.pendingImportedPlayerSeasonHistory?.();if(pending)return window.playerSeasonTransfer.mergeReviewed(pending,rows);
  return window.playerSeasonTransfer.validate(rows);
 };
 window.preparePlayerSeasonTransfer=async(history,options)=>{
  if(window.prepareImportedPlayerSeasons)return window.prepareImportedPlayerSeasons(history,options);
  if(window.hasUnsavedSeasonChanges())throw Error('Save or reset your existing season edits before importing history.');
  const token=generation;await load();if(token!==generation||!loaded)throw Error('Could not load destination seasons.');
  const planned=window.playerSeasonTransfer.plan(history,lastRows);
  return ()=>{if(token!==generation||busy)throw Error('Player changed during import.');for(const row of planned)drafts.set(row.slot,{token:row.token,changes:row.changes});render(lastRows);window.dispatchEvent(new Event('player-editor:staged'));};
 };
 window.refreshPlayerSeasonsAfterImport=async()=>{drafts.clear();loaded=false;await load();};
 const cols=[['mpg','MPG'],['ppg','PPG'],['orpg','ORPG'],['rpg','RPG',true],['apg','APG'],['spg','SPG'],['bpg','BPG'],['topg','TOPG'],['fgPercent','FG%',true],['threePercent','3P%',true],['ftPercent','FT%',true],['fpg','FPG',true],['plusMinusAverage','+/-',true],['points','PTS'],['rebounds','REB',true],['assists','AST'],['steals','STL'],['blocks','BLK'],['turnovers','TO'],['fgm','FGM'],['fga','FGA'],['threeMade','3PM'],['threeAttempted','3PA'],['ftm','FTM'],['fta','FTA'],['minutes','MINS'],['fouls','FLS',true],['gs','GMSSTART'],['gp','GP'],['doubleDoubles','DD',true],['tripleDoubles','TD',true],['plusMinus','+/- (total)',true]];
 const identity=()=>({recordIndex:player.recordIndex,firstName:player.firstName,lastName:player.lastName,cyberfaceId:player.cyberfaceId});
 const math=window.playerSeasonMath;
 function syncImportLock(){const value=window.isSavingPlayerSeasons()||window.hasPendingSeasonImport?.()===true;importStats.disabled=window.isSavingPlayerSeasons()||player?.source!=='live-roster';refresh.disabled=value||player?.source!=='live-roster';const oldest=lastRows.reduce((a,b)=>!a||b.slot>a.slot?b:a,null);addOlder.disabled=value||!loaded||!oldest||oldest.slot>=24||oldest.year<=1900;removeOldest.disabled=value||!loaded||!oldest||!!oldest.sharedWarning;for(const control of [...wrap.querySelectorAll('input,button,select'),...awardsWrap.querySelectorAll('input,button,select')])control.disabled=value||control.dataset.empty==='true';}
 function lock(value){busy=value;syncImportLock();window.dispatchEvent(new Event('player-editor:staged'));}
 window.addEventListener('player-editor:staged',syncImportLock);
 async function changeStructure(action){
  if(busy||!loaded)return;
  if(window.hasUnsavedSeasonChanges()){note.textContent='Save or reset season edits before adding or removing a season.';return;}
  const oldest=lastRows.reduce((a,b)=>!a||b.slot>a.slot?b:a,null);if(!oldest)return;
  let confirmed;
  if(document.documentElement.classList.contains('roster-browser')){
   const year=action==='add-older'?oldest.year-1:oldest.year;
   const change=action==='add-older'?`Add ${year-1}–${String(year).slice(-2)} with zero totals and the same team (${oldest.team}) as the oldest season? You can then edit its totals.`:`Remove ${year-1}–${String(year).slice(-2)} and its stats and awards from this player?`;
   confirmed=window.confirm(`${change}\n\nStay at the main menu or roster editor, outside gameplay. Use a backup roster. The game will pause briefly while this change is verified.\n\nOnly this player is affected. Save the roster in-game to retain the change.`);
   if(!confirmed)return;
  }
  const token=generation;lock(true);
  try{const result=await window.modLauncher.playerSeasons({...identity(),action,token:oldest.token,...(confirmed?{confirmed:true}:{})});if(token!==generation)return;if(result.canceled)return;drafts.clear();lock(false);await load();if(token===generation)note.textContent='Season history changed and verified. Save the roster in-game to retain it.';}
  catch(error){if(token===generation)note.textContent=error.message;}finally{if(token===generation)lock(false);}
 }
 addOlder.addEventListener('click',()=>changeStructure('add-older'));removeOldest.addEventListener('click',()=>changeStructure('remove-oldest'));
 async function load(){
  if(!player||player.source!=='live-roster'||busy||!window.modLauncher.playerSeasons)return;
  const token=generation;loaded=false;lock(true);note.textContent='Loading season history…';
  try{const rows=await window.modLauncher.playerSeasons({...identity(),action:'read'});if(token!==generation)return;render(rows);loaded=true;note.textContent=rows.length?'Totals and averages update together. Shooting averages keep attempts fixed and use the closest possible makes. Save season applies one row.':'No regular-season history is stored for this player.';awardNote.textContent=rows.length?'Awards are grouped by season. Save awards applies only the selected season.':'No regular-season awards are stored for this player.';}
  catch(error){if(token===generation){note.textContent=error.message;awardNote.textContent=error.message;}}
  finally{if(token===generation)lock(false);}
 }
 async function saveRow(row,changes,inputs=[]){if(window.isSavingPlayerSeasons()||window.hasPendingSeasonImport?.())return;for(const input of inputs)if(!input.reportValidity())return;if(!Object.keys(changes).length)return;const token=generation;lock(true);
  try{const result=await window.modLauncher.playerSeasons({...identity(),action:'save',token:row.token,changes});if(token!==generation)return;if(result.canceled){note.textContent='Save canceled. Your edits are still here.';awardNote.textContent=note.textContent;return;}drafts.delete(row.slot);lock(false);await load();if(token===generation){note.textContent='Season saved and verified. Save the roster in-game to retain it.';awardNote.textContent=note.textContent;}}
  catch(error){if(token===generation){note.textContent=error.message;awardNote.textContent=error.message;}}finally{if(token===generation)lock(false);}
 }
 const seasonLabel=row=>`${row.year-1}–${String(row.year).slice(-2)}`;
 function render(rows){lastRows=rows;wrap.replaceChildren();const table=document.createElement('table'),head=document.createElement('thead'),hr=document.createElement('tr');
  for(const text of ['Season','Team',...cols.map(c=>c[1]),'']){const th=document.createElement('th');th.textContent=text;hr.append(th);}head.append(hr);table.append(head);
  const body=document.createElement('tbody');table.append(body);
  for(const [seasonIndex,row] of rows.entries()){const tr=document.createElement('tr'),savedDraft=drafts.get(row.slot),changes=savedDraft?.token===row.token?savedDraft.changes:{};tr.className='studio-season-data-row';tr.dataset.band=String(seasonIndex%2);drafts.set(row.slot,{token:row.token,changes});tr.classList.toggle('is-edited',Object.keys(changes).length>0);let totals={...row.totals,...changes};const inputs=new Map();
   function synchronize(active){const values=math.display(totals);for(const [key,input] of inputs){if(input===active)continue;const value=values[key];input.value=value==null?'':key in totals||key==='rebounds'?String(value):value.toFixed(1);input.setCustomValidity('');}const edited=Object.keys(changes).length>0;tr.classList.toggle('is-edited',edited);save.disabled=!edited;save.dataset.empty=String(!edited);}
   tr.dataset.seasonSlot=String(row.slot);for(const text of [seasonLabel(row),row.team]){const td=document.createElement('td');td.textContent=td.title=text;tr.append(td);}
   for(const [key,label] of cols){const td=document.createElement('td'),input=document.createElement('input');input.type='number';if(!key.startsWith('plusMinus'))input.min='0';input.step=key in totals||key==='rebounds'?'1':'0.1';input.setAttribute('aria-label',`${row.year-1}–${String(row.year).slice(-2)} ${label}`);inputs.set(key,input);
    let editBaseline=null;
    input.addEventListener('focus',()=>{editBaseline={...totals};});
    input.addEventListener('input',()=>{try{if(input.value==='')throw Error('Enter a number.');editBaseline||={...totals};const value=Number(input.value),shown=math.display(editBaseline)[key],sameDisplay=shown!=null&&value===Number(key in totals||key==='rebounds'?shown:shown.toFixed(1));totals=sameDisplay?{...editBaseline}:math.change(editBaseline,key,value);input.setCustomValidity('');for(const field of Object.keys(row.totals)){if(totals[field]===row.totals[field])delete changes[field];else changes[field]=totals[field];}synchronize(input);}catch(error){input.setCustomValidity(error.message);note.textContent=error.message;}});
    input.addEventListener('blur',()=>{editBaseline=null;if(input.validity.valid)synchronize();});td.append(input);
    tr.append(td);
   }
   const td=document.createElement('td'),save=document.createElement('button');save.type='button';save.className='secondary studio-season-save';save.textContent='Save season';save.title=row.sharedWarning||'Save this season';
   save.addEventListener('click',()=>saveRow(row,changes,inputs.values()));td.append(save);tr.append(td);body.append(tr);synchronize();
  }wrap.append(table);renderAwards(rows);requestAnimationFrame(syncScroll);
 }
 function renderAwards(rows){awardsWrap.replaceChildren();for(const [seasonIndex,row] of rows.entries()){
  const changes=drafts.get(row.slot)?.changes||{},card=document.createElement('section');card.className='studio-award-season';card.dataset.band=String(seasonIndex%2);card.dataset.seasonSlot=String(row.slot);
  const header=document.createElement('header'),identityBox=document.createElement('div'),year=document.createElement('strong'),team=document.createElement('span');year.textContent=seasonLabel(row);team.textContent=row.team;identityBox.append(year,team);
  const save=document.createElement('button');save.type='button';save.className='secondary';save.textContent='Save awards';header.append(identityBox,save);card.append(header);
  const grid=document.createElement('div');grid.className='studio-award-grid';
  const refreshCard=()=>{const edited=Object.keys(changes).some(key=>key.startsWith('award:'));card.classList.toggle('is-edited',edited);save.disabled=!edited;save.dataset.empty=String(!edited);wrap.querySelector(`[data-season-slot="${row.slot}"]`)?.classList.toggle('is-edited',Object.keys(changes).length>0);};
  for(const f of window.playerSeasonAwards.fields){const label=document.createElement('label'),name=document.createElement('span'),select=document.createElement('select');name.textContent=f.label;select.setAttribute('aria-label',`${row.year} ${f.label}`);
   const values=f.choices||(f.width===1?['No','Yes']:['None','1st team','2nd team',...(f.max===3?['3rd team']:[])]);values.forEach((text,index)=>select.add(new Option(text,String(index))));
   const original=row.awards?.[f.key]??0,current=changes['award:'+f.key]??original;if(current>=values.length){const option=new Option('Unknown (preserve)',String(current));option.disabled=true;select.add(option);}select.value=String(current);
   select.addEventListener('change',()=>{const value=Number(select.value);if(value===original)delete changes['award:'+f.key];else changes['award:'+f.key]=value;refreshCard();window.dispatchEvent(new Event('player-editor:staged'));});label.append(name,select);grid.append(label);
  }
  save.addEventListener('click',()=>saveRow(row,changes));card.append(grid);awardsWrap.append(card);refreshCard();
 }}
 refresh.addEventListener('click',()=>{if(window.hasUnsavedSeasonChanges()&&!window.confirm('Refresh season history? Stale season drafts may be discarded.'))return;load();});
 document.getElementById('studioTabStats').addEventListener('click',()=>{if(!loaded)load();});
 document.getElementById('studioTabStats').addEventListener('focus',()=>{if(!panel.hidden&&!loaded)load();});
 document.getElementById('studioTabAwards').addEventListener('click',()=>{if(!loaded)load();});
 document.getElementById('studioTabAwards').addEventListener('focus',()=>{if(!awardsPanel.hidden&&!loaded)load();});
 window.addEventListener('player-editor:committed',event=>{player=event.detail.player;});
 window.addEventListener('player-seasons:imported',event=>{if(event.detail.recordIndex===player?.recordIndex)note.textContent=awardNote.textContent=event.detail.message;});
 window.addEventListener('player-editor:open',event=>{generation++;player=event.detail;loaded=false;busy=false;lastRows=[];drafts.clear();wrap.replaceChildren();awardsWrap.replaceChildren();addOlder.disabled=true;removeOldest.disabled=true;refresh.disabled=player?.source!=='live-roster';note.textContent=refresh.disabled?'Season history requires a loaded live roster.':'Open Stats to load season history.';awardNote.textContent=refresh.disabled?'Season awards require a loaded live roster.':'Open Awards to load season awards.';if(!panel.hidden||!awardsPanel.hidden)load();});
})();
