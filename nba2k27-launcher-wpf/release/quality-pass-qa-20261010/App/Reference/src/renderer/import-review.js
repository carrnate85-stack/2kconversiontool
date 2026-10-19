(function(root,factory){const api=factory(root);if(typeof module==='object'&&module.exports)module.exports=api;else root.ImportReview=api;})(globalThis,root=>{
 function diff(before,after,prefix=''){
  const rows=[];for(const [key,value] of Object.entries(after||{})){const label=prefix?prefix+' / '+key:key,old=before?.[key];
   if(value&&typeof value==='object')rows.push(...diff(old,value,label));else rows.push({label,before:old,after:value});
  }return rows;
 }
 const format=v=>v===undefined?'Unavailable / new':v===null?'None':String(v);
 function rating(field,value){if(value===undefined)return value;const choices=field.choices||(field.group==='badges'?['None','Bronze','Silver','Gold','Hall of Fame','Legend']:field.group==='hotzones'?['Cold','Neutral','Hot']:null);return choices?.[value]??(field.money?value/1000000+' million':field.group==='attributes'?Math.min(110,25+Math.floor((value+1)/3)):value);}
 function playerRows(data,player,schema,changes=[]){const raw=[...(player.ratingsRaw||[])];for(const row of changes)raw[row.index]=row.value;
  return [...diff(player,data.info||{},'Player info'),...Object.entries(data.fields||{}).map(([key,value])=>{const index=schema.findIndex(f=>f.key===key),f=schema[index]||{label:key};return {label:(f.group||'Fields')+' / '+f.label,before:rating(f,raw[index]),after:rating(f,value)};})];
 }
 function seasonRows(review){return review.rows.flatMap(row=>[...diff(row.before?.totals,row.totals,String(row.year)+' totals'),...(row.awards?diff(row.before?.awards,row.awards,String(row.year)+' awards'):[]),{label:row.year+' team',before:row.before?.team,after:row.primaryTeam.displayName||row.primaryTeam.name}]);}
 let active=null;
 function confirm({title,rows,action='Stage changes',note='Review the requested changes. Nothing has been written to the game.',valid=()=>true}){
  if(active)return Promise.resolve(false);
  return new Promise(resolve=>{
   const el=(tag,text)=>{const n=document.createElement(tag);if(text!==undefined)n.textContent=text;return n;},dialog=el('dialog');dialog.className='import-review';dialog.setAttribute('aria-label',title);active=dialog;
   const heading=el('h2',title),message=el('p',note),counter=el('p'),scroll=el('div'),table=el('table');scroll.className='quality-scroll';table.className='quality-table';
   const head=el('tr');for(const text of ['Field','Before','After'])head.append(el('th',text));table.append(head);
   let changed=0;for(const row of rows){const tr=el('tr'),isChanged=row.before!==row.after;changed+=Number(isChanged);tr.dataset.changed=String(isChanged);tr.append(el('td',row.label),el('td',format(row.before)),el('td',format(row.after)));table.append(tr);}
   counter.textContent=`${changed} changed · ${rows.length-changed} unchanged`;scroll.append(table);
   const footer=el('footer'),cancel=el('button','Cancel'),accept=el('button',action);cancel.type=accept.type='button';cancel.className='secondary';accept.className='primary';footer.append(cancel,accept);dialog.append(heading,message,counter,scroll,footer);document.body.append(dialog);
   const invalidate=()=>finish(false);let finished=false;
   function finish(allowed){if(finished)return;finished=true;let accepted=false;try{accepted=Boolean(allowed&&valid());}catch{}active=null;for(const event of ['player-editor:open','player-editor:committed'])root.removeEventListener(event,invalidate);dialog.close();dialog.remove();resolve(accepted);}
   cancel.onclick=()=>finish(false);accept.onclick=()=>finish(true);dialog.addEventListener('cancel',e=>{e.preventDefault();finish(false);});dialog.addEventListener('close',()=>finish(false));for(const event of ['player-editor:open','player-editor:committed'])root.addEventListener(event,invalidate);
   dialog.showModal();cancel.focus();
  });
 }
 return {diff,format,rating,playerRows,seasonRows,confirm};
});
