(() => {
 const $=id=>document.getElementById(id),panel=$('studioDNA'),schema=window.playerRatingSchema;let current=null,version=0,busy=false,players=[];
 panel.querySelector('.studio-empty')?.remove();
 const note=document.createElement('p');note.className='studio-empty';note.textContent='Export a .2kplayer file, import one, or copy mapped fields from another loaded player. Imports replace selected fields on this player—not team assignments. Review before Save & verify.';panel.append(note);
 const sections=document.createElement('div');sections.className='studio-dna-sections';
 const names={info:'Player info',appearance:'Appearance',audio:'Full-name audio ID',attributes:'Attributes',development:'Development',tendencies:'Tendencies',playtypes:'Play types',personality:'Personality',badges:'Badges',hotzones:'Hot zones',signature:'Signature',equipment:'Equipment',contract:'Contract & preferences',stats:'Stat highs',seasons:'Season history (review selected years)'};
 for(const [key,name] of Object.entries(names)){const label=document.createElement('label'),input=document.createElement('input');input.type='checkbox';input.checked=key!=='seasons';input.value=key;label.append(input,document.createTextNode(name));sections.append(label);}panel.append(sections);
 const actions=document.createElement('div');actions.className='studio-dna-actions';
 const button=(text,id)=>{const b=document.createElement('button');b.type='button';b.className='secondary';b.textContent=text;b.id=id;return b;};
 const exportButton=button('Export .2kplayer','playerDnaExport'),importButton=button('Import .2kplayer','playerDnaImport');actions.append(exportButton,importButton);panel.append(actions);
 const heading=document.createElement('h3');heading.className='studio-group-title';heading.textContent='Copy from another player';
 const search=document.createElement('input');search.type='search';search.className='studio-rating-search';search.placeholder='Search loaded players or teams';search.setAttribute('aria-label','Search DNA source players');
 const source=document.createElement('select');source.id='playerDnaSource';source.size=5;source.setAttribute('aria-label','Player to copy from');source.className='studio-dna-source';
 const copyButton=button('Stage selected player','playerDnaCopy'),status=document.createElement('p');status.id='playerDnaStatus';status.className='studio-empty';status.setAttribute('role','status');
 const workspace=document.createElement('div');workspace.className='studio-dna-workspace';const files=document.createElement('div'),copy=document.createElement('div');files.append(sections,actions);copy.append(heading,search,source,copyButton);workspace.append(files,copy);panel.append(workspace,status);
 const ids={firstName:'playerEditFirstName',lastName:'playerEditLastName',cyberfaceId:'playerEditCyberface',positionId:'playerEditPosition',secondaryPositionId:'playerEditSecondary',jerseyNumberRaw:'playerEditJersey',birthDate:'playerEditBirthDate',yearsPro:'playerEditYearsPro',weightLbs:'playerEditWeight',heightCm:'playerEditHeight',wingspanCm:'playerEditWingspan',potential:'playerEditPotential',fullNameId:'playerAudioFullName'};
 const selected=()=>{const values=[...sections.querySelectorAll('input:checked')].map(i=>i.value);if(!values.length)throw Error('Choose at least one DNA section.');return values;};
 const valid=()=>current?.source==='live-roster'&&current.ratingsRaw?.length===schema.length;
 function enable(){for(const b of [exportButton,importButton,copyButton])b.disabled=busy||!valid();copyButton.disabled||=!source.value;}
 function populate(){const selectedId=source.value,q=search.value.trim().toLowerCase();source.replaceChildren();const matches=players.filter(p=>p.recordIndex!==current?.recordIndex&&(`${p.name} ${p.teamName}`).toLowerCase().includes(q));for(const p of matches.slice(0,100)){const o=document.createElement('option');o.value=String(p.recordIndex);o.textContent=p.name+(p.teamName?' · '+p.teamName:'');source.append(o);}source.value=selectedId;if(!source.value&&source.options.length)source.selectedIndex=0;enable();}
 async function stage(dna,options){
  if(!dna)return false;const token=version,owner=current;
  const before={...current};for(const [key,id] of Object.entries(ids)){const input=$(id);if(!input||input.disabled)continue;before[key]=key==='heightCm'?window.playerMeasurements.readHeight(input,$('playerEditHeightInches')):key==='wingspanCm'?window.playerMeasurements.readWingspan(input):['birthDate','firstName','lastName'].includes(key)?input.value:key==='jerseyNumberRaw'&&input.value==='00'?100:input.value===''?undefined:Number(input.value);}
  before.fromName=$('playerEditFrom')?.selectedOptions?.[0]?.textContent;
  const baselineFields=Object.fromEntries(schema.map((f,i)=>[f.key,current.ratingsRaw[i]]));for(const change of window.playerRatingChanges())baselineFields[schema[change.index].key]=change.value;
  const available=Object.fromEntries(Object.entries(ids).map(([key,id])=>[key,!!$(id)&&!$(id).disabled]));available.fromName=!!$('playerEditFrom')&&!$('playerEditFrom').disabled;
  if(!window.EditorDestinationCapabilities)throw Error('Editor destination capabilities are unavailable. Reopen the editor.');
  const filtered=window.EditorDestinationCapabilities.filterPlayer(dna,{info:before,fields:baselineFields},current.editCapabilities,available);dna=filtered.data;
  const skipped=filtered.skipped.length?' Left unchanged because this destination cannot save them: '+filtered.skipped.join(', ')+'.':'';
  if(!filtered.count&&!dna.seasonHistory?.length){status.textContent='No supported changes to stage.'+skipped;return {noChanges:true,message:status.textContent};}
  const draft=()=>JSON.stringify({inputs:Object.values(ids).map(id=>[$(id)?.value,$(id)?.disabled]),from:$('playerEditFrom')?.value,ratings:window.playerRatingChanges()});const originalDraft=draft();
  if(dna.seasonHistory?.length>1){
   if(!window.DnaSeasonSelection)throw Error('Season selection is unavailable. Reopen the editor.');
   const caps=await window.modLauncher.playerSeasons({action:'teams',recordIndex:owner.recordIndex,key:JSON.stringify([owner.firstName,owner.lastName,owner.cyberfaceId])});
   if(token!==version||current!==owner||draft()!==originalDraft||$('playerEditorModal').classList.contains('hidden'))return false;
   const c=caps?.capabilities,bulk=c?.structure===true&&c.bulkApply===true&&c.existingRowsOnly===false&&c.nonsharedOnly===false&&c.maxStagedRows===25;
   const history=await window.DnaSeasonSelection.choose(dna.seasonHistory,{bulk,valid:()=>token===version&&current===owner&&draft()===originalDraft&&!$('playerEditorModal').classList.contains('hidden')});
   if(history===null){if(token===version)status.textContent='Import canceled. Existing edits are unchanged.';return false;}
   dna={...dna,seasonHistory:history};if(!history.length)delete dna.seasonHistory;
   if(!filtered.count&&!history.length){status.textContent='No supported changes to stage.'+skipped;return {noChanges:true,message:status.textContent};}
  }
  const approved=!window.ImportReview||await window.ImportReview.confirm({title:'Import Player DNA · '+dna.name,rows:window.ImportReview.playerRows(dna,before,schema,window.playerRatingChanges()),note:'Review before staging. Save & verify is still required. Any season history receives its own verified destination review.'+skipped,valid:()=>token===version&&current===owner&&draft()===originalDraft&&!$('playerEditorModal').classList.contains('hidden')});if(token!==version||current!==owner||!current||$('playerEditorModal').classList.contains('hidden'))return false;if(draft()!==originalDraft)throw Error('Destination edits changed during import review. Nothing was staged.');if(!approved){status.textContent='Import canceled. Existing edits are unchanged.';return false;}
  const commitSeasons=dna.seasonHistory?await window.preparePlayerSeasonTransfer(dna.seasonHistory,options):null;if(token!==version||current!==owner)return false;if(draft()!==originalDraft)throw Error('Destination edits changed during season review. Nothing was staged.');window.stagePlayerRatingValues(dna.fields);
  const skippedBio=[];
  for(const [key,value] of Object.entries(dna.info)){const input=$(ids[key]);if(input&&!input.disabled){if(key==='heightCm')window.playerMeasurements.writeHeight(input,$('playerEditHeightInches'),value);else if(key==='wingspanCm')window.playerMeasurements.writeWingspan(input,value);else input.value=key==='jerseyNumberRaw'&&value===100?'00':String(value);}else if(['birthDate','yearsPro'].includes(key))skippedBio.push(key==='birthDate'?'Birthday':'Years pro');}
  let originSkipped=false;
  if(dna.info.fromName){const select=$('playerEditFrom'),matches=[...select.options].filter(o=>o.value!==''&&o.textContent.trim().toLowerCase()===dna.info.fromName.trim().toLowerCase());if(!select.disabled&&matches.length===1)select.value=matches[0].value;else originSkipped=true;}
  status.textContent=`Staged ${Object.keys(dna.fields).length+Object.keys(dna.info).length} fields from ${dna.name}. Review the tabs, then Save & verify. Nothing has been written to the game.`;
  if(originSkipped)status.textContent+=' From was left unchanged: no unique available roster match.';
  if(skippedBio.length)status.textContent+=' '+skippedBio.join(' and ')+' left unchanged: refresh the verified player-detail mapping.';
  status.textContent+=skipped;
  let seasonResult;if(commitSeasons){seasonResult=commitSeasons();status.textContent+=seasonResult?.noChanges?' The existing season already matches; no season changes were staged.':commitSeasons.existingRowsOnly?' One existing season staged locally. Review it, then click Save season.':' Selected seasons are queued for review; use Apply selected seasons before Save & verify.';}
  if(!filtered.count&&seasonResult?.noChanges)return {noChanges:true,message:status.textContent};
  window.dispatchEvent(new Event('player-editor:staged'));
  return true;
 }
 // Catalog imports use the same staging path and existing Save & verify flow.
 // No catalog button gets an alternate unguarded live-game writer.
 window.stageCatalogPlayerDna=async(data,options)=>{
  if(!valid()||busy||window.isSavingPlayerSeasons?.())throw Error('Open an editable player and wait for the current operation to finish.');
  const token=version;busy=true;enable();try{return await stage(data,options);}finally{if(token===version){busy=false;enable();}}
 };
 async function run(action){
  if(!valid()||busy)return;const token=version;busy=true;enable();status.textContent='';
  try{
   const included=selected();let result;
   if(action==='export'){
    const dna=await window.modLauncher.playerDna({action:'copy',recordIndex:current.recordIndex,key:JSON.stringify([current.firstName,current.lastName,current.cyberfaceId]),sections:Object.keys(names).filter(k=>k!=='seasons')});
    if(token!==version||$('playerEditorModal').classList.contains('hidden'))return;
    for(const c of window.playerRatingChanges())dna.fields[schema[c.index].key]=c.value;
    for(const [key,id] of Object.entries(ids)){const input=$(id);if(input.disabled)continue;if(input.value===''){if(key==='yearsPro'||key==='birthDate'&&dna.info.birthDate)throw Error('Enter a complete '+(key==='yearsPro'?'years pro value':'birthday')+' or reset the field.');continue;}if(key==='birthDate')window.playerBio.dateParts(input.value);dna.info[key]=key==='heightCm'?window.playerMeasurements.readHeight(input,$('playerEditHeightInches')):key==='wingspanCm'?window.playerMeasurements.readWingspan(input):['firstName','lastName','birthDate'].includes(key)?input.value.trim():key==='jerseyNumberRaw'&&input.value==='00'?100:Number(input.value);}
    dna.name=[dna.info.firstName,dna.info.lastName].filter(Boolean).join(' ');
    const originSelect=$('playerEditFrom');if(!originSelect.disabled&&originSelect.value!=='')dna.info.fromName=originSelect.selectedOptions[0].textContent;
    if(included.includes('seasons')){dna.seasonHistory=await window.exportPlayerSeasonHistory();dna.version=2;if(token!==version)return;}
    result=await window.modLauncher.playerDna({action:'export',data:dna,sections:included});if(token===version&&result?.saved)status.textContent='Player DNA exported. No portraits, game files, addresses, or local paths are included.';
   }else if(action==='import')result=await window.modLauncher.playerDna({action:'import',sections:included});
   else{const p=players.find(p=>p.recordIndex===Number(source.value));if(!p)throw Error('Choose a loaded player to copy.');result=await window.modLauncher.playerDna({action:'copy',recordIndex:p.recordIndex,key:p.key,sections:included});}
   if(token===version&&!$('playerEditorModal').classList.contains('hidden')&&action!=='export')await stage(result);
  }catch(error){if(token===version)status.textContent=String(error.message||error).replace(/^Error invoking remote method '[^']+': (?:Error: )?/,'');}
  finally{if(token===version){busy=false;enable();}}
 }
 exportButton.addEventListener('click',()=>run('export'));importButton.addEventListener('click',()=>run('import'));copyButton.addEventListener('click',()=>run('copy'));search.addEventListener('input',populate);source.addEventListener('change',enable);
 $('studioTabDNA').addEventListener('click',async()=>{const token=version;try{const result=await window.modLauncher.playerDna({action:'players'});if(token===version){players=Array.isArray(result)?result:[];populate();}}catch(error){if(token===version)status.textContent='Refresh the live roster to copy another player.';}});
 window.addEventListener('player-editor:open',event=>{current=event.detail;version++;status.textContent='';search.value='';populate();});enable();
 window.addEventListener('player-editor:closed',()=>{current=null;version++;busy=false;enable();});
 window.addEventListener('player-editor:committed',event=>{current=event.detail.player;});
 window.isPlayerDnaBusy=()=>busy;
 const web=document.createElement('section');web.className='studio-subsection';
 const webTitle=document.createElement('h3');webTitle.textContent='Import ratings from 2KDB or 2KAuction';
 const link=document.createElement('input');link.type='url';link.placeholder='Paste a 2KDB or 2KAuction MyNBA player link';link.setAttribute('aria-label','2KDB or 2KAuction player URL');
 const fetchButton=button('Import ratings','playerDna2kdbRead');
 const importControl=document.createElement('div');importControl.className='studio-ratings-import';importControl.append(link,fetchButton);
 const webStatus=document.createElement('p');webStatus.setAttribute('role','status');
 const review=document.createElement('div');review.style.cssText='max-height:360px;overflow:auto';
 const foot=document.createElement('p');foot.className='studio-empty';foot.textContent='Import immediately applies names, height, wingspan, weight, positions, From (when uniquely matched), mapped attributes, tendencies, badges, hot zones and potential. Missing or incompatible fields stay unchanged. For 2KAuction, include the desired game/roster selection in the link. Older-game and MyTEAM values may differ from standard 2K27 rosters. Save the roster in-game to keep changes.';
 web.append(webTitle,foot,importControl,webStatus,review);panel.append(web);
 function clearWeb(){review.replaceChildren();webStatus.textContent='';fetchButton.disabled=busy||!valid();}
 const isAppliedRow=r=>['attributes','tendencies','badges','hotzones'].includes(r.group)||['firstName','lastName','heightCm','wingspanCm','weightLbs','fromName','positionId','secondaryPositionId'].includes(r.key);
 window.addEventListener('player-editor:open',clearWeb);
 fetchButton.addEventListener('click',async()=>{if(busy||!valid())return;if(current.editCapabilities){webStatus.textContent='Online page capture is not connected in this dev build. Use saved player-page data below, then review and Save & verify.';return;}const token=version;busy=true;enable();fetchButton.disabled=true;clearWeb();webStatus.textContent='Opening the ratings source. Let the selected player page load; close its window to cancel.';
  try{const result=await window.modLauncher.playerDna({action:'2kdb',url:link.value.trim()});if(!result||token!==version||$('playerEditorModal').classList.contains('hidden'))return;
   webStatus.textContent='Applying ratings and verifying changes…';
   const applied=await window.applyTwokdbRatings(result.data);if(applied.canceled){webStatus.textContent='Import canceled. Nothing was written.';return;}const count=applied.count;
   if(applied.warnings.length)result.warnings.push(...applied.warnings);
   if(applied.warnings.some(w=>w.startsWith('From ')))result.rows=result.rows.filter(r=>r.key!=='fromName');
   if(token!==version)return;
   webStatus.textContent=(result.source||'2KDB')+' · '+result.name+' · NBA 2K'+result.sourceVersion+' · '+count+' mapped fields applied and verified. Save the roster in-game to keep them. '+result.warnings.join(' ');
   const table=document.createElement('table');table.className='studio-season-table';const head=document.createElement('tr');for(const text of ['Category / field','Imported']){const th=document.createElement('th');th.textContent=text;head.append(th);}table.append(head);
   for(const row of result.rows.filter(isAppliedRow)){const tr=document.createElement('tr');for(const text of [row.group+' / '+row.label,row.value]){const td=document.createElement('td');td.textContent=String(text??'—');tr.append(td);}table.append(tr);}review.append(table);
   const details=document.createElement('details'),summary=document.createElement('summary');summary.textContent='Not imported / reference information';details.append(summary);const pre=document.createElement('pre');pre.style.whiteSpace='pre-wrap';pre.textContent='Unfilled editor fields:\n'+result.missing.map(f=>f.group+' — '+f.label).join('\n')+'\n\nMapped fields (left unchanged):\n'+result.rows.filter(r=>!isAppliedRow(r)).map(r=>r.label+': '+r.value).join('\n')+'\n\nOther source information (not mapped):\n'+result.reference.map(r=>r.key+': '+r.value).join('\n');details.append(pre);review.append(details);
  }catch(e){if(token===version)webStatus.textContent=String(e.message||e);}finally{if(token===version){busy=false;fetchButton.disabled=!valid();enable();}}
 });
 clearWeb();
})();
