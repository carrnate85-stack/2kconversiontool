'use strict';
const {randomInt}=require('node:crypto');
function useFreestylePlaylist(settings,audio){
  if(settings.usePlaylistInFreestyle===false||!audio.audioPolicy)return false;
  const {main,custom}=audio.audioPolicy;
  const count=typeof custom==='string'?[...custom].filter(c=>c==='1').length:0;
  if(!/^[01]{71}$/.test(custom)||custom!==main||count<1||count>71)
    throw new Error('Use playlist in Freestyle currently supports imported songs without stock songs mixed in. Uncheck it in Jukebox to play a mixed playlist in the main menu only.');
  return true;
}
function sampleFreestyleMask(audioPolicy,random=randomInt){
  useFreestylePlaylist({}, {audioPolicy});
  const slots=[...audioPolicy.custom].flatMap((c,i)=>c==='1'?[i]:[]);
  if(slots.length>58){
    for(let i=slots.length-1;i>0;i--){const j=random(i+1);[slots[i],slots[j]]=[slots[j],slots[i]];}
  }
  const mask=Array(71).fill('0');for(const i of slots.slice(0,58))mask[i]='1';
  return mask.join('');
}
module.exports={useFreestylePlaylist,sampleFreestyleMask};
