(() => {
  const source=document.getElementById('blacktopPlayerSelect');
  if(!source)return;
  source.hidden=true;
  const button=document.createElement('button');button.id='blacktopPlayerMenuButton';button.type='button';
  button.setAttribute('aria-haspopup','menu');button.setAttribute('aria-expanded','false');
  source.after(button);
  const menu=document.createElement('div');menu.id='blacktopPlayerMenu';menu.hidden=true;menu.setAttribute('role','menu');
  button.setAttribute('aria-controls',menu.id);document.body.append(menu);
  let reopenRandom=false;
  function close(resize=true){
    if(menu.hidden)return;
    menu.hidden=true;button.setAttribute('aria-expanded','false');document.body.classList.remove('player-menu-open');
    if(resize)window.modLauncher.sizeBlacktopPlayerMenu?.(0).catch(()=>{});
    reopenRandom=false;
  }
  function sync(){
    button.disabled=source.disabled||!source.options.length;
    button.textContent=(source.selectedOptions[0]?.textContent||'Select player')+' ▾';
    menu.replaceChildren();
    for(const option of source.options){
      const item=document.createElement('button');item.type='button';item.textContent=option.textContent;
      item.setAttribute('role','menuitemradio');item.setAttribute('aria-checked',String(option.selected));
      item.onclick=()=>{if(source.disabled)return;const reopen=reopenRandom;source.value=option.value;source.dispatchEvent(new Event('change'));sync();close(!reopen);if(reopen)document.getElementById('blacktopRandomPlayer')?.click();};
      menu.append(item);
    }
    if(button.disabled)close();
  }
  button.onclick=()=>{
    if(!menu.hidden){close();return;}
    // Player selection replaces the random-mode row. Closing without an
    // intermediate resize avoids stacking the player list below it.
    const randomMenu=document.querySelector('.blacktop-random-menu');reopenRandom=Boolean(randomMenu);randomMenu?._close?.(false);
    sync();if(button.disabled)return;
    menu.style.top=Math.ceil(button.closest('header').getBoundingClientRect().bottom)+'px';
    menu.hidden=false;button.setAttribute('aria-expanded','true');document.body.classList.add('player-menu-open');
    window.modLauncher.sizeBlacktopPlayerMenu?.(Math.ceil(source.options.length/2)).catch(()=>{});
  };
  new MutationObserver(sync).observe(source,{childList:true,subtree:true,attributes:true});
  source.addEventListener('change',sync);
  source.addEventListener('blacktop-player-selection',sync);
  document.addEventListener('pointerdown',event=>{if(!menu.contains(event.target)&&!button.contains(event.target))close();});
  window.modLauncher.onBlacktopPanelShown?.(close);
  window.modLauncher.onBlacktopPanelCollapsed?.(close);
  sync();
})();
