(() => {
 const $=id=>document.getElementById(id);let current=null,version=0;
 const editor=$('playerEditorModal'),find=selector=>editor.querySelector(selector);
 const prepared=new Map();const revision={portrait:0,logo:0};
 const artPanel=document.createElement('div');artPanel.className='player-hero-art';artPanel.append($('playerHeroLogo'),find('.player-portrait'));find('.player-hero').prepend(artPanel);
 const ratingPanel=document.createElement('div');ratingPanel.className='player-team-rating';ratingPanel.append($('playerHeroTeamMark'),find('.player-hero-overall'));find('.player-hero-team').append(ratingPanel);
 const actions=document.createElement('div');actions.className='studio-art-actions';
 actions.append($('choosePlayerPortrait'),$('choosePlayerLogo'));find('.team-editor-head').insertBefore(actions,$('closePlayerEditor'));
 async function prepareArtwork(value,kind){
  if(!value)return null;const cacheKey=kind+value;if(prepared.has(cacheKey))return prepared.get(cacheKey);
  const img=new Image();img.src=value;await img.decode();let result=value;
  if(kind==='portrait'||kind==='logo'){
   const canvas=document.createElement('canvas');const scale=Math.min(1,768/Math.max(img.naturalWidth,img.naturalHeight));canvas.width=Math.max(1,Math.round(img.naturalWidth*scale));canvas.height=Math.max(1,Math.round(img.naturalHeight*scale));
   const ctx=canvas.getContext('2d',{willReadFrequently:true});ctx.drawImage(img,0,0,canvas.width,canvas.height);
   const pixels=ctx.getImageData(0,0,canvas.width,canvas.height).data;let left=canvas.width,top=canvas.height,right=-1,bottom=-1;
   for(let y=0;y<canvas.height;y++)for(let x=0;x<canvas.width;x++)if(pixels[(y*canvas.width+x)*4+3]>16){left=Math.min(left,x);top=Math.min(top,y);right=Math.max(right,x);bottom=Math.max(bottom,y);}
   if(right>=left&&bottom>=top){const crop=document.createElement('canvas');crop.width=right-left+1;crop.height=bottom-top+1;crop.getContext('2d').drawImage(canvas,left,top,crop.width,crop.height,0,0,crop.width,crop.height);result=crop.toDataURL('image/png');}
  }
  prepared.set(cacheKey,result);while(prepared.size>24)prepared.delete(prepared.keys().next().value);return result;
 }
 const tabs=Array.from(editor.querySelectorAll('.studio-tabs [role="tab"]'));
 for(const tab of tabs.slice(1)){const panel=document.createElement('section');panel.id=tab.getAttribute('aria-controls');panel.setAttribute('role','tabpanel');panel.setAttribute('aria-labelledby',tab.id);panel.hidden=true;const title=document.createElement('div');title.className='studio-section-title';title.textContent=tab.textContent;const note=document.createElement('p');note.className='studio-empty';note.textContent='No editable fields are available in this section yet.';panel.append(title,note);find('.studio-panels').append(panel);}
 $('playerEditSecondary').closest('label').after(...['playerEditCyberface','playerEditHeight','playerEditWingspan','playerEditWeight'].map(id=>$(id).closest('label')));
 // Keep editing controls in Player info below the display-only portrait header.
 function selectTab(tab){for(const item of tabs){const active=item===tab;item.setAttribute('aria-selected',String(active));item.tabIndex=active?0:-1;$(item.getAttribute('aria-controls')).hidden=!active;}}
 tabs.forEach((tab,index)=>{tab.addEventListener('click',()=>selectTab(tab));tab.addEventListener('keydown',event=>{let next;if(event.key==='ArrowDown')next=(index+1)%tabs.length;else if(event.key==='ArrowUp')next=(index+tabs.length-1)%tabs.length;else if(event.key==='Home')next=0;else if(event.key==='End')next=tabs.length-1;else return;event.preventDefault();selectTab(tabs[next]);tabs[next].focus();});});
 const key=kind=>kind==='logo'?current?.teamName||'':JSON.stringify([current?.firstName||'',current?.lastName||'',current?.cyberfaceId??null]);
 window.addEventListener('player-editor:committed',event=>{
  current=event.detail.player;version++;find('.player-portrait')?.classList.remove('is-loading');
  const p=current,first=document.createElement('span'),last=document.createElement('strong');first.textContent=p.lastName?(p.firstName||''):'';last.textContent=p.lastName||p.firstName||'Player';$('playerEditTitle').replaceChildren(first,last);
  $('playerHeroPosition').textContent=[p.position,p.jerseyNumber!==undefined?'#'+p.jerseyNumber:''].filter(Boolean).join(' | ');
  const inches=Number.isFinite(p.heightCm)?Math.round(p.heightCm/2.54):null;$('playerHeroHeight').textContent=inches===null?'—':Math.floor(inches/12)+'′'+inches%12+'″';
  $('playerHeroWeight').textContent=Number.isFinite(p.weightLbs)?Math.round(p.weightLbs)+' lb':'—';$('playerHeroFrom').textContent=p.collegeName||'—';
  const yearsPro=Number.isInteger(p.yearsPro)?p.yearsPro:null;
  $('playerHeroYearsPro').textContent=yearsPro??'—';$('playerHeroYearsPro').title=yearsPro===null?'Not mapped in the live roster yet.':'';
 });
 function artwork(kind,value){const img=$(kind==='logo'?'playerHeroLogo':'playerHeroPortrait');if(value)img.src=value;else img.removeAttribute('src');img.classList.toggle('hidden',!value);if(kind==='portrait')$('playerPortraitFallback').classList.toggle('hidden',!!value);else{const mark=$('playerHeroTeamMark');if(value)mark.src=value;else mark.removeAttribute('src');mark.classList.toggle('hidden',!value);}$(kind==='logo'?'choosePlayerLogo':'choosePlayerPortrait').textContent=value?'Change '+(kind==='logo'?'logo':'portrait'):'Add '+(kind==='logo'?'team logo':'portrait');}
 window.addEventListener('player-editor:open',async event=>{
  selectTab(tabs[0]);
  current=event.detail;const token=++version,p=current||{};artwork('portrait',null);artwork('logo',null);$('playerArtStatus').textContent='';$('choosePlayerPortrait').disabled=false;
  const position={Center:'C','Point Guard':'PG','Shooting Guard':'SG','Small Forward':'SF','Power Forward':'PF'}[p.position]||p.position;
  $('playerHeroPosition').textContent=[position,p.jerseyNumber!==undefined?'#'+p.jerseyNumber:''].filter(Boolean).join(' | ')||'PLAYER';
  const first=document.createElement('span'),last=document.createElement('strong');first.textContent=p.lastName?(p.firstName||''):'';last.textContent=p.lastName||p.name||p.firstName||'Player';$('playerEditTitle').replaceChildren(first,last);
  const city=p.teamCity||(p.source==='layout-preview'?'Philadelphia':'');const team=p.teamName||'Unassigned team';
  const cityLine=document.createElement('small'),teamLine=document.createElement('strong');cityLine.textContent=city;teamLine.textContent=city&&team.startsWith(city)?team.slice(city.length).trim():team;$('playerHeroTeam').replaceChildren(cityLine,teamLine);
  const inches=Number.isFinite(p.heightCm)?Math.round(p.heightCm/2.54):null;
  $('playerHeroHeight').textContent=inches!==null?Math.floor(inches/12)+'′'+(inches%12)+'″':'—';
  $('playerHeroWeight').textContent=Number.isFinite(p.weightLbs)?Math.round(p.weightLbs)+' lb':'—';
  const sample=p.source==='layout-preview';
  for(const [id,value] of [['playerHeroAge',p.age??(sample?30:null)],['playerHeroFrom',p.collegeName??(sample?'Kansas':null)],['playerHeroYearsPro',p.yearsPro??(sample?8:null)],['playerHeroPersonality',p.personalityName??(sample?'Laid Back':null)],['playerHeroArchetype',p.archetypeName??(sample?'2-Way Inside-Out Scorer':null)]]){$(id).textContent=value??'—';$(id).title=value==null?'Not mapped in the live roster yet.':'';}
  $('playerHeroOverall').textContent=Number.isInteger(p.overall)?p.overall:'—';$('choosePlayerLogo').disabled=!p.teamName;
  if(!sample&&!p.archetypeName&&p.archetypeKey&&window.modLauncher.playerEditorHeaderLabels){
   $('playerHeroArchetype').title='Loading the game’s cached archetype label…';
   window.modLauncher.playerEditorHeaderLabels({recordIndex:p.recordIndex,playerKey:key('portrait')}).then(labels=>{
    if(token!==version)return;const name=labels?.archetypeName;$('playerHeroArchetype').textContent=name||'—';$('playerHeroArchetype').title=name?'':'The game has not provided a readable archetype label.';
   }).catch(()=>{if(token===version)$('playerHeroArchetype').title='The game’s archetype label is unavailable.';});
  }
  await Promise.all(['portrait','logo'].map(async kind=>{const identity=key(kind),rev=++revision[kind];if(!identity)return;const frame=kind==='portrait'?find('.player-portrait'):null;frame?.classList.add('is-loading');try{const source=await window.modLauncher.playerEditorArtwork({kind,key:identity,playerKey:key('portrait'),recordIndex:p.recordIndex,preview:p.source==='layout-preview'});const value=await prepareArtwork(source,kind);if(token===version&&rev===revision[kind])artwork(kind,value);}catch{if(token===version&&rev===revision[kind])$('playerArtStatus').textContent='Game artwork is unavailable. You can add a local image.';}finally{if(token===version&&rev===revision[kind])frame?.classList.remove('is-loading');}}));
 });
 for(const kind of ['portrait','logo'])$(kind==='logo'?'choosePlayerLogo':'choosePlayerPortrait').addEventListener('click',async()=>{
  if(!current||!key(kind))return;const token=version,button=$(kind==='logo'?'choosePlayerLogo':'choosePlayerPortrait');button.disabled=true;
  try{const source=await window.modLauncher.playerEditorArtwork({kind,key:key(kind),choose:true});if(token===version&&source){const rev=++revision[kind];const value=await prepareArtwork(source,kind);if(token===version&&rev===revision[kind]){artwork(kind,value);find('.player-portrait').classList.remove('is-loading');$('playerArtStatus').textContent='Artwork saved in the app. Game files are unchanged.';}}}
  catch(error){if(token===version)$('playerArtStatus').textContent=String(error.message||'Could not save artwork.').replace(/^Error invoking remote method '[^']+': (?:Error: )?/,'');}
  finally{if(token===version)button.disabled=false;}
 });
})();
