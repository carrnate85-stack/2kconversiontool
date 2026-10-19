(() => {
 if(!document.documentElement.classList.contains('roster-workspace'))return;
 document.title='2k Roster Editor';
 const switcher=document.getElementById('rosterSectionSwitch');
 const activate=section=>{
   if((state.editingPlayer||state.editingTeam)&&!document.querySelector(`#page-${section}`)?.classList.contains('active')){toast('Close the current editor before switching sections.',true);return;}
   const safe=section==='teams'?'teams':'players';
   document.querySelectorAll('.page').forEach(page=>page.classList.toggle('active',page.id===`page-${safe}`));
   document.querySelectorAll('.nav-item').forEach(item=>item.classList.toggle('active',item.dataset.page===safe));
   switcher.querySelectorAll('button').forEach(item=>item.setAttribute('aria-current',item.dataset.section===safe?'page':'false'));
   $('pageTitle').textContent=safe==='teams'?'Team Editor':'Player Editor';
   const url=new URL(location.href);url.searchParams.set('section',safe);history.replaceState(null,'',url.pathname+url.search);
   if(['players','teams'].includes(safe)&&state.playerResearch?.status==='idle')refreshPlayerResearch();
 };
 for(const item of switcher.querySelectorAll('.roster-section-button'))item.onclick=()=>activate(item.dataset.section);
 const button=document.getElementById('rosterLauncherButton');
 button.onclick=()=>window.modLauncher.openLauncher().catch(error=>toast(error.message,true));
 const initial=new URLSearchParams(location.search).get('section')==='teams'?'teams':'players';
 switcher.querySelectorAll('button').forEach(item=>item.setAttribute('aria-current',item.dataset.section===initial?'page':'false'));
 document.querySelector('#appStartup strong').textContent='Loading Roster Editor…';
 document.getElementById('appStartupMessage').textContent='Opening the player workspace.';
 // One startup attachment, not a periodic roster redraw. Preserve open edits.
 if(document.documentElement.classList.contains('roster-browser')) {
   const notice=document.getElementById('rosterAutoAttach');notice.dataset.state='waiting';
   notice.textContent='Waiting for NBA 2K27. The roster will load automatically.';
   async function attach() {
     if(state.editingPlayer||state.editingTeam||state.playerResearch?.status==='scanning'||!$('appStartup').classList.contains('hidden')) {
       setTimeout(attach,5000);return;
     }
     try {
       const result=await window.modLauncher.rosterAutoAttach();
       if(result.status==='ready') {
       if(state.editingPlayer||state.editingTeam){setTimeout(attach,5000);return;}
         renderPlayerResearch(result);notice.dataset.state='ready';notice.textContent='Live roster ready.';return;
       }
       notice.dataset.state=result.status||'waiting';notice.textContent=result.message;
     } catch { notice.dataset.state='waiting';notice.textContent='Waiting for the launcher connection. Keep the launcher open.'; }
     setTimeout(attach,5000);
   }
   setTimeout(attach,500);
 }
 // Let the browser show its standard unsaved-work prompt on close or refresh.
 window.addEventListener('beforeunload',event=>{
   if(!state.editingTeam&&(!state.editingPlayer||!window.hasUnsavedPlayerChanges?.()))return;
   event.preventDefault();event.returnValue='';
 });
})();
