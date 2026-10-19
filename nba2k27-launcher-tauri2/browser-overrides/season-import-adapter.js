(()=>{
 'use strict';
 let player=null;
 const identity=()=>player?.source==='live-roster'?{recordIndex:player.recordIndex,firstName:player.firstName,lastName:player.lastName,cyberfaceId:player.cyberfaceId}:null;
 const controller=window.VersionTwoSeasonStaging.create({api:request=>window.modLauncher.playerSeasons(request),identity,hasUnsaved:()=>window.hasUnsavedSeasonChanges?.()===true,
  onCapabilities:capabilities=>window.setVersionTwoSeasonCapabilities?.(capabilities),stage:result=>window.stageVersionTwoSeasonReview(result)});
 const prepare=window.preparePlayerSeasonTransfer,importSelected=window.importSelectedPlayerSeasons;
 window.preparePlayerSeasonTransfer=async(history,options)=>await controller.prepare(history,options)||await prepare(history,options);
 window.importSelectedPlayerSeasons=async(history,options)=>{
  const commit=await controller.prepare(history,options);
  if(!commit)return importSelected(history,options);
  const result=commit();const message=result?.noChanges?'The existing season already matches. No changes were staged or written.':'One existing season staged locally. Review its totals, then click Save season. Nothing has been written.';window.dispatchEvent(new CustomEvent(result?.noChanges?'player-seasons:import-noop':'player-seasons:staged-import',{detail:{recordIndex:player.recordIndex,message}}));return true;
 };
 window.addEventListener('player-editor:open',event=>{controller.invalidate();player=event.detail;window.setVersionTwoSeasonCapabilities?.(null);});
 window.addEventListener('player-editor:closed',()=>{controller.invalidate();player=null;window.setVersionTwoSeasonCapabilities?.(null);});
 window.addEventListener('player-editor:committed',event=>{controller.invalidate();player=event.detail.player;});
})();
