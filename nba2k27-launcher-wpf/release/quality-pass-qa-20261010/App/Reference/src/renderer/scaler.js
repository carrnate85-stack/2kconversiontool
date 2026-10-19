(() => {
 if(document.documentElement?.classList?.contains('roster-workspace'))return;
 const $=id=>document.getElementById(id),api=window.modLauncher;
 const slider=$('scalerPercent'),enabled=$('scalerEnabled'),output=$('scalerValue'),status=$('scalerStatus');
 const groups={coaches:$('scalerCoaches'),cheerleaders:$('scalerCheerleaders'),arenaPersonnel:$('scalerArenaPersonnel')},groupInputs=Object.values(groups);
 let dirty=false,busy=false;
 const retry=$('scalerApply'),buttons=[$('scalerSave'),$('scalerReset'),retry];
 function payload(percent=Number(slider.value)){return {percent,enabled:enabled.checked,...Object.fromEntries(Object.entries(groups).map(([key,input])=>[key,input.checked]))};}
 function controls(){enabled.disabled=busy;slider.disabled=busy||!enabled.checked;groupInputs.forEach(input=>input.disabled=busy||!enabled.checked);buttons.forEach(button=>button.disabled=busy);}
 function paint(s){if(!dirty&&!busy){slider.value=s.percent;enabled.checked=s.enabled;for(const [key,input] of Object.entries(groups))input.checked=s[key]??true;output.textContent=s.percent+'%';}retry.hidden=s.phase!=='timeout';status.textContent=s.message;status.dataset.phase=s.phase;controls();}
 async function run(fn){busy=true;controls();try{const s=await fn();dirty=false;busy=false;paint(s);}catch(e){status.textContent='Could not save character size. Please try again.';}finally{busy=false;controls();}}
 slider.addEventListener('input',()=>{dirty=true;output.textContent=slider.value+'%';});enabled.addEventListener('change',()=>{dirty=true;controls();return run(()=>api.saveScaler(payload()));});
 for(const input of groupInputs)input.addEventListener('change',()=>{dirty=true;return run(()=>api.saveScaler(payload()));});
 $('scalerSave').addEventListener('click',()=>run(()=>api.saveScaler(payload())));
 $('scalerReset').addEventListener('click',()=>run(()=>api.saveScaler(payload(100))));
 $('scalerApply').addEventListener('click',()=>run(()=>api.applyScaler()));
 api.onScalerStatus(paint);api.scalerStatus().then(paint).catch(()=>{status.textContent='Scaler is not ready yet.';});
})();
