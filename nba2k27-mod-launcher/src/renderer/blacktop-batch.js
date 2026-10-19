(function(root){
 async function randomAll(api,initial,onProgress=()=>{}){
  const players=initial?.players||[];
  if(!players.length||players.length>10||players.some(p=>p.error||!['male','female'].includes(p.clothingPool)))throw Error('Refresh players; every selected player must have a verified clothing fit.');
  const signature=s=>JSON.stringify(s.players.map(p=>[p.slot,p.name,p.cyberface]));
  const expected=signature(initial);let snapshot=initial,completed=0;
  for(const player of players){
   if(signature(snapshot)!==expected)throw Error(`Lineup changed after ${completed} outfits; stopped.`);
   onProgress(completed,players.length,player.name);
   snapshot={...snapshot,...await api.applyBlacktop({token:snapshot.token,slot:player.slot,randomFit:true,randomMode:'all',omitCatalog:true,rosterShoes:true})};
   const r=snapshot.report;
   if(!r||(!r.unchanged&&!(r.consumed&&r.pointerMatches&&r.uniformMatches&&r.countMatches!==false)))throw Error(`Stopped after ${completed} confirmed outfits; refresh was not confirmed for ${player.name}.`);
   completed++;
  }
  return {snapshot,completed};
 }
 const api={randomAll};if(typeof module==='object')module.exports=api;else root.BlacktopBatch=api;
})(globalThis);
