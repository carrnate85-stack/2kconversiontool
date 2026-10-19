(() => {
  const trigger=document.getElementById('previewBlacktopOverlay');
  const dialog=document.getElementById('blacktopOverlayPreview');
  const frame=document.getElementById('blacktopOverlayPreviewFrame');
  if(!trigger||!dialog||!frame)return;
  let library=null,opening=0;
  const targetOrigin=location.protocol==='file:'?'*':location.origin;
  const thumbnailAssets=new Set();
  const selection=id=>document.getElementById(id)?.textContent?.trim()||'Not selected';
  const sendChoices=async()=>{
    const target=frame.contentWindow;
    const revision=opening;
    const [scoreboardState,ballState,loadedLibrary]=await Promise.all([
      window.modLauncher.scoreboardStatus().catch(()=>null),
      window.modLauncher.ballStatus().catch(()=>null),
      library?Promise.resolve(library):window.modLauncher.listOutfitPresets()
    ]);
    if(revision!==opening)return;
    library=loadedLibrary;
    thumbnailAssets.clear();
    for(const p of library.catalog||[])for(const asset of [p.top,p.previewAsset,p.bottom,...(p.shoes||[]),...(p.socks||[])])if(asset)thumbnailAssets.add(asset);
    for(const item of [...(library.accessories||[]),...(library.shoes||[])])thumbnailAssets.add(item.asset);
    if(target!==frame.contentWindow)return;
    target?.postMessage({type:'blacktop-preview-choices',
      arena:selection('arenaSelected'),scoreboard:selection('scoreboardSelected'),ball:selection('ballSelected'),scoreboardState,ballState,library,
      draft:{streetId:document.getElementById('outfitTop')?.value,bottom:document.getElementById('outfitBottom')?.value,
        shoe:document.getElementById('outfitShoe')?.value,sock:document.getElementById('outfitSock')?.value}},targetOrigin);
  };
  trigger.addEventListener('click',()=>{
    opening++;library=null;
    dialog.classList.remove('rules-open');
    dialog.showModal();
    // Each opening reloads the actual F8 markup and styles, not a second hand-built bar.
    frame.src=`blacktop-overlay-preview-frame.html?opened=${Date.now()}`;
  });
  frame.addEventListener('load',sendChoices);
  window.addEventListener('message',event=>{
    if(event.source!==frame.contentWindow||event.origin!==location.origin)return;
    if(event.data?.type==='blacktop-preview-ready')sendChoices();
    if(event.data?.type==='blacktop-preview-view')dialog.classList.toggle('rules-open',event.data.view!=='compact');
    if(event.data?.type==='blacktop-preview-close')dialog.close();
    if(event.data?.type==='blacktop-preview-thumbnail'){
      const {id,asset}=event.data,target=frame.contentWindow;
      if(!Number.isSafeInteger(id)||!thumbnailAssets.has(asset))return;
      // This bridge exposes only cached pictures, never game or outfit writes.
      window.modLauncher.cachedClothingThumbnail(asset).catch(()=>null).then(result=>{
        if(target===frame.contentWindow)target.postMessage({type:'blacktop-preview-thumbnail-result',id,asset,result},targetOrigin);
      });
    }
  });
  document.getElementById('closeBlacktopOverlayPreview').addEventListener('click',()=>dialog.close());
  dialog.addEventListener('click',event=>{if(event.target===dialog)dialog.close();});
})();
