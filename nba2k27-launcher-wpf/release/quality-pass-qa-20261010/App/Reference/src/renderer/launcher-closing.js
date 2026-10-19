(()=>{
 if(typeof window.modLauncher.onClosing!=='function')return;
 const dialog=document.createElement('dialog'),title=document.createElement('h2'),note=document.createElement('p');
 dialog.id='launcherClosing';dialog.setAttribute('aria-labelledby','launcherClosingTitle');title.id='launcherClosingTitle';
 Object.assign(dialog.style,{background:'#101c27',color:'#e5edf5',border:'1px solid #375163',borderRadius:'12px',padding:'24px',maxWidth:'440px'});
 title.textContent='Closing launcher…';note.textContent='Restoring session files and stopping helpers.';
 dialog.append(title,note);document.body.append(dialog);dialog.addEventListener('cancel',event=>event.preventDefault());
 window.modLauncher.onClosing(value=>{if(value?.active){title.textContent=value.message||'Closing launcher…';if(!dialog.open)dialog.showModal();}else if(dialog.open)dialog.close();});
})();
