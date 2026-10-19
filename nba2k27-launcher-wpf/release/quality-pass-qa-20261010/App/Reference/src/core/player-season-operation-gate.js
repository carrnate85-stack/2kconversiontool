'use strict';
// Shared roster bridge: season commits must not overlap another player/team
// edit or a review based on a half-finished native edit.
const writers=new Set(['player:save','team:save','team-uniform:save','team-uniform:link']);
const seasonEdits=new Set(['save','add-older','remove-oldest','review-import','apply-import']);
class PlayerSeasonOperationGate {
 constructor(){this.busy=false;}
 async run(channel,args,handler){
  const editing=writers.has(channel)||channel==='player:seasons'&&seasonEdits.has(args[0]?.action);
  if(this.busy&&(editing||channel==='player:seasons'||channel==='player-dna:transfer'))throw Error('Wait for the current roster edit or season import to finish.');
  if(!editing)return handler();this.busy=true;try{return await handler();}finally{this.busy=false;}
 }
}
module.exports={PlayerSeasonOperationGate};
