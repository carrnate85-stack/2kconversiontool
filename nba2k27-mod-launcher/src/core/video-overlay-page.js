'use strict';
const video=document.getElementById('intro');let closing=false,readyAt=performance.now()+700,padNeutral=false,padSkip=false,releaseAt=0;
// Set a quiet initial level before assigning src; never start at HTML's 100% default.
video.volume=0.25;
function applyVolume(value){if(!value||!Number.isFinite(value.volume)||value.volume<0||value.volume>1||typeof value.muted!=='boolean')return;video.volume=value.volume;video.muted=value.muted;}
let volumeUpdates=0;
window.startupVideo.onVolume(value=>{volumeUpdates++;applyVolume(value);});
async function finish(reason){if(closing)return;closing=true;video.pause();await window.startupVideo.finish(reason);}
video.addEventListener('ended',()=>{void finish('ended');});
video.addEventListener('error',()=>{document.getElementById('error').hidden=false;setTimeout(()=>{void finish('error');},2500);});
document.addEventListener('pointerup',event=>{event.preventDefault();event.stopPropagation();if(performance.now()>readyAt)void finish('skip');});
document.addEventListener('keydown',event=>{if(['Escape','Enter',' '].includes(event.key)){event.preventDefault();event.stopPropagation();}});
document.addEventListener('keyup',event=>{if(['Escape','Enter',' '].includes(event.key)){event.preventDefault();event.stopPropagation();if(performance.now()>readyAt)void finish('skip');}});
function poll(){
  if(closing)return;
  const pads=Array.from(navigator.getGamepads?.()||[]).filter(Boolean);
  const down=pads.some(p=>p.buttons.some((b,i)=>[0,1,9].includes(i)&&b.pressed));
  if(!down)padNeutral=true;
  if(padNeutral&&down&&performance.now()>readyAt){padSkip=true;releaseAt=0;}
  // No input interception/injection: XInput may also reach the game behind us.
  if(padSkip&&!down){releaseAt||=performance.now();if(performance.now()-releaseAt>200){void finish('skip');return;}}
  requestAnimationFrame(poll);
}
requestAnimationFrame(poll);
Promise.all([window.startupVideo.source(),window.startupVideo.volume().catch(()=>null)]).then(async([source,volume])=>{if(!volumeUpdates)applyVolume(volume);video.src=source;await video.play();}).catch(()=>{document.getElementById('error').hidden=false;setTimeout(()=>{void finish('error');},2500);});
