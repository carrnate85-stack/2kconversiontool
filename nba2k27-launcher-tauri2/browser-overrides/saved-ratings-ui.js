(()=>{
 'use strict';
 const panel=document.getElementById('studioDNA');if(!panel)return;
 const section=document.createElement('section'),title=document.createElement('h3'),note=document.createElement('p'),url=document.createElement('input'),file=document.createElement('input'),status=document.createElement('p');
 title.textContent='Import saved ratings page data';
 note.textContent='Choose saved JSON or HTML from a 2KDB or 2KAuction player page and enter its original URL. The page is read as text only. Supported values are reviewed and staged locally; Save & verify remains required. Source authenticity is not verified.';
 url.type='url';url.placeholder='Original HTTPS player page URL';url.setAttribute('aria-label','Saved ratings source URL');
 file.type='file';file.accept='.json,.html,.htm,.txt';file.setAttribute('aria-label','Saved ratings player data');
 status.setAttribute('role','status');section.append(title,note,url,file,status);panel.append(section);
 let epoch=0,player=null,busy=false;
 const reset=e=>{epoch++;player=e?.detail?.player||e?.detail||null;file.value='';status.textContent='';file.disabled=player?.source!=='live-roster';};
 window.addEventListener('player-editor:open',reset);window.addEventListener('player-editor:closed',()=>reset());window.addEventListener('player-editor:committed',reset);
 file.disabled=true;
 file.addEventListener('change',async()=>{
  const picked=file.files?.[0],owner=player,visit=epoch,address=url.value.trim();if(!picked||busy||owner?.source!=='live-roster')return;
  const current=()=>epoch===visit&&player===owner&&!document.getElementById('playerEditorModal').classList.contains('hidden');
  busy=true;file.disabled=true;
  try{
   if(picked.size>4000000)throw Error('Saved player data must be at most 4 MB.');
   window.SavedRatingsSource.address(address);const text=await picked.text();if(!current()||url.value.trim()!==address)return;
   const result=window.SavedRatingsSource.read(text,address,window.playerRatingSchema);
   const applied=await window.stageCatalogPlayerDna(result.data);if(!current())return;
   status.textContent=applied===false?'Import canceled. Existing edits are unchanged.':applied?.noChanges?'No supported changes to stage.':result.source+' · '+result.name+' · supported values staged locally. Review the tabs, then Save & verify. '+result.warnings.join(' ');
  }catch(e){if(current())status.textContent=e.message||String(e);}
  finally{busy=false;file.value='';file.disabled=player?.source!=='live-roster';}
 });
})();
