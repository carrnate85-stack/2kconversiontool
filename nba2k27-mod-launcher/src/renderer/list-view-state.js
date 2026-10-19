// Preserve the user's place across data refreshes, not across new searches.
(()=>{
 const contexts=new WeakMap();
 function capture(root,context){
  const same=contexts.get(root)===context;contexts.set(root,context);
  const positions=[];for(let node=root;node;node=node.parentElement)positions.push([node,node.scrollTop,node.scrollLeft]);
  const active=document.activeElement,row=root.contains(active)?active.closest('[data-view-key]'):null;
  const key=row?.getAttribute('data-view-key');
  const control=row?[...row.querySelectorAll('button,input,select,a,[tabindex]')].indexOf(active):-1;
  const selection=active&&typeof active.selectionStart==='number'?[active.selectionStart,active.selectionEnd]:null;
  return ()=>{
   // Only restore focus if the replacement detached the original control.
   if(same&&key&&active&&!active.isConnected&&(document.activeElement===document.body||document.activeElement===active)){
    const replacement=[...root.querySelectorAll('[data-view-key]')].find(n=>n.getAttribute('data-view-key')===key);
    const target=control>=0?replacement?.querySelectorAll('button,input,select,a,[tabindex]')[control]:replacement;
    if(target&&!target.disabled){target.focus({preventScroll:true});if(selection&&target.setSelectionRange)target.setSelectionRange(...selection);}
   }
   for(const [node,top,left] of positions){node.scrollTop=node===root&&!same?0:top;node.scrollLeft=node===root&&!same?0:left;}
  };
 }
 globalThis.LauncherListView={capture};
})();
