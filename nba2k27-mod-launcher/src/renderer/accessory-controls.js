(() => {
 const iconPaths={headwear:'M3 14h18 M6 14V8c0-5 12-5 12 0v6 M9 6v8',headphones:'M4 14V9a8 8 0 0 1 16 0v5 M3 12h4v8H3z M17 12h4v8h-4z',neckwear:'M4 5c4 7 12 7 16 0 M11 12h2v6h-2z',bodywear:'M7 3l10 12 M6 12h13v9H6z',waist:'M3 10h18v5H3z M9 9h6v7H9z',leftPiercing:'M8 4c8-5 12 4 6 9 M9 15a3 3 0 1 0 6 0',rightPiercing:'M8 4c8-5 12 4 6 9 M9 15a3 3 0 1 0 6 0',hand:'M7 11V4 M10 11V2 M13 11V3 M16 11V5 M7 9l-3 2 3 10h10l3-10-4-1',wrist:'M7 2h10v7H7z M7 15h10v7H7z M5 9h14v6H5z',faceMask:'M5 8l7-3 7 3v8l-7 5-7-5z M7 10h3 M14 10h3 M10 15h4 M5 8L2 6 M19 8l3-2',headband:'M4 10c4-3 12-3 16 0v5c-4-3-12-3-16 0z',eyewear:'M3 10h7v5H3z M14 10h7v5h-7z M10 11h4 M1 9l2 2 M23 9l-2 2',backpack:'M8 7V5a4 4 0 0 1 8 0v2 M6 7h12v14H6z M9 13h6v5H9z',arm:'M10 3l5 2-2 8 4 6-4 3-6-8z',leg:'M8 2h8l-2 11 3 8H9l-3-8z',top:'M8 3l4 2 4-2 5 5-4 3-1-2v12H8V9l-1 2-4-3z',bottom:'M6 3h12l2 17h-7l-1-9-1 9H4z',sock:'M8 3h8v12l-6 6H4v-5l4-3z',shoe:'M5 7l5 5 5 1 5 4v3H3v-8z'};
 function icon(kind){const svg=document.createElementNS('http://www.w3.org/2000/svg','svg');svg.setAttribute('viewBox','0 0 24 24');svg.setAttribute('aria-hidden','true');svg.setAttribute('class','outfit-item-icon');const path=document.createElementNS(svg.namespaceURI,'path');path.setAttribute('d',iconPaths[kind]||iconPaths[kind.endsWith('Arm')?'arm':kind.endsWith('Hand')?'hand':kind.endsWith('Wrist')?'wrist':'leg']);path.setAttribute('fill','none');path.setAttribute('stroke','currentColor');path.setAttribute('stroke-width','1.3');path.setAttribute('stroke-linejoin','round');svg.append(path);return svg;}
 window.AccessoryControls={icon,mount(host,{choices,values=[],baseCount,onChange,layout}){
  const cards=layout==='cards';if(cards)host.classList.add('accessory-cards');
  host.classList.add('accessory-slots');let selected=[...values];
  let catalogKey='';const choicesBySlot=new Map(window.AccessorySlots.slots.map(([key])=>[key,[]]));
  const heading=document.createElement('div');heading.className='accessory-slots-heading';
  const title=document.createElement('strong');title.textContent='Accessories';
  const capacity=document.createElement('span');capacity.setAttribute('role','status');
  const clear=document.createElement('button');clear.type='button';clear.textContent='Remove all';
  clear.onclick=()=>{selected=[];refresh();onChange();};heading.append(title,capacity,clear);
  const grid=document.createElement('div');grid.className='accessory-slots-grid';host.append(heading,grid);
  const primaryKeys=['headband','eyewear','faceMask','backpack','leftArm','rightArm','leftLeg','rightLeg'];
  const fields=new Map();
  if(cards)grid.classList.add('accessory-primary-grid');
  const controls=window.AccessorySlots.slots.map(([key,label])=>{
   const field=document.createElement('div');field.className='accessory-slot';field.dataset.slot=key;
   const caption=document.createElement('label');caption.textContent=label;
   const select=document.createElement('select');select.setAttribute('aria-label',label+' accessory');caption.append(select);
   const browse=document.createElement('button');browse.type='button';browse.className='accessory-slot-browse';browse.textContent='Browse';browse.setAttribute('aria-label','Browse '+label.toLowerCase());
   const choose=value=>{selected=selected.filter(a=>window.AccessorySlots.slot(a)!==key);if(value)selected.push(value);refresh();onChange();};
   select.onchange=()=>choose(select.value);
   browse.onclick=()=>window.ClothingPicker.open({title:label+' · Accessories',selected:select.value,filterKind:'accessories',
    items:[...select.options].map(o=>({id:o.value,label:o.textContent,asset:o.value,unavailable:o.disabled?'No accessory space left. Remove another item first.':undefined})),onChoose:choose});
   if(cards){
    caption.hidden=true;
    const visual=document.createElement('span');visual.className='accessory-tile-preview';
    const category=document.createElement('span');category.className='accessory-tile-category';category.textContent=label;
    const itemName=document.createElement('span');itemName.className='accessory-tile-name';
    browse.className='accessory-tile';browse.replaceChildren(visual,category,itemName);
    const remove=document.createElement('button');remove.type='button';remove.className='accessory-tile-remove';remove.textContent='×';remove.setAttribute('aria-label','Remove '+label.toLowerCase());remove.onclick=()=>choose('');
    field.append(caption,browse,remove);fields.set(key,field);grid.append(field);
    return {key,label,select,browse,field,visual,itemName,remove};
   }
   field.append(caption,browse);grid.append(field);return {key,select,browse};
  });
  if(cards)for(const key of [...primaryKeys,...window.AccessorySlots.slots.map(([key])=>key).filter(key=>!primaryKeys.includes(key))])grid.append(fields.get(key));
  function refresh(){
   const catalog=choices(),nextKey=JSON.stringify(catalog.map(a=>[a.asset,a.label]));
   if(nextKey!==catalogKey){catalogKey=nextKey;for(const bucket of choicesBySlot.values())bucket.length=0;for(const item of catalog)choicesBySlot.get(window.AccessorySlots.slot(item.asset))?.push(item);for(const c of controls)c.optionsReady=false;}
   const count=baseCount(),available=Math.max(0,7-count),used=selected.length;
   capacity.textContent=cards?`${used} / ${available} equipped`:`${used} of ${available} spaces used`;capacity.classList.toggle('over-capacity',used>available);
   capacity.title='Seven total clothing pieces, including top, bottom, socks, shoes and required layers.';clear.disabled=!used;
   for(const c of controls){
    const value=selected.find(a=>window.AccessorySlots.slot(a)===c.key)||'';
    const option=(asset,label)=>{const o=document.createElement('option');o.value=asset;o.textContent=label;return o;};
    if(!c.optionsReady){
     const fragment=document.createDocumentFragment();fragment.append(option('','None'));
     for(const a of choicesBySlot.get(c.key)||[])fragment.append(option(a.asset,a.label));
     c.select.replaceChildren(fragment);c.optionsReady=true;
    }
    c.select.value=value;
    c.select.title=c.select.selectedOptions[0]?.textContent||'None';
    for(const o of c.select.options)o.disabled=Boolean(o.value&&!value&&used>=available);
    c.select.disabled=count>=7&&!value;c.browse.disabled=c.select.disabled;
    if(cards){
     c.field.classList.toggle('is-equipped',Boolean(value));c.remove.hidden=!value;
     c.itemName.textContent=value?(c.select.selectedOptions[0]?.textContent||window.AccessorySlots.label(value)):'Add item';
     c.browse.setAttribute('aria-label',`${value?'Change':'Add'} ${c.label.toLowerCase()}${value?': '+c.itemName.textContent:''}`);c.browse.title=c.itemName.textContent;
     if(c.previewAsset===value)continue;c.previewAsset=value;c.visual.replaceChildren(icon(c.key));
     const rendered=value?window.AccessoryPreviewCatalog?.get(value):null;
     const fallback=()=>{if(c.previewAsset===value&&c.browse.isConnected)c.visual.replaceChildren(icon(c.key));};
     const loadNative=()=>window.modLauncher.clothingThumbnail(value).then(result=>{if(c.previewAsset!==value||!c.browse.isConnected||result?.state!=='ready'||!/^data:image\/png;base64,/.test(result.dataUrl))return;const img=document.createElement('img');img.onerror=fallback;img.src=result.dataUrl;img.alt='';c.visual.replaceChildren(img);}).catch(()=>{});
     if(rendered){const img=document.createElement('img');img.src=window.PreviewImageUrl('accessory',rendered.thumbnail);img.alt='';img.onerror=()=>{if(c.previewAsset===value){fallback();loadNative();}};c.visual.replaceChildren(img);}
     else if(value)loadNative();
     continue;
    }
    if(c.previewAsset===value)continue;
    c.previewAsset=value;c.browse.textContent=value?'Change':'Browse';
    const rendered=value?window.AccessoryPreviewCatalog?.get(value):null;
    if(rendered){const img=document.createElement('img');img.src=window.PreviewImageUrl('accessory',rendered.thumbnail);img.alt='';img.onerror=()=>{if(c.previewAsset===value)loadNative();};c.browse.replaceChildren(img,document.createTextNode('Change'));}
    else if(value)loadNative();
    function loadNative(){return window.modLauncher.clothingThumbnail(value).then(result=>{
     if(c.previewAsset!==value||!c.browse.isConnected||result?.state!=='ready')return;
     if(!/^data:image\/png;base64,/.test(result.dataUrl))return;
     const img=document.createElement('img');img.src=result.dataUrl;img.alt='';c.browse.replaceChildren(img,document.createTextNode('Change'));
    }).catch(()=>{});}
   }
  }
  refresh();return {get values(){return [...selected];},set(values){selected=[...values];refresh();},refresh,refreshPreviews(){for(const c of controls)c.previewAsset=undefined;refresh();},get valid(){return selected.length+baseCount()<=7;}};
 }};
})();
