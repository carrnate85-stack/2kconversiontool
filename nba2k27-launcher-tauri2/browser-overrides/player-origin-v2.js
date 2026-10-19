(()=>{
 'use strict';
 let current=null;const select=document.getElementById('playerEditFrom'),nativeSave=window.modLauncher?.savePlayer;
 if(!select||typeof nativeSave!=='function')return;
 function open(player){
  current=player;if(!player?.editCapabilities)return;
  const origins=typeof state==='object'&&Array.isArray(state.playerResearch?.origins)?state.playerResearch.origins:[];
  select.replaceChildren();const unset=document.createElement('option');unset.value='';unset.textContent=player.originState==='invalid'?'Current origin unavailable':'Unassigned';unset.disabled=true;select.append(unset);
  for(const origin of origins){const option=document.createElement('option');option.value=String(origin.index);option.textContent=origin.name;select.append(option);}
  const index=player.nativeOriginIndex;
  if(Number.isInteger(index)&&!origins.some(o=>o.index===index)){const option=document.createElement('option');option.value=String(index);option.textContent=player.collegeName||'Unmapped current origin';option.disabled=true;select.append(option);}
  select.value=Number.isInteger(index)?String(index):'';select.disabled=player.editCapabilities.fromName!==true;
  select.title=select.disabled?'This origin cannot be changed in the current verified roster.':'';
 }
 window.addEventListener('player-editor:open',e=>open(e.detail));
 window.addEventListener('player-editor:closed',()=>{current=null;});
 window.addEventListener('player-editor:committed',e=>open(e.detail?.player));
 Object.defineProperty(window.modLauncher,'savePlayer',{value:request=>{
  const player=current;
  if(player?.editCapabilities&&request.recordIndex===player.recordIndex&&request.expectedFirstName===player.firstName&&request.expectedLastName===player.lastName&&request.expectedCyberfaceId===player.cyberfaceId&&!document.getElementById('playerEditorModal').classList.contains('hidden')){
   if(player.editCapabilities.fromName===true){
    const value=Object.hasOwn(request,'fromIndex')?request.fromIndex:select.value===''?null:Number(select.value);
    request={...request};delete request.fromIndex;
    if(value===null){if(Number.isInteger(player.nativeOriginIndex))throw Error('From cannot be cleared. Choose an existing origin.');}
    else{
     if(!Number.isInteger(value)||value<0)throw Error('Choose a verified existing origin.');
     if(value!==player.nativeOriginIndex){
      const origins=typeof state==='object'?state.playerResearch?.origins||[]:[];
      if(origins.filter(o=>o.index===value).length!==1)throw Error('The origin choices changed. Reopen the player.');
      request.nativeOriginIndex=value;request.nativeOriginCatalogId=player.nativeOriginCatalogId;
     }
    }
   }else if(Object.hasOwn(request,'fromIndex')&&request.fromIndex!==0)throw Error('From is read only for this player.');
  }
  return nativeSave(request);
 }});
})();
