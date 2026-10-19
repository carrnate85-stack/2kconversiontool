(function(root,factory){if(typeof module==='object'&&module.exports)module.exports=factory();else root.DnaSeasonSelection=factory();})(typeof window==='object'?window:globalThis,()=>{
 'use strict';
 function choices(history){
  if(!Array.isArray(history)||history.length>25)throw Error('Invalid season history.');
  const seen=new Set();
  return history.map((row,index)=>{
   if(!row||!Number.isInteger(row.year)||row.year<1900||row.year>2155||typeof row.team!=='string'||!row.team.trim()||row.team.length>100)throw Error('Invalid source season.');
   const key=JSON.stringify([row.year,row.team,row.secondaryTeam??null]);
   if(seen.has(key))throw Error('Duplicate source season.');seen.add(key);
   return {index,label:`${row.year-1}–${String(row.year).slice(-2)} · ${row.team}`};
  });
 }
 async function choose(history,{valid=()=>true,bulk=false,document:doc=globalThis.document}={}){
  const rows=choices(history);if(!valid())throw Error('The destination changed. Reopen Player DNA.');
  if(rows.length<=1)return history;
  return new Promise(resolve=>{
   const dialog=doc.createElement('dialog'),title=doc.createElement('h3'),note=doc.createElement('p'),select=doc.createElement('select'),actions=doc.createElement('div');
   title.textContent=bulk?'Choose seasons to import':'Choose a season to import';note.textContent=bulk?'Use all source seasons or choose one. Each destination change receives a separate review before Apply selected seasons.':'This destination stages one existing, unshared season at a time. Save it before importing another year.';
   select.setAttribute('aria-label','Source season');select.size=Math.min(rows.length,8);
   for(const row of rows){const o=doc.createElement('option');o.value=String(row.index);o.textContent=row.label;select.append(o);}select.selectedIndex=0;
   let finished=false;const events=['player-editor:open','player-editor:closed','player-editor:committed'];
   const changed=()=>{if(!valid())finish(null);};
   const finish=value=>{if(finished)return;finished=true;for(const e of events)doc.defaultView?.removeEventListener(e,changed);dialog.close();dialog.remove();resolve(valid()?value:null);};
   for(const e of events)doc.defaultView?.addEventListener(e,changed);
   for(const [label,action] of [...(bulk?[['Use all source seasons',()=>finish(history)]]:[]),['Use selected season',()=>finish([history[Number(select.value)]])],['Skip season history',()=>finish([])],['Cancel import',()=>finish(null)]]){
    const b=doc.createElement('button');b.type='button';b.textContent=label;b.addEventListener('click',action);actions.append(b);
   }
   dialog.addEventListener('cancel',e=>{e.preventDefault();finish(null);});
   dialog.append(title,note,select,actions);doc.body.append(dialog);dialog.showModal();select.focus();
  });
 }
 return {choices,choose};
});
