'use strict';
(()=>{
 const api=window.modLauncher,input=document.getElementById('blacktopCrowdEnabled'),status=document.getElementById('blacktopCrowdStatus'),restore=document.getElementById('blacktopCrowdRestore');
 if(!input||!status||!restore||!api.blacktopCrowdStatus)return;
 let confirmed={enabled:true,canRestore:false,phase:'configured',message:''},pending=false;
 function render(value){confirmed=value;input.checked=value.enabled===false;const message=(value.message||'').trim();status.textContent=value.phase==='configured'||message==='Crowd control leaves the original setting unchanged.'||message==='Unlock helper stopped safely. Restart the game before retrying.'||message==='Game ended; memory-only crowd changes cleared.'?'':message;status.hidden=!status.textContent;restore.hidden=!value.canRestore;restore.disabled=pending;}
 api.onBlacktopCrowdStatus(value=>{if(!pending)render(value);});
 api.blacktopCrowdStatus().then(render).catch(e=>render({...confirmed,phase:'error',message:e.message}));
 async function save(enabled){pending=true;input.disabled=true;restore.disabled=true;try{render(await api.setBlacktopCrowdEnabled(enabled));}catch(e){render({...confirmed,phase:'error',message:e.message});}finally{pending=false;input.disabled=false;restore.disabled=false;}}
 input.addEventListener('change',()=>save(!input.checked));
 restore.addEventListener('click',()=>save(true));
})();
