(() => {
 'use strict';
 document.documentElement.classList.add('roster-browser');
 const key='2k-roster-session';let token=location.hash.slice(1);
 const previewOwner=crypto.randomUUID();let previewSession=null;
 if(/^[a-f0-9]{64}$/.test(token)){try{sessionStorage.setItem(key,token);}catch{}history.replaceState(null,'',location.pathname+location.search);}else{try{token=sessionStorage.getItem(key)||'';}catch{token='';}}
 async function invoke(method,...args){
  let response;try{response=await fetch('/rpc',{method:'POST',headers:{'Content-Type':'application/json','X-Roster-Token':token,'X-Animation-Owner':previewOwner},body:JSON.stringify({method,args})});}
  catch{throw Error('Launcher disconnected. Keep the launcher open, then reopen the roster editor from it.');}
  let data;try{data=await response.json();}catch{throw Error('Editor session expired. Reopen the editor from the launcher.');}
  if(!response.ok||data.error)throw Error(data.error||'Editor request failed.');return data.result;
 }
 const preview={
  onlineRatings:request=>invoke('onlineRatings',request),
  warmAnimationPreview:enabled=>invoke('warmAnimationPreview',enabled),
  async openAnimationPreview(){const row=await invoke('openAnimationPreview');previewSession=row.session;return row;},
  async closeAnimationPreview(session){if(previewSession===session)previewSession=null;return invoke('closeAnimationPreview',session);},
  async animationPreviewRequest(op,args){const row=await invoke('animationPreviewRequest',{op,args});if(row?.url?.startsWith('preview-image://images/__animation/'))row.url='/animation-payload/'+row.url.split('/').at(-1);return row;},
  animationPreviewFetch(url,options={}){if(!/^\/animation-payload\/[a-f0-9]{48}$/.test(url))throw Error('Unexpected animation payload.');return fetch(url,{...options,headers:{'X-Roster-Token':token,'X-Animation-Owner':previewOwner}});},
  onAnimationPreviewProgress(callback){let stopped=false,busy=false;const session=previewSession;const timer=setInterval(async()=>{if(stopped||busy||!session)return;busy=true;try{const row=await invoke('animationPreviewProgress',session);if(!stopped)callback(row);}catch{}finally{busy=false;}},500);return()=>{stopped=true;clearInterval(timer);};},
 };
 window.modLauncher=new Proxy(preview,{get:(target,name)=>Object.hasOwn(target,name)?target[name]:String(name).startsWith('on')?()=>()=>{}:(...args)=>invoke(name,...args)});
 window.addEventListener('beforeunload',()=>{for(const [method,args] of [['warmAnimationPreview',[false]],...(previewSession?[['closeAnimationPreview',[previewSession]]]:[])])void fetch('/rpc',{method:'POST',keepalive:true,headers:{'Content-Type':'application/json','X-Roster-Token':token,'X-Animation-Owner':previewOwner},body:JSON.stringify({method,args})}).catch(()=>{});});
 window.addEventListener('DOMContentLoaded',()=>{
  const banner=document.createElement('div');banner.id='rosterConnection';banner.textContent='Local editor · Keep the launcher open';banner.setAttribute('role','status');document.body.append(banner);
  let busy=false;setInterval(async()=>{if(busy)return;busy=true;try{await invoke('ping');banner.textContent='Local editor · Keep the launcher open';banner.classList.remove('disconnected');}catch{banner.textContent='Launcher disconnected · Reopen the editor from the launcher. Unsaved fields remain here.';banner.classList.add('disconnected');}finally{busy=false;}},5000);
 });
})();
