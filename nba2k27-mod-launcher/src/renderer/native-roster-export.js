(() => {
 const list=document.getElementById('listNativeRosters'),select=document.getElementById('nativeRosterSlot'),save=document.getElementById('exportNativeRoster'),status=document.getElementById('nativeRosterExportStatus');
 if(!list)return;
 let busy=false;
 const controls=()=>{list.disabled=busy;select.disabled=busy||select.options.length===0||!select.options[0].value;save.disabled=busy||!select.value;};
 select.addEventListener('change',controls);
 list.addEventListener('click',async()=>{
  if(busy)return;busy=true;select.replaceChildren();controls();status.textContent='Reading saved NBA rosters… Keep the game at the main menu.';
  try{
   const rows=await window.modLauncher.nativeRosterExport({action:'list'});
   for(const row of rows){const option=document.createElement('option');option.value=String(row.slot);option.textContent=row.name||`Roster ${row.slot}`;select.append(option);}
   status.textContent=rows.length?'Choose one saved roster to export. Unsaved changes are not included.':'No saved NBA rosters found. Save a roster in-game, then try again.';
  }catch(error){status.textContent=error.message;}finally{busy=false;controls();}
 });
 save.addEventListener('click',async()=>{
  if(busy||!select.value)return;busy=true;controls();status.textContent='Exporting the saved roster… The save dialog opens in the launcher.';
  try{const result=await window.modLauncher.nativeRosterExport({action:'export',slot:Number(select.value)});
   status.textContent=result?`Exported ${result.name} (${(result.bytes/1048576).toFixed(2)} MB). Native import remains unavailable; cross-account compatibility is unverified.`:'Export canceled.';
  }catch(error){status.textContent=error.message;}finally{busy=false;controls();}
 });
})();
