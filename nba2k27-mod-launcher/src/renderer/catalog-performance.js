(()=>{
 const api=window.modLauncher;if(!api?.recordPerformance||api.__previewOnly)return;
 const targets={ballMenuButton:['ball','body.ball-catalog-open .ball-picker.is-open'],scoreboardMenuButton:['scoreboard','.scoreboard-catalog.is-open'],arenaMenuButton:['arena','.arena-options'],editBlacktopPlayer:['outfit','body[data-overlay-view="editBlacktopPlayer"] .wardrobe-catalog-heading'],openBlacktopRules:['rules','#blacktopRulesPane']};
 let sequence=0;
 document.addEventListener('click',event=>{
  const button=event.target.closest('button'),target=button&&targets[button.id];if(!target)return;
  const token=++sequence;if(button.disabled||button.getAttribute('aria-expanded')==='true'||button.getAttribute('aria-pressed')==='true')return;
  const start=performance.now(),deadline=start+10000;
  function poll(){if(token!==sequence||performance.now()>deadline)return;
   const node=document.querySelector(target[1]);if(node&&!node.hidden&&node.getBoundingClientRect().height>0){requestAnimationFrame(()=>{if(token===sequence&&node.isConnected&&node.getBoundingClientRect().height>0)api.recordPerformance('catalog.'+target[0],performance.now()-start).catch(()=>{});});return;}
   requestAnimationFrame(poll);
  }
  requestAnimationFrame(poll);
 },true);
 document.addEventListener('keydown',e=>{if(e.key==='Escape')sequence++;});
})();
