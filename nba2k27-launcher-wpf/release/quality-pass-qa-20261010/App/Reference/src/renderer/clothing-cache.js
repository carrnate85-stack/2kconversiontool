(() => {
 if(document.documentElement.classList.contains('roster-workspace'))return;
  const start=document.getElementById('cacheClothingPreviews'),cancel=document.getElementById('cancelClothingCache');
  const importPack=document.getElementById('importPreviewPack'),packLabel=document.getElementById('previewPackStatus');
  const progress=document.getElementById('clothingCacheProgress'),label=document.getElementById('clothingCacheStatus');
  if(!start)return;
  function render(s) {
    const busy=['indexing','running','cancelling'].includes(s.phase);
    start.disabled=busy;cancel.hidden=!busy;cancel.disabled=s.phase==='cancelling';progress.hidden=!busy;
    if(s.total){progress.max=s.total;progress.value=s.completed;}else progress.removeAttribute('value');
    const counts=`${s.batch?`Batch ${s.batch} / ${s.batches} · `:''}${s.completed} / ${s.total} unique previews checked · ${s.ready} ready · ${s.missing} without previews · ${s.failed} failed`;
    label.textContent=s.phase==='idle'?'Previews also save as you browse.'
      :s.phase==='indexing'?'Finding available clothing previews…'
      :s.phase==='cancelling'?'Finishing active pictures, then stopping…'
      :s.phase==='cancelled'?`Stopped. ${counts}. Saved pictures will be reused next time.`
      :s.phase==='failed'?`Could not prepare previews: ${s.error}`
      :s.phase==='complete'?`Finished. ${counts}.`:counts;
  }
  async function invoke(fn){try{render(await fn());}catch(error){label.textContent=error.message;start.disabled=false;cancel.hidden=true;progress.hidden=true;}}
  start.addEventListener('click',()=>{start.disabled=true;invoke(()=>window.modLauncher.startClothingCache());});
  cancel.addEventListener('click',()=>invoke(()=>window.modLauncher.cancelClothingCache()));
  window.modLauncher.onClothingCacheStatus?.(render);
  if(window.modLauncher.clothingCacheStatus)invoke(()=>window.modLauncher.clothingCacheStatus());
  window.modLauncher.previewPackStatus?.().then(value=>{if(packLabel)packLabel.textContent=value.installed?'Rendered preview ZIP saved locally and reused every launch.':'Optional rendered previews: import the ZIP to use them.';}).catch(()=>{});
  window.modLauncher.onPreviewPackProgress?.(value=>{
    if(!packLabel)return;
    if(value.phase==='copying')packLabel.textContent=`Importing preview ZIP… ${Math.round(100*value.copied/value.total)}%`;
    else if(value.phase==='validating')packLabel.textContent='Checking preview ZIP…';
  });
  importPack?.addEventListener('click',async()=>{
    importPack.disabled=true;
    try{
      const result=await window.modLauncher.importPreviewPack();
      if(result.cancelled){packLabel.textContent='Preview ZIP import canceled.';return;}
      window.PreviewImageUrl?.refresh?.();
      window.dispatchEvent(new Event('preview-pack-changed'));
      packLabel.textContent=`Saved ${result.count.toLocaleString()} rendered previews. Reused every launch.`;
    }catch(error){packLabel.textContent=`Preview ZIP not imported: ${error.message}`;}
    finally{importPack.disabled=false;}
  });
})();
