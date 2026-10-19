(()=>{
 'use strict';
 const bridge=window.modLauncher?.onlineRatings,button=document.getElementById('playerDna2kdbRead');
 if(typeof bridge!=='function'||!button||!window.SavedRatingsSource)return;
 const control=button.parentElement,url=control?.querySelector('input');if(!url)return;
 const status=document.createElement('p'),cancel=document.createElement('button');status.setAttribute('role','status');cancel.type='button';cancel.textContent='Cancel capture';cancel.disabled=true;control.append(cancel);control.parentElement.append(status);
 const foot=control.parentElement.querySelector('p.studio-empty');if(foot)foot.textContent='Open the player page in an isolated browser. Only matching supported values are reviewed and staged locally. Save & verify remains required. Missing or incompatible fields stay unchanged.';
 let player=null,epoch=0,active=null;
 const identity=p=>({recordIndex:p.recordIndex,firstName:p.firstName,lastName:p.lastName,cyberfaceId:p.cyberfaceId});
 const visible=()=>!document.getElementById('playerEditorModal').classList.contains('hidden');
 const valid=()=>player?.source==='live-roster'&&!!player.editCapabilities&&visible();
 function controls(){button.disabled=!!active||!valid();cancel.disabled=!active;}
 async function close(entry){if(!entry.id)return;if(entry.closing)return entry.closing;const id=entry.id;entry.closing=bridge({action:'cancel',id,destination:entry.destination}).then(receipt=>{if(receipt?.closed!==true)throw Error('The isolated browser is still closing. No values were staged.');entry.id=null;}).finally(()=>{entry.closing=null;});return entry.closing;}
 function retire(message){const prior=active;if(prior){prior.canceled=true;active=null;void close(prior).catch(()=>{});}if(message)status.textContent=message;controls();}
 const reset=e=>{epoch++;retire();player=e?.detail?.player||e?.detail||null;status.textContent='';controls();};
 window.addEventListener('player-editor:open',reset);window.addEventListener('player-editor:committed',reset);window.addEventListener('player-editor:closed',()=>reset());
 cancel.addEventListener('click',()=>{epoch++;retire('Capture canceled. Existing edits are unchanged.');});url.addEventListener('input',()=>{if(active){epoch++;retire('Source URL changed. Existing edits are unchanged.');}});
 const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
 async function open(address){
  if(active||!valid())return false;
  const parsed=window.SavedRatingsSource.address(address),owner=player,visit=epoch,entry={destination:identity(owner),id:null,canceled:false};active=entry;controls();
  const current=()=>active===entry&&!entry.canceled&&epoch===visit&&player===owner&&visible()&&url.value.trim()===address;
  status.textContent='Opening isolated ratings page…';
  try{
   const session=await bridge({action:'start',url:parsed.url,destination:entry.destination});entry.id=session?.id;
   if(typeof entry.id!=='string'||session.url!==parsed.url||session.isolation!=='native-webview2-no-launcher-bridge')throw Error('Invalid isolated capture receipt.');
   if(!current())return false;
   const until=Date.now()+120000;let mapped;
   for(let attempt=0;attempt<80&&Date.now()<until;attempt++){
    if(!current())return false;status.textContent='Waiting for matching player/game/roster data. You may complete a site challenge in the isolated window.';
    const sample=await bridge({action:'sample',id:entry.id,destination:entry.destination});if(!current())return false;
    if(sample?.id!==entry.id||sample.url!==parsed.url)throw Error('Ratings source or capture identity changed.');
    if(sample.sample?.ready===true){
     if(typeof sample.sample.text!=='string'||new TextEncoder().encode(sample.sample.text).length>4000000)throw Error('Ratings page data exceeds 4 MB.');
     try{mapped=window.SavedRatingsSource.read(sample.sample.text,parsed.url,window.playerRatingSchema);break;}catch(error){if(error.message!=='No unique matching player/game/roster was found in the saved data.')throw error;}
    }
    await delay(1500);
   }
   if(!mapped)throw Error('Ratings capture timed out. No player values were changed.');
   await close(entry);if(!current())return false;
   status.textContent='Reviewing supported values…';const applied=await window.stageCatalogPlayerDna(mapped.data);if(!current())return false;
   status.textContent=applied===false?'Review canceled. Existing edits are unchanged.':applied?.noChanges?'No supported changes to stage.':`${mapped.source} · ${mapped.name} · supported values staged locally. Review the tabs, then Save & verify. ${mapped.warnings.join(' ')}`;
   return applied!==false;
  }catch(error){if(current())status.textContent=error.message||String(error);return false;}
  finally{await close(entry).catch(()=>{});if(active===entry){active=null;controls();}}
 }
 // Capture-phase interception preserves the old legacy implementation when the
 // native bridge/capability is absent. The remote page never receives this API.
 button.addEventListener('click',event=>{if(!valid())return;event.preventDefault();event.stopImmediatePropagation();void open(url.value.trim()).catch(error=>{status.textContent=error.message||String(error);controls();});},true);
 window.LiveRatingsCapture={open};window.isLiveRatingsBusy=()=>!!active;controls();
})();
