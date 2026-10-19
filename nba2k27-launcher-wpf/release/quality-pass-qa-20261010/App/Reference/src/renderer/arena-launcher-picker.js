(()=>{
 const api=window.modLauncher,select=document.getElementById('arenaSelect'),favorite=document.getElementById('arenaFavorite');
 const status=document.getElementById('arenaStatus'),retry=document.getElementById('arenaRetry'),browser=window.ArenaBrowser;
 if(!select||!favorite||!api.arenaStatus||!browser)return;
 let state,favorites=[],favoritesReady=false,busy=false;
 const selectedChoice=()=>state?.choices.find(choice=>choice.id===state.selected);
 function paintStatus(phase,message){
  const short=phase==='pending'?'Waiting for Blacktop':phase==='applied'?'Active':phase==='idle'?'Choose for Blacktop':message;
  status.textContent=short||'Ready';status.dataset.phase=phase||'idle';status.title=message||short;retry.hidden=phase!=='error';
 }
 function rebuild(){
  if(!state)return;const previous=state.selected;select.replaceChildren();
  const appendGroup=(label,rows)=>{if(!rows.length)return;const group=document.createElement('optgroup');group.label=label;
   for(const choice of rows){const option=new Option(choice.label,choice.id);option.disabled=choice.disabled===true;group.append(option);}select.append(group);};
  const starred=browser.filter(state.choices,{category:'Favorites',favorites});appendGroup('★ Favorites',starred);
  for(const group of browser.groups)appendGroup(group,browser.filter(state.choices,{category:group}));
  select.value=previous;select.disabled=busy;
  const current=selectedChoice(),saved=favorites.includes(previous);favorite.disabled=busy||!favoritesReady||!current;
  favorite.innerHTML=`<span aria-hidden="true">${saved?'★':'☆'}</span>`;favorite.setAttribute('aria-pressed',String(saved));
  favorite.setAttribute('aria-label',saved?'Remove selected arena from favorites':'Add selected arena to favorites');
  favorite.title=saved?'Remove from favorites':'Add to favorites';
  favorite.setAttribute('aria-label',(saved?'Remove ':'Add ')+(current?.label||'selected arena')+(saved?' from favorites':' to favorites'));
  favorite.title=favorite.getAttribute('aria-label');
 }
 function render(value){state=value;paintStatus(value.phase,value.message);rebuild();}
 async function choose(id){if(busy||!id)return;busy=true;rebuild();try{render(await api.selectArena(id));}catch(error){paintStatus('error',error.message||'Could not select arena.');}finally{busy=false;rebuild();}}
 select.addEventListener('change',()=>choose(select.value));retry.addEventListener('click',()=>choose(state?.selected));
 favorite.addEventListener('click',async()=>{const current=selectedChoice();if(busy||!current||!favoritesReady)return;busy=true;rebuild();
  try{const saved=favorites.includes(current.id);favorites=await api.setArenaFavorite(current.id,!saved);rebuild();}
  catch(error){paintStatus('error',error.message||'Could not save favorite.');}finally{busy=false;rebuild();}
 });
 api.onArenaStatus(render);
 Promise.all([api.arenaStatus(),api.arenaFavorites()]).then(([value,saved])=>{favorites=saved;favoritesReady=true;render(value);})
  .catch(error=>paintStatus('error',error.message||'Arena selector unavailable.'));
})();
