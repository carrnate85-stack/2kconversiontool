(() => {
  let dialog, generation=0;
  const cache=new Map(),pending=new Map();
  const views=new Map();
  const make=(tag,text,cls)=>{const e=document.createElement(tag);if(text)e.textContent=text;if(cls)e.className=cls;return e;};
  async function thumbnail(asset) {
    if(cache.has(asset)) return cache.get(asset);
    if(pending.has(asset))return pending.get(asset);
    const request=window.modLauncher.clothingThumbnail?.(asset);
    pending.set(asset,request);
    let result;try{result=await request;}finally{pending.delete(asset);}
    if(result?.state==='ready' && /^data:image\/png;base64,/.test(result.dataUrl)) {
      if(cache.size>=128) cache.delete(cache.keys().next().value);
      const preview={url:result.dataUrl,note:result.note};cache.set(asset,preview); return preview;
    }
    return null;
  }
  window.ClothingPicker={updateSelected:null,open({title,items,selected,onChoose,noteText,filterKind}) {
    if(filterKind==='tops')items=[...items].sort((a,b)=>Number(b.id==='shirtless-male')-Number(a.id==='shirtless-male'));
    const host=document.getElementById('wardrobeCatalog');
    const viewKey=(host?'overlay:':'creator:')+(filterKind||'')+':'+title.split('·')[0].trim();
    const remembered=views.get(viewKey)||{};
    dialog?.close(); dialog?.remove();
    dialog=make('dialog',null,'clothing-picker');
    const current=dialog;
    const heading=make('div',null,'clothing-picker-heading'), h=make('h2',title), close=make('button','Back');
    h.id='clothing-picker-title'; current.setAttribute('aria-labelledby',h.id);
    close.type='button';close.onclick=()=>current.close();heading.append(h);if(!host)heading.append(close);
    const search=make('input');search.type='search';search.placeholder='Search clothing or brand';search.setAttribute('aria-label','Search clothing');
    search.value=remembered.query||'';
    const clearSearch=make('button','Clear search');clearSearch.type='button';
    const controls=make('div',null,'clothing-picker-filters');controls.append(search,clearSearch);
    let category=remembered.category||'All';const chips=make('div',null,'clothing-category-filters');chips.setAttribute('aria-label','Clothing categories');
    for(const name of window.ClothingCatalogUI?.categories(items,filterKind)||[]){
      const chip=make('button',name);chip.type='button';chip.dataset.category=name;chip.setAttribute('aria-pressed',String(name===category));
      chip.onclick=()=>{category=name;page=0;for(const button of chips.children)button.setAttribute('aria-pressed',String(button.dataset.category===category));render();};chips.append(chip);
    }
    if(chips.children.length)controls.append(chips);
    if(![...chips.children].some(c=>c.dataset.category===category))category='All';
    const note=make('p',noteText||'','clothing-picker-note');
    const grid=make('div',null,'clothing-picker-grid'), pager=make('div',null,'clothing-picker-pager');
    const previous=make('button','Previous'), next=make('button','Next'), count=make('span');count.setAttribute('role','status');
    previous.type=next.type='button';pager.append(previous,count,next);
    current.append(heading,controls);if(!host&&noteText)current.append(note);current.append(grid,pager);(host||document.body).append(current);
    if(host)current.classList.add('clothing-picker-inline');
    const pageSize=host?24:(document.body?.classList?.contains?.('blacktop-overlay')?18:16);
    let page=remembered.page||0,timer;
    function openRenderDetail(item,preview,kind='top'){
      if(!preview.thumbnail)return;
      const detail=make('dialog',null,'clothing-render-detail');
      const title=make('h3',item.label),image=make('img'),closeDetail=make('button','Close');
      image.src=window.PreviewImageUrl(kind,preview.thumbnail);image.alt=`Larger preview of ${item.label}`;
      image.onerror=()=>{image.replaceWith(make('p','Render unavailable'));};
      closeDetail.type='button';closeDetail.onclick=()=>detail.close();
      detail.append(title,image,closeDetail);document.body.append(detail);
      detail.addEventListener('close',()=>detail.remove());detail.showModal();
    }
    function render() {
      const token=++generation;
      grid.replaceChildren();
      const query=search.value.trim().toLowerCase();
      const matches=items.filter(i=>(category==='All'||window.ClothingCatalogUI.category(i.asset,filterKind,i.label)===category)&&query.split(/\s+/).every(q=>`${i.label} ${i.asset||''}`.toLowerCase().includes(q)));
      const pages=Math.max(1,Math.ceil(matches.length/pageSize));page=Math.min(page,pages-1);
      count.textContent=matches.length?`${page*pageSize+1}–${Math.min((page+1)*pageSize,matches.length)} of ${matches.length} · Page ${page+1} / ${pages}`:'No results';
      clearSearch.disabled=!search.value;
      for(const chip of chips.children)chip.setAttribute('aria-pressed',String(chip.dataset.category===category));
      if(views.size>=24&&!views.has(viewKey))views.delete(views.keys().next().value);
      views.set(viewKey,{query:search.value,category,page});
      previous.disabled=page===0;next.disabled=page>=pages-1;
      const requests=[];
      const cachedFallback=(picture,item)=>thumbnail(item.asset||item.id).then(preview=>{
        if(token!==generation||!current.open)return;
        if(preview){const image=make('img');image.src=preview.url;image.alt=item.label;picture.replaceChildren(image);}
        else picture.textContent='Preview unavailable';
      }).catch(()=>{if(token===generation&&current.open)picture.textContent='Preview unavailable';});
      for(const item of matches.slice(page*pageSize,page*pageSize+pageSize)) {
        const card=make('button',null,'clothing-tile');card.type='button';card.dataset.itemId=item.id;card.setAttribute('aria-pressed',String(item.id===selected));
        card.disabled=Boolean(item.unavailable);card.title=item.unavailable||item.label;
        const sockPreview=filterKind==='socks'?window.SockPreviewCatalog?.get(item.id):null;
        const shoePreview=filterKind==='shoes'?window.ShoePreviewCatalog?.get(item.id):null;
        const topPreview=filterKind==='tops'?window.TopPreviewCatalog?.get(item.renderAsset||item.asset):null;
        const bottomPreview=filterKind==='bottoms'?window.BottomPreviewCatalog?.get(item.asset||item.id):null;
        const accessoryPreview=filterKind==='accessories'?window.AccessoryPreviewCatalog?.get(item.id):null;
        const picture=make('span',sockPreview||shoePreview||topPreview||bottomPreview||accessoryPreview?null:'Loading preview…','clothing-picture');
        const name=make('span',item.label,'clothing-tile-name');card.append(picture,name);
        if(item.id===selected)card.append(make('small','Selected','clothing-selected'));
        if(item.unavailable)card.append(make('small','Unavailable','clothing-unavailable'));
        card.onclick=()=>{selected=item.id;onChoose(item.id);if(!host)current.close();else render();};
        if(topPreview?.thumbnail||shoePreview){
          const wrapper=make('div',null,'clothing-tile-wrap'),inspect=make('button','View render','clothing-render-view');
          inspect.type='button';inspect.textContent=shoePreview?'View preview':'View render';inspect.setAttribute('aria-label',`View larger ${shoePreview?'shoe preview':'jersey render'} of ${item.label}`);
          inspect.onclick=()=>openRenderDetail(item,shoePreview||topPreview,shoePreview?'shoe':'top');wrapper.append(card,inspect);grid.append(wrapper);
        }else grid.append(card);
        if(sockPreview){const image=make('img');image.src=window.PreviewImageUrl('sock',sockPreview.thumbnail);image.alt=item.label;image.loading='lazy';image.decoding='async';image.onerror=()=>cachedFallback(picture,item);picture.replaceChildren(image);}
        else if(shoePreview){const image=make('img');image.src=window.PreviewImageUrl('shoe',shoePreview.thumbnail);image.alt=item.label;image.loading='lazy';image.decoding='async';image.onerror=()=>cachedFallback(picture,item);picture.replaceChildren(image);}
        else if(topPreview){const image=make('img');image.src=window.PreviewImageUrl('top',topPreview.thumbnail);image.alt=item.label;image.loading='lazy';image.decoding='async';image.onerror=()=>cachedFallback(picture,item);picture.replaceChildren(image);}
        else if(bottomPreview){const image=make('img');image.src=window.PreviewImageUrl('bottom',bottomPreview.thumbnail);image.alt=item.label;image.loading='lazy';image.decoding='async';image.onerror=()=>cachedFallback(picture,item);picture.replaceChildren(image);}
        else if(accessoryPreview){const image=make('img');image.src=window.PreviewImageUrl('accessory',accessoryPreview.thumbnail);image.alt=item.label;image.loading='lazy';image.decoding='async';image.onerror=()=>cachedFallback(picture,item);picture.replaceChildren(image);}
        else if(item.asset==='NO SHIRT'||item.id==='shirtless-male')picture.textContent='No top';
        else requests.push({picture,item});
      }
      if(!matches.length){const empty=make('div',null,'clothing-empty');empty.append(make('p','Nothing matches these filters.'));const reset=make('button','Reset filters');reset.type='button';reset.onclick=()=>{search.value='';category='All';page=0;render();};empty.append(reset);grid.append(empty);}
      // Request the visible page together: disk hits bypass bounded native decoding.
      // A few cold images must not hold cached cards behind them.
      for(let worker=0;worker<pageSize;worker++) (async()=>{
        while(token===generation && current.open && requests.length) {
          const {picture,item,topPreview,accessoryPreview}=requests.shift();
          if(!item.asset){if(!topPreview&&!accessoryPreview)picture.textContent='—';continue;}
          try {const preview=await thumbnail(item.asset);if(token!==generation||!current.open)break;
            if(preview){const image=make('img');image.src=preview.url;image.alt=item.label;image.onerror=()=>{picture.textContent='No preview';};picture.replaceChildren(image);
              if(preview.note){picture.title=preview.note;picture.parentElement.append(make('small',preview.note,'clothing-new'));}}
            else if(!topPreview&&!accessoryPreview)picture.textContent='No preview';
          } catch {if(token===generation&&!topPreview&&!accessoryPreview)picture.textContent='Preview unavailable';}
        }
      })();
    }
    search.oninput=()=>{clearTimeout(timer);timer=setTimeout(()=>{page=0;render();},150);};
    window.ClothingPicker.updateSelected=(id,disabled)=>{
      if(!current.isConnected||!current.open)return;
      selected=id;
      if(disabled)for(const item of items)item.unavailable=disabled.get(item.id)?'Not available for this combination.':undefined;
      for(const card of grid.querySelectorAll('.clothing-tile')){
        if(disabled){
          card.disabled=Boolean(disabled.get(card.dataset.itemId));
          const unavailable=card.querySelector('.clothing-unavailable');
          if(!card.disabled)unavailable?.remove();
          else if(!unavailable)card.append(make('small','Unavailable','clothing-unavailable'));
          card.title=card.disabled?'Not available for this combination.':items.find(i=>i.id===card.dataset.itemId)?.label||'';
        }
        const active=card.dataset.itemId===id;card.setAttribute('aria-pressed',String(active));
        const marker=card.querySelector('.clothing-selected');if(!active)marker?.remove();
        else if(!marker)card.append(make('small','Selected','clothing-selected'));
      }
    };
    clearSearch.onclick=()=>{clearTimeout(timer);search.value='';page=0;render();search.focus();};
    previous.onclick=()=>{page--;render();};next.onclick=()=>{page++;render();};
    current.addEventListener('close',()=>{clearTimeout(timer);if(dialog===current)generation++;});
    if(host)current.show();else current.showModal();render();if(!host)search.focus();
  }};
})();
