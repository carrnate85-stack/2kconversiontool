(() => {
 if(document.documentElement.classList.contains('roster-workspace'))return;
  const list = document.getElementById('blacktopPlayers');
  const status = document.getElementById('blacktopStatus');
  const refresh = document.getElementById('refreshBlacktop');
  const picker = document.getElementById('blacktopPlayerPicker');
  const randomAll=document.getElementById('blacktopRandomAll')||document.createElement('button');randomAll.textContent='Random all players';randomAll.type='button';randomAll.disabled=true;if(!randomAll.isConnected&&refresh)refresh.before(randomAll);
  const playerSelect=document.getElementById('blacktopPlayerSelect');
  const editPlayer=document.getElementById('editBlacktopPlayer');
  const randomPlayer=document.getElementById('blacktopRandomPlayer');
  randomPlayer?.addEventListener('click',()=>{
    const openMenu=document.querySelector('.blacktop-random-menu');
    if(openMenu){openMenu._close?.();return;}
    if(!busy)list.querySelector('.blacktop-outfit-row:not([hidden])')?._openRandomMenu?.();
  });
  const collapse=document.getElementById('collapseBlacktopPanel');
  const closeEditor=document.getElementById('closeBlacktopEditor');
  const clearSecondaryPanes=()=>{
    document.body.classList.remove('rules-open');
    document.getElementById('blacktopRulesPane').hidden=true;
  };
  let browseIndex=0;
  list.addEventListener('click',event=>{const button=event.target.closest('.blacktop-browse,.accessory-slot-browse');if(button)browseIndex=[...button.closest('article').querySelectorAll('.blacktop-browse,.accessory-slot-browse')].indexOf(button);},true);
  function expandEditor(expanded, exceptButtonId=null) {
    if(!playerSelect)return Promise.resolve(false);
    const alreadyExpanded=!document.body.classList.contains('compact-bar')&&Boolean(document.body.dataset.overlayView);
    document.querySelector('.blacktop-random-menu')?._close?.();
    for(const id of ['arenaMenuButton','scoreboardMenuButton','ballMenuButton']){
      if(expanded&&id===exceptButtonId)continue;
      const button=document.getElementById(id);
      if(button?._closeCatalog)button._closeCatalog();
      else if(button?.getAttribute('aria-expanded')==='true')button.click();
    }
    const active=expanded?(exceptButtonId||'editBlacktopPlayer'):'';
    clearSecondaryPanes();
    if(active==='openBlacktopRules'){
      document.body.classList.add('rules-open');
      document.getElementById('blacktopRulesPane').hidden=false;
    }
    document.body.dataset.overlayView=active;
    for(const id of ['editBlacktopPlayer','openBlacktopRules','scoreboardMenuButton','ballMenuButton'])document.getElementById(id)?.setAttribute('aria-pressed',String(id===active));
    document.body.classList.toggle('compact-bar',!expanded);
    collapse.hidden=!expanded;
    closeEditor.hidden=!expanded;
    // Switching content inside an expanded panel does not resize/refocus its
    // native window or wait for another UI/backend/native round trip.
    if(expanded&&alreadyExpanded)return Promise.resolve(true);
    return window.modLauncher.expandBlacktopPanel(expanded)
      .then(result=>result?.expanded===undefined||result.expanded===expanded)
      .catch(()=>false);
  }
  window.BlacktopOverlay={expand:expandEditor};
  editPlayer?.addEventListener('click',()=>expandEditor(document.body.dataset.overlayView!=='editBlacktopPlayer','editBlacktopPlayer'));
  collapse?.addEventListener('click',()=>{clearSecondaryPanes();expandEditor(false);});
  closeEditor?.addEventListener('click',()=>{clearSecondaryPanes();expandEditor(false);});
  window.modLauncher.onBlacktopPanelCollapsed?.(()=>{clearSecondaryPanes();expandEditor(false);});
  let selectedPlayerKey = null;
  let snapshot = null;
  let busy = false;
  let pendingApply;
  let savedPresets=[];
  let renderedSignature=null;
  let rowUpdates=[];
  function applyReportMessage(report){
    if(!report.unchanged&&!(report.consumed&&report.pointerMatches&&report.uniformMatches&&report.countMatches!==false))
      return 'Outfit request sent, but the update was not confirmed. Keep player selection open, then reopen the overlay.';
    if(report.matchCarryover==='unsupported-piece-count')
      return 'Outfit changed at player selection, but match carry-over needs 3–7 verified clothing pieces.';
    if(report.matchCarryover!=='queued')
      return 'Outfit changed at player selection, but match carry-over was not verified. Reopen the overlay before starting the match.';
    return '';
  }
  function updateRandomAll(){
    const players=snapshot?.players||[];
    randomAll.disabled=busy||!players.length||players.some(p=>p.error||!['male','female'].includes(p.clothingPool));
    if(playerSelect)playerSelect.disabled=!players.length;
    if(editPlayer)editPlayer.disabled=!players.length;
    if(randomPlayer)randomPlayer.disabled=busy||!players.length;
  }
  if(window.modLauncher.blacktopCarryStatus){
    const carry=document.createElement('p');carry.className='blacktop-session-status';status.after(carry);
    const update=state=>{carry.textContent=state?.message||'';if(state?.phase==='warning'||state?.phase==='error')status.textContent=state.message;};
    window.modLauncher.onBlacktopCarryStatus?.(update);
    window.modLauncher.blacktopCarryStatus().then(update).catch(()=>{});
  }
  window.addEventListener('blacktop-presets-changed',event=>{
    savedPresets=event.detail||[];if(!busy)render();
  });
  document.addEventListener('visibilitychange',()=>{if(document.hidden)clearTimeout(pendingApply);});
  const node = (tag, text, className) => {
    const element = document.createElement(tag);
    if (text != null) element.textContent = text;
    if (className) element.className = className;
    return element;
  };
  function render() {
    clearTimeout(pendingApply);
    const signature=JSON.stringify({presets:savedPresets,players:(snapshot?.players||[]).map(p=>({slot:p.slot,name:p.name,cyberface:p.cyberface,pool:p.clothingPool,error:p.error,outfits:p.outfits.filter(o=>o.id!=='custom')}))});
    if(snapshot&&signature===renderedSignature&&rowUpdates.length){
      for(const update of rowUpdates)update();
      updateRandomAll();
      return;
    }
    renderedSignature=signature;rowUpdates=[];
    list.replaceChildren();
    picker?.replaceChildren();
    playerSelect?.replaceChildren();
    const playerKey = p => `${snapshot?.pid}:${p.slot}:${p.roster || p.name}`;
    const players = snapshot?.players || [];
    updateRandomAll();
    if (!players.some(p => playerKey(p) === selectedPlayerKey)) selectedPlayerKey = players.length ? playerKey(players[0]) : null;
    const panels = [];
    const choosePlayer = key => {
      clearTimeout(pendingApply);
      selectedPlayerKey = key;
      if(playerSelect){playerSelect.value=key;playerSelect.dispatchEvent(new Event('blacktop-player-selection'));}
      for (const panel of panels) {
        const active = panel.key === key;
        panel.row.hidden = !active;
        panel.button.setAttribute('aria-pressed', String(active));
        if (active && !panel.built) { panel.built = true; panel.build(); }
      }
    };
    if(playerSelect)playerSelect.onchange=()=>choosePlayer(playerSelect.value);
    if (picker && !players.length) list.append(node('p', 'No players to show. Select players in Blacktop, then reopen this overlay.', 'blacktop-empty'));
    for (const player of snapshot?.players || []) {
      const row = node('article', null, 'card blacktop-outfit-row');
      if (picker) {
        const key = playerKey(player);
        const button = node('button', null, 'blacktop-player-button');
        button.type = 'button';
        button.append(node('small', `Player ${player.slot + 1}`), node('span', player.name || `Player ${player.slot + 1}`));
        if (player.error) button.append(node('small', 'Needs reload'));
        button.setAttribute('aria-controls', `blacktop-player-${player.slot}`);
        row.id = `blacktop-player-${player.slot}`;
        row.setAttribute('aria-label', `Outfit for ${player.name || 'player'}`);
        panels.push({ key, row, button, build: buildRow, built: false });
        button.addEventListener('click', () => choosePlayer(key));
        picker.append(button);
        if(playerSelect){const option=node('option',player.name||`Player ${player.slot+1}`);option.value=key;playerSelect.append(option);}
      }
      function buildRow() {
      const latest=snapshot?.players.find(p=>p.slot===player.slot);
      if(latest)Object.assign(player,latest);
      const title = node('div', null, 'blacktop-player-heading');
      const identity = node('div', null, 'blacktop-player-identity');
      identity.append(node('strong', player.name || `Player ${player.slot + 1}`), node('small', `Selected slot ${player.slot + 1}`));
      title.append(identity);
      row.append(title);
      if (player.error) { row.append(node('p', player.error)); return; }
      const loaded = player.outfits.filter(o => o.items?.length === 2);
      const street = snapshot.streetwear || [];
      const streetById=new Map(street.map(p=>[p.id,p]));
      const streetOutfits = street.filter(p => p.pool === player.clothingPool).map(p => ({
        id:p.id, streetId:p.id, items:[p.id,p.bottom], itemLabels:[p.topLabel,p.bottomLabel],
        extras:p.extras,topAsset:p.previewAsset||p.top,topRenderAsset:p.top,experimental:p.experimental
      }));
      const outfits = [...loaded, ...streetOutfits];
      const topSources = new Map(), bottomSources = new Map();
      for (const outfit of outfits) {
        if (!topSources.has(outfit.items[0])) topSources.set(outfit.items[0], outfit);
        if (!bottomSources.has(outfit.items[1])) bottomSources.set(outfit.items[1], outfit);
      }
      const pieceTile = (browse, select, label, kind, assetFor) => {
        browse.classList.add('blacktop-piece-tile');
        browse.setAttribute('aria-label', `Change ${label.toLowerCase()}`);
        const category=node('span',label,'blacktop-piece-category');
        const picture=node('span',null,'blacktop-piece-preview');
        const name=node('span','Choose…','blacktop-piece-name');
        const change=node('span',null,'blacktop-piece-change');
        change.append(node('span','Change'),node('span','↗'));
        browse.replaceChildren(category,picture,name,change);
        let request=0;
        return () => {
          const token=++request;
          const option=select.selectedOptions[0];
          name.textContent=option?.textContent||`Choose ${label.toLowerCase()}`;
          name.title=name.textContent;
          picture.replaceChildren(window.AccessoryControls?.icon?.(kind)||node('span','—'));
          const asset=assetFor();
          if(!asset||!window.modLauncher.clothingThumbnail)return;
          const loadNative=()=>window.modLauncher.clothingThumbnail(asset).then(result=>{
            if(token!==request||!browse.isConnected||result?.state!=='ready'||!/^data:image\/png;base64,/.test(result.dataUrl))return;
            const image=node('img');image.src=result.dataUrl;image.alt='';picture.replaceChildren(image);
          }).catch(()=>{});
          const sockPreview=kind==='sock'&&select.value?window.SockPreviewCatalog?.get(select.value):null;
          const shoePreview=kind==='shoe'?window.ShoePreviewCatalog?.get(asset):null;
          const topPreview=kind==='top'?window.TopPreviewCatalog?.get(topSources.get(top.value)?.topRenderAsset):null;
          const bottomPreview=kind==='bottom'?window.BottomPreviewCatalog?.get(select.value):null;
          if(sockPreview){const image=node('img');image.src=window.PreviewImageUrl('sock',sockPreview.thumbnail);image.alt='Sock preview';image.onerror=()=>{if(token===request)loadNative();};picture.replaceChildren(image);}
          else if(shoePreview){const image=node('img');image.src=window.PreviewImageUrl('shoe',shoePreview.thumbnail);image.alt='Shoe preview';image.onerror=()=>{if(token===request)loadNative();};picture.replaceChildren(image);}
          else if(topPreview){const image=node('img');image.src=window.PreviewImageUrl('top',topPreview.thumbnail);image.alt='Top preview';image.onerror=()=>{if(token===request)loadNative();};picture.replaceChildren(image);}
          else if(bottomPreview){const image=node('img');image.src=window.PreviewImageUrl('bottom',bottomPreview.thumbnail);image.alt='Bottom preview';image.onerror=()=>{if(token===request)loadNative();};picture.replaceChildren(image);}
          else if(asset==='NO SHIRT')picture.textContent='No top';
          else loadNative();
        };
      };
      const loadedBottoms = new Set(loaded.map(o=>o.items[1]));
      const streetBottoms = new Set(streetOutfits.map(o=>o.items[1]));
      const selected = loaded.find(o => o.id === player.selected);
      const selectors = ['Top', 'Bottom'].map((label, index) => {
        const field = node('label', null, 'blacktop-piece');
        const select = node('select',null,'blacktop-piece-select'); select.setAttribute('aria-label', `${label} for ${player.name}`);
        const browse = node('button', `Browse ${label.toLowerCase()}s`, 'blacktop-browse'); browse.type='button';
        browse.addEventListener('click', () => window.ClothingPicker?.open({ title:`${label} · ${player.name}`, selected:select.value,filterKind:index===0?'tops':'bottoms',
          items:[...select.options].map(o => {
            const source=(index===0?topSources:bottomSources).get(o.value);
            return {id:o.value,label:o.textContent,asset:index===0 ? source?.topAsset || o.value : o.value,
              renderAsset:index===0 ? source?.topRenderAsset : undefined,
              experimental:Boolean(source?.experimental),unavailable:o.disabled ? o.title || 'Not compatible with this top.' : source?.unavailable};
          }), onChoose:id=>{ select.value=id; select.dispatchEvent(new Event('change')); } }));
        field.append(select, browse); return { field, select, browse };
      });
      const [top, bottom] = selectors.map(s => s.select);
      const shoeField=node('label',null,'blacktop-piece');
      const shoe=node('select',null,'blacktop-piece-select');shoe.setAttribute('aria-label',`Shoes for ${player.name}`);
      const shoeDefault=node('option','Style’s original shoes');shoeDefault.value='';shoe.append(shoeDefault);
      const rosterShoe=node('option','Player’s roster shoes');rosterShoe.value='roster';shoe.append(rosterShoe);
      const extraShoes=new Map((snapshot.shoes||[]).filter(s=>s.pools.includes(player.clothingPool)).map(s=>[s.asset,s]));
      const shoeAssets=[...new Set([...street.filter(p=>p.pool===player.clothingPool).flatMap(p=>p.shoes||[]),...extraShoes.keys()])];
      for(const asset of shoeAssets){const option=node('option',snapshot.shoeLabels?.[asset]||extraShoes.get(asset)?.label||asset.replace(/^female_/,'').replaceAll('_',' '));option.value=asset;shoe.append(option);}
      shoe.value=player.streetShoe==='roster'||shoeAssets.includes(player.streetShoe)?player.streetShoe:'';
      const shoeBrowse=node('button','Browse shoes','blacktop-browse');shoeBrowse.type='button';
      shoeBrowse.onclick=()=>window.ClothingPicker?.open({title:`Shoes · ${player.name}`,selected:shoe.value,filterKind:'shoes',
        items:[...shoe.options].map(o=>({id:o.value,label:o.textContent,experimental:extraShoes.get(o.value)?.experimental,asset:o.value==='roster'?null:o.value||streetById.get(top.value)?.shoes?.[0]})),
        onChoose:id=>{shoe.value=id;shoe.dispatchEvent(new Event('change'));}});
      shoeField.append(shoe,shoeBrowse);
      const sockField=node('label',null,'blacktop-piece');
      const sock=node('select',null,'blacktop-piece-select');sock.setAttribute('aria-label',`Socks for ${player.name}`);
      const sockDefault=node('option','Style’s original socks');sockDefault.value='';sock.append(sockDefault);
      const sockAssets=[...new Set(street.filter(p=>p.pool===player.clothingPool).flatMap(p=>p.socks||[]))];
      for(const asset of sockAssets){const option=node('option',window.ClothingCatalogUI.label(asset));option.value=asset;sock.append(option);}
      sock.value=sockAssets.includes(player.streetSock)?player.streetSock:'';
      const sockBrowse=node('button','Browse socks','blacktop-browse');sockBrowse.type='button';
      sockBrowse.onclick=()=>window.ClothingPicker?.open({title:`Socks · ${player.name}`,selected:sock.value,filterKind:'socks',
        items:[...sock.options].map(o=>({id:o.value,label:o.textContent,asset:o.value||streetById.get(top.value)?.socks?.[0]})),
        onChoose:id=>{sock.value=id;sock.dispatchEvent(new Event('change'));}});
      sockField.append(sock,sockBrowse);
      const syncPieceTiles=[
        pieceTile(selectors[0].browse,top,'Top','top',()=>topSources.get(top.value)?.topAsset||top.value),
        pieceTile(selectors[1].browse,bottom,'Bottom','bottom',()=>bottom.value),
        pieceTile(sockBrowse,sock,'Socks','sock',()=>sock.value||streetById.get(top.value)?.socks?.[0]),
        pieceTile(shoeBrowse,shoe,'Shoes','shoe',()=>shoe.value==='roster'?null:shoe.value||streetById.get(top.value)?.shoes?.[0])
      ];
      let selectedLayer=player.streetLayer||''; // Keep existing fits; changing tops uses the stock layer.
      const accessoryField=node('section');
      const accessories=window.AccessoryControls.mount(accessoryField,{choices:()=>(snapshot.accessories||[]).filter(a=>(a.asset.startsWith('female_')?'female':'male')===player.clothingPool),
        values:player.streetAccessories||(player.streetAccessory?[player.streetAccessory]:[]),
        baseCount:()=>window.AccessorySlots.baseCount(streetById.get(top.value),sock.value,selectedLayer)-(shoe.value==='roster'?1:0),onChange:()=>autoApply(),layout:'cards'});
      const apply = node('button', picker ? 'Apply outfit' : 'Apply', 'primary small');
      const hint = node('p', null, 'blacktop-piece-hint'); hint.setAttribute('aria-live', 'polite');
      const updateCombination = () => {
        const topValue=top.value,bottomValue=bottom.value;
        const match = outfits.find(o => o.items[0] === topValue && o.items[1] === bottomValue);
        const bottomSource = bottomSources.get(bottom.value);
        const topSource = topSources.get(top.value);
        const shoeMapped=topSource?.streetId&&streetById.get(topValue)?.shoes?.length===1;
        shoe.disabled=shoeBrowse.disabled=!shoeMapped;
        shoeDefault.textContent=shoeMapped?'Style’s original shoes':'Choose streetwear to edit shoes';
        sock.disabled=sockBrowse.disabled=!topSource?.streetId;
        accessories.refresh();
        sockDefault.textContent=topSource?.streetId?'Style’s original socks':'Choose streetwear to edit socks';
        const allowedBottoms = topSource?.streetId ? streetBottoms : loadedBottoms;
        const known = topSource && bottomSource;
        const unavailable = !allowedBottoms.has(bottom.value) ? 'Choose a compatible bottom for this top.' : match?.unavailable || bottomSource?.unavailable;
        apply.disabled = !known || Boolean(unavailable) || !accessories.valid;
        hint.textContent = unavailable || (!known ? 'Choose a top and bottom.' : topSource?.streetId
          ? `${!player.streetId?'Streetwear ready: choosing a piece applies this set. ':''}Socks and shoes are separate; check the fit in game.`
          : !match ? 'New combination; check the fit in game.' : '');
        hint.hidden = !hint.textContent;
        // Never silently change the other piece. The user chooses the replacement.
        for (const option of bottom.options) {
          const source = bottomSources.get(option.value);
          option.disabled = !source || Boolean(source.unavailable) || !allowedBottoms.has(option.value);
          const text = option.dataset.label ? `${option.dataset.label}${source?.unavailable ? ' — unavailable' : ''}` : 'Choose…';
          if(option.textContent!==text)option.textContent=text;
          option.title = source?.unavailable || (!allowedBottoms.has(option.value) ? 'Not compatible with this top.' : '');
        }
        for(const sync of syncPieceTiles)sync();
      };
      const fillCollection = () => {
        selectors.forEach(({ select,browse }, index) => {
          select.replaceChildren();
          const pieces = new Map();
          for (const outfit of outfits) pieces.set(outfit.items[index], outfit.itemLabels?.[index] || outfit.items[index].replaceAll('_', ' '));
          const counts = new Map(); const variants = new Map();
          for (const text of pieces.values()) counts.set(text,(counts.get(text)||0)+1);
          const fragment=document.createDocumentFragment();
          for (const [id, text] of pieces) {
            const n = (variants.get(text) || 0) + 1; variants.set(text, n);
            const label = counts.get(text) > 1 ? `${text} (style ${n})` : text;
            const experimental = (index===0?topSources:bottomSources).get(id)?.experimental;
            const display = label;
            const option = node('option', display); option.value = id; option.dataset.label = display; fragment.append(option);
          }
          select.append(fragment);
          select.disabled = !pieces.size;
          browse.disabled=select.disabled;
        });
        syncSelection();
      };
      const syncSelection=()=>{
        const selected=player.outfits.find(o=>o.id===player.selected);
        const defaults=street.filter(p=>p.pool===player.clothingPool&&p.separable&&!p.manualOnly&&p.shoes?.length===1);
        const streetDefault=defaults.find(p=>p.top===selected?.items?.[0])||defaults[0];
        const pair = topSources.get(player.streetId)?.streetId ? [player.streetId, player.streetBottom]
          : streetDefault ? [streetDefault.id,streetBottoms.has(selected?.items?.[1])?selected.items[1]:streetDefault.bottom]
          : selected?.items || outfits[0]?.items;
        if (pair) { top.value = pair[0]; bottom.value = pair[1]; }
        selectedLayer=player.streetLayer||'';
        shoe.value=player.streetShoe==='roster'||shoeAssets.includes(player.streetShoe)?player.streetShoe:'';
        sock.value=sockAssets.includes(player.streetSock)?player.streetSock:'';
        accessories.set(player.streetAccessories||(player.streetAccessory?[player.streetAccessory]:[]));
        updateCombination();
      };
      fillCollection();
      const act = (restoreOriginal,randomFit=false,randomMode='streetwear') => operation(async () => {
        const result = await window.modLauncher.applyBlacktop({ token: snapshot.token, slot: player.slot,omitCatalog:true,
          top: top.value, bottom: bottom.value, restore: restoreOriginal,randomFit,randomMode,
          ...(topSources.get(top.value)?.streetId ? { streetId: top.value,shoe:streetById.get(top.value)?.shoes?.length===1?shoe.value:'',sock:sock.value,layer:selectedLayer,accessories:accessories.values } : {}) });
        snapshot = {...snapshot,...result};
        const report = result.report;
        if(report.timings)status.title=`Game apply: ${report.timings.nativeApplyMs} ms; verification: ${report.timings.verifiedScanMs} ms`;
        return applyReportMessage(report);
      }, restoreOriginal ? 'Restoring outfit…' : 'Applying outfit…');
      const autoApply = () => {
        clearTimeout(pendingApply); updateCombination();
        if (!busy && !apply.disabled) pendingApply=setTimeout(()=>{if(!busy && row.isConnected && !row.hidden)act(false);},350);
      };
      top.addEventListener('change',()=>{
        selectedLayer='';
        const chosen=streetById.get(top.value);
        if(chosen&&!streetBottoms.has(bottom.value))bottom.value=chosen.bottom;
        autoApply();
      }); bottom.addEventListener('change', autoApply);
      shoe.addEventListener('change',autoApply);
      sock.addEventListener('change',autoApply);
      const randomChoices=street.filter(p=>p.pool===player.clothingPool&&p.separable&&p.shoes?.length===1);
      row._runRandomMode=mode=>{clearTimeout(pendingApply);return act(false,true,mode);};
      row._openRandomMenu=()=>{
        if(busy||!randomChoices.length)return;
        document.querySelector('.blacktop-random-menu')?._close?.();
        const compact=document.body.classList.contains('compact-bar'),anchor=randomPlayer;
        if(!compact||!anchor)return;
        const menu=node('div',null,'blacktop-random-menu');menu.id='blacktopRandomMenu';menu.setAttribute('role','menu');
        const rows=innerWidth<700?2:1;
        const close=(resize=true)=>{menu.remove();anchor.setAttribute('aria-expanded','false');document.body.classList.remove('random-menu-open');document.removeEventListener('pointerdown',outside);document.removeEventListener('keydown',keys);if(compact&&resize)window.modLauncher.sizeBlacktopPlayerMenu?.(0).catch(()=>{});};
        Object.assign(menu,{_close:close});
        const outside=e=>{if(e.target.closest?.('#blacktopPlayerMenu,#blacktopPlayerMenuButton'))return;if(!menu.contains(e.target)&&e.target!==anchor)close();};
        const keys=e=>{if(e.key==='Escape'){e.preventDefault();e.stopImmediatePropagation();close();anchor.focus();}};
        for(const [mode,label] of [['streetwear','Random streetwear'],['team','Random team clothes'],['all','Random all'],['accessories','Random accessories']]){
          const button=node('button',label,'secondary small');button.type='button';button.setAttribute('role','menuitem');
          button.onclick=async()=>{
            if(busy)return;
            const target=list.querySelector('.blacktop-outfit-row:not([hidden])');
            if(!target?._runRandomMode){close();return;}
            for(const item of menu.children)item.disabled=true;
            try{await target._runRandomMode(mode);}finally{for(const item of menu.children)item.disabled=false;}
          };menu.append(button);
        }
        document.body.append(menu);anchor.setAttribute('aria-controls',menu.id);anchor.setAttribute('aria-expanded','true');const rect=anchor.getBoundingClientRect();
        if(compact){document.body.classList.add('random-menu-open');window.modLauncher.sizeBlacktopPlayerMenu?.(rows).catch(()=>{});}
        Object.assign(menu.style,{position:'fixed',zIndex:'10000',display:'grid',gap:'4px',padding:'8px',border:'1px solid var(--border, #506052)',borderRadius:'8px',background:'var(--panel, #18251c)',boxShadow:'0 8px 24px #0005',left:`${Math.max(8,Math.min(rect.left,innerWidth-210))}px`,top:`${compact?Math.ceil(document.querySelector('.wardrobe-topbar').getBoundingClientRect().bottom):Math.max(8,Math.min(rect.bottom+4,innerHeight-menu.offsetHeight-8))}px`,minWidth:'194px'});
        if(compact)Object.assign(menu.style,{left:'12px',right:'12px',minWidth:'0',gridTemplateColumns:`repeat(${innerWidth<700?2:4}, minmax(0,1fr))`,boxShadow:'none',gap:'6px',padding:'8px',borderRadius:'6px'});
        document.addEventListener('pointerdown',outside);document.addEventListener('keydown',keys);menu.firstElementChild.focus();
      };
      title.append(node('small','Compatible choices apply automatically.','blacktop-auto-apply-note'));
      row.append(...selectors.map(s => s.field),sockField,shoeField,accessoryField,hint);
      const presetBar=node('div',null,'blacktop-saved-outfit');
      const presetSelect=node('select');presetSelect.setAttribute('aria-label',`Saved outfit for ${player.name}`);
      const placeholder=node('option','Choose a saved outfit…');placeholder.value='';presetSelect.append(placeholder);
      for(const preset of savedPresets.filter(p=>p.available&&p.pool===player.clothingPool)) {
        const option=node('option',preset.name);option.value=preset.id;presetSelect.append(option);
      }
      const presetApply=node('button','Apply preset','secondary small');presetApply.type='button';presetApply.disabled=true;
      presetSelect.disabled=presetSelect.options.length===1;
      if(presetSelect.disabled)placeholder.textContent='No saved outfits for this clothing fit';
      presetSelect.onchange=()=>{clearTimeout(pendingApply);presetApply.disabled=!presetSelect.value;};
      presetApply.onclick=()=>{
        clearTimeout(pendingApply);
        operation(async()=>{
          snapshot={...snapshot,...await window.modLauncher.applyBlacktop({token:snapshot.token,slot:player.slot,outfitPresetId:presetSelect.value,omitCatalog:true})};
          const report=snapshot.report;
          return applyReportMessage(report);
        },'Applying saved outfit…');
      };
      presetBar.append(presetSelect,presetApply);
      if(playerSelect)identity.after(presetBar);else row.append(presetBar);
      rowUpdates.push(()=>{
        const fresh=snapshot.players.find(p=>p.slot===player.slot);
        if(!fresh)return;
        Object.assign(player,fresh);syncSelection();
        if(!row.hidden){
          const activeSelect=[top,bottom,sock,shoe,...accessoryField.querySelectorAll('select')][browseIndex];
          if(activeSelect)window.ClothingPicker?.updateSelected?.(activeSelect.value,new Map([...activeSelect.options].map(o=>[o.value,o.disabled])));
        }
      });
      }
      list.append(row);
      if (!picker) buildRow();
    }
    if (picker) choosePlayer(selectedPlayerKey);
  }
  async function operation(action, message) {
    if (busy) return;
    busy = true;if(refresh)refresh.disabled=true;randomAll.disabled=true; status.textContent = message; status.classList.add('blacktop-busy');
    updateRandomAll();
    list.inert=true;
    try { status.textContent = await action(); }
    catch (error) { snapshot = null; status.textContent = `${error.message} Reopen the overlay to reconnect; original outfits are saved.`; }
    finally { busy = false;if(refresh)refresh.disabled=false;list.inert=false;status.classList.remove('blacktop-busy');randomAll.textContent='Random all players';randomAll.removeAttribute('aria-busy');render(); }
  }
  randomAll.addEventListener('click',()=>{clearTimeout(pendingApply);operation(async()=>{
    randomAll.setAttribute('aria-busy','true');
    const result=await window.BlacktopBatch.randomAll(window.modLauncher,snapshot,(done,total,name)=>{status.textContent=`Randomizing ${done+1}/${total}: ${name}…`;randomAll.textContent=`Applying ${done+1}/${total}…`;});
    snapshot=result.snapshot;return '';
  },'Randomizing all players…');});
  const reloadPlayers=()=>operation(async () => {
    savedPresets=(await window.modLauncher.listOutfitPresets({omitCatalog:true})).outfits;
    // Catalog metadata belongs to this renderer/app version, not a game
    // session. Reuse it, but always scan players and get a fresh apply token.
    snapshot = {...snapshot,...await window.modLauncher.scanBlacktop({omitCatalog:Array.isArray(snapshot?.streetwear)})};
    return picker ? ''
      : `Found ${snapshot.players.length} selected player${snapshot.players.length === 1 ? '' : 's'}. Tops and bottoms can now be mixed independently. New combinations need a visual check in game.`;
  }, 'Checking Blacktop players…');
  refresh?.addEventListener('click',reloadPlayers);
  document.getElementById('hideBlacktopPanel')?.addEventListener('click', () => window.modLauncher.hideBlacktopPanel());
  window.modLauncher.onBlacktopPanelShown?.(() => { expandEditor(false); reloadPlayers(); });
  // No periodic refresh: keep editing controls stable after the overlay opens.
  const openPanel = document.getElementById('openBlacktopPanel');
  const panelHint = document.getElementById('blacktopPanelHint');
  let listenerStatusTimer;
  const showPanelStatus = result => {
    clearTimeout(listenerStatusTimer);
    if (panelHint && (result?.lastError || result?.diagnosticError)) {
      panelHint.textContent = `Panel error: ${result.lastError || result.diagnosticError} · Shortcut received: ${result.hotkeyPresses || 0} time(s).`;
      return;
    }
    if (panelHint && result?.listener && result.listener.state !== 'ready') {
      panelHint.textContent = result.listener.error || `F8 listener: ${result.listener.state}. Open game panel to retry if needed.`;
      if (result.listener.state === 'starting') listenerStatusTimer = setTimeout(() => {
        window.modLauncher.blacktopPanelStatus?.().then(showPanelStatus).catch(error => { panelHint.textContent = error.message; });
      }, 1000);
      return;
    }
    if (panelHint) panelHint.textContent = result?.registered
      ? `F8 listener ready (no hook). Panel ${result.ready ? 'loaded' : 'loading'} · ${result.visible ? 'visible' : 'hidden'} · ${result.focused ? 'focused' : 'not focused'}. Shortcut received: ${result.hotkeyPresses || 0} time(s). Diagnostics saved locally.`
      : 'F8 is unavailable (another app may be using it). Close other launcher copies, then click Open game panel to retry.';
  };
  openPanel?.addEventListener('click', async () => {
    try { showPanelStatus(await window.modLauncher.openBlacktopPanel()); }
    catch (error) { panelHint.textContent = error.message; }
  });
  window.modLauncher.blacktopPanelStatus?.().then(showPanelStatus).catch(error => { if (panelHint) panelHint.textContent = error.message; });
  if (panelHint) window.addEventListener('focus', () => {
    window.modLauncher.blacktopPanelStatus?.().then(showPanelStatus).catch(error => { panelHint.textContent = error.message; });
  });
})();
