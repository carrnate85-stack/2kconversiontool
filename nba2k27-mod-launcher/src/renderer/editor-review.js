(() => {
 const dialog=document.createElement('dialog');dialog.className='editor-review-dialog';dialog.setAttribute('aria-label','Review staged changes');document.body.append(dialog);
 function open({title,rows}){
  const el=(tag,text)=>{const n=document.createElement(tag);if(text!==undefined)n.textContent=String(text);return n;};
  const heading=el('div');heading.className='editor-review-head';const close=el('button','Back to editor');close.type='button';close.className='secondary';close.addEventListener('click',()=>dialog.close());heading.append(el('h2',title),close);
  const note=el('p','Review only. No game data is written here. Remove a row to discard just that staged edit.'),body=el('div');body.className='editor-review-body';
  const table=el('table');table.className='studio-season-table';const head=el('tr');for(const text of ['Field','Before','After',''])head.append(el('th',text));table.append(head);
  for(const row of rows){const tr=el('tr');tr.append(el('td',row.label),el('td',row.before),el('td',row.after));const td=el('td'),button=el('button','Remove');button.type='button';button.className='secondary small';button.disabled=!row.remove;button.addEventListener('click',()=>{if(row.remove()!==false){tr.remove();if(table.children.length===1)note.textContent='No staged changes.';}});td.append(button);tr.append(td);table.append(tr);}
  if(!rows.length)note.textContent='No staged changes.';body.append(table);dialog.replaceChildren(heading,note,body);if(!dialog.open)dialog.showModal();
 }
 window.EditorReview={open,close:()=>dialog.close()};
 window.addEventListener('player-editor:open',()=>dialog.close());
})();
