import './model-preview.js';
function fail(message){document.querySelector('.spinner').hidden=true;document.getElementById('loadingText').hidden=true;const e=document.getElementById('error');e.hidden=false;e.textContent=message;}
window.addEventListener('message',async e=>{
 if(e.source!==parent)return;
 if(e.data?.type==='fit-preview:error'){fail(e.data.message);return;}
 if(e.data?.type!=='fit-preview:result')return;
 try{
  await window.modelPreview.load(e.data.payload);
  document.getElementById('loading').hidden=true;document.getElementById('reset').disabled=false;
  const warnings=e.data.payload.warnings||[];
  const notes=document.getElementById('notes');notes.hidden=!warnings.length;
  if(warnings.length){notes.querySelector('summary').textContent='Preview limitations · '+warnings.length;notes.open=true;for(const text of warnings){const li=document.createElement('li');li.textContent=text;notes.querySelector('ul').append(li);}}
  window.modelPreview.resize();document.body.dataset.ready='true';
 }catch(error){fail('Could not render this outfit: '+error.message);}
});
document.getElementById('reset').onclick=()=>window.modelPreview.fitCamera();
parent.postMessage({type:'fit-preview:ready'},'*');
