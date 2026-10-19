(() => {
 let active=null;
 const make=(tag,text,className)=>{const element=document.createElement(tag);if(text)element.textContent=text;if(className)element.className=className;return element;};
 function open({selection,library,onApply}){
  if(active)active.close();
  const dialog=make('dialog',null,'fit-preview-dialog');dialog.setAttribute('aria-label','Clothing mannequin preview');
  const header=make('header',null,'fit-preview-header'),title=make('h2','Clothing mannequin'),close=make('button','Discard','secondary');
  close.type='button';
  const use=make('button','Use these choices','primary');use.type='button';
  const randomClothes=make('button','Random clothes','secondary'),randomAccessories=make('button','Random accessories','secondary');
  randomClothes.type=randomAccessories.type='button';
  randomClothes.dataset.action='random-clothes';randomAccessories.dataset.action='random-accessories';
  randomClothes.title='Replace top, bottom and socks; keep shoes unchanged.';
  randomAccessories.title='Replace accessories with up to three items; keep clothes and shoes unchanged.';
  header.append(title,use,randomClothes,randomAccessories,close);
  const layout=make('div',null,'fit-preview-layout'),view=make('div',null,'fit-preview-view'),frame=make('iframe');
  frame.title='3D clothing mannequin';frame.src='fit-preview/index.html';const fitLoading=make('div','Updating fit…','fit-preview-loading');view.append(frame,fitLoading);
  const sidebar=make('aside',null,'fit-preview-sidebar');
  const selectedPhotos=make('div',null,'fit-preview-selected-photos');
  const status=make('p','Loading the mannequin…','fit-preview-status');status.setAttribute('role','status');
  const note=make('p','Choose a card to browse the full catalog. Shoe colors are an approximate render of the game’s saved colorway; team-dependent and unsupported shoes stay neutral. Save the outfit separately to keep your selection.','fit-preview-note');
  const detail=make('details',null,'fit-preview-details'),detailTitle=make('summary','Details'),metrics=make('p',null,'fit-preview-metrics'),warning=make('p',null,'fit-preview-warning');
  detail.append(detailTitle,metrics);warning.hidden=true;
  sidebar.append(status,make('p','4 garments selected','fit-preview-count'),selectedPhotos,warning,note,detail);
  layout.append(view,sidebar);dialog.append(header,layout);document.body.append(dialog);dialog.showModal();
  let current={...selection,accessories:[...(selection.accessories||(selection.accessory?[selection.accessory]:[]))]};
  const accessoryHost=make('div',null,'mannequin-accessories');selectedPhotos.after(accessoryHost);
  const accessoryChoices=(library.accessories||[]).filter(a=>!a.asset.startsWith('female_'));
  const accessoryControls=window.AccessoryControls.mount(accessoryHost,{layout:'cards',values:current.accessories,choices:()=>accessoryChoices,baseCount:()=>window.AccessorySlots.baseCount(library.catalog.find(p=>p.id===current.streetId),current.sock,current.layer),onChange:()=>{current.accessories=accessoryControls.values;paintAccessoryStates();request();}});
  let accessoryResults=new Map();
  function paintAccessoryStates(results){
   if(results)accessoryResults=new Map(results.map(a=>[a.asset,a]));
   use.disabled=!accessoryControls.valid;
   for(const field of accessoryHost.querySelectorAll('.accessory-slot')){
    const asset=field.querySelector('select').value;let badge=field.querySelector('.accessory-preview-badge');
    if(!badge){badge=make('small',null,'accessory-preview-badge');field.append(badge);}
    const result=accessoryResults.get(asset);badge.hidden=!asset;badge.dataset.state=result?.state||'checking';
    badge.textContent=result?.state==='shown'?'3D preview':result?.state==='unavailable'?'3D unavailable':'Checking preview';
    badge.title=result?.reason||'';
   }
   accessoryHost.querySelector('.accessory-slots-heading span').textContent=`${current.accessories.length} selected / ${Math.max(0,7-window.AccessorySlots.baseCount(library.catalog.find(p=>p.id===current.streetId),current.sock,current.layer))} spaces`;
  }
  let generation=0,requestTimer=null,ready=false,closed=false,mannequinShown=false,knownMeshes=[],knownTextures=[];
  const photoCache=new Map(),photoJobs=new Map(),uncachedPhotos=new Set();
  function photo(asset,cachedOnly=false){
   if(photoCache.has(asset)){const value=photoCache.get(asset);photoCache.delete(asset);photoCache.set(asset,value);return Promise.resolve(value);}
   if(cachedOnly&&uncachedPhotos.has(asset))return Promise.resolve(null);
   const key=`${asset}:${cachedOnly?'cached':'full'}`;
   if(photoJobs.has(key))return photoJobs.get(key);
   const api=cachedOnly?window.modLauncher.cachedClothingThumbnail:window.modLauncher.clothingThumbnail;
   if(typeof api!=='function')return Promise.resolve(null);
   const job=api(asset).then(result=>{
    if(result?.state==='ready'&&/^data:image\/png;base64,/.test(result.dataUrl)){
     const value={url:result.dataUrl,note:result.note};photoCache.set(asset,value);uncachedPhotos.delete(asset);
     while(photoCache.size>192)photoCache.delete(photoCache.keys().next().value);
     return value;
    }
    if(cachedOnly&&result?.state==='uncached'){
     uncachedPhotos.add(asset);while(uncachedPhotos.size>512)uncachedPhotos.delete(uncachedPhotos.values().next().value);
    }
    return null;
   }).catch(()=>null).finally(()=>photoJobs.delete(key));
   photoJobs.set(key,job);return job;
  }
  function fallbackPhoto(host){
   const label=host.closest('.fit-preview-selected-photo')?.querySelector('small')?.textContent||'Top';
   const shapes={Top:'M8 5 4 8l4 5v13h16V13l4-5-4-3-5 3h-6z',Bottom:'M7 5h18l2 21H17l-1-11-1 11H5z',Sock:'M11 4h10v14l5 3v5H10l-4-4 5-8z',Shoe:'M5 12l8 4 6-3 3 5 6 3v5H4v-8z'};
   const svg=document.createElementNS('http://www.w3.org/2000/svg','svg');svg.setAttribute('viewBox','0 0 32 32');svg.setAttribute('aria-hidden','true');
   const p=document.createElementNS(svg.namespaceURI,'path');p.setAttribute('d',shapes[label]||shapes.Top);svg.append(p);host.replaceChildren(svg);host.classList.add('has-fallback');
  }
  function readableName(asset){
   const label=window.ClothingCatalogUI.label(asset).replace(/\bgry\b/gi,'gray').replace(/\bblk\b/gi,'black').replace(/\bwht\b/gi,'white').replace(/\b0000\b/g,'').replace(/\s+/g,' ').trim();
   if(/^tanktop solid (.+)$/i.test(label))return label.replace(/^tanktop solid (.+)$/i,'$1 tank top');
   if(/^solid (.+) crew$/i.test(label))return label.replace(/^solid (.+) crew$/i,'$1 crew socks');
   if(/^basketball shorts (.+) camo (.+)$/i.test(label))return label.replace(/^basketball shorts (.+) camo (.+)$/i,'$1 $2 camo shorts');
   return label.replace(/^shoe default$/i,'Default shoes');
  }
  function putPhoto(host,asset,value){
   if(!host.isConnected||host.dataset.asset!==asset)return;
   if(!value){fallbackPhoto(host);return;}host.classList.remove('has-fallback');
   const img=make('img');img.src=value.url;img.alt='';host.replaceChildren(img);if(value.note)host.title=value.note;
  }
  function paintSelectedPhotos(){
   const top=topById.get(current.streetId),items=[['Top',top?.previewAsset||top?.top],['Bottom',current.bottom],['Sock',current.sock||top?.socks?.[0]],['Shoe',current.shoe||top?.shoes?.[0]]];
   selectedPhotos.replaceChildren();
   for(const [label,asset] of items){
    const card=make('button',null,'fit-preview-selected-photo'),picture=make('div','Loading photo…','fit-preview-selected-picture');
    card.type='button';card.setAttribute('aria-label',`Browse ${label.toLowerCase()} catalog`);
    picture.dataset.asset=asset||'';card.append(make('small',label),picture,make('span',label==='Top'?readableName(top?.topLabel||'Choose top'):readableName(asset||`Choose ${label.toLowerCase()}`),'fit-preview-selected-label'));selectedPhotos.append(card);fallbackPhoto(picture);
    card.onclick=()=>browse(label);
    if(asset==='NO SHIRT'){picture.textContent='No top';continue;}
    if(asset){
     const sock=label==='Sock'&&window.SockPreviewCatalog?.get(asset);
     const shoe=label==='Shoe'&&window.ShoePreviewCatalog?.get(asset);
     const topPreview=label==='Top'&&window.TopPreviewCatalog?.get(top?.top);
     const bottomPreview=label==='Bottom'&&window.BottomPreviewCatalog?.get(asset);
     if(sock){const img=make('img');img.src=window.PreviewImageUrl('sock',sock.thumbnail);img.alt='';img.onerror=()=>photo(asset).then(value=>putPhoto(picture,asset,value));picture.replaceChildren(img);}
     else if(shoe){const img=make('img');img.src=window.PreviewImageUrl('shoe',shoe.thumbnail);img.alt='';img.onerror=()=>photo(asset).then(value=>putPhoto(picture,asset,value));picture.replaceChildren(img);}
     else if(topPreview){const img=make('img');img.src=window.PreviewImageUrl('top',topPreview.thumbnail);img.alt='';img.onerror=()=>photo(asset).then(value=>putPhoto(picture,asset,value));picture.replaceChildren(img);}
     else if(bottomPreview){const img=make('img');img.src=window.PreviewImageUrl('bottom',bottomPreview.thumbnail);img.alt='';img.onerror=()=>photo(asset).then(value=>putPhoto(picture,asset,value));picture.replaceChildren(img);}
     else{const cached=photoCache.get(asset);if(cached)putPhoto(picture,asset,cached);
     else photo(asset).then(value=>putPhoto(picture,asset,value));}
    }else fallbackPhoto(picture);
   }
  }
  const stopProgress=window.modLauncher.onFitPreviewProgress(({token,payload})=>{
   if(closed||token!==generation||!ready)return;
   frame.contentWindow.postMessage({type:'fit-preview:result',payload},'*');
   if(payload.parts.length)view.dataset.loading='false';
   status.textContent=payload.stage==='mannequin'?'Mannequin ready · loading outfit…':`${payload.parts.length} garments · ${payload.stats.ms} ms base · loading artwork…`;
  });
  const tops=library.catalog.filter(p=>p.pool==='male');
  const uniqueTops=new Map();
  for(const preset of tops){
   const key=`${preset.top}\0${(preset.layers||[]).join('\0')}`;
   if(!uniqueTops.has(key)||preset.id===current.streetId)uniqueTops.set(key,preset);
  }
  const topItems=[...uniqueTops.values()].map(p=>({id:p.id,label:p.topLabel,asset:p.top,category:window.ClothingCatalogUI.category(p.top,'tops'),search:`${p.topLabel} ${p.top}`.toLowerCase()}));
  const topById=new Map(tops.map(p=>[p.id,p]));
  const bottomItem=p=>({id:p.bottom,label:p.bottomLabel,asset:p.bottom,category:window.ClothingCatalogUI.category(p.bottom,'bottoms'),search:`${p.bottomLabel} ${p.bottom}`.toLowerCase()});
  const allBottoms=[...new Map(tops.filter(p=>p.separable).map(p=>[p.bottom,bottomItem(p)])).values()];
  const sockItems=[...new Set(tops.flatMap(p=>p.socks||[]))].map(asset=>({id:asset,asset,label:window.ClothingCatalogUI.label(asset)}));
  const shoeItems=[...new Set([...tops.flatMap(p=>p.shoes||[]),...(library.shoes||[]).filter(item=>item.pools.includes('male')).map(item=>item.asset)])].map(asset=>({id:asset,asset,label:library.shoeLabels?.[asset]||window.ClothingCatalogUI.label(asset)}));
  function bottomChoices(){
   const p=topById.get(current.streetId);
   return p?.separable?allBottoms:p?[bottomItem(p)]:[];
  }
  function randomize(kind){
   if(closed)return;
   try{
    current=window.MannequinRandom[kind](current,library);
    accessoryResults.clear();accessoryControls.set(current.accessories);
    paintAccessoryStates();paintSelectedPhotos();request(true);
   }catch(error){status.dataset.state='error';status.textContent=error.message;}
  }
  randomClothes.onclick=()=>randomize('clothes');
  randomAccessories.onclick=()=>randomize('accessories');
  function browse(kind){
   const isTop=kind==='Top',top=topById.get(current.streetId);
   const items=isTop?topItems:kind==='Bottom'?bottomChoices():kind==='Sock'?sockItems:top?.shoes?.length===1?shoeItems:shoeItems.filter(item=>top?.shoes?.includes(item.id));
   window.ClothingPicker.open({title:`${kind} · Mannequin`,items,selected:isTop?current.streetId:kind==='Bottom'?current.bottom:kind==='Sock'?current.sock:current.shoe,filterKind:isTop?'tops':kind==='Bottom'?'bottoms':kind==='Sock'?'socks':'shoes',
    noteText:'Preview only until you apply this selection to the outfit.',onChoose:id=>{
     if(isTop){if(id===current.streetId)return;current.streetId=id;const next=topById.get(id);if(!bottomChoices().some(b=>b.id===current.bottom))current.bottom=next.bottom;if(next.shoes?.length!==1)current.shoe='';if(next.socks?.length>1)current.sock='';}
     else if(kind==='Bottom'){if(id===current.bottom)return;current.bottom=id;}
     else if(kind==='Sock'){if(id===current.sock)return;current.sock=id;}
     else{if(id===current.shoe)return;current.shoe=id;}
     accessoryControls.refresh();paintAccessoryStates();paintSelectedPhotos();request();
    }});
  }
  function request(immediate=false){
   if(!ready||closed)return;
   const token=++generation;status.textContent='Updating fit…';status.dataset.state='loading';view.dataset.loading='true';
   accessoryResults.clear();paintAccessoryStates();
   if(!accessoryControls.valid){view.dataset.loading='false';status.dataset.state='error';status.textContent='Seven pieces maximum. Remove an accessory to use these choices.';return;}
   clearTimeout(requestTimer);
   requestTimer=setTimeout(()=>{const showMannequinEarly=!mannequinShown;window.modLauncher.fitPreview({selection:current,knownMeshes,knownTextures,showMannequinEarly,previewToken:token}).then(payload=>{
    if(closed||token!==generation)return;
    frame.contentWindow.postMessage({type:'fit-preview:result',payload},'*');
    view.dataset.loading='false';
    const count=payload.parts.length,cache=payload.stats.cache;
    paintAccessoryStates(payload.accessories||[]);
    const unavailable=(payload.accessories||[]).filter(a=>a.state==='unavailable');
    status.textContent=unavailable.length?'Preview ready · '+unavailable.length+' accessory preview'+(unavailable.length===1?'':'s')+' unavailable':'Preview ready';status.dataset.state='ready';
    sidebar.querySelector('.fit-preview-count').textContent=`${payload.parts.filter(p=>!p.accessorySlot).length} garments · ${current.accessories.length} accessories selected`;
    metrics.textContent=`${payload.stats.ms} ms preparation · ${cache.memory} memory / ${cache.disk} disk / ${cache.decoded} decoded`;
    warning.textContent=payload.warnings.map(w=>/shoe color resources are unavailable/i.test(w)?'Shoe colors unavailable. A neutral shoe shape is shown.':w).join(' ');warning.hidden=!payload.warnings.length;
    if(unavailable.length){warning.textContent+=(warning.textContent?' ':'')+unavailable.map(a=>window.AccessorySlots.label(a.asset)+': '+a.reason).join(' ');warning.hidden=false;}

   }).catch(error=>{if(closed||token!==generation)return;status.dataset.state='error';status.textContent=String(error.message).replace(/^Error invoking remote method '[^']+': (?:Error: )?/,'');});},immediate?0:35);
  }
  function receive(event){
   if(event.source!==frame.contentWindow)return;
   if(event.data?.type==='fit-preview:ready'){knownMeshes=[];knownTextures=[];mannequinShown=false;ready=true;request(true);}
   if(event.data?.type==='fit-preview:displayed')mannequinShown=true;
   if(event.data?.type==='fit-preview:known'){knownMeshes=event.data.meshes||[];knownTextures=event.data.textures||[];}
   if(event.data?.type==='fit-preview:display-error'){status.dataset.state='error';status.textContent=event.data.message;}
  }
  function finish(){
   if(closed)return;closed=true;generation++;clearTimeout(requestTimer);
   const replaced=active?.dialog!==dialog;
   window.modLauncher.cancelFitPreview().then(()=>{
    if(!replaced&&document.querySelector('.nav-item[data-page="blacktop"]')?.classList.contains('active'))window.modLauncher.warmFitPreview().catch(()=>{});
   }).catch(()=>{});
   stopProgress();window.removeEventListener('message',receive);dialog.remove();if(!replaced)active=null;
  }
  window.addEventListener('message',receive);
  close.onclick=()=>dialog.close();dialog.addEventListener('close',finish,{once:true});
  dialog.addEventListener('cancel',()=>{});
  use.onclick=()=>{onApply(current);dialog.close();};
  paintSelectedPhotos();paintAccessoryStates();active={dialog,close:()=>dialog.close()};
 }
 document.querySelector('.nav-item[data-page="blacktop"]')?.addEventListener('click',()=>{
  window.modLauncher.warmFitPreview().catch(()=>{});
 });
 document.querySelectorAll('.nav-item[data-page]:not([data-page="blacktop"])').forEach(button=>{
  button.addEventListener('click',()=>window.modLauncher.cancelFitPreview().catch(()=>{}));
 });
 window.FastFitPreviewUI={open};
})();
