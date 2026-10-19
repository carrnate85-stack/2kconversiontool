(function(root,factory){const api=factory();if(typeof module==='object'&&module.exports)module.exports=api;if(root)root.EditorPresetView=api;})(typeof window==='undefined'?globalThis:window,()=>{
 function create(host,options){
  if(!globalThis.modLauncher?.editorPresets)return;
  const d=host.ownerDocument,el=(tag,text)=>{const e=d.createElement(tag);if(text)e.textContent=text;return e;};
  const bar=el('div');bar.className='editor-preset-bar';const name=el('input');name.maxLength=60;name.placeholder='Preset name';name.setAttribute('aria-label','New '+options.kind+' preset name');
  const save=el('button','Save preset'),list=el('select'),load=el('button','Stage preset'),status=el('small');list.setAttribute('aria-label',options.kind+' presets');status.setAttribute('role','status');
  save.type=load.type='button';save.className=load.className='secondary small';bar.append(name,save,list,load,status);host.prepend(bar);let revision=0,busy=false;
  const request=r=>globalThis.modLauncher.editorPresets({kind:options.kind,...r});
  async function refresh(){const token=++revision;try{const rows=await request({action:'list'});if(token!==revision)return;list.replaceChildren();const blank=el('option','Choose '+options.kind+' preset');blank.value='';list.append(blank);for(const row of rows){const o=el('option',row.name);o.value=row.id;list.append(o);}}catch(e){if(token===revision)status.textContent=e.message;}}
  async function operation(action){if(busy||options.enabled?.()===false)return;const identity=options.identity();if(identity==null)return;busy=true;save.disabled=load.disabled=true;status.textContent='';
   try{if(action==='save'){if(!name.value.trim())throw Error('Enter a preset name.');await request({action:'save',name:name.value.trim(),values:options.read()});if(options.identity()!==identity)return;status.textContent='Preset saved in Library/editor-presets.';await refresh();}
    else{if(!list.value)throw Error('Choose a preset.');const v=await request({action:'read',id:list.value});if(options.identity()!==identity)return;options.stage(v.values);status.textContent='Preset staged. Review, then Save & verify.';}}
   catch(e){if(options.identity()===identity)status.textContent=e.message;}
   finally{busy=false;save.disabled=load.disabled=false;}
  }
  save.addEventListener('click',()=>void operation('save'));load.addEventListener('click',()=>void operation('read'));
  return {refresh,reset(){revision++;status.textContent='';name.value='';void refresh();}};
 }
 return {create};
});
