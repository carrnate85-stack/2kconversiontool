(function(root,factory){const api=factory();if(typeof module==='object'&&module.exports)module.exports=api;if(root)root.TeamRotationView=api;})(typeof window==='undefined'?globalThis:window,function(){
 'use strict';
 function create(host,request){
  const d=host.ownerDocument,el=(tag,text,cls)=>{const n=d.createElement(tag);if(text!==undefined)n.textContent=text;if(cls)n.className=cls;return n;};
  let team=null,revision=0,loaded=false,busy=false,snapshot=null,inputs=[];
  const head=el('div',undefined,'team-staff-head'),note=el('p','Advanced rotation · 48 segments, five lineup slots each. Only players assigned to this team can be selected. Save the roster in-game to retain changes.','team-staff-note'),refresh=el('button','Refresh rotations','secondary small'),content=el('div'),actions=el('div',undefined,'team-staff-head'),reset=el('button','Reset staged edits','secondary small'),save=el('button','Save & verify','primary small'),status=el('p','','team-staff-note');status.setAttribute('role','status');head.append(note,refresh);actions.append(reset,save,status);host.replaceChildren(head,content,actions);
  for(const b of [refresh,reset,save])b.type='button';save.disabled=reset.disabled=true;
  const identity=()=>({recordIndex:team.recordIndex,teamId:team.teamId,expectedName:team.name});
  const tools=el('div',undefined,'rotation-lineup-tools'),usage=el('p','','rotation-usage');
  const number=(label,value)=>{const field=el('label',label+' '),input=el('input');input.type='number';input.min=1;input.max=48;input.step=1;input.value=String(value);input.setAttribute('aria-label',label);field.append(input);tools.append(field);return input;};
  const source=number('Copy lineup from segment',1),start=number('To segments',1),end=number('Through',48),copy=el('button','Stage lineup range','secondary small'),review=el('button','Review changes','secondary small');copy.type=review.type='button';tools.append(copy,review);head.after(tools);tools.after(usage);
  function updateUsage(){if(!snapshot)return;const counts=new Map();for(const row of rows())for(const id of row)counts.set(id,(counts.get(id)||0)+1);usage.textContent='Segment appearances (not minutes): '+snapshot.members.map(m=>`${m.name}: ${counts.get(m.id)||0}`).join(' · ');}
  copy.addEventListener('click',()=>{if(busy||!snapshot)return;const indexes=[source,start,end].map(input=>Number(input.value));if(indexes.some(n=>!Number.isInteger(n)||n<1||n>48)||indexes[1]>indexes[2]){status.textContent='Choose a source and a valid 1–48 segment range.';return;}const lineup=rows()[indexes[0]-1];if(new Set(lineup).size!==5||lineup.some(id=>!snapshot.members.some(m=>m.id===id))){status.textContent='The source lineup must contain five distinct assigned players.';return;}for(let i=indexes[1]-1;i<indexes[2];i++)lineup.forEach((id,j)=>inputs[i][j].value=String(id));updateUsage();status.textContent='Lineup range staged. Review, then Save & verify.';});
  review.addEventListener('click',()=>{if(busy||!snapshot)return;const name=id=>snapshot.members.find(m=>m.id===id)?.name||String(id);globalThis.EditorReview?.open({title:'Review rotation changes',rows:rows().flatMap((row,i)=>row.map((id,j)=>({id,i,j}))).filter(c=>c.id!==snapshot.rows[c.i][c.j]).map(c=>({label:`Segment ${c.i+1}, slot ${c.j+1}`,before:name(snapshot.rows[c.i][c.j]),after:name(c.id),remove:()=>{if(busy)return false;inputs[c.i][c.j].value=String(snapshot.rows[c.i][c.j]);updateUsage();return true;}}))});});
  function render(r){
   snapshot=r;inputs=[];const table=el('table',undefined,'studio-season-table team-rotation-table'),heading=el('tr');for(const t of ['Segment','Slot 1','Slot 2','Slot 3','Slot 4','Slot 5'])heading.append(el('th',t));table.append(heading);
   for(const [i,row] of r.rows.entries()){const tr=el('tr');tr.append(el('td',String(i+1)));const cells=[];for(const [j,id] of row.entries()){const td=el('td'),select=el('select');select.setAttribute('aria-label',`Segment ${i+1}, slot ${j+1}`);
    for(const m of r.members){const o=el('option',m.name);o.value=String(m.id);select.append(o);}if(!r.members.some(m=>m.id===id)){const o=el('option',`Unresolved ID ${id} · unchanged`);o.value=String(id);select.append(o);}select.value=String(id);td.append(select);tr.append(td);cells.push(select);select.addEventListener('change',()=>{status.textContent='Rotation changes staged. Review, then Save & verify.';});}inputs.push(cells);table.append(tr);}
   content.replaceChildren(table);save.disabled=reset.disabled=false;updateUsage();
  }
  async function load(force=false){
   if(!team||busy||(!force&&loaded))return;const current=team,token=++revision;busy=true;refresh.disabled=save.disabled=reset.disabled=true;status.textContent='Reading rotations…';
   try{if(team.source!=='live-team')throw Error('Refresh the live roster before opening rotations.');const r=await request({action:'read',...identity()});if(token!==revision||team!==current)return;if(!Array.isArray(r.rows)||r.rows.length!==48||!Array.isArray(r.members)||r.readOnly!==true)throw Error('Invalid rotation response.');render(r);loaded=true;status.textContent='No staged changes.';}
   catch(e){if(token===revision){loaded=false;snapshot=null;content.replaceChildren();status.textContent=e.message;}}
   finally{busy=false;if(token===revision){refresh.disabled=false;save.disabled=reset.disabled=!snapshot;}}
  }
  const rows=()=>inputs.map(row=>row.map(select=>Number(select.value)));
  refresh.addEventListener('click',()=>void load(true));reset.addEventListener('click',()=>{if(!busy&&snapshot){render(snapshot);status.textContent='No staged changes.';}});
  save.addEventListener('click',async()=>{if(!team||!snapshot||busy)return;const token=revision,values=rows();busy=true;refresh.disabled=reset.disabled=save.disabled=true;status.textContent='Saving and verifying rotations…';
   try{const r=await request({action:'save',...identity(),token:snapshot.token,rows:values});if(token!==revision)return;if(!r.saved||!r.readBackVerified)throw Error('Rotation save was not verified.');snapshot.rows=values;status.textContent='Saved & verified. Save the roster in-game.';}
   catch(e){if(token===revision)status.textContent=e.message;}
   finally{busy=false;if(token===revision)refresh.disabled=reset.disabled=save.disabled=false;}
  });
  content.addEventListener('change',updateUsage);
  return {load,reset(value){team=value;revision++;loaded=false;snapshot=null;inputs=[];content.replaceChildren();usage.textContent='';status.textContent='';save.disabled=reset.disabled=true;},
   async portable(){const current=team;await load();if(team!==current||!snapshot)throw Error('Refresh rotations before exporting.');return rows().map(row=>row.map(id=>{const m=snapshot.members.find(m=>m.id===id);if(!m)throw Error('Unresolved rotation players cannot be exported.');return m.name;}));},
   async stage(names){const current=team;await load();if(team!==current||!snapshot||busy)throw Error('Refresh rotations before importing.');const values=names.map(row=>row.map(name=>{const matches=snapshot.members.filter(m=>m.name===name);if(matches.length!==1)throw Error(`Rotation player “${name}” has no unique match on this team. Rotations were left unchanged.`);return matches[0].id;}));
    values.forEach((row,i)=>row.forEach((id,j)=>{inputs[i][j].value=String(id);}));updateUsage();status.textContent='Imported rotations staged. Review, then Save & verify.';}};
 }
 return {create};
});
