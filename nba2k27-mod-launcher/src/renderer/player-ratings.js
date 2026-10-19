(() => {
 const schema=window.playerRatingSchema||[],layout=window.playerEditorLayout,panels={};let original=null;
 const tiers=['None','Bronze','Silver','Gold','Hall of Fame','Legend'];
 const maximum=f=>Math.min(2**f.bits-1,f.max??(f.choices?f.choices.length-1:f.group==='badges'?5:f.group==='tendencies'?100:2**f.bits-1));
 const choices=f=>f.group==='hotzones'?['Cold','Neutral','Hot']:f.choices||(f.group==='badges'?tiers:f.bits===1?['No','Yes']:null);
 const display=(f,raw)=>f.money?raw/1000000:f.group==='attributes'?Math.min(110,25+Math.floor((raw+1)/3)):raw;
 const element=(tag,cls,text)=>{const e=document.createElement(tag);if(cls)e.className=cls;if(text)e.textContent=text;return e;};
 function updateRow(row){
  if(row.slider){row.slider.value=row.input.value;row.slider.disabled=row.input.disabled;}
  if(row.field.group==='badges')row.label.dataset.tier=row.input.value;
  if(row.zone){row.zone.dataset.level=row.input.value;row.zone.setAttribute('aria-label',row.field.label+': '+(choices(row.field)[row.input.value]||'Unavailable'));row.zone.setAttribute('aria-disabled',String(row.input.disabled));row.zoneText.textContent=row.input.value===''?'—':['C','N','H'][Number(row.input.value)]||'?';if(row.zoneButton){row.zoneButton.dataset.level=row.input.value;row.zoneButton.title=choices(row.field)[row.input.value]||'Unavailable';}}
 }
 for(const [group,id] of [['attributes','studioAttributes'],['development','studioDevelopment'],['draft','studioDraft'],['injuries','studioInjuries'],['tendencies','studioTendencies'],['playtypes','studioPlaytypes'],['badges','studioBadges'],['personality','studioPersonality'],['contract','studioContract'],['appearance','studioAppearance'],['hotzones','studioHotZones'],['signature','studioSignature'],['equipment','studioEquipment'],['stats','studioStats']]){
  const panel=document.getElementById(id);panel.querySelector('.studio-empty')?.remove();
  const note=element('p','studio-empty'),search=element('input','studio-rating-search');search.type='search';search.placeholder='Search '+group;search.setAttribute('aria-label','Search '+group);
  const body=element('div','studio-field-body'),empty=element('p','studio-empty','No matching fields.');empty.hidden=true;
  const model=panels[group]={note,search,rows:[],sections:[],active:'All',pairs:[],empty};
  const categoryNames=layout.categories[group]||[''];let filters;
  if(layout.categories[group]){
   filters=element('div','studio-category-tabs');filters.setAttribute('role','tablist');filters.setAttribute('aria-label',group+' categories');body.id=id+'Fields';body.setAttribute('role','tabpanel');
   for(const name of ['All',...categoryNames]){
    const button=element('button','',name);button.type='button';button.id=id+'Filter'+filters.children.length;button.setAttribute('role','tab');button.setAttribute('aria-controls',body.id);button.dataset.category=name;
    button.addEventListener('click',()=>{model.active=name;search.value='';filter();});
    button.addEventListener('keydown',event=>{const buttons=[...filters.children],index=buttons.indexOf(button);let next;if(event.key==='ArrowRight')next=(index+1)%buttons.length;else if(event.key==='ArrowLeft')next=(index+buttons.length-1)%buttons.length;else if(event.key==='Home')next=0;else if(event.key==='End')next=buttons.length-1;else return;event.preventDefault();buttons[next].click();buttons[next].focus();});filters.append(button);
   }
  }
  for(const category of categoryNames){
   const section=element('section','studio-field-section'),heading=element('h3','studio-group-title',category),grid=element('div',group==='badges'?'studio-badge-grid':'team-edit-grid');
   if(category)section.append(heading);section.append(grid);body.append(section);model.sections.push({category,section,grid});
  }
  schema.forEach((field,index)=>{if(field.group!==group)return;
   const category=layout.category(field),section=model.sections.find(s=>s.category===category)||model.sections[0],label=element('label',group==='badges'?'studio-badge':'');label.htmlFor='studioField'+index;
   label.append(element('span','studio-field-name',group==='stats'?field.label.replace(/^(Season|Career) High /,''):field.label));const options=choices(field),input=element(options?'select':'input');input.id='studioField'+index;
   if(options){options.forEach((name,value)=>{const option=element('option','',name);option.value=String(value);input.append(option);});}
   else{input.type='number';input.min=group==='attributes'?25:0;input.max=field.money?200:group==='attributes'?99:maximum(field);input.step=field.money?'0.000001':'1';input.readOnly=true;}
   input.disabled=true;input.dataset.fieldIndex=String(index);const row={field,index,label,input,category};
   if(group==='attributes'||(group==='tendencies'&&!options)){
    const controls=element('span','studio-attribute-control'),slider=element('input');slider.type='range';slider.min=input.min;slider.max=input.max;slider.step=input.step;slider.disabled=true;slider.setAttribute('aria-label',field.label+' slider');
    slider.addEventListener('input',()=>{input.value=slider.value;input.dispatchEvent(new Event('input',{bubbles:true}));});controls.append(slider,input);label.append(controls);row.slider=slider;
   }else label.append(input);
   if(group==='badges'){const icon=element('span','studio-badge-icon',category.charAt(0));icon.setAttribute('aria-hidden','true');label.prepend(icon);row.badgeIcon=icon;input.setAttribute('aria-label',field.label+' tier');}
   input.addEventListener('input',()=>{stagedRaw.delete(index);updateRow(row);});input.addEventListener('change',()=>{stagedRaw.delete(index);updateRow(row);});section.grid.append(label);model.rows.push(row);
  });
  if(group==='signature'||group==='stats')for(const section of model.sections)for(const row of model.rows.filter(row=>row.category===section.category).sort((a,b)=>layout.order(a.field)-layout.order(b.field)))section.grid.append(row.label);
  if(group==='personality')for(const pair of layout.personalityPairs){
   const rows=pair.keys.map(key=>model.rows.find(row=>row.field.key===key));rows.forEach(row=>{row.paired=true;row.label.hidden=true;});const label=element('label','studio-personality-pair-row'),input=element('select');input.dataset.personalityPair=pair.label;
   pair.choices.forEach((name,value)=>{const option=element('option','',name);option.value=String(value);input.append(option);});
   input.addEventListener('change',()=>{if(input.value==='')return;rows.forEach((row,index)=>{row.input.value=index===Number(input.value)?'1':'0';row.input.dispatchEvent(new Event('input',{bubbles:true}));});});
   label.append(element('span','',pair.label),input);rows[0].label.before(label);model.pairs.push({...pair,label,input,rows});
  }
  function filter(){
   const q=search.value.trim().toLowerCase();let count=0;
   for(const row of model.rows){if(row.headbandSetting)continue;row.label.hidden=!!row.paired||!!q&&!row.field.label.toLowerCase().includes(q)||(model.active!=='All'&&row.category!==model.active);if(!row.label.hidden)count++;}
   for(const pair of model.pairs){pair.label.hidden=!!q&&!pair.label.textContent.toLowerCase().includes(q);if(!pair.label.hidden)count++;}
   for(const {section,grid} of model.sections)section.hidden=![...grid.children].some(row=>!row.hidden);empty.hidden=count!==0;
   if(filters)for(const button of filters.children){const selected=button.dataset.category===model.active;button.setAttribute('aria-selected',String(selected));button.tabIndex=selected?0:-1;if(selected)body.setAttribute('aria-labelledby',button.id);}
   model.refreshCourt?.();
  }
  model.filter=filter;search.addEventListener('input',()=>{model.active='All';filter();});panel.append(note);if(filters)panel.append(filters);if(!['personality','contract'].includes(group))panel.append(search);panel.append(body,empty);filter();
  if(group==='appearance'){
   const grid=document.querySelector('#studioInfo > .team-edit-grid');
   for(const row of model.rows)(['HairLength','FaceType'].includes(row.field.key)?document.getElementById('studioHiddenFields'):grid).append(row.label);
   const portrait=model.rows.find(row=>row.field.key==='PhotoId');
   if(portrait)document.getElementById('playerEditCyberface').closest('label').after(portrait.label);
   panel.hidden=true;
  }
 }
 let badgeIconsLoading=false,badgeIconsLoaded=false;
 async function loadBadgeIcons(){if(badgeIconsLoading||badgeIconsLoaded||!window.modLauncher.playerEditorBadgeIcons)return;badgeIconsLoading=true;try{const rows=panels.badges?.rows||[],icons=await window.modLauncher.playerEditorBadgeIcons({labels:rows.map(row=>row.field.label)});for(const row of rows){const source=icons?.[row.field.label];if(!source)continue;const image=element('img');image.alt='';image.src=source;row.badgeIcon.replaceChildren(image);row.badgeIcon.classList.add('has-image');}badgeIconsLoaded=true;}catch{}finally{badgeIconsLoading=false;}}
 document.getElementById('studioTabBadges')?.addEventListener('click',loadBadgeIcons);
 // Keep headband-specific controls beside the headband, not as unrelated rows.
 {
  const model=panels.equipment,headband=model.rows.find(r=>r.field.key==='Headband'),logo=model.rows.find(r=>r.field.key==='HeadbandLogoPosition'),frequency=model.rows.find(r=>r.field.key==='HeadbandFrequency');
  if(headband&&logo&&frequency){
   const controls=element('span','studio-headband-control'),gear=element('button','studio-headband-gear','⚙');gear.type='button';gear.title='Headband settings';gear.setAttribute('aria-label','Headband settings');gear.setAttribute('aria-expanded','false');gear.setAttribute('aria-controls','studioHeadbandSettings');
   headband.input.replaceWith(controls);controls.append(headband.input,gear);
   const box=element('section','studio-headband-settings');box.id='studioHeadbandSettings';box.hidden=true;box.setAttribute('aria-label','Headband settings');
   const top=element('div','studio-headband-settings-title'),close=element('button','','Close');close.type='button';top.append(element('strong','','Headband settings'),close);
   logo.headbandSetting=true;frequency.headbandSetting=true;logo.label.hidden=false;frequency.label.hidden=false;
   box.append(top,frequency.label,logo.label,element('p','','Wear frequency and logo location apply with Save & verify. Uniform logo type is a separate setting.'));
   headband.label.after(box);
   const toggle=open=>{box.hidden=!open;gear.setAttribute('aria-expanded',String(open));if(open)logo.input.focus();};
   gear.addEventListener('click',event=>{event.preventDefault();toggle(box.hidden);});close.addEventListener('click',()=>{toggle(false);gear.focus();});
   box.addEventListener('keydown',event=>{if(event.key==='Escape'){event.stopPropagation();toggle(false);gear.focus();}});
   model.search.addEventListener('input',()=>toggle(false));
   window.addEventListener('player-editor:open',()=>toggle(false));
  }
 }
 // Court geometry is presentation only. Each zone edits its mapped packed field.
 {
  const model=panels.hotzones,panel=document.getElementById('studioHotZones'),ns='http://www.w3.org/2000/svg';
  const court=element('div','studio-court'),svg=document.createElementNS(ns,'svg');svg.setAttribute('viewBox','0 0 500 410');svg.setAttribute('role','group');svg.setAttribute('aria-label','Half-court hot zones, basket at the top');
  const point=(rx,ry,cy,a)=>[250+rx*Math.cos(a*Math.PI/180),cy+ry*Math.sin(a*Math.PI/180)].map(v=>v.toFixed(2)).join(' ');
  const ring=(a,b)=>`M ${point(225,205,145,a)} A 225 205 0 0 1 ${point(225,205,145,b)} L ${point(145,95,135,b)} A 145 95 0 0 0 ${point(145,95,135,a)} Z`;
  const shapes={UnderBasket:'M 200 0 H 300 V 70 A 50 50 0 0 1 200 70 Z',CloseLeft:'M 105 0 H 200 V 70 A 50 50 0 0 0 225 113.3 L 177.5 217.27 A 145 95 0 0 1 105 135 Z',CloseCenter:'M 225 113.3 A 50 50 0 0 0 275 113.3 L 322.5 217.27 A 145 95 0 0 1 177.5 217.27 Z',CloseRight:'M 300 0 H 395 V 135 A 145 95 0 0 1 322.5 217.27 L 275 113.3 A 50 50 0 0 0 300 70 Z',MedLeft:ring(135,180)+' M 25 0 H 105 V 135 L 25 145 Z',MedLeftCenter:ring(105,135),MedCenter:ring(75,105),MedRightCenter:ring(45,75),MedRight:ring(0,45)+' M 395 0 H 475 V 145 L 395 135 Z',ThreeLeft:'M 0 0 H 25 V 145 A 225 205 0 0 0 71.28 269.08 L 0 330 Z',ThreeRight:'M 475 0 H 500 V 330 L 428.72 269.08 A 225 205 0 0 0 475 145 Z',ThreeLeftCenter:'M 0 330 L 71.28 269.08 A 225 205 0 0 0 191.77 343.01 L 172 410 H 0 Z',ThreeCenter:'M 191.77 343.01 A 225 205 0 0 0 308.23 343.01 L 328 410 H 172 Z',ThreeRightCenter:'M 500 330 L 428.72 269.08 A 225 205 0 0 1 308.23 343.01 L 328 410 H 500 Z'};
  const labels={UnderBasket:[250,85],CloseLeft:[155,94],CloseCenter:[250,181],CloseRight:[345,94],MedLeft:[65,105],MedLeftCenter:[151,261],MedCenter:[250,290],MedRightCenter:[349,261],MedRight:[435,105],ThreeLeft:[12,96],ThreeRight:[488,96],ThreeLeftCenter:[91,365],ThreeCenter:[250,385],ThreeRightCenter:[409,365]};let selected='HotZoneUnderBasket';const picker=element('div','studio-zone-picker');
  for(const row of [...model.rows].sort((a,b)=>Object.keys(shapes).indexOf(a.field.key.replace('HotZone',''))-Object.keys(shapes).indexOf(b.field.key.replace('HotZone','')))){
   const key=row.field.key.replace('HotZone',''),zone=document.createElementNS(ns,'g'),path=document.createElementNS(ns,'path'),text=document.createElementNS(ns,'text');zone.classList.add('studio-zone');zone.setAttribute('role','button');zone.setAttribute('tabindex','0');path.setAttribute('d',shapes[key]);zone.append(path);
   text.setAttribute('x',labels[key][0]);text.setAttribute('y',labels[key][1]);text.setAttribute('text-anchor','middle');text.setAttribute('aria-hidden','true');zone.append(text);row.zoneText=text;
   const choose=()=>{selected=row.field.key;refresh();};zone.addEventListener('click',choose);zone.addEventListener('keydown',event=>{if(event.key==='Enter'||event.key===' '){event.preventDefault();choose();}});row.input.addEventListener('change',()=>refresh());row.zone=zone;svg.append(zone);
   const zoneButton=element('button','',row.field.label.replace('Three-point','3PT'));zoneButton.type='button';zoneButton.addEventListener('click',choose);row.zoneButton=zoneButton;picker.append(zoneButton);
  }
  const markings=document.createElementNS(ns,'path');markings.setAttribute('d','M 216 35 H 284 M 238 54 a 12 12 0 1 0 24 0 a 12 12 0 1 0 -24 0');markings.classList.add('studio-court-markings');svg.append(markings);
  const legend=element('div','studio-court-legend');const legendButtons=[];for(const [i,name] of ['Cold','Neutral','Hot'].entries()){
   const item=element('button','',name);item.type='button';item.dataset.level=String(i);item.setAttribute('aria-label',`Set selected hot zone to ${name}`);
   item.addEventListener('click',()=>{const row=model.rows.find(row=>row.field.key===selected);if(!row||row.input.disabled)return;row.input.value=String(i);row.input.dispatchEvent(new Event('input',{bubbles:true}));refresh();});
   legendButtons.push(item);legend.append(item);
  }
  const toggle=element('button','secondary','Show all zone fields');toggle.type='button';toggle.setAttribute('aria-pressed','false');let list=false;toggle.addEventListener('click',()=>{list=!list;toggle.setAttribute('aria-pressed',String(list));toggle.textContent=list?'Show selected zone only':'Show all zone fields';refresh();});
  function refresh(){for(const row of model.rows){updateRow(row);row.zone.setAttribute('aria-pressed',String(row.field.key===selected));row.zoneButton.setAttribute('aria-pressed',String(row.field.key===selected));row.label.hidden=!list&&row.field.key!==selected;}const active=model.rows.find(row=>row.field.key===selected);for(const button of legendButtons){button.disabled=!active||active.input.disabled;button.setAttribute('aria-pressed',String(button.dataset.level===active?.input.value));}model.sections[0].section.hidden=false;}
  model.refreshCourt=refresh;model.search.hidden=true;court.append(legend,svg);const workspace=element('div','studio-hotzone-workspace'),controls=element('div','studio-zone-controls'),body=panel.querySelector('.studio-field-body');panel.insertBefore(workspace,model.search);controls.append(picker,body,toggle);workspace.append(controls,court);refresh();
 }
 function syncPairs(){for(const pair of panels.personality.pairs){pair.input.querySelectorAll('[data-unknown]').forEach(option=>option.remove());const values=pair.rows.map(row=>Number(row.input.value)),valid=original&&pair.rows.every(row=>!row.input.disabled);pair.input.disabled=!valid;
   if(valid&&values[0]+values[1]===1)pair.input.value=values[1]?'1':'0';else{const option=element('option','',valid?(values.every(Boolean)?'Both set — unchanged':'Unspecified — unchanged'):'Unavailable');option.value='';option.disabled=true;option.dataset.unknown='true';pair.input.append(option);pair.input.value='';}
  }}
 window.addEventListener('player-editor:open',event=>{
  const p=event.detail||{},live=p.source==='live-roster';original=live&&Array.isArray(p.ratingsRaw)&&p.ratingsRaw.length===schema.length?p.ratingsRaw.slice():null;
  for(const [group,panel] of Object.entries(panels)){
   panel.search.value='';panel.active='All';panel.note.textContent=original?'Changes apply with Save & verify.':'Refresh a live roster to edit these fields.';
   if(group==='attributes')panel.note.textContent+=' Game ratings use 25–99. Existing values above 99 stay unchanged unless edited.';
   if(group==='stats')panel.note.textContent+=' Single-game season and career highs. Season awards have their own Awards tab.';
   if(['draft','injuries'].includes(group))panel.note.textContent='Read-only raw game data. Calendar encoding and injury lifecycle need verification before editing can be enabled.';
   if(group==='development')panel.note.textContent+=' Potential bounds are shown as raw values; no ratings conversion is assumed.';
   if(group==='equipment')panel.note.textContent+=' Shoe model IDs and colorway hashes are read-only until compatible asset mapping is verified.';
   for(const row of panel.rows){const {field,index,input}=row,raw=original?.[index],valid=Number.isInteger(raw)&&raw>=0&&raw<2**field.bits,enabled=valid&&field.key!=='PotentialRaw'&&!field.readOnly;input.disabled=!enabled;if(input.tagName==='INPUT')input.readOnly=!enabled;
    input.querySelectorAll?.('[data-unknown]').forEach(o=>o.remove());if(input.tagName==='SELECT'&&valid&&raw>=choices(field).length){const o=element('option','','Unknown ('+raw+') — unchanged');o.value=String(raw);o.disabled=true;o.dataset.unknown='true';input.append(o);}
    input.value=valid?String(display(field,raw)):'';input.title=field.key==='PotentialRaw'?'Edit Potential in Player info.':valid?'Stored value: '+raw:'';updateRow(row);
   }panel.filter();
  }syncPairs();
 });
 window.playerRatingChanges=()=>{
  if(!original)return [];const changes=[];
  for(const panel of Object.values(panels))for(const {field,index,input} of panel.rows){if(input.disabled)continue;const text=input.value.trim(),value=Number(text),before=original[index];
   const staged=stagedRaw.get(index);if(staged&&text===staged.text){if(staged.value!==before)changes.push({index,expected:before,value:staged.value});continue;}
   if(value===display(field,before)&&text!=='')continue;
   if(text===''||!Number.isFinite(value)||(!field.money&&!Number.isInteger(value))||(field.money&&!/^\d+(?:\.\d{1,6})?$/.test(text))||value<Number(input.min||0)||value>(field.money?200:field.group==='attributes'?99:maximum(field)))throw Error('Enter a valid value for '+field.label+(field.money?' (0–200 million, up to six decimal places).':'.'));
   const raw=field.money?Math.round(value*1000000):field.group==='attributes'?(value<=25?0:Math.min(255,(value-25)*3-1)):value;changes.push({index,expected:before,value:raw});
  }return changes;
 };
 // DNA stages exact raw values; merely importing must not round packed ratings.
 let stagedRaw=new Map();
 let presetPlayer=null;
 const presetViews=['signature','equipment'].map(group=>window.EditorPresetView?.create(document.getElementById(group==='signature'?'studioSignature':'studioEquipment'),{
  kind:group,identity:()=>presetPlayer,enabled:()=>presetPlayer?.source==='live-roster'&&!!original,
  read:()=>Object.fromEntries(schema.filter(f=>f.group===group&&!f.readOnly&&f.key!=='PotentialRaw').map(f=>[f.key,window.playerAnimationValues([f.key])[f.key]]).filter(([key,v])=>Number.isInteger(v)&&v>=0&&v<=maximum(schema.find(f=>f.key===key)))),
  stage:values=>{window.stagePlayerRatingValues(values);window.dispatchEvent(new Event('player-editor:staged'));}
 }));
 window.addEventListener('player-editor:open',e=>{presetPlayer=e.detail;for(const view of presetViews)view?.reset();});
 window.addEventListener('player-editor:open',()=>stagedRaw.clear());
 window.resetStagedPlayerRatingValues=indexes=>{
  window.dispatchEvent(new Event('player-editor:preview-invalidated'));
  if(indexes===undefined)stagedRaw.clear();else for(const index of indexes)stagedRaw.delete(index);
  syncPairs();panels.hotzones.refreshCourt();
 };
 window.stagePlayerRatingValues=values=>{if(!original)throw Error('Refresh a live player first.');for(const [key,raw] of Object.entries(values)){const field=schema.find(f=>f.key===key);if(!field||field.key==='PotentialRaw'||field.readOnly)throw Error('Unsupported editable field: '+key);if(!Number.isInteger(raw)||raw<0||raw>maximum(field))throw Error('Invalid DNA value for '+field.label);}
  window.dispatchEvent(new Event('player-editor:preview-invalidated'));
  for(const [key,value] of Object.entries(values)){const index=schema.findIndex(f=>f.key===key),field=schema[index];const row=panels[field.group].rows.find(row=>row.index===index);row.input.value=String(display(field,value));updateRow(row);stagedRaw.set(index,{value,text:row.input.value});}syncPairs();panels.hotzones.refreshCourt();
 };
 window.acceptImportedPlayerRatings=values=>{
  window.stagePlayerRatingValues(values);
  for(const [key,value] of Object.entries(values)){const index=schema.findIndex(f=>f.key===key);if(index>=0){original[index]=value;stagedRaw.delete(index);}}
 };
 window.playerAnimationValues=keys=>Object.fromEntries(keys.map(key=>{
  const index=schema.findIndex(f=>f.key===key),field=schema[index];
  const row=field&&panels[field.group]?.rows.find(r=>r.index===index);
  const text=row?.input.value.trim(),value=text!==''?Number(text):original?.[index];
  return [key,Number.isInteger(value)?value:original?.[index]];
 }));
})();
