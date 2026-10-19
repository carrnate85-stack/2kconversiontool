import {MotionStage,poseActor} from './renderer.js';
import {sampleFrame} from './motion.js';
import {composeShot} from './jumpshot_blend.js';
import {buildReleasePlan,releaseBallPose} from './ball_release.js';
import {liveShotLoader,clipCache} from './jumpshot_loader.js';
import {freeThrowPhases,composeFreeThrow} from './free_throw_sequence.js';

const shots=new Set(['ShotJumper','ShotRelease','ShotRelease2','ShotTiming','ShotBlendingRatio']);
const make=(tag,text,cls)=>{const el=document.createElement(tag);if(text)el.textContent=text;if(cls)el.className=cls;return el;};
const button=text=>{const el=make('button',text);el.type='button';return el;};

// Transport is injected. This component knows no launcher API, game path or port.
export function openAnimationPopup({transport,catalog,fieldKey,values,onUse,onClose,onRetry,autoplay=false,host=document.body}){
 const session=crypto.randomUUID(),dialog=make('dialog',null,'animation-popup');dialog.setAttribute('aria-label','Player animation preview');
 const header=make('header'),title=make('h2','Animation preview'),close=button('Close'),use=button('Use these choices');header.append(title);if(onUse)header.append(use);header.append(close);
 const choices=make('div',null,'animation-choices'),variantRow=make('label','Package motion '),variant=make('select');variant.setAttribute('aria-label','Package motion');variantRow.append(variant);variantRow.hidden=true;
 const scene=make('div',null,'animation-scene'),canvas=make('canvas');canvas.setAttribute('aria-label','3D animation preview');scene.append(canvas);
 const controls=make('div',null,'animation-transport'),play=button('Play'),restart=button('Restart'),reset=button('Reset camera'),clock=make('output','0.00 seconds'),scrub=make('input');scrub.type='range';scrub.min='0';scrub.step='0.001';scrub.value='0';scrub.disabled=true;scrub.setAttribute('aria-label','Animation timeline');controls.append(play,restart,reset,clock);
 const status=make('p','Loading preview…');status.setAttribute('role','status');const note=make('p',(onUse?'Neutral player preview. Choices are staged only when you select Use these choices.':'Neutral player preview. This preview does not save choices.')+' Body adaptation, blending and synthetic ball flight are approximate.','animation-note');
 dialog.append(header,choices,variantRow,scene,controls,scrub,status,note);host.append(dialog);dialog.showModal();close.focus();
 let stage,resize,raf=0,disposed=false,revision=0,data=null,plan=null,time=0,playing=false,last=0,character=null;
 play.disabled=true;
 let staged={...values},inputs={},parts=null,loadCount=0,frames=0,drawStarted=0,refreshMix=()=>{};const events=new AbortController();
 const listen=(el,name,fn)=>el.addEventListener(name,fn,{signal:events.signal});
 const setStatus=text=>{if(!disposed)status.textContent=text;};
 const stop=()=>{time=0;playing=false;play.textContent='Play';scrub.value='0';clock.textContent='0.00 seconds';};
 const retry=button('Retry preview');retry.hidden=true;status.after(retry);
 const fail=error=>{if(disposed)return;stop();data=null;play.disabled=true;scrub.disabled=true;for(const a of stage?.actors||[])a.anchor.visible=false;retry.hidden=!loader&&!onRetry;setStatus((error.message||String(error))+(!loader&&!onRetry?' Close this preview and recheck the game folder to retry.':''));};
 let loader,closedPromise;
 const release=()=>{if(closedPromise)return closedPromise;disposed=true;revision++;events.abort();loader?.dispose();partCache.clear();data=null;plan=null;character=null;parts=null;inputs={};staged={};cancelAnimationFrame(raf);resize?.disconnect();stage?.dispose();dialog.close();dialog.remove();closedPromise=(async()=>{try{await transport.close(session);}catch{}onClose?.({frames,fps:drawStarted?frames*1000/(performance.now()-drawStarted):0,resourcesReleased:!!stage?.disposed,gpuGeometries:stage?.renderer.info.memory.geometries??0,gpuTextures:stage?.renderer.info.memory.textures??0,gpuContextLost:stage?.renderer.getContext().isContextLost()??false});})();return closedPromise;};
 listen(close,'click',release);listen(dialog,'cancel',event=>{event.preventDefault();void release();});
 listen(retry,'click',()=>{if(onRetry){onRetry();return;}retry.hidden=true;stop();loader?.schedule(isShot?{...staged}:{value:staged[fieldKey],variant:0},true);});
 const option=(value,label)=>{const el=make('option',label);el.value=String(value);return el;};
 function fieldSelect(key,label){
  const row=make('label',label+' '),input=make('select');input.setAttribute('aria-label',label);const f=catalog.fields.find(f=>f.key===key);
  if(!f)throw Error('Animation field is absent from the verified catalog');
  const ordered=key==='ShotRelease2'?[...f.choices.filter(c=>c.status==='none'),...f.choices.filter(c=>c.status!=='none')]:f.choices;
  for(const c of ordered)input.append(option(c.value,c.label));
  if(!f.choices.some(c=>c.value===staged[key])){
   if(staged[key]!==undefined){const unknown=option(staged[key],'Unknown ('+staged[key]+') — unchanged');unknown.disabled=true;input.append(unknown);}
   else staged[key]=f.choices[0].value;
  }
  input.value=String(staged[key]);inputs[key]=input;row.append(input);choices.append(row);return input;
 }
 function install(next,keepCamera=false){
  if(disposed)return;const camera=keepCamera&&data?{target:stage.target.clone(),radius:stage.radius,theta:stage.theta,phi:stage.phi}:null;
  data=next;plan=buildReleasePlan(data);stage.load([data],character);stage.display('mesh',false);retry.hidden=true;
  if(camera){stage.focusPlayerMotion();Object.assign(stage,{radius:camera.radius,theta:camera.theta,phi:camera.phi});stage.updateCamera();}
  time=Math.min(time,data.duration);scrub.max=String(data.duration);scrub.disabled=false;play.disabled=false;
  if(autoplay){playing=true;play.textContent='Pause';}
  poseActor(stage.actors[0],sampleFrame(data,time),releaseBallPose(plan,time));loadCount++;
  if(!camera)stage.focusPlayerMotion();
  stage.followPosition=null;
  stage.followActor();
 }
 const isShot=shots.has(fieldKey);
 const partCache=clipCache((key,value)=>transport.shotPart({field:key,value,session,request:++revision}),12);
 function compose(){
  if(!parts)return;const ratio=parts[2]?Number(staged.ShotBlendingRatio??0)/100:0;
  if(!Number.isInteger(staged.ShotBlendingRatio)||staged.ShotBlendingRatio<0||staged.ShotBlendingRatio>100)throw Error('The stored blending ratio is outside the supported range; it is unchanged.');
  const timing=catalog.release_timing.choices.find(c=>c.value===Number(staged.ShotTiming??2));
  if(!timing)throw Error('Release speed is unavailable for this game revision');
  install(composeShot(...parts,ratio,timing),true);
 }
 const ready=(async()=>{try{
  stage=new MotionStage(canvas);resize=new ResizeObserver(()=>stage.resize(scene.clientWidth,scene.clientHeight));resize.observe(scene);
  const loadedCharacter=await transport.character({session,request:++revision});if(disposed)return;character=loadedCharacter;
  if(isShot){
   const keys=[['ShotJumper','Base'],['ShotRelease','Upper release 1'],['ShotRelease2','Upper release 2']];
   for(const [key,label] of keys)fieldSelect(key,label);
   const timingRow=make('label','Release speed '),timing=make('select');timing.setAttribute('aria-label','Release speed');for(const c of catalog.release_timing.choices)timing.append(option(c.value,c.label));if(staged.ShotTiming!==undefined&&!catalog.release_timing.choices.some(c=>c.value===staged.ShotTiming)){const unknown=option(staged.ShotTiming,'Unknown ('+staged.ShotTiming+') — unchanged');unknown.disabled=true;timing.append(unknown);}timing.value=String(staged.ShotTiming??2);staged.ShotTiming=Number(timing.value);timingRow.append(timing);inputs.ShotTiming=timing;choices.append(timingRow);
   const ratioRow=make('label','Release 2 mix '),ratio=make('input');ratio.type='range';ratio.min='0';ratio.max='100';ratio.step='1';ratio.value=String(staged.ShotBlendingRatio??0);staged.ShotBlendingRatio??=0;ratio.setAttribute('aria-label','Release 2 mix');const mix=make('output');ratioRow.append(ratio,mix);inputs.ShotBlendingRatio=ratio;choices.append(ratioRow);
   const none=()=>catalog.fields.find(f=>f.key==='ShotRelease2').choices.find(c=>c.value===staged.ShotRelease2)?.status==='none';
   const mixText=()=>{ratio.disabled=none();mix.textContent=none()?'Release 1 only':`${100-Number(ratio.value)}% / ${ratio.value}%`;};
   loader=liveShotLoader({load:async selection=>{const a=await partCache('ShotJumper',selection.ShotJumper),b=await partCache('ShotRelease',selection.ShotRelease);const second=catalog.fields.find(f=>f.key==='ShotRelease2').choices.find(c=>c.value===selection.ShotRelease2);const c=second?.status==='none'?null:await partCache('ShotRelease2',selection.ShotRelease2);return [a,b,c];},apply:next=>{parts=next;stop();compose();setStatus('Combined jump shot · Preview blending and flight are approximate.');},onError:fail,onBusy:busy=>{if(busy){play.disabled=true;setStatus('Updating preview…');}}});
   for(const [key] of keys)listen(inputs[key],'change',()=>{staged[key]=Number(inputs[key].value);stop();parts=null;mixText();loader.schedule({...staged});});
   listen(ratio,'input',()=>{staged.ShotBlendingRatio=Number(ratio.value);mixText();try{compose();}catch(error){fail(error);}});
   listen(timing,'change',()=>{staged.ShotTiming=Number(timing.value);stop();try{compose();}catch(error){fail(error);}});
   refreshMix=mixText;mixText();loader.schedule({...staged},true);
  }else{
   const f=catalog.fields.find(f=>f.key===fieldKey);title.textContent=f?.label||'Animation preview';const field=fieldSelect(fieldKey,f?.label||fieldKey);
   loader=liveShotLoader({load:async selection=>{
    const resolved=await transport.resolve({field:fieldKey,value:selection.value});
    if(!resolved.variants.length)return {resolved,data:null};
    if(fieldKey==='FreeThrow'){
     const clips=[];
     for(const phase of freeThrowPhases(resolved)){
      if(disposed)throw Error('Preview closed.');
      setStatus('Loading full free throw: '+phase.phase+'…');
      clips.push(await transport.load({field:fieldKey,value:selection.value,variant:phase.variant_index,session,request:++revision}));
     }
     return {resolved,data:composeFreeThrow(clips),fullFreeThrow:true};
    }
    const variantIndex=selection.variant??0;
    const motion=await transport.load({field:fieldKey,value:selection.value,variant:variantIndex,session,request:++revision});return {resolved,data:motion,variantIndex};
   },apply:result=>{
    retry.hidden=true;
    variant.replaceChildren(...result.resolved.variants.map(v=>option(v.variant_index,v.phase?`${v.phase} · Motion ${v.variant_index+1}`:`Motion ${v.variant_index+1}`)));variant.value=String(result.variantIndex??0);variantRow.hidden=result.fullFreeThrow||result.resolved.variants.length<2;
    stop();if(result.data){install(result.data,true);setStatus(result.fullFreeThrow?'Full free throw · Preparation, routine, set and shot. Phase transitions are approximate.':result.resolved.status==='contextual'?result.resolved.reason:'Preview ready.');}else{data=null;play.disabled=true;scrub.disabled=true;for(const a of stage.actors)a.anchor.visible=false;setStatus(result.resolved.reason);}
   },onError:fail,onBusy:busy=>{if(busy){play.disabled=true;setStatus('Updating preview…');}}});
   listen(field,'change',()=>{staged[fieldKey]=Number(field.value);stop();loader.schedule({value:staged[fieldKey],variant:0});});
   listen(variant,'change',()=>{stop();loader.schedule({value:staged[fieldKey],variant:Number(variant.value)});});
   loader.schedule({value:staged[fieldKey],variant:0},true);
  }
  listen(play,'click',()=>{if(!data)return;playing=!playing;if(time>=data.duration)time=0;play.textContent=playing?'Pause':'Play';});
  listen(restart,'click',stop);listen(reset,'click',()=>stage.focusPlayerMotion());listen(scrub,'input',()=>{time=Number(scrub.value);playing=false;play.textContent='Play';});
  listen(use,'click',()=>{const keys=isShot?['ShotJumper','ShotRelease','ShotRelease2','ShotTiming','ShotBlendingRatio']:[fieldKey];try{onUse?.(Object.fromEntries(keys.map(k=>[k,staged[k]])));setStatus('Choices staged. Save them separately in the player editor.');}catch(error){setStatus(error.message||String(error));}});
  const draw=now=>{if(disposed)return;if(data){if(playing){time+=Math.min(.1,(now-last)/1000);if(time>=data.duration){time=data.duration;playing=false;play.textContent='Play';}}poseActor(stage.actors[0],sampleFrame(data,time),releaseBallPose(plan,time));stage.followActor();scrub.value=String(time);clock.textContent=`${time.toFixed(2)} / ${data.duration.toFixed(2)} seconds`;}last=now;stage.render();frames++;raf=requestAnimationFrame(draw);};drawStarted=performance.now();raf=requestAnimationFrame(draw);
 }catch(error){fail(error);}})();
 return {ready,close:release,setValues(next){Object.assign(staged,next);for(const [key,input] of Object.entries(inputs))if(next[key]!==undefined)input.value=String(next[key]);refreshMix();stop();parts=null;loader?.schedule(isShot?{...staged}:{value:staged[fieldKey],variant:0});},inspect(){return {disposed,playing,time,loadCount,frames,session,renderer:stage?.inspect(),values:{...staged}};}};
}
