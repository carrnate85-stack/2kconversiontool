(()=>{
 const api=window.modLauncher,button=document.getElementById('ballMenuButton');if(!button)return;
 let state,busy=false,picker=null,query='',category='All',page=0,openToken=0;
 const make=(tag,text,cls)=>{const element=document.createElement(tag);if(text!==undefined)element.textContent=text;if(cls)element.className=cls;return element;};
 const trace=event=>window.__blacktopCatalogTrace?.({kind:'ball',event,choices:state?.choices?.length,picker:!!picker,token:openToken,view:document.body.dataset.overlayView,stack:new Error().stack});
 function choice(){return state?.choices?.find(item=>item.id===state.selected);}
 function paint(value){const previous=state;state={...state,...value,choices:value.choices||state?.choices||[]};const selected=choice();button.textContent=`${selected?.label||'Current installed'} ▾`;button.dataset.phase=state.phase;button.title=state.message||'Choose the Blacktop ball';button.setAttribute('aria-label',`Ball: ${selected?.label||'Current installed'}. ${state.message||''}`);if(picker?.isConnected){if(previous?.selected!==state.selected||JSON.stringify(previous?.choices)!==JSON.stringify(state.choices))render({preserveView:true});else{const message=picker.querySelector('.ball-picker-footer > span');if(message){message.textContent=state.message||'';message.dataset.phase=state.phase||'idle';}}}}
 function close(collapse=true){
  trace('close:'+Boolean(collapse));
  openToken++;
  const wasOpen=Boolean(picker?.isConnected||document.body.dataset.overlayView==='ballMenuButton');
  picker?.remove();picker=null;document.body.classList.remove('ball-catalog-open');button.setAttribute('aria-expanded','false');
  button.setAttribute('aria-busy','false');
  if(collapse&&wasOpen)window.BlacktopOverlay?.expand(false);
 }
 async function open(){
  trace('open');
  if(!state?.choices?.length)return;
  if(picker||document.body.dataset.overlayView==='ballMenuButton'){close();return;}
  const token=++openToken;
  button.setAttribute('aria-busy','true');
  const expansion=window.BlacktopOverlay?.expand(true,'ballMenuButton');
  // Prepare the cards while the native window expands, but keep them detached
  // until acknowledgement so they can never draw over the compact bar.
  picker=make('section',undefined,'ball-picker is-open');picker.setAttribute('role','dialog');picker.setAttribute('aria-labelledby','ballPickerTitle');
  const preparedState=state,hydrate=render();
  const expanded=await expansion;
  trace('expanded:'+String(expanded));
  if(token!==openToken)return;
  if(expanded===false){close(false);window.BlacktopOverlay?.expand(false);return;}
  document.body.classList.add('ball-catalog-open');
  button.setAttribute('aria-expanded','true');button.setAttribute('aria-busy','false');document.body.append(picker);
  if(preparedState!==state)render();else{hydrate?.();picker.querySelector('.ball-picker-controls > input')?.focus();}
  trace('attached');
 }
 function render({preserveView=false}={}){
  if(!picker||!state)return;
  const oldGrid=preserveView?picker.querySelector('.ball-picker-grid'):null;
  const scrollTop=oldGrid?.scrollTop||0;
  const searchWasFocused=preserveView&&document.activeElement===picker.querySelector('.ball-picker-controls > input');
  picker.replaceChildren();
  const heading=make('header',undefined,'ball-picker-heading'),identity=make('div'),eyebrow=make('small','BLACKTOP'),title=make('h2','Ball catalog');title.id='ballPickerTitle';identity.append(eyebrow,title);
  const exit=make('button','Back to game','ball-picker-back');exit.type='button';exit.onclick=close;heading.append(identity,exit);
  const controls=make('section',undefined,'ball-picker-controls'),search=make('input');search.type='search';search.value=query;search.placeholder='Search ball names or categories';search.setAttribute('aria-label','Search balls');
  const selectedButton=make('button','Show selected');selectedButton.type='button';selectedButton.onclick=()=>{query='';category='All';const index=state.choices.findIndex(item=>item.id===state.selected);page=Math.max(0,Math.floor(index/24));render();};controls.append(search,selectedButton);
  const categories=['All',...new Set(state.choices.map(item=>item.group))],filters=make('div',undefined,'ball-picker-filters');filters.setAttribute('aria-label','Ball categories');
  for(const name of categories){const filter=make('button',name);filter.type='button';filter.setAttribute('aria-pressed',String(name===category));filter.onclick=()=>{category=name;page=0;render();};filters.append(filter);}controls.append(filters);
  const words=query.trim().toLowerCase().split(/\s+/).filter(Boolean),matches=state.choices.filter(item=>(category==='All'||item.group===category)&&words.every(word=>`${item.label} ${item.group} ${item.id}`.toLowerCase().includes(word)));
  const pages=Math.max(1,Math.ceil(matches.length/24));page=Math.min(page,pages-1);const grid=make('section',undefined,'ball-picker-grid');
  const previews=[];for(const item of matches.slice(page*24,page*24+24)){
   const card=make('button',undefined,'ball-tile');card.type='button';card.disabled=busy;card.setAttribute('aria-pressed',String(item.id===state.selected));card.title=item.renderNote|| (item.id==='current'?'Use the ball currently installed in the mods folder.':item.id);
   const art=make('span',item.id==='current'?'CURRENT':item.preview&&!item.thumbnail?'Loading…':'●','ball-tile-art');art.dataset.group=item.group;const text=make('span',undefined,'ball-tile-copy');text.append(make('strong',item.label),make('small',item.group));card.append(art,text);if(item.id===state.selected)card.append(make('span','Selected','ball-tile-selected'));
   if(item.thumbnail){const image=make('img');image.src=window.PreviewImageUrl('ball',item.thumbnail);image.alt='';image.loading='lazy';image.decoding='async';image.onerror=()=>{api.ballThumbnail(item.id).then(result=>{if(!art.isConnected)return;if(result?.state==='ready'&&/^data:image\/png;base64,/.test(result.dataUrl)){const fallback=make('img');fallback.src=result.dataUrl;fallback.alt='';art.replaceChildren(fallback);}else{art.classList.remove('has-preview');art.textContent='●';}}).catch(()=>{if(art.isConnected){art.classList.remove('has-preview');art.textContent='●';}});};art.replaceChildren(image);art.classList.add('has-preview');}
   else if(item.preview)previews.push({item,art});
   card.onclick=()=>{if(busy||api.__previewOnly)return;busy=true;state={...state,selected:item.id,phase:'pending',message:`Selecting ${item.label}…`};render();close();api.selectBall(item.id).then(paint).catch(error=>paint({...state,phase:'error',message:error.message})).finally(()=>{busy=false;if(picker?.isConnected)render();});};grid.append(card);
  }
  if(!matches.length){const empty=make('div',undefined,'ball-picker-empty');empty.append(make('strong','No balls match those filters.'),make('span','Try another name or show every category.'));const reset=make('button','Reset filters');reset.type='button';reset.onclick=()=>{query='';category='All';page=0;render();};empty.append(reset);grid.append(empty);}
  const footer=make('footer',undefined,'ball-picker-footer'),message=make('span',state.message||`${matches.length} balls`);message.dataset.phase=state.phase;
  const paging=make('div'),previous=make('button','Previous'),count=make('span',matches.length?`${page*24+1}–${Math.min((page+1)*24,matches.length)} of ${matches.length}`:'No results'),next=make('button','Next');previous.type=next.type='button';previous.disabled=page===0||busy;next.disabled=page>=pages-1||busy;previous.onclick=()=>{page--;render();};next.onclick=()=>{page++;render();};paging.append(previous,count,next);footer.append(message,paging);
  picker.append(heading,controls,grid,footer);search.oninput=()=>{query=search.value;page=0;render();};if(preserveView)grid.scrollTop=scrollTop;if(picker.isConnected&&(!preserveView||searchWasFocused))search.focus();
  const hydrate=()=>{for(const {item,art} of previews)api.ballThumbnail(item.id).then(result=>{if(!art.isConnected)return;if(result?.state==='ready'&&/^data:image\/png;base64,/.test(result.dataUrl)){const image=make('img');image.src=result.dataUrl;image.alt='';art.replaceChildren(image);art.classList.add('has-preview');}else art.textContent='●';}).catch(()=>{if(art.isConnected)art.textContent='●';});};
  if(picker.isConnected)hydrate();return hydrate;
 }
 button._closeCatalog=()=>close(false);
 button.setAttribute('aria-haspopup','dialog');button.setAttribute('aria-expanded','false');button.onclick=()=>{void open();};
 api.onBallStatus(paint);api.ballStatus().then(paint).catch(error=>{button.textContent='Ball unavailable';button.title=error.message;button.disabled=true;});
 api.onBlacktopPanelCollapsed?.(()=>close(false));api.onBlacktopPanelShown?.(()=>{close(false);api.ballStatus().then(paint).catch(()=>{});});
})();
