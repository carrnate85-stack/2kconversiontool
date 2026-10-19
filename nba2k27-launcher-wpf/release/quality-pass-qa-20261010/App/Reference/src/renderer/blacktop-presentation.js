'use strict';
(()=>{
 const api=window.modLauncher,status=document.getElementById('blacktopRemovalStatus');
 const inputs={drone:document.getElementById('blacktopRemoveDrone'),dj:document.getElementById('blacktopRemoveDj'),mc:document.getElementById('blacktopRemoveMc')};
 if(!api.blacktopPresentationStatus||Object.values(inputs).some(v=>!v))return;
 let confirmed={drone:false,dj:false,mc:false},pending=false;
 function render(value){confirmed=value.options;for(const key of Object.keys(inputs))inputs[key].checked=confirmed[key]===true;const message=(value.message||'').trim();const error=['error','conflict','unsupported','write-failed','timeout'].includes(value.phase)&&message!=='Unlock helper stopped safely. Restart the game before retrying.';status.textContent=error?message:'';status.hidden=!status.textContent;}
 api.onBlacktopPresentationStatus(value=>{if(!pending)render(value);});
 api.blacktopPresentationStatus().then(render).catch(e=>{render({options:confirmed,phase:'error',message:e.message});});
 for(const input of Object.values(inputs))input.addEventListener('change',async()=>{
  pending=true;for(const item of Object.values(inputs))item.disabled=true;
  try{render(await api.setBlacktopPresentation(Object.fromEntries(Object.entries(inputs).map(([key,item])=>[key,item.checked]))));}
  catch(e){render({options:confirmed,phase:'error',message:e.message});}
  finally{pending=false;for(const item of Object.values(inputs))item.disabled=false;}
 });
})();
