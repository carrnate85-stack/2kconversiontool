(() => {
 const modal=document.getElementById('playerEditorModal');
 const ids=['playerEditFirstName','playerEditLastName','playerEditCyberface','playerEditJersey','playerEditPosition','playerEditSecondary','playerEditFrom','playerEditBirthDate','playerEditYearsPro','playerEditHeight','playerEditHeightInches','playerEditWingspan','playerEditWeight','playerEditPotential','playerAudioFullName'];
 let baseline=new Map(),current=null,importVerified=false,resetting=false;
 const status=document.createElement('span');status.id='playerUnsavedStatus';status.setAttribute('role','status');
 const reset=document.createElement('button');reset.type='button';reset.className='secondary';reset.textContent='Reset staged edits';
 const actions=modal.querySelector('.team-editor-actions');actions.prepend(status,reset);
 const review=document.createElement('button');review.type='button';review.className='secondary';review.textContent='Review changes';actions.prepend(review);
 review.addEventListener('click',()=>{
  const entries=changed().filter(([input])=>!input.dataset.personalityPair),included=new Set(entries.map(([input])=>input.dataset.fieldIndex));
  let ratingEdits;try{ratingEdits=new Map((window.playerRatingChanges?.()||[]).map(edit=>[String(edit.index),edit]));for(const edit of ratingEdits.values())if(!included.has(String(edit.index))){const input=document.getElementById('studioField'+edit.index);if(input)entries.push([input,String(edit.expected)]);}}catch(error){toast(error.message,true);return;}
  const text=(input,value)=>input.tagName==='SELECT'?([...input.options].find(o=>o.value===String(value))?.textContent||value):value;
  const rows=entries.map(([input,before])=>{const edit=ratingEdits.get(input.dataset.fieldIndex),field=window.playerRatingSchema[Number(input.dataset.fieldIndex)];return {label:input.dataset.fieldIndex!==undefined?(field.label+(edit&&field.group==='attributes'?' (stored raw)':'')):input.closest('label')?.firstChild?.textContent||input.id,before:edit?(field.choices?.[edit.expected]??String(edit.expected)):text(input,before),after:edit?(field.choices?.[edit.value]??String(edit.value)):text(input,input.value),remove:()=>{if(window.isPlayerDnaBusy?.()||window.isSavingPlayerSeasons?.()||document.getElementById('savePlayerEdit').textContent==='Saving…')return false;restoreBatch([input]);return true;}};});
  if(window.hasUnsavedSeasonChanges?.())rows.push({label:'Season history',before:'Loaded season history',after:window.playerSeasonImportSummary?.()||'Unsaved season edits (review in Stats / Awards)',remove:()=>{if(window.isSavingPlayerSeasons?.())return false;window.resetPlayerSeasonDrafts?.();update();return true;}});
  window.EditorReview?.open({title:'Review player changes',rows});
 });
 const nav=document.createElement('select');nav.id='playerQuickSwitch';nav.setAttribute('aria-label','Switch player');
 modal.querySelector('.team-editor-head>div').append(nav);
 function controls(){return [...modal.querySelectorAll('[data-field-index],[data-personality-pair],'+ids.map(id=>'#'+id).join(','))].filter(e=>!e.disabled&&!e.readOnly);}
 function changed(){return [...baseline].filter(([input,value])=>input.value!==value);}
 function dirty(){if(changed().length||window.hasUnsavedSeasonChanges?.())return true;try{return !!window.playerRatingChanges?.().length;}catch{return true;}}
 window.hasUnsavedPlayerChanges=dirty;
 window.canLeavePlayerEditor=()=>{
  if(document.getElementById('savePlayerEdit').textContent==='Saving…'||window.isSavingPlayerSeasons?.()||window.isPlayerDnaBusy?.()){toast('Please wait for the current editor operation to finish.',true);return false;}
  return !dirty()||window.confirm('Discard unsaved player and season changes?');
 };
 function update(){
  if(resetting)return;
  const pending=dirty(),seasonSummary=window.playerSeasonImportSummary?.();status.textContent=seasonSummary||(importVerified?(pending?'Import verified · Other edits unsaved':'Import verified · Save roster in-game'):(pending?'Unsaved changes':'No staged changes'));status.classList.toggle('is-edited',pending);reset.disabled=!pending;
  let ratingEdits=new Set();try{ratingEdits=new Set((window.playerRatingChanges?.()||[]).map(edit=>edit.index));}catch{}
  const labels=new Map();for(const [input,value] of baseline){const label=input.closest('label');if(label)labels.set(label,(labels.get(label)||false)||input.value!==value||input.dataset.fieldIndex!==undefined&&ratingEdits.has(Number(input.dataset.fieldIndex)));}
  for(const [label,edited] of labels){label.classList.toggle('has-staged-edit',edited);const button=label.querySelector('.player-reset-field');if(button)button.hidden=!edited;}
 }
 function restore(input){if(!baseline.has(input))return;input.value=baseline.get(input);input.setCustomValidity('');input.dispatchEvent(new Event('input',{bubbles:true}));input.dispatchEvent(new Event('change',{bubbles:true}));}
 function restoreBatch(inputs,all=false){
  if(window.isSavingPlayerSeasons?.()||window.isPlayerDnaBusy?.()||document.getElementById('savePlayerEdit').textContent==='Saving…'){toast('Please wait for the current editor operation to finish.',true);return;}
  resetting=true;
  try{for(const input of inputs)restore(input);window.resetStagedPlayerRatingValues?.(all?undefined:inputs.filter(input=>input.dataset.fieldIndex!==undefined).map(input=>Number(input.dataset.fieldIndex)));if(all)window.resetPlayerSeasonDrafts?.();importVerified=false;}
  finally{resetting=false;update();}
 }
 reset.addEventListener('click',()=>{if(window.isSavingPlayerSeasons?.()||window.isPlayerDnaBusy?.()||document.getElementById('savePlayerEdit').textContent==='Saving…'){toast('Please wait for the current editor operation to finish.',true);return;}if(!window.confirm('Reset all staged edits to the values loaded when you opened this player? Saved game changes are not undone.'))return;restoreBatch([...baseline.keys()],true);});
 nav.addEventListener('change',()=>{const p=(state.playerResearch?.players||[]).find(p=>p.source==='live-roster'&&String(p.recordIndex)===nav.value);if(p)openPlayerEditor(p);nav.value=String(state.editingPlayer?.recordIndex??'');});
 window.addEventListener('player-editor:open',event=>{
  importVerified=false;current=event.detail;baseline=new Map(controls().map(input=>[input,input.value]));
  for(const input of baseline.keys()){
   const label=input.closest('label');if(!label||label.querySelector('.player-reset-field')||input.dataset.personalityPair)continue;
   label.classList.add('player-reset-row');const button=document.createElement('button');button.type='button';button.className='player-reset-field';button.textContent='↶';button.title=button.ariaLabel='Reset '+(label.querySelector('.studio-field-name')?.textContent||label.firstChild?.textContent||'field');button.hidden=true;
   button.addEventListener('click',event=>{event.preventDefault();restoreBatch([...baseline.keys()].filter(control=>control.closest('label')===label));});label.append(button);
  }
  nav.replaceChildren();const placeholder=document.createElement('option');placeholder.value='';placeholder.textContent='Choose a loaded player';placeholder.disabled=true;nav.append(placeholder);for(const p of (state.playerResearch?.players||[]).filter(p=>p.source==='live-roster')){const option=document.createElement('option');option.value=String(p.recordIndex);option.textContent=p.name+(p.teamName?' · '+p.teamName:'');nav.append(option);}nav.value=current.source==='live-roster'?String(current.recordIndex):'';nav.disabled=nav.options.length===1;update();
 });
 function edited(){if(resetting)return;importVerified=false;update();}
 modal.addEventListener('input',edited);modal.addEventListener('change',edited);
 window.addEventListener('player-editor:import-verified',()=>{importVerified=true;update();});
 window.addEventListener('player-editor:staged',update);
 window.addEventListener('player-editor:committed',event=>{
  current=event.detail.player;
  const map={firstName:['playerEditFirstName'],lastName:['playerEditLastName'],positionId:['playerEditPosition'],secondaryPositionId:['playerEditSecondary'],fromName:['playerEditFrom'],birthDate:['playerEditBirthDate'],yearsPro:['playerEditYearsPro'],heightCm:['playerEditHeight','playerEditHeightInches'],wingspanCm:['playerEditWingspan'],weightLbs:['playerEditWeight'],potential:['playerEditPotential']};
  const changedIds=new Set(event.detail.infoKeys.flatMap(k=>map[k]||[])),ratingKeys=new Set(event.detail.ratingKeys);
  for(const input of controls())if(changedIds.has(input.id)||input.dataset.fieldIndex!==undefined&&ratingKeys.has(window.playerRatingSchema[Number(input.dataset.fieldIndex)]?.key))baseline.set(input,input.value);
  const option=[...nav.options].find(o=>o.value===String(current.recordIndex));if(option)option.textContent=current.name+(current.teamName?' · '+current.teamName:'');
  update();
 });
})();
