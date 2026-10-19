(()=>{
 const api=window.modLauncher,$=id=>document.getElementById(id);if(!$('launcherUpdateCard'))return;
 let busy=false,pending=false;
 const notes=(id,values)=>$(id).replaceChildren(...(values||[]).map(value=>{const li=document.createElement('li');li.textContent=value;return li;}));
 const status=text=>{$('launcherUpdateStatus').textContent=text;};
 function controls(){for(const id of ['chooseLauncherUpdate','restorePreviousLauncher'])$(id).disabled=busy||pending||(id==='restorePreviousLauncher'&&!$('restorePreviousLauncher').dataset.version);$('restartLauncherUpdate').disabled=busy;$('cancelLauncherUpdate').disabled=busy;$('launcherUpdateReview').hidden=!pending;}
 window.refreshLauncherBuild=async()=>{try{
  const value=await api.launcherBuildInfo();$('launcherUpdateCard').hidden=false;
  const label=`${value.current.version} · ${value.current.channel}`;$('launcherBuildLabel').textContent=label;$('version').textContent=label;
  $('launcherBuildDate').textContent=value.current.builtAt?`Built ${new Date(value.current.builtAt).toLocaleDateString()}`:'';notes('launcherReleaseNotes',value.current.notes);
  $('restorePreviousLauncher').dataset.version=value.previous?.version||'';$('restorePreviousLauncher').textContent=value.previous?`Restore ${value.previous.version}`:'Restore previous version';
  if(!busy){pending=!!value.pending;if(value.choice){$('launcherUpdateChoice').textContent=`${value.choice.version} · ${value.choice.channel}`;notes('launcherUpdateNotes',value.choice.notes);}}
  if(!pending&&!busy&&value.result)status(value.result.message);controls();
 }catch{/* Non-WPF hosts do not expose the native update service. */}};
 async function prepare(action){if(busy||pending)return;busy=true;controls();status('Checking update…');$('launcherUpdateProgress').hidden=false;$('launcherUpdateProgress').removeAttribute('value');
  try{const choice=await action();if(!choice){status('Update selection canceled.');return;}pending=true;$('launcherUpdateChoice').textContent=`${choice.version} · ${choice.channel}`;notes('launcherUpdateNotes',choice.notes);status('Verified and ready. Install and restart when ready.');}
  catch(error){status(error.message);}finally{busy=false;$('launcherUpdateProgress').hidden=true;controls();}
 }
 $('chooseLauncherUpdate').onclick=()=>prepare(()=>api.chooseLauncherUpdate());$('restorePreviousLauncher').onclick=()=>prepare(()=>api.restorePreviousLauncher());
 $('cancelLauncherUpdate').onclick=async()=>{try{await api.cancelLauncherUpdate();pending=false;status('Update canceled.');controls();}catch(error){status(error.message);}};
 $('restartLauncherUpdate').onclick=async()=>{busy=true;controls();try{await api.restartLauncherUpdate();status('Finishing session cleanup, then installing and restarting…');}catch(error){status(error.message);}finally{busy=false;controls();}};
 api.onLauncherUpdateProgress?.(value=>{if(!busy)return;status(value.message);if(value.total){$('launcherUpdateProgress').max=value.total;$('launcherUpdateProgress').value=value.completed;}});
 window.refreshLauncherBuild();
})();
