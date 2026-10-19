(() => {
 const open=document.getElementById('openBlacktopRules');
 const pane=document.getElementById('blacktopRulesPane');
 if(!open||!pane||!window.modLauncher?.readGameRules)return;
 const list=document.getElementById('blacktopRulesList');
 const status=document.getElementById('blacktopRulesStatus');
 const refresh=document.getElementById('refreshBlacktopRules');
 let snapshot=null,busy=false;
 const show=text=>{status.textContent=text;};
 function render(){
  list.replaceChildren();
  for(const row of snapshot?.values||[]){
   const item=document.createElement('div');item.className='blacktop-rule';
   const name=document.createElement('span');name.className='blacktop-rule-name';name.textContent=row.label;
   const controls=document.createElement('div');controls.className='blacktop-rule-controls';
   const options=window.GameRuleValues?.optionsFor(row.field,row.width)||[];
   if(row.width===1&&options.some(option=>option.value===row.value)){
    for(const [text,value] of [['Off',0],['On',1]]){
     const button=document.createElement('button');button.type='button';button.textContent=text;
     button.setAttribute('aria-pressed',String(row.value===value));
     button.disabled=busy||row.value===value;
     button.addEventListener('click',()=>setRule(row.field,value));controls.append(button);
    }
   }else if(row.width===4&&options.some(option=>option.value===row.value)){
    const select=document.createElement('select');select.setAttribute('aria-label',row.label);
    for(const option of options){const choice=document.createElement('option');choice.value=String(option.value);choice.textContent=option.label;select.append(choice);}
    select.value=String(row.value);select.disabled=busy;
    select.addEventListener('change',()=>setRule(row.field,Number(select.value)));controls.append(select);
   }else{
    const value=document.createElement('span');value.className='blacktop-rule-number';
    value.textContent=`Value ${row.value}`;value.title='Numeric field; value meaning not yet established';controls.append(value);
   }
   item.append(name,controls);list.append(item);
  }
 }
 async function read(){
  if(busy)return;busy=true;refresh.disabled=true;show('Reading live rule values…');
  try{snapshot=await window.modLauncher.readGameRules();show(`${snapshot.values.length} live rules loaded`);}
  catch(error){snapshot=null;show(error.message||'Game Rules are unavailable.');}
  finally{busy=false;refresh.disabled=false;render();}
 }
 async function setRule(field,value){
  if(busy||!snapshot)return;
  busy=true;render();show('Applying rule…');
  try{snapshot=await window.modLauncher.setGameRule(snapshot,field,value);show('Rule updated. Other values were left unchanged.');}
  catch(error){show(error.message||'Rule update failed. Refresh before trying again.');snapshot=null;}
  finally{busy=false;render();}
 }
 open.addEventListener('click',()=>{
  if(document.body.dataset.overlayView==='openBlacktopRules'){window.BlacktopOverlay?.expand(false);return;}
  window.BlacktopOverlay?.expand(true,'openBlacktopRules');read();
 });
 refresh.addEventListener('click',read);
})();
