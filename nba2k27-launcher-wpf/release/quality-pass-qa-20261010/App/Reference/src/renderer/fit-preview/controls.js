// Keep exit controls usable even if WebGL or the renderer module fails.
(() => {
 const close=()=>parent.postMessage({type:'fit-preview:close'},'*');
 document.getElementById('close').onclick=close;
 window.addEventListener('keydown',e=>{if(e.key==='Escape'){e.preventDefault();close();}});
 window.addEventListener('error',()=>{
  document.querySelector('.spinner').hidden=true;document.getElementById('loadingText').hidden=true;
  const error=document.getElementById('error');error.hidden=false;error.textContent='The 3D renderer could not start. You can close this preview and continue using the launcher.';
 });
})();
