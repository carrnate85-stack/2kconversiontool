(()=>{
 const api=window.modLauncher,button=document.getElementById('arenaMenuButton');if(!button)return;
 const menu=document.createElement('section');menu.id='arenaMenu';menu.hidden=true;
 const search=document.createElement('input');search.type='search';search.placeholder='Search arenas';search.setAttribute('aria-label','Search arenas');
 const list=document.createElement('div');list.className='arena-options';
 const browser=window.ArenaBrowser,filters=document.createElement('div');filters.className='arena-filters';filters.setAttribute('role','group');filters.setAttribute('aria-label','Arena groups');
 const count=document.createElement('small');count.setAttribute('role','status');
 const note=document.createElement('small');note.setAttribute('role','status');menu.append(search,filters,count,list,note);document.body.append(menu);
 let state,busy=false,category='All',favorites=[],favoritesReady=false,favoriteError='',favoriteBusy=new Set();button.setAttribute('aria-expanded','false');button.setAttribute('aria-controls','arenaMenu');
 for(const label of ['All','Favorites',...browser.groups]){const tab=document.createElement('button');tab.type='button';tab.textContent=label==='Favorites'?'★ Favorites':label;tab.dataset.category=label;tab.onclick=()=>{category=label;list.scrollTop=0;draw();};filters.append(tab);}
 function close(){menu.hidden=true;button.setAttribute('aria-expanded','false');document.body.classList.remove('arena-menu-open');api.sizeBlacktopPlayerMenu?.(0).catch(()=>{});}
 function render(value){state=value;const c=value.choices.find(x=>x.id===value.selected);button.textContent=(c?.label||'Default')+' ▾';button.dataset.phase=value.phase;button.setAttribute('aria-label','Arena: '+button.textContent+'. '+value.message);button.title=value.message;note.textContent=value.message;draw();}
 function draw(){if(!state)return;
  for(const tab of filters.children)tab.setAttribute('aria-pressed',String(tab.dataset.category===category));
  const scroll=list.scrollTop;list.replaceChildren();const matches=browser.filter(state.choices,{query:search.value,category,favorites});count.textContent=matches.length+' arena'+(matches.length===1?'':'s');note.textContent=favoriteError||state.message;
  if(!matches.length){const empty=document.createElement('div');empty.className='arena-empty';const text=document.createElement('span');text.textContent=category==='Favorites'&&!favorites.length?'Star an arena to find it here.':'No matching arenas.';const reset=document.createElement('button');reset.type='button';reset.textContent='Show all arenas';reset.onclick=()=>{search.value='';category='All';draw();};empty.append(text,reset);list.append(empty);}
  for(const c of matches){const row=document.createElement('div');row.className='arena-option';const item=document.createElement('button');item.type='button';item.className='arena-choice';item.dataset.arena=c.id;
   const label=document.createElement('span');label.textContent=c.label+(c.disabled?' · Needs work':'');const group=document.createElement('small');group.textContent=browser.group(c);item.append(label,group);item.disabled=c.disabled||busy;item.title=c.caution||c.file||'Uses Day / Night';
   item.setAttribute('aria-pressed',String(c.id===state.selected));item.onclick=async()=>{if(busy)return;busy=true;draw();try{render(await api.selectArena(c.id));close();}catch(e){state={...state,message:e.message};}finally{busy=false;draw();}};
   const star=document.createElement('button');star.type='button';star.className='arena-star';const saved=favorites.includes(c.id);star.textContent=saved?'★':'☆';star.dataset.favorite=c.id;star.setAttribute('aria-pressed',String(saved));star.setAttribute('aria-label',(saved?'Remove '+c.label+' from':'Add '+c.label+' to')+' favorites');star.title=star.getAttribute('aria-label');star.disabled=!favoritesReady||favoriteBusy.has(c.id);
   star.onclick=async()=>{if(favoriteBusy.has(c.id))return;favoriteBusy.add(c.id);draw();try{favorites=await api.setArenaFavorite(c.id,!saved);favoriteError='';}catch(e){favoriteError=e.message||'Could not save favorite.';}finally{favoriteBusy.delete(c.id);draw();list.querySelector('[data-favorite="'+c.id+'"]')?.focus();}};
   row.append(item,star);list.append(row);
  }list.scrollTop=scroll;
 }
 button.onclick=()=>{if(!menu.hidden){close();return;}if(!state)return;menu.hidden=false;button.setAttribute('aria-expanded','true');document.body.classList.add('arena-menu-open');
  menu.style.top=Math.ceil(button.closest('header').getBoundingClientRect().bottom)+'px';api.sizeBlacktopPlayerMenu?.(7).catch(()=>{});draw();};
 search.placeholder='Search team, court or arena file…';search.addEventListener('input',()=>{list.scrollTop=0;draw();});document.addEventListener('pointerdown',e=>{if(!menu.hidden&&!menu.contains(e.target)&&!button.contains(e.target))close();});
 document.addEventListener('keydown',e=>{if(e.key==='Escape'&&!menu.hidden)close();});
 api.onBlacktopPanelShown?.(close);api.onBlacktopPanelCollapsed?.(close);api.onArenaStatus(render);api.arenaStatus().then(render).catch(()=>{button.textContent='Arena unavailable';});
 api.arenaFavorites().then(ids=>{favorites=ids;favoritesReady=true;draw();}).catch(e=>{favoriteError=e.message||'Could not load favorites.';draw();});
})();
