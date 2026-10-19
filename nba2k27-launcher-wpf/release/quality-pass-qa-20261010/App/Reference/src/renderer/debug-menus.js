'use strict';
(()=>{
 const input=document.getElementById('enableDebugMenus'),status=document.getElementById('debugMenuStatus');
 if(!input||!window.modLauncher.debugMenuStatus)return;
 let confirmed=false;
 function render(value){confirmed=value.enabled===true;input.checked=confirmed;const message=(value.message||'').trim();status.textContent=message==='Unlock helper stopped safely. Restart the game before retrying.'?'':message;}
 window.modLauncher.onDebugMenuStatus(render);
 window.modLauncher.debugMenuStatus().then(render).catch(error=>{status.textContent=error.message;});
 input.addEventListener('change',async()=>{
  input.disabled=true;
  try{render(await window.modLauncher.setDebugMenus(input.checked));}
  catch(error){input.checked=confirmed;status.textContent=error.message;}
  finally{input.disabled=false;}
 });
})();
