(() => {
  const api=window.modLauncher,button=document.getElementById('scoreboardMenuButton');
  if(!button)return;
  let state=null,picker=null,query='',category='All',page=0,busy=false,openToken=0;
  const make=(tag,text,cls)=>{const element=document.createElement(tag);if(text!==undefined)element.textContent=text;if(cls)element.className=cls;return element;};
  const group=item=>item.tested===false?'Experimental':item.id?.startsWith('era-')?'Eras':item.nativeController==='SCOREBUG_MAIN_GAME'?'Broadcast':'Classic';
  const sameChoices=(left,right)=>left===right||JSON.stringify(left)===JSON.stringify(right);
  const selected=()=>state?.choices?.find(item=>item.id===state.selected);
  function paint(value){
    const previous=state;
    state={...state,...value,choices:value.choices||state?.choices||[]};
    const label=(selected()?.label||'Default').split(' · ')[0];
    button.textContent=`${label} ▾`;
    button.title=state.message||'Choose the Blacktop scoreboard';
    button.setAttribute('aria-label',`Scoreboard: ${label}. ${state.message||''}`);
    if(picker?.isConnected){
      if(previous?.selected!==state.selected||!sameChoices(previous?.choices,state.choices))render({preserveView:true});
      else {const message=picker.querySelector('.ball-picker-footer > span');if(message){message.textContent=state.message||'';message.dataset.phase=state.phase||'idle';}}
    }
  }
  function close(collapse=true){
    openToken++;
    const wasOpen=Boolean(picker?.isConnected||document.body.dataset.overlayView==='scoreboardMenuButton');
    picker?.remove();picker=null;
    document.body.classList.remove('scoreboard-catalog-open');
    button.setAttribute('aria-expanded','false');
    button.setAttribute('aria-busy','false');
    if(collapse&&wasOpen)window.BlacktopOverlay?.expand(false);
  }
  async function open(){
    if(!state?.choices?.length)return;
    if(picker||document.body.dataset.overlayView==='scoreboardMenuButton'){close();return;}
    const token=++openToken;
    const arena=document.getElementById('arenaMenuButton');
    if(arena?.getAttribute('aria-expanded')==='true')arena.click();
    button.setAttribute('aria-busy','true');
    const expansion=window.BlacktopOverlay?.expand(true,'scoreboardMenuButton');
    picker=make('section',undefined,'ball-picker scoreboard-catalog is-open');
    picker.setAttribute('role','dialog');picker.setAttribute('aria-labelledby','scoreboardPickerTitle');
    const preparedState=state;render();
    const expanded=await expansion;
    if(token!==openToken)return;
    if(expanded===false){close(false);window.BlacktopOverlay?.expand(false);return;}
    document.body.classList.add('scoreboard-catalog-open');
    button.setAttribute('aria-expanded','true');button.setAttribute('aria-busy','false');
    document.body.append(picker);
    if(preparedState!==state)render();else picker.querySelector('.ball-picker-controls > input')?.focus();
  }
  function choose(item){
    if(busy||api.__previewOnly)return;
    const prior=state;busy=true;
    paint({selected:item.id,phase:'pending',message:`Selecting ${item.label}…`});
    close();
    api.selectScoreboard(item.id).then(paint)
      .catch(error=>paint({selected:prior.selected,phase:'error',message:error.message||'Could not save scoreboard choice.'}))
      .finally(()=>{busy=false;if(picker?.isConnected)render({preserveView:true});});
  }
  function render({preserveView=false}={}){
    if(!picker||!state)return;
    const previousGrid=preserveView?picker.querySelector('.scoreboard-picker-grid'):null;
    const scrollTop=previousGrid?.scrollTop||0;
    const searchWasFocused=preserveView&&document.activeElement===picker.querySelector('.ball-picker-controls > input');
    const heading=make('header',undefined,'ball-picker-heading'),identity=make('div'),eyebrow=make('small','BLACKTOP'),title=make('h2','Scoreboard catalog');
    title.id='scoreboardPickerTitle';identity.append(eyebrow,title);
    const back=make('button','Back to game','ball-picker-back');back.type='button';back.onclick=()=>close();heading.append(identity,back);
    const controls=make('section',undefined,'ball-picker-controls'),search=make('input');search.type='search';search.value=query;
    search.placeholder='Search scoreboard names';search.setAttribute('aria-label','Search scoreboards');controls.append(search);
    const filters=make('div',undefined,'ball-picker-filters');filters.setAttribute('aria-label','Scoreboard categories');
    const groups=['All','Classic','Broadcast','Eras','Experimental'];
    for(const name of groups.filter(name=>name==='All'||state.choices.some(item=>group(item)===name))){
      const filter=make('button',name);filter.type='button';filter.setAttribute('aria-pressed',String(name===category));
      filter.onclick=()=>{category=name;page=0;render();};filters.append(filter);
    }
    controls.append(filters);
    const words=query.trim().toLowerCase().split(/\s+/).filter(Boolean);
    const matches=state.choices.filter(item=>(category==='All'||group(item)===category)&&words.every(word=>`${item.label} ${item.id} ${group(item)}`.toLowerCase().includes(word)));
    const pageSize=24,pages=Math.max(1,Math.ceil(matches.length/pageSize));page=Math.min(page,pages-1);
    const grid=make('section',undefined,'ball-picker-grid scoreboard-picker-grid');
    for(const item of matches.slice(page*pageSize,page*pageSize+pageSize)){
      const card=make('button',undefined,'ball-tile scoreboard-tile');card.type='button';card.disabled=busy||item.disabled===true||api.__previewOnly===true;
      card.setAttribute('aria-pressed',String(item.id===state.selected));card.title=item.label;
      const detail=window.ScoreboardPreviewCatalog?.[item.id];
      const art=make('span',undefined,'scoreboard-tile-art');
      if(detail?.image){
        const image=make('img');image.src=detail.image;image.alt='';image.loading='lazy';image.decoding='async';art.append(image);
      }else art.append(make('span','PREVIEW UNAVAILABLE','scoreboard-tile-empty'));
      const copy=make('span',undefined,'scoreboard-tile-copy');
      copy.append(make('strong',item.label),make('small',group(item)),make('small',detail?.caption||'Preview unavailable','scoreboard-tile-caption'));
      card.append(art,copy);
      if(item.id===state.selected)card.append(make('span','Selected','ball-tile-selected'));
      card.onclick=()=>choose(item);grid.append(card);
    }
    if(!matches.length){const empty=make('div',undefined,'ball-picker-empty');empty.append(make('strong','No scoreboards match those filters.'));
      const reset=make('button','Reset filters');reset.type='button';reset.onclick=()=>{query='';category='All';page=0;render();};empty.append(reset);grid.append(empty);}
    const footer=make('footer',undefined,'ball-picker-footer'),message=make('span',state.message||`${matches.length} scoreboards`);message.dataset.phase=state.phase||'idle';
    const paging=make('div'),previous=make('button','Previous'),count=make('span',matches.length?`${page*pageSize+1}–${Math.min((page+1)*pageSize,matches.length)} of ${matches.length}`:'No results'),next=make('button','Next');
    previous.type=next.type='button';previous.disabled=page===0||busy;next.disabled=page>=pages-1||busy;
    previous.onclick=()=>{page--;render();};next.onclick=()=>{page++;render();};paging.append(previous,count,next);footer.append(message,paging);
    picker.replaceChildren(heading,controls,grid,footer);
    search.oninput=()=>{query=search.value;page=0;render();};
    if(preserveView)grid.scrollTop=scrollTop;
    if(picker.isConnected&&(!preserveView||searchWasFocused))search.focus();
  }
  button.setAttribute('aria-haspopup','dialog');button.setAttribute('aria-expanded','false');
  button._closeCatalog=()=>close(false);
  button.onclick=()=>{void open();};
  api.onScoreboardStatus(paint);
  api.scoreboardStatus().then(paint).catch(error=>{button.textContent='Scoreboard unavailable';button.title=error.message;button.disabled=true;});
  api.onBlacktopPanelCollapsed?.(()=>close(false));
  api.onBlacktopPanelShown?.(()=>{close(false);api.scoreboardStatus().then(paint).catch(()=>{});});
})();
