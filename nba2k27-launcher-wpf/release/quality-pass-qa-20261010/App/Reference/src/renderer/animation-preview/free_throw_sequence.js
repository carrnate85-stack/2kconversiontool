import {sampleFrame,validateMotion,worldPose,propWorldPose} from './motion.js';
import {buildReleasePlan} from './ball_release.js';

// Wait/additive/catch are contextual next-attempt states, not steps after shooting.
export const FREE_THROW_PHASES=Object.freeze(['pre-routine','routine','set','shot']);
export function freeThrowPhases(resolved){
 return FREE_THROW_PHASES.map(phase=>{
  const matches=resolved.variants.filter(v=>v.phase===phase);
  if(matches.length!==1)throw Error(`The full free throw is unavailable: missing or ambiguous ${phase}.`);
  return matches[0];
 });
}

export function composeFreeThrow(parts){
 if(parts.length!==4)throw Error('A full free throw requires preparation, routine, set and shot.');
 parts.forEach(validateMotion);
 const first=parts[0],frames=[],segments=[];let offset=0,last=null,releaseTime=null;
 for(const [index,data] of parts.entries()){
  if(JSON.stringify(data.parents)!==JSON.stringify(first.parents)||data.bone_names.length!==22)
   throw Error('Free-throw phases have incompatible skeletons.');
  const origin=worldPose(data,data.frames[0]).positions[0];
  const shift=last?[last.translations_cm[0][0]+first.bind_offsets_cm[0][0]-origin.x,last.translations_cm[0][2]+first.bind_offsets_cm[0][2]-origin.z]:[-origin.x,-origin.z];
  const start=offset+(last?.12:0),initial=data.frames[0].time;
  const converted=data.frames.map(frame=>{
   const pose=worldPose(data,frame);
   const root=pose.positions[0].toArray();root[0]+=shift[0];root[2]+=shift[1];
   return {...frame,time:start+frame.time-initial,trajectory:null,
    rotations_xyzw:frame.rotations_xyzw.map((q,i)=>i===0?pose.rotations[0].toArray():[...q]),
    translations_cm:frame.translations_cm.map((p,i)=>i===0?root.map((v,k)=>v-first.bind_offsets_cm[0][k]):p.map((v,k)=>v+data.bind_offsets_cm[i][k]-first.bind_offsets_cm[i][k])),
    props:(frame.props||[]).map(prop=>{
     if((prop.attachment_joint??-1)>=0)return {...prop};
     const value=propWorldPose(data,frame,prop,pose),p=value.position.toArray();p[0]+=shift[0];p[2]+=shift[1];
     return {...prop,translation_cm:p,rotation_xyzw:value.rotation.toArray()};
    })};
  });
  if(last){
   // Brief visual crossfade; native interactive state timing is not reproduced.
   const bridge={frames:[last,converted[0]]};
   for(let t=offset+1/60;t<start-1e-8;t+=1/60)frames.push(sampleFrame(bridge,t));
  }
  frames.push(...converted);offset=frames.at(-1).time;last=frames.at(-1);
  segments.push({phase:FREE_THROW_PHASES[index],start,end:offset,source:data.source_clip});
  if(index===3){const plan=buildReleasePlan(data);if(plan?.time!=null)releaseTime=start+plan.time-initial;}
 }
 const result={...first,frames,duration:offset,fps:60,use_container_placement:false,
  source_clip:parts[3].source_clip,free_throw_sequence:{segments,transition:'Approximate visual phase transitions'},
  preview_ball_release:releaseTime==null?{mode:'recorded'}:{mode:'preview',time:releaseTime}};
 validateMotion(result);return result;
}
