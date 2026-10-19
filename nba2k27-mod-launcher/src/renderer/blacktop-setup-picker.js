(()=>{
 const api=window.modLauncher,browser=window.ArenaBrowser;
 const buttons={scoreboard:document.getElementById('scoreboardCatalogButton'),arena:document.getElementById('arenaCatalogButton'),ball:document.getElementById('ballCatalogButton')};
 if(!buttons.scoreboard||!buttons.arena||!buttons.ball||!browser)return;
 const labels={scoreboard:document.getElementById('scoreboardSelected'),arena:document.getElementById('arenaSelected'),ball:document.getElementById('ballSelected')};
 const statuses={scoreboard:document.getElementById('scoreboardStatus'),arena:document.getElementById('arenaStatus'),ball:document.getElementById('ballStatus')};
 const selects={scoreboard:document.getElementById('scoreboardSelect'),arena:document.getElementById('arenaSelect')};
 const states={scoreboard:null,arena:null,ball:null};let favorites=[],favoritesReady=false,active=null,dialog=null,query='',category='All',page=0,busy=false,generation=0;
 const make=(tag,text,cls)=>{const element=document.createElement(tag);if(text!==undefined)element.textContent=text;if(cls)element.className=cls;return element;};
 const choice=kind=>states[kind]?.choices?.find(item=>item.id===states[kind].selected);
 const sameChoices=(left,right)=>left===right||JSON.stringify(left)===JSON.stringify(right);
 const shortStatus=(kind,state)=>{
  if(state.phase==='error')return state.message||'Unavailable';
  if(state.phase==='unavailable')return 'Not available for live swap';
  if(state.phase==='incompatible')return 'Stock ball changed; swap paused';
  if(state.phase==='pending')return kind==='ball'?'Waiting for safe selection':'Waiting for Blacktop';
  if(state.phase==='loading')return 'Waiting for ball load';
  if(state.phase==='applied')return kind==='ball'?'Ball loaded':'Active';
  if(state.phase==='restoring')return 'Restoring original ball';
  if(state.phase==='restored')return 'Original ball restored';
  if(state.phase==='current')return 'Current installed ball';
  if(state.phase==='saved')return kind==='ball'?'Saved for next launch':'Saved';
  if(state.phase==='ready')return kind==='ball'?'Ready for Blacktop':'Ready';
  return kind==='scoreboard'?'Stock scoreboard':kind==='arena'?'Choose for Blacktop':'Ready';
 };
 function group(kind,item){
  if(kind==='arena')return browser.group(item);
  if(kind==='ball')return item.group||'Other';
  if(item.tested===false)return 'Experimental';
  if(item.id?.startsWith('era-'))return 'Eras';
  if(item.nativeController==='SCOREBUG_MAIN_GAME')return 'Broadcast';
  return 'Classic';
 }
 function categories(kind){
  const values=[...new Set((states[kind]?.choices||[]).map(item=>group(kind,item)))];
  const order=kind==='arena'?['Favorites',...browser.groups]:kind==='scoreboard'?['Classic','Broadcast','Eras','Experimental']:values.sort();
  return ['All',...order.filter(value=>value==='Favorites'||values.includes(value))];
 }
 function renderRow(kind,{rebuildChoices=true,redrawCatalog=true,preserveCatalog=false}={}){
  const state=states[kind];if(!state)return;const selected=choice(kind);labels[kind].textContent=selected?.label||'Unavailable';labels[kind].title=selected?.label||'';
  const status=shortStatus(kind,state);statuses[kind].textContent=status;statuses[kind].dataset.phase=state.phase||'idle';statuses[kind].title=state.message||status;buttons[kind].disabled=!state.choices?.length;
  const select=selects[kind];if(select){if(rebuildChoices){select.replaceChildren();for(const item of state.choices||[]){const option=make('option',item.label);option.value=item.id;option.disabled=item.disabled===true;select.append(option);}}select.value=state.selected;}
  if(dialog?.open&&active===kind){
   if(redrawCatalog)draw({preserveView:preserveCatalog});
   else {const message=dialog.querySelector('.blacktop-catalog-footer > span');if(message)message.textContent=state.message||'';}
  }
 }
 function merge(kind,value){
  const previous=states[kind];states[kind]={...previous,...value,choices:value.choices||previous?.choices||[]};
  const rebuildChoices=!sameChoices(previous?.choices,states[kind].choices);
  renderRow(kind,{rebuildChoices,redrawCatalog:rebuildChoices||previous?.selected!==states[kind].selected,preserveCatalog:true});
 }
 async function toggleFavorite(id){if(!favoritesReady)return;const saved=favorites.includes(id);favoritesReady=false;renderRow('arena');try{favorites=await api.setArenaFavorite(id,!saved);}finally{favoritesReady=true;renderRow('arena');}}
 async function choose(kind,id){if(busy)return;busy=true;const state=states[kind],selected=state.choices.find(item=>item.id===id);merge(kind,{selected:id,phase:'pending',message:`Saving ${selected?.label||'selection'}…`});dialog?.close();try{const method=kind==='scoreboard'?'selectScoreboard':kind==='arena'?'selectArena':'selectBall';merge(kind,await api[method](id));}catch(error){merge(kind,{selected:state.selected,phase:'error',message:error.message||'Could not save selection.'});}finally{busy=false;}}
 function filtered(){
  const state=states[active],terms=query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  return (state?.choices||[]).filter(item=>{
   const itemGroup=group(active,item),categoryMatch=category==='All'||(category==='Favorites'?favorites.includes(item.id):itemGroup===category);
   return categoryMatch&&terms.every(term=>`${item.label} ${item.id} ${itemGroup} ${item.resource||''} ${item.file||''}`.toLowerCase().includes(term));
  });
 }
 function draw({preserveView=false}={}){
  if(!dialog?.open||!active)return;
  const oldGrid=preserveView?dialog.querySelector('.blacktop-catalog-grid'):null;
  const scrollTop=oldGrid?.scrollTop||0;
  const searchWasFocused=preserveView&&document.activeElement===dialog.querySelector('.blacktop-catalog-controls > input');
  dialog.dataset.catalogKind=active;const token=++generation,header=make('header',undefined,'blacktop-catalog-header'),identity=make('div'),eyebrow=make('small','BLACKTOP PRESENTATION'),title=make('h2',active[0].toUpperCase()+active.slice(1)+' catalog'),close=make('button','Close');close.type='button';close.onclick=()=>dialog.close();identity.append(eyebrow,title);header.append(identity,close);
  const controls=make('section',undefined,'blacktop-catalog-controls'),search=make('input');search.type='search';search.placeholder=`Search ${active} catalog`;search.value=query;search.setAttribute('aria-label',search.placeholder);const filters=make('div',undefined,'blacktop-catalog-filters');
  for(const name of categories(active)){const filter=make('button',name==='Favorites'?'★ Favorites':name);filter.type='button';filter.setAttribute('aria-pressed',String(name===category));filter.onclick=()=>{category=name;page=0;draw();};filters.append(filter);}controls.append(search,filters);
  const matches=filtered(),pageSize=24,pages=Math.max(1,Math.ceil(matches.length/pageSize));page=Math.min(page,pages-1);const grid=make('section',undefined,'blacktop-catalog-grid'),previewJobs=[];
  for(const item of matches.slice(page*pageSize,page*pageSize+pageSize)){
   const entry=make('div',undefined,'blacktop-catalog-entry'),card=make('button',undefined,'blacktop-catalog-item');card.type='button';card.disabled=busy||item.disabled===true;card.setAttribute('aria-pressed',String(item.id===states[active].selected));
   if(active==='scoreboard'){
    const detail=window.ScoreboardPreviewCatalog?.[item.id],frame=make('span',undefined,'blacktop-scoreboard-preview');
    if(detail?.image){const image=make('img');image.src=detail.image;image.alt='';image.loading='lazy';image.decoding='async';frame.append(image);}
    else frame.append(make('span','PREVIEW UNAVAILABLE','blacktop-scoreboard-preview-empty'));
    card.append(frame);
   }
   card.append(make('strong',item.label),make('small',group(active,item)));
   if(active==='scoreboard')card.append(make('small',window.ScoreboardPreviewCatalog?.[item.id]?.caption||'Preview unavailable','blacktop-scoreboard-caption'));
   card.onclick=()=>choose(active,item.id);entry.append(card);
   if(active==='ball'){const preview=make('span',item.id==='current'?'CURRENT':'●','blacktop-catalog-ball');card.prepend(preview);
    if(item.thumbnail){const image=make('img');image.src=window.PreviewImageUrl('ball',item.thumbnail);image.alt='';image.loading='lazy';image.decoding='async';image.className='blacktop-catalog-preview';image.onerror=()=>{image.replaceWith(preview);api.ballThumbnail(item.id).then(result=>{if(token!==generation||!preview.isConnected)return;if(result?.state==='ready'&&/^data:image\/png;base64,/.test(result.dataUrl)){const fallback=make('img');fallback.src=result.dataUrl;fallback.alt='';fallback.className='blacktop-catalog-preview';preview.replaceWith(fallback);}}).catch(()=>{});};preview.replaceWith(image);}
    else if(item.preview)previewJobs.push({item,preview});if(item.renderNote)card.title=item.renderNote;}
   if(active==='arena'){const star=make('button',favorites.includes(item.id)?'★':'☆','blacktop-catalog-star');star.type='button';star.setAttribute('aria-pressed',String(favorites.includes(item.id)));star.setAttribute('aria-label','Toggle '+item.label+' favorite');star.disabled=!favoritesReady;star.onclick=event=>{event.stopPropagation();toggleFavorite(item.id);};entry.append(star);}grid.append(entry);
  }
  if(!matches.length)grid.append(make('div',category==='Favorites'?'Star an arena to find it here.':'No matching choices.','blacktop-catalog-empty'));
  const footer=make('footer',undefined,'blacktop-catalog-footer'),message=make('span',states[active].message||''),paging=make('div'),previous=make('button','Previous'),count=make('span',matches.length?`${page*pageSize+1}–${Math.min((page+1)*pageSize,matches.length)} of ${matches.length}`:'No results'),next=make('button','Next');previous.type=next.type='button';previous.disabled=page===0;next.disabled=page>=pages-1;previous.onclick=()=>{page--;draw();};next.onclick=()=>{page++;draw();};paging.append(previous,count,next);footer.append(message,paging);
  dialog.replaceChildren(header,controls,grid,footer);search.oninput=()=>{query=search.value;page=0;draw();};if(preserveView)grid.scrollTop=scrollTop;if(!preserveView||searchWasFocused)search.focus();
  for(const {item,preview} of previewJobs)api.ballThumbnail(item.id).then(result=>{if(token!==generation||!preview.isConnected)return;if(result?.state==='ready'&&/^data:image\/png;base64,/.test(result.dataUrl)){const image=make('img');image.src=result.dataUrl;image.alt='';image.className='blacktop-catalog-preview';preview.replaceWith(image);}}).catch(()=>{});
 }
 function open(kind){if(!states[kind]?.choices?.length)return;active=kind;query='';category='All';page=0;if(!dialog){dialog=make('dialog',undefined,'blacktop-catalog');dialog.addEventListener('close',()=>{active=null;generation++;});document.body.append(dialog);}dialog.dataset.catalogKind=kind;dialog.showModal();draw();}
 for(const kind of Object.keys(buttons))buttons[kind].onclick=()=>open(kind);
 api.onScoreboardStatus(value=>merge('scoreboard',value));api.onArenaStatus(value=>merge('arena',value));api.onBallStatus(value=>merge('ball',value));
 api.scoreboardStatus().then(value=>merge('scoreboard',value)).catch(error=>merge('scoreboard',{phase:'error',message:error.message}));
 Promise.all([api.arenaStatus(),api.arenaFavorites()]).then(([value,saved])=>{favorites=saved;favoritesReady=true;merge('arena',value);}).catch(error=>merge('arena',{phase:'error',message:error.message}));
 api.ballStatus().then(value=>merge('ball',value)).catch(error=>merge('ball',{phase:'error',message:error.message}));
})();
