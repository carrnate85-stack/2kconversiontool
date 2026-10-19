(function(root,factory){const api=factory();if(typeof module==='object'&&module.exports)module.exports=api;if(root)root.TeamStaffView=api;})(typeof window==='undefined'?globalThis:window,function(){
 'use strict';
 function create(host,read,save){
  const d=host.ownerDocument;let revision=0,team=null,loaded=false,busy=false,loading=null,models=[];
  const el=(tag,text,cls)=>{const n=d.createElement(tag);if(text!==undefined)n.textContent=text;if(cls)n.className=cls;return n;};
  const head=el('div',undefined,'team-staff-head'),note=el('p','Stage mapped settings, then Save & verify for each staff member. IDs and assignments stay unchanged. Save the roster in-game to keep edits.','team-staff-note'),button=el('button','Refresh staff','secondary small'),content=el('div',undefined,'team-staff-list');
  button.type='button';head.append(note,button);host.replaceChildren(head,content);
  const identity=()=>({recordIndex:team.recordIndex,teamId:team.teamId,expectedName:team.name});
  function render(staff){
   models=[];if(!staff.length){content.replaceChildren(el('p','No assigned staff found in this live team record.'));return;}
   content.replaceChildren(...staff.map((s,i)=>{
    const card=el('details',undefined,'team-staff-card');card.open=i===0;
    const summary=el('summary');summary.append(el('strong',s.name),el('small',s.role.replaceAll('_',' ').toLowerCase()));card.append(summary);
    card.append(el('p',`Staff ID ${s.uniqueId} · record #${s.recordIndex} · player link ${s.playerUniqueId} (protected)`,'team-staff-note'));
    const model={staff:s,rows:[],groups:new Map()};models.push(model);
    for(const f of s.controls||[]){
     let grid=model.groups.get(f.group);if(!grid){const group=el(f.group==='Badges'?'details':'section',undefined,'team-staff-group');group.append(el(f.group==='Badges'?'summary':'h4',['Attributes','System proficiency','Badges'].includes(f.group)?f.group+' · raw values':f.group));grid=el('div',undefined,'team-edit-grid team-staff-edit-grid');group.append(grid);card.append(group);model.groups.set(f.group,grid);}
     const label=el('label',f.label),input=el(f.choices?'select':'input');
     if(f.choices){for(const [n,text] of f.choices.entries()){const o=el('option',text);o.value=String(n);input.append(o);}if(f.key.endsWith('SystemId')){const o=el('option','None / unset');o.value='15';input.append(o);}if(![...input.children].some(o=>o.value===String(s.values[f.key]))){const o=el('option',`Unknown (${s.values[f.key]}) · unchanged`);o.value=String(s.values[f.key]);input.append(o);}}
     else{input.type=f.text?'text':'number';if(f.text)input.maxLength=f.bytes/2-1;else{input.min=f.min??0;input.max=f.max??2**f.bits-1;input.step=1;}}
     input.value=String(s.values[f.key]);input.disabled=typeof save!=='function';input.setAttribute('aria-label',f.label);label.append(input);grid.append(label);model.rows.push({field:f,input});
    }
    const actions=el('div',undefined,'team-staff-head'),reset=el('button','Reset staged edits','secondary small'),commit=el('button','Save & verify','primary small'),status=el('p','No staged changes.','team-staff-note');status.setAttribute('role','status');reset.type=commit.type='button';commit.disabled=typeof save!=='function';actions.append(reset,commit,status);card.append(actions);
    model.stage=values=>{for(const row of model.rows)if(Object.hasOwn(values,row.field.key))row.input.value=String(values[row.field.key]);status.textContent='Imported values staged. Review, then Save & verify.';};
    model.values=()=>Object.fromEntries(model.rows.map(r=>[r.field.key,r.field.text?r.input.value:Number(r.input.value)]));
    globalThis.EditorPresetView?.create(card,{kind:'coaching',identity:()=>team,enabled:()=>!busy,
     read:()=>Object.fromEntries(model.rows.filter(r=>!['Identity','Contract'].includes(r.field.group)&&!r.field.text&&!['faceId','soundId','wage'].includes(r.field.key)).map(r=>[r.field.key,Number(r.input.value)]).filter(([key,v])=>{const f=model.rows.find(r=>r.field.key===key).field;return Number.isInteger(v)&&v>=(f.min??0)&&v<=(f.max??(f.choices?f.choices.length-1:2**f.bits-1));})),
     stage:values=>model.stage(values)})?.refresh();
    const review=el('button','Review changes','secondary small');review.type='button';actions.prepend(review);
    review.addEventListener('click',()=>{if(busy)return;globalThis.EditorReview?.open({title:'Review '+s.name,rows:model.rows.filter(r=>(r.field.text?r.input.value:Number(r.input.value))!==s.values[r.field.key]).map(r=>({label:r.field.label,before:r.field.choices?.[s.values[r.field.key]]??s.values[r.field.key],after:r.field.choices?.[Number(r.input.value)]??r.input.value,remove:()=>{if(busy)return false;r.input.value=String(s.values[r.field.key]);return true;}}))});});
    reset.addEventListener('click',()=>{if(busy)return;model.stage(s.values);status.textContent='No staged changes.';});
    commit.addEventListener('click',async()=>{
     if(busy||!team)return;const token=revision,values=model.values(),changes=Object.fromEntries(Object.entries(values).filter(([key,v])=>v!==s.values[key]));
     if(!Object.keys(changes).length){status.textContent='No staged changes.';return;}busy=true;button.disabled=commit.disabled=true;status.textContent='Saving and verifying…';
     try{const r=await save({...identity(),staffIndex:s.recordIndex,slot:s.slot,values:changes,expected:Object.fromEntries(Object.keys(changes).map(key=>[key,s.values[key]]))});if(token!==revision)return;if(!r.saved||!r.readBackVerified)throw Error('Staff save was not verified.');Object.assign(s.values,changes);status.textContent='Saved & verified. Save the roster in-game.';}
     catch(e){if(token===revision)status.textContent=e.message;}
     finally{busy=false;if(token===revision){button.disabled=false;commit.disabled=false;}}
    });return card;
   }));
  }
  async function load(force=false){
   if(loading)return loading;
   if(!team||busy||(!force&&loaded))return;const current=team,id=++revision;loaded=true;models=[];button.disabled=true;content.replaceChildren(el('p','Reading assigned staff…'));
   const pending=(async()=>{try{if(current.source!=='live-team')throw Error('Load this team’s roster and refresh live teams first.');const result=await read(identity());if(id!==revision)return;if(result.readOnly!==true||!Array.isArray(result.staff))throw Error('Invalid staff response.');render(result.staff);}
   catch(error){if(id===revision){loaded=false;content.replaceChildren(el('p',error.message,'team-staff-error'));}}
   finally{if(id===revision)button.disabled=false;}})();
   loading=pending;try{await pending;}finally{if(loading===pending)loading=null;}
  }
  button.addEventListener('click',()=>void load(true));
  return {reset(value){revision++;team=value;loaded=false;loading=null;models=[];button.disabled=false;content.replaceChildren();},load,
   snapshot(){if(!loaded||loading)throw Error('Refresh staff before exporting.');return models.map(m=>({slot:m.staff.slot,values:m.values()}));},
   async stage(rows){const current=team;await load();if(!team||team!==current||busy||!loaded)throw Error('Open this team’s staff first.');const missing=[];for(const row of rows){const model=models.find(m=>m.staff.slot===row.slot);if(model)model.stage(row.values);else missing.push(row.slot);}return missing;}};
 }
 return {create};
});
