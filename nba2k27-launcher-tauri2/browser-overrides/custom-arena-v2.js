(function(root,factory){if(typeof module==='object'&&module.exports)module.exports=factory();else factory().install(root);})(typeof window==='object'?window:globalThis,()=>{
 'use strict';
 function prepare(request,team){
  if(!team||team.source!=='live-team'||request.recordIndex!==team.recordIndex||request.teamId!==team.teamId||request.expectedTeamName!==team.name)throw Error('Team changed. Reopen the team editor.');
  if(typeof team.nativeArenaCatalogId!=='string'||!/^[a-f0-9]{64}$/i.test(team.nativeArenaCatalogId))throw Error('Refresh live teams before cloning an arena.');
  const {expectedTargetName,...edit}=request;
  return {...edit,catalogId:team.nativeArenaCatalogId};
 }
 function install(root){
  const original=root.openTeamEditor;if(typeof original!=='function')return;
  let current=null,busy=false,epoch=0;
  root.openTeamEditor=function(team){epoch++;current=team;return original.apply(this,arguments);};
  const closeEditor=root.closeTeamEditor;if(typeof closeEditor==='function')root.closeTeamEditor=function(){epoch++;current=null;return closeEditor.apply(this,arguments);};
  const button=document.getElementById('createCustomArena');if(!button)return;
  const saved=document.createElement('button');saved.type='button';saved.textContent='Saved custom arenas';button.insertAdjacentElement('afterend',saved);
  saved.addEventListener('click',async()=>{
   const dialog=document.createElement('dialog'),title=document.createElement('h3'),list=document.createElement('div'),note=document.createElement('p'),close=document.createElement('button');
   title.textContent='Saved custom arenas';note.textContent='Enabled profiles apply on the next Play. They populate their reserved arena slot; they do not change team links. Reloading a roster during the same game still needs a fresh check.';close.textContent='Close';close.type='button';close.onclick=()=>dialog.close();dialog.append(title,note,list,close);document.body.append(dialog);dialog.addEventListener('close',()=>dialog.remove(),{once:true});dialog.showModal();
   let renderEpoch=0;
   async function render(){
    const renderVisit=++renderEpoch;
    const result=await root.modLauncher.customArenaProfiles({action:'read'});if(!dialog.open||renderVisit!==renderEpoch)return;list.replaceChildren();
    for(const profile of result.profiles||[]){
     const row=document.createElement('p'),label=document.createElement('span');label.textContent=`${profile.arenaName} · ${profile.expectedTeamName} · slot ${profile.targetArenaId} · ${profile.enabled?'Enabled':'Disabled'} `;row.append(label);
     for(const [action,text]of [[profile.enabled?'disable':'enable',profile.enabled?'Disable':'Enable for future Play'],['remove','Remove profile']]){
      const control=document.createElement('button');control.type='button';control.textContent=text;control.onclick=async()=>{control.disabled=true;try{await root.modLauncher.customArenaProfiles({action,id:profile.id});await render();}catch(error){note.textContent=error.message;control.disabled=false;}};row.append(control);
     }list.append(row);
    }
    if(!(result.profiles||[]).length)list.textContent='No saved custom arenas. Create one from an empty reserved slot first.';
    if(result.legacyReviewAvailable){
     const reviewButton=document.createElement('button');reviewButton.type='button';reviewButton.textContent='Review WPF profiles';list.append(reviewButton);
     let legacyPanel=null,legacyReviewBusy=false,legacyImportBusy=false;
     reviewButton.onclick=async()=>{
      if(legacyReviewBusy||legacyImportBusy)return;legacyReviewBusy=true;reviewButton.disabled=true;if(legacyPanel)legacyPanel.remove();
      try{
       const review=await root.modLauncher.customArenaProfiles({action:'reviewLegacy'});if(!dialog.open||renderVisit!==renderEpoch)return;
       if(review?.schema!=='nba2k27-custom-arena-profiles-review-v1'||!Array.isArray(review.entries)||review.entries.length>50||typeof review.reviewToken!=='string'||!/^[a-f0-9]{64}$/i.test(review.reviewToken))throw Error('The WPF profile review changed. Review it again.');
       const panel=document.createElement('div'),choices=[];legacyPanel=panel;
       for(const entry of review.entries||[]){const label=document.createElement('label'),checkbox=document.createElement('input'),text=document.createElement('span');checkbox.type='checkbox';checkbox.checked=false;checkbox.disabled=entry.valid!==true;text.textContent=entry.valid===true?`${entry.profile.arenaName} · ${entry.profile.expectedTeamName} · slot ${entry.profile.targetArenaId}`:`Profile ${entry.index+1}: ${(entry.issues||[]).map(issue=>issue.message).join('; ')}`;label.append(checkbox,text);panel.append(label,document.createElement('br'));choices.push({entry,checkbox});}
       const importButton=document.createElement('button');importButton.type='button';importButton.textContent='Import selected as disabled';panel.append(importButton);list.append(panel);
       importButton.onclick=async()=>{
        if(legacyImportBusy||legacyPanel!==panel||renderVisit!==renderEpoch||!dialog.open)return;
        const selectedIds=choices.filter(choice=>choice.entry.valid===true&&choice.checkbox.checked).map(choice=>choice.entry.id);
        if(!selectedIds.length){note.textContent='Select at least one valid WPF profile.';return;}
        legacyImportBusy=true;importButton.disabled=true;
        try{const imported=await root.modLauncher.customArenaProfiles({action:'importLegacy',reviewToken:review.reviewToken,selectedIds});if(!dialog.open||renderVisit!==renderEpoch)return;if(imported?.schema!=='nba2k27-custom-arena-profiles-review-v1'||imported?.activation!==false||imported?.memoryWrites!==0||imported?.legacyUnchanged!==true||imported?.importedCount!==selectedIds.length||!Array.isArray(imported.imported)||imported.imported.length!==selectedIds.length||new Set(imported.imported.map(row=>row.sourceId)).size!==selectedIds.length||new Set(imported.imported.map(row=>row.id)).size!==selectedIds.length||!imported.imported.every(row=>selectedIds.includes(row.sourceId)&&row.enabled===false&&typeof row.id==='string'&&/^[a-f0-9]{48}$/i.test(row.id)))throw Error('Legacy import did not return a local-only receipt. Refresh saved profiles.');note.textContent='Selected profiles imported as disabled. Enable each profile to apply it on a future Play session.';await render();}catch(error){if(dialog.open&&renderVisit===renderEpoch){note.textContent=error.message;importButton.disabled=true;reviewButton.disabled=false;}}finally{legacyImportBusy=false;}
       };
      }catch(error){if(dialog.open&&renderVisit===renderEpoch){note.textContent=error.message;reviewButton.disabled=false;}}finally{legacyReviewBusy=false;}
     };
    }

   }
   try{await render();}catch(error){note.textContent=error.message;}
  });
  button.addEventListener('click',async event=>{
   event.preventDefault();event.stopImmediatePropagation();if(busy)return;
   const owner=current,visit=epoch,modal=document.getElementById('teamEditorModal');
   if(!owner||modal.classList.contains('hidden'))return;
   const get=id=>document.getElementById(id).value;
   busy=true;button.disabled=true;button.textContent='Cloning & verifying…';
   try{
    const request=prepare({recordIndex:owner.recordIndex,teamId:owner.teamId,expectedTeamName:owner.name,donorArenaId:Number(get('customArenaDonor')),targetArenaId:Number(get('customArenaTarget')),arenaName:get('customArenaName'),arenaFile:get('customArenaFile'),floorFile:get('customArenaFloor')},owner);
    const result=await root.modLauncher.cloneArena(request);
    if(result?.canceled)return;
    if(current!==owner||visit!==epoch||modal.classList.contains('hidden'))return;
    if(result?.report?.ok!==true||result?.report?.readBackVerified!==true)throw Error('The native arena transaction did not confirm a verified clone. Refresh live teams before another edit.');
    if(result?.playerResearch)root.renderPlayerResearch(result.playerResearch);
    root.closeTeamEditor();root.toast(result?.profileSaveError||(result?.refreshRequired?'Arena created. Refresh live teams before another edit.':'Custom arena created, verified and linked.'),Boolean(result?.profileSaveError));
   }catch(error){if(current===owner&&visit===epoch)root.toast(error.message,true);}
   finally{busy=false;button.disabled=false;button.textContent='Create, verify & link';}
  },true);
 }
 return {prepare,install};
});
