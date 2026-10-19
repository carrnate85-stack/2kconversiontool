(()=>{
 'use strict';
 const api=window.modLauncher,original=api?.playerEditorArtwork;
 if(typeof original!=='function')return;
 let player=null,epoch=0,active=null;
 const destination=p=>({recordIndex:p.recordIndex,firstName:p.firstName,lastName:p.lastName,cyberfaceId:p.cyberfaceId});
 const visible=()=>!document.getElementById('playerEditorModal')?.classList.contains('hidden');
 const key=(kind,p)=>kind==='logo'?p.teamName||'':JSON.stringify([p.firstName||'',p.lastName||'',p.cyberfaceId??null]);
 async function cancel(entry){if(!entry?.id)return;const id=entry.id;entry.id=null;await original.call(api,{...entry.request,action:'cancel',id}).catch(()=>{});}
 function reset(event){epoch++;const previous=active;active=null;void cancel(previous);player=event?.detail?.player||event?.detail||null;}
 window.addEventListener('player-editor:open',reset);window.addEventListener('player-editor:committed',reset);window.addEventListener('player-editor:closed',()=>reset());
 const modal=document.getElementById('playerEditorModal');
 if(modal&&typeof MutationObserver==='function')new MutationObserver(()=>{if(!visible()&&player)reset();}).observe(modal,{attributes:true,attributeFilter:['class','hidden']});
 api.playerEditorArtwork=async function(request){
  if(request?.choose!==true)return original.call(api,request);
  if(active)throw Error('Another artwork chooser is active.');
  if(player?.source!=='live-roster'||!visible())throw Error('Open a live player before choosing artwork.');
  if(!['portrait','logo'].includes(request.kind)||request.key!==key(request.kind,player)||!request.key)throw Error('Artwork destination changed. Reopen the player editor.');
  const owner=player,visit=epoch,entry={id:null,request:{kind:request.kind,key:request.key,destination:destination(owner)}};active=entry;
  const current=()=>active===entry&&epoch===visit&&player===owner&&visible();
  try{
   const prepared=await original.call(api,{...entry.request,choose:true});
   if(prepared===null)return null;
   if(typeof prepared?.id==='string')entry.id=prepared.id;
   if(typeof prepared?.id!=='string'||typeof prepared?.dataUrl!=='string'||!prepared.dataUrl.startsWith('data:image/png;base64,')||prepared.dataUrl.length>2*1024*1024)throw Error('Invalid prepared artwork receipt.');
   if(!current())return null;
   const value=await original.call(api,{...entry.request,action:'commit',id:entry.id});
   if(typeof value!=='string'||value!==prepared.dataUrl)throw Error('Artwork publication receipt changed.');
   entry.id=null;return current()?value:null;
  }finally{await cancel(entry);if(active===entry)active=null;}
 };
})();
