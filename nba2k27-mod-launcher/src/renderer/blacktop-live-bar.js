(() => {
 const detail=document.getElementById('blacktopStatus'),badge=document.getElementById('blacktopBarStatus');
 if(!detail||!badge)return;
 const update=()=>{
  const message=detail.textContent.trim();
  const state=/error|failed|unavailable|could not|reopen|needs reload/i.test(message)?'error':/reading|loading|scanning|applying|randomizing|saving/i.test(message)?'loading':'ready';
  badge.dataset.state=state;
  badge.lastChild.textContent=state==='error'?'Check status':state==='loading'?'Loading':'Ready';
  badge.title=message||'Blacktop overlay ready';
 };
 new MutationObserver(update).observe(detail,{childList:true,characterData:true,subtree:true});
 update();
})();
