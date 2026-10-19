(() => {
 if(document.documentElement.classList.contains('roster-workspace'))return;
  const root=document.getElementById('outfitCreator');if(!root)return;
  const get=id=>document.getElementById('outfit'+id);
  const saved=get('Saved'),name=get('Name'),pool=get('Pool'),top=get('Top'),bottom=get('Bottom'),shoe=get('Shoe'),sock=get('Sock');
  const status=get('CreatorStatus'),details=get('Details'),save=get('Save'),remove=get('Delete');
  let selectedLayer=''; // Preserve older saved fits; new tops use their stock layers.
  let library={outfits:[],catalog:[]},active='',baseline='',busy=false;
  let cachedCatalog=null,lastBottomKey='',lastShoeKey='',lastSockKey='';
  const poolChoices=new Map();let catalogById=new Map();
  const accessories=window.AccessoryControls.mount(get('Accessories'),{layout:'cards',choices:()=>(library.accessories||[]).filter(a=>(a.asset.startsWith('female_')?'female':'male')===pool.value),
    baseCount:()=>window.AccessorySlots.baseCount(library.catalog.find(p=>p.id===top.value),sock.value,selectedLayer),onChange:()=>syncActions()});
  const signature=()=>JSON.stringify([name.value,pool.value,top.value,bottom.value,shoe.value,sock.value,selectedLayer,accessories.values]);
  const draft=()=>({name:name.value,streetId:top.value,bottom:bottom.value,shoe:shoe.value,sock:sock.value,layer:selectedLayer,accessories:accessories.values});
  function syncActions(){
    accessories.refresh();const valid=Boolean(library.catalog.some(p=>p.id===top.value)&&bottom.value)&&accessories.valid;
    save.disabled=busy||!name.value.trim()||!valid;
    get('Mannequin').disabled=busy||!valid||pool.value!=='male';
    get('Export').disabled=busy||!valid;
    remove.disabled=busy||!active;
  }
  const dirty=()=>signature()!==baseline;
  const canLeave=()=>!dirty()||window.confirm('Discard unsaved outfit changes?');
  const option=(value,text)=>{const e=document.createElement('option');e.value=value;e.textContent=text;return e;};
  function fill(select,items,selected) {
    select.replaceChildren(...items.map(i=>option(i.id,i.label)));select.value=selected;
    if(select.selectedIndex<0&&select.options.length)select.selectedIndex=0;
  }
  function choicesForPool() {
    if(cachedCatalog!==library.catalog){
      cachedCatalog=library.catalog;catalogById=new Map(library.catalog.map(p=>[p.id,p]));
      poolChoices.clear();lastBottomKey=lastShoeKey=lastSockKey='';
    }
    if(!poolChoices.has(pool.value)){
      const presets=library.catalog.filter(p=>p.pool===pool.value);
      const bottoms=new Map(presets.filter(p=>p.separable).map(p=>[p.bottom,{id:p.bottom,label:p.bottomLabel}]));
      const extraShoes=new Map((library.shoes||[]).filter(s=>s.pools.includes(pool.value)).map(s=>[s.asset,s]));
      const shoes=[...new Set([...presets.flatMap(p=>p.shoes||[]),...extraShoes.keys()])];
      const socks=[...new Set(presets.flatMap(p=>p.socks||[]))];
      poolChoices.set(pool.value,{bottoms,shoes,extraShoes,socks});
    }
    return poolChoices.get(pool.value);
  }
  function refreshLibrary() {
    saved.replaceChildren(option('','New outfit'),...library.outfits.map(o=>option(o.id,o.name+(o.available?'':' · unavailable'))));
    saved.value=active;remove.disabled=!active||busy;
  }
  function updateBottoms(selected) {
    const p=library.catalog.find(p=>p.id===top.value),choices=choicesForPool();
    const extra=p?.separable&&!choices.bottoms.has(p.bottom)?p.bottom:'';
    const key=p?.separable?`${pool.value}:all:${extra}`:`${pool.value}:single:${p?.bottom||''}`;
    const items=p?.separable?[...choices.bottoms.values(),...(extra?[{id:extra,label:p.bottomLabel}]:[])]:p?[{id:p.bottom,label:p.bottomLabel}]:[];
    if(key!==lastBottomKey){fill(bottom,items,selected);lastBottomKey=key;}
    else bottom.value=selected;
    if(bottom.selectedIndex<0&&p)bottom.value=p.bottom;
    paint();
  }
  function updateShoes(selected='') {
    const p=library.catalog.find(p=>p.id===top.value),choices=choicesForPool();
    const assets=p?.shoes?.length===1?choices.shoes:[];
    const key=`${pool.value}:${assets.length?'enabled':'original'}`;
    if(key!==lastShoeKey){
      fill(shoe,[{id:'',label:'Style’s original shoes'},...assets.map(id=>({id,label:library.shoeLabels?.[id]||choices.extraShoes.get(id)?.label||id.replace(/^female_/,'').replaceAll('_',' ')}))],selected);
      lastShoeKey=key;
    }else shoe.value=selected;
    if(!assets.includes(selected))shoe.value='';
    shoe.disabled=busy||!assets.length;get('ShoeBrowse').disabled=busy||!assets.length;
  }
  function updateSocks(selected='') {
    const assets=choicesForPool().socks;
    if(lastSockKey!==pool.value){
      fill(sock,[{id:'',label:'Style’s original socks'},...assets.map(id=>({id,label:window.ClothingCatalogUI.label(id)}))],selected);
      lastSockKey=pool.value;
    }else sock.value=selected;
    if(!assets.includes(selected))sock.value='';
  }
  function updateTops(selected,selectedBottom,selectedShoe,selectedSock) {
    const variants=new Map();
    fill(top,library.catalog.filter(p=>p.pool===pool.value).map(p=>{
      const n=(variants.get(p.topLabel)||0)+1;variants.set(p.topLabel,n);
      return {id:p.id,label:p.topLabel+(n>1?` · style ${n}`:'')};
    }),selected);
    updateShoes(selectedShoe);updateSocks(selectedSock);updateBottoms(selectedBottom || library.catalog.find(p=>p.id===top.value)?.bottom);
  }
  function paint() {
    const p=library.catalog.find(p=>p.id===top.value);
    details.textContent='';details.hidden=true;
    for(const [kind,asset] of [['Top',p?.previewAsset||p?.top],['Bottom',bottom.value],['Sock',sock.value||p?.socks?.[0]],['Shoe',shoe.value||p?.shoes?.[0]]]) {
      const picture=get(kind+'Preview'),select=get(kind),label=get(kind+'Label'),button=get(kind+'Browse');
      label.textContent=select.selectedOptions[0]?.textContent||'Choose an item';
      button.setAttribute('aria-label',`Change ${kind==='Sock'?'socks':kind==='Shoe'?'shoes':kind.toLowerCase()}: ${label.textContent}`);
      button.title=label.textContent;
      if(kind==='Shoe')picture.title='Browse shoes';
      const sockPreview=kind==='Sock'&&sock.value?window.SockPreviewCatalog?.get(sock.value):null;
      const shoePreview=kind==='Shoe'?window.ShoePreviewCatalog?.get(asset):null;
      const topPreview=kind==='Top'?window.TopPreviewCatalog?.get(p?.top):null;
      const bottomPreview=kind==='Bottom'?window.BottomPreviewCatalog?.get(bottom.value):null;
      const previewKey=(asset||'')+(sockPreview?'|sock-render':shoePreview?'|shoe-render':topPreview?`|top-render:${p.top}`:bottomPreview?`|bottom-render:${bottom.value}`:'|native');
      if(picture.dataset.previewKey===previewKey)continue;
      picture.dataset.asset=asset||'';picture.dataset.previewKey=previewKey;picture.textContent='Loading preview…';
      if(asset==='NO SHIRT'){picture.textContent='No top';continue;}
      if(!asset){picture.replaceChildren(window.AccessoryControls.icon(kind.toLowerCase()));continue;}
      const loadNative=()=>window.modLauncher.clothingThumbnail(asset).then(result=>{
        if(picture.dataset.previewKey!==previewKey)return;
        if(result?.state==='ready'&&/^data:image\/png;base64,/.test(result.dataUrl)) {
          const img=document.createElement('img');img.src=result.dataUrl;img.alt=kind+' preview';picture.replaceChildren(img);
          if(kind==='Shoe')picture.title=result.note||'Browse shoes';
        }else if(!topPreview&&!bottomPreview&&!shoePreview)picture.replaceChildren(window.AccessoryControls.icon(kind.toLowerCase()));
      }).catch(()=>{if(picture.dataset.previewKey===previewKey&&!topPreview&&!bottomPreview&&!shoePreview)picture.replaceChildren(window.AccessoryControls.icon(kind.toLowerCase()));});
      if(sockPreview){const img=document.createElement('img');img.src=window.PreviewImageUrl('sock',sockPreview.thumbnail);img.alt='Sock preview';img.onerror=()=>{if(picture.dataset.previewKey===previewKey)loadNative();};picture.replaceChildren(img);}
      else if(shoePreview){const img=document.createElement('img');img.src=window.PreviewImageUrl('shoe',shoePreview.thumbnail);img.alt='Shoe preview';img.onerror=()=>{if(picture.dataset.previewKey===previewKey)loadNative();};picture.replaceChildren(img);}
      else if(topPreview){const img=document.createElement('img');img.src=window.PreviewImageUrl('top',topPreview.thumbnail);img.alt='Top preview';img.onerror=()=>{if(picture.dataset.previewKey===previewKey)loadNative();};picture.replaceChildren(img);}
      else if(bottomPreview){const img=document.createElement('img');img.src=window.PreviewImageUrl('bottom',bottomPreview.thumbnail);img.alt='Bottom preview';img.onerror=()=>{if(picture.dataset.previewKey===previewKey)loadNative();};picture.replaceChildren(img);}
      else loadNative();
    }
    syncActions();
  }
  window.addEventListener('preview-pack-changed',()=>{
    for(const kind of ['Top','Bottom','Sock','Shoe'])get(kind+'Preview').dataset.previewKey='';
    accessories.refreshPreviews();paint();
  });
  function openRecord(id) {
    active=id;const record=library.outfits.find(o=>o.id===id);
    name.value=record?.name||'';pool.value=record?.pool||pool.value||'male';
    selectedLayer=record?.layer||'';
    accessories.set(record?.accessories||(record?.accessory?[record.accessory]:[]));
    updateTops(record?.streetId,record?.bottom,record?.shoe,record?.sock);refreshLibrary();baseline=signature();
    status.textContent=record&&!record.available?'This saved outfit is unavailable in the current catalog. Choose replacement pieces and save to repair it.'
      :'Saved outfits apply when assigned to a player in F8.';
  }
  async function operation(action) {
    if(busy)return;busy=true;root.querySelectorAll('button,input,select').forEach(e=>e.disabled=true);status.classList.add('blacktop-busy');
    try {await action();}
    catch(error){status.textContent=error.message;}
    finally {
      busy=false;status.classList.remove('blacktop-busy');
      root.querySelectorAll('button,input,select').forEach(e=>e.disabled=false);
      try{updateShoes(shoe.value);}catch(error){status.textContent=error.message;}
      syncActions();
    }
  }
  saved.onchange=()=>{if(canLeave())openRecord(saved.value);else saved.value=active;};
  const actions=get('Actions');
  actions.addEventListener('click',event=>{if(event.target.closest('button'))actions.open=false;});
  document.addEventListener('click',event=>{if(!actions.contains(event.target))actions.open=false;});
  actions.addEventListener('keydown',event=>{if(event.key==='Escape'){actions.open=false;actions.querySelector('summary').focus();}});
  get('New').onclick=()=>{if(canLeave())openRecord('');};
  get('Mannequin').onclick=()=>window.FastFitPreviewUI.open({selection:draft(),library,onApply:value=>{
    top.value=value.streetId;top.dispatchEvent(new Event('change'));
    if(typeof value.layer==='string')selectedLayer=value.layer;
    bottom.value=value.bottom;bottom.dispatchEvent(new Event('change'));
    if(value.shoe&&[...shoe.options].some(option=>option.value===value.shoe)){shoe.value=value.shoe;shoe.dispatchEvent(new Event('change'));}
    if(value.sock&&[...sock.options].some(option=>option.value===value.sock)){sock.value=value.sock;sock.dispatchEvent(new Event('change'));}
    accessories.set(value.accessories||[]);syncActions();
    status.textContent='Mannequin selections copied into the outfit draft. Save to keep them.';
  }});
  pool.onchange=()=>{selectedLayer='';accessories.set([]);updateTops();status.textContent='Choose pieces for this clothing fit, then save.';};
  name.oninput=syncActions;
  top.onchange=()=>{selectedLayer='';updateShoes(shoe.value);updateSocks(sock.value);updateBottoms(bottom.value);};bottom.onchange=paint;shoe.onchange=paint;sock.onchange=paint;
  for(const kind of ['Top','Bottom','Sock','Shoe'])get(kind+'Browse').onclick=()=>{
    const isTop=kind==='Top',select=isTop?top:kind==='Shoe'?shoe:kind==='Sock'?sock:bottom;
    const choices=choicesForPool(),original=catalogById.get(top.value);
    window.ClothingPicker.open({title:kind+' · Outfit creator',selected:select.value,filterKind:kind==='Top'?'tops':kind==='Bottom'?'bottoms':kind==='Shoe'?'shoes':kind==='Sock'?'socks':undefined,
      noteText:'This edits your outfit draft only. Save it, then apply it to a selected Blacktop player.',
      items:[...select.options].map(o=>{
        const preset=isTop?catalogById.get(o.value):null;
        return {id:o.value,label:o.textContent,
          experimental:isTop?preset?.experimental:kind==='Shoe'&&choices.extraShoes.get(o.value)?.experimental,
          asset:isTop?(preset?.previewAsset||preset?.top):o.value||original?.[kind==='Sock'?'socks':'shoes']?.[0],
          renderAsset:isTop?preset?.top:undefined};
      }),
      onChoose:id=>{select.value=id;select.dispatchEvent(new Event('change'));}});
  };
  save.onclick=()=>operation(async()=>{
    const record=await window.modLauncher.saveOutfitPreset({id:active||undefined,...draft()});
    library=await window.modLauncher.listOutfitPresets();openRecord(record.id);
    status.textContent=`Saved “${record.name}”. Available below and in the F8 panel after Refresh players.`;
    window.dispatchEvent(new CustomEvent('blacktop-presets-changed',{detail:library.outfits}));
  });
  get('Export').onclick=()=>operation(async()=>{
    const value=draft();value.name=value.name.trim()||'Custom outfit';
    const result=await window.modLauncher.exportOutfitPreset(value);
    status.textContent=result?'Exported .2koutfit — ready to share.':'Export canceled.';
  });
  get('Import').onclick=()=>{if(canLeave())operation(async()=>{
    const record=await window.modLauncher.importOutfitPreset();if(!record){status.textContent='Import canceled.';return;}
    library=await window.modLauncher.listOutfitPresets();openRecord(record.id);
    status.textContent=`Imported “${record.name}”. Saved without changing any player.`;
    window.dispatchEvent(new CustomEvent('blacktop-presets-changed',{detail:library.outfits}));
  });};
  remove.onclick=()=>{if(active&&window.confirm(`Delete “${name.value}” from saved outfits? This will not change a player’s current clothes.`))operation(async()=>{
    await window.modLauncher.deleteOutfitPreset(active);library=await window.modLauncher.listOutfitPresets();openRecord('');status.textContent='Outfit deleted. Players were not changed.';
    window.dispatchEvent(new CustomEvent('blacktop-presets-changed',{detail:library.outfits}));
  });};
  operation(async()=>{library=await window.modLauncher.listOutfitPresets();openRecord('');});
})();
