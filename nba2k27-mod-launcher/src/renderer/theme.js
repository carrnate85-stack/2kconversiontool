(() => {
  if(new URLSearchParams(location.search).get('workspace')==='roster') { document.documentElement.dataset.theme='dark';document.documentElement.classList.add('roster-workspace');return; }
  const key='mod-launcher-theme';
  let current='dark';
  try { if(localStorage.getItem(key)==='light')current='light'; } catch {}
  const apply=()=>{document.documentElement.dataset.theme=current;};
  apply();
  document.addEventListener('DOMContentLoaded',()=>{
    const sidebar=document.querySelector('.app-shell > .sidebar');
    if(!sidebar)return;
    const group=document.createElement('div');group.className='theme-switch';
    group.setAttribute('role','group');group.setAttribute('aria-label','Appearance');
    const buttons=['light','dark'].map(theme=>{
      const button=document.createElement('button');button.type='button';button.className='theme-choice';
      button.setAttribute('aria-label',theme==='light'?'Light theme':'Dark theme');
      button.innerHTML=theme==='light'
        ? '<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="4"/><path d="M12 2v2m0 16v2M2 12h2m16 0h2M5 5l1.5 1.5m11 11L19 19M5 19l1.5-1.5m11-11L19 5"/></svg><span>Light</span>'
        : '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M20 15a9 9 0 0 1-11-11 9 9 0 1 0 11 11Z"/></svg><span>Dark</span>';
      button.onclick=()=>{current=theme;apply();label();try{localStorage.setItem(key,current);}catch{}};
      group.append(button);return button;
    });
    const label=()=>buttons.forEach((button,index)=>button.setAttribute('aria-pressed',String(current===['light','dark'][index])));
    sidebar.append(group);label();
    window.addEventListener('storage',e=>{if(e.key===key){current=e.newValue==='light'?'light':'dark';apply();label();}});
  });
})();
