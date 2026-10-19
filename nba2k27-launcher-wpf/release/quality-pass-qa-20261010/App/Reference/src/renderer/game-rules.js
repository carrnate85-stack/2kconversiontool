'use strict';
(()=>{
 const input=document.getElementById('enableGameRules'),status=document.getElementById('gameRulesStatus');
 if(!input||!window.modLauncher.gameRulesStatus)return;let confirmed=false;
 function render(value){confirmed=value.enabled===true;input.checked=confirmed;status.textContent=value.message;}
 window.modLauncher.onGameRulesStatus(render);window.modLauncher.gameRulesStatus().then(render).catch(error=>{status.textContent=error.message;});
 input.addEventListener('change',async()=>{input.disabled=true;try{render(await window.modLauncher.setGameRules(input.checked));}catch(error){input.checked=confirmed;status.textContent=error.message;}finally{input.disabled=false;}});
})();
