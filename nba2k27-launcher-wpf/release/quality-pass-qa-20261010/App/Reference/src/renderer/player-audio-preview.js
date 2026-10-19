(() => {
 const modal=document.getElementById('playerEditorModal'),details=document.getElementById('playerAudioDetails');
 const status=document.createElement('small');status.setAttribute('role','status');status.style.cssText='display:block;margin:8px 0';details.querySelector('summary').after(status);
 const audio=new Audio();audio.volume=0.35;let generation=0,loading=false,playingButton=null;
 const buttons=[];
 function stop(){generation++;audio.pause();audio.removeAttribute('src');audio.load();if(playingButton)playingButton.textContent='▶';playingButton=null;}
 function update(){for(const {input,button} of buttons){const value=input.value.trim();button.disabled=loading||value===''||!/^\d+$/.test(value)||Number(value)>32767;}}
 for(const [id,bank] of [['playerAudioFullName','auto'],['playerAudioFirstCommentary','commentary'],['playerAudioFirstPa','pa'],['playerAudioLastCommentary','commentary'],['playerAudioLastPa','pa']]){
  const input=document.getElementById(id),row=document.createElement('span');row.style.cssText='display:flex;gap:6px;align-items:center';input.before(row);row.append(input);input.style.minWidth='0';input.style.flex='1';
  const button=document.createElement('button');button.type='button';button.className='secondary';button.textContent='▶';button.title=button.ariaLabel='Preview '+input.closest('label').firstChild.textContent.trim();row.append(button);buttons.push({input,button});input.addEventListener('input',()=>{stop();update();});
  button.addEventListener('click',async()=>{
   if(playingButton===button){stop();status.textContent='';return;}
   stop();const token=generation;loading=true;update();status.textContent='Loading stock audio…';
   try{const result=await window.modLauncher.previewPlayerAudio({id:Number(input.value),bank});if(token!==generation||modal.classList.contains('hidden'))return;
    audio.src='data:audio/ogg;base64,'+result.base64;await audio.play();if(token!==generation){audio.pause();return;}playingButton=button;button.textContent='■';status.textContent=(result.bank==='pa'?'PA announcer':'Commentary')+' · Stock English preview; game delivery may differ.';
   }catch(error){if(token===generation){status.textContent=String(error.message||'Preview unavailable.').replace(/^Error invoking remote method '[^']+': (?:Error: )?/,'');if(typeof toast==='function')toast(status.textContent,true);}}
   finally{loading=false;update();}
  });
 }
 audio.addEventListener('ended',()=>{if(playingButton)playingButton.textContent='▶';playingButton=null;});
 new MutationObserver(()=>{stop();status.textContent='';update();}).observe(modal,{attributes:true,attributeFilter:['class']});
 new MutationObserver(()=>{stop();status.textContent='';update();}).observe(document.getElementById('playerEditTitle'),{childList:true,characterData:true,subtree:true});
 window.addEventListener('beforeunload',stop);update();
})();
