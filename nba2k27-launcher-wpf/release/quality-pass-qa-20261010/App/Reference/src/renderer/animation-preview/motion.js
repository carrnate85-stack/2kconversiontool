import {Vector3,Quaternion} from './vendor/three.core.min.js';
export const BODY_PARENTS=[-1,0,1,2,3,4,3,6,7,8,3,10,11,12,0,14,15,16,0,18,19,20];
const finiteArray=(value,n)=>Array.isArray(value)&&value.length===n&&value.every(Number.isFinite);

export function validateMotion(data){
  if(!data||!Array.isArray(data.frames)||!data.frames.length)throw new Error('The file has no animation frames.');
  if(!Number.isFinite(data.fps)||data.fps<=0)throw new Error('Invalid animation sample rate.');
  if(data.actor_index!=null&&(!Number.isInteger(data.actor_index)||data.actor_index<0||data.actor_index>63))throw new Error('Invalid human actor index.');
  const count=data.bone_names?.length;
  if(data.schema!=='nba2k27-spatial-clip-v1'&&(count!==22||JSON.stringify(data.parents)!==JSON.stringify(BODY_PARENTS)))throw new Error('Choose a decoded 22-joint human animation.');
  if(!Number.isInteger(count)||count<1||count>63||data.parents?.length!==count||!data.parents.every((p,i)=>Number.isInteger(p)&&p>=-1&&p<i))throw new Error('Invalid spatial joint hierarchy.');
  if(!data.bind_offsets_cm?.every(v=>finiteArray(v,3))||data.bind_offsets_cm.length!==count)throw new Error('Invalid animation bind offsets.');
  let previous=-Infinity;
  for(const frame of data.frames){
    if(!Number.isFinite(frame.time)||frame.time<0||frame.time<previous)throw new Error('Invalid animation frame timing.');
    previous=frame.time;
    if(frame.rotations_xyzw?.length!==count||!frame.rotations_xyzw.every(q=>finiteArray(q,4)&&q.some(v=>v!==0))||frame.translations_cm?.length!==count||!frame.translations_cm.every(v=>finiteArray(v,3)))throw new Error('Invalid body pose in animation.');
    if(frame.trajectory&&(!finiteArray(frame.trajectory.position_xz_cm,2)||!Number.isFinite(frame.trajectory.heading_units)))throw new Error('Invalid movement track.');
    for(const prop of frame.props||[])if(!finiteArray(prop.translation_cm,3)||!finiteArray(prop.rotation_xyzw,4))throw new Error('Invalid prop track.');
    if(frame.hand_rotations_xyzw&&(frame.hand_rotations_xyzw.length!==2||!frame.hand_rotations_xyzw.every(hand=>hand.length===17&&hand.every(q=>finiteArray(q,4)&&q.some(v=>v!==0)))))throw new Error('Invalid finger pose.');
  }
  anchorFor(data);
  return data;
}
export function actorsFrom(data){
  const actors=Array.isArray(data?.actors)?data.actors:[data];
  if(!actors.length)throw new Error('No human actors in this animation.');
  return actors.map(validateMotion);
}
// Keep accumulation explicit: parents precede children in the verified hierarchy.
export function bindPositions(data){
  const result=[];
  for(let i=0;i<data.parents.length;i++)result.push(new Vector3(...data.bind_offsets_cm[i]).add(data.parents[i]>=0?result[data.parents[i]]:new Vector3()));
  return result;
}
export function anchorFor(data){
  if(!data.use_container_placement)return {position:[0,0],heading:0};
  if(data.preview_anchor_placement){
    const anchor=data.preview_anchor_placement;
    if(!finiteArray(anchor.position_xz_cm,2)||!Number.isFinite(anchor.heading_units))throw new Error('Invalid saved actor anchor.');
    return {position:anchor.position_xz_cm,heading:anchor.heading_units};
  }
  const p=data.container_placement;if(!p)throw new Error('Group actor is missing its placement table.');
  if(!finiteArray(p.position_xz_raw,2)||!Number.isInteger(p.orientation_word_raw)||p.orientation_word_raw<0||p.orientation_word_raw>65535)throw new Error('Invalid actor placement table.');
  const word=p.orientation_word_raw;const heading=word&32768?word-65536:word;
  const initial=data.frames[0].trajectory?.position_xz_cm||[0,0];
  return {position:initial.every(v=>v===0)?p.position_xz_raw:[0,0],heading};
}
const angle=units=>units*Math.PI*2/65536;
export function worldPose(data,frame){
  const anchor=anchorFor(data);const travel=frame.trajectory||{position_xz_cm:[0,0],heading_units:0};
  const turn=new Quaternion().setFromAxisAngle(new Vector3(0,1,0),angle(travel.heading_units+anchor.heading));
  const anchorTurn=new Quaternion().setFromAxisAngle(new Vector3(0,1,0),angle(anchor.heading));
  const origin=new Vector3(travel.position_xz_cm[0],0,travel.position_xz_cm[1]).applyQuaternion(anchorTurn).add(new Vector3(anchor.position[0],0,anchor.position[1]));
  const positions=[],rotations=[];
  for(let i=0;i<data.parents.length;i++){
    const parent=data.parents[i],q=new Quaternion(...frame.rotations_xyzw[i]).normalize();
    const p=new Vector3(...data.bind_offsets_cm[i]).add(new Vector3(...frame.translations_cm[i]));
    if(parent>=0){p.applyQuaternion(rotations[parent]).add(positions[parent]);q.premultiply(rotations[parent]);}
    positions.push(p);rotations.push(q);
  }
  return {positions:positions.map(p=>p.applyQuaternion(turn).add(origin)),rotations:rotations.map(q=>q.premultiply(turn)),anchor,origin};
}
// A negative attachment means model space, not scene/world space. Model-space
// props share the actor's planar travel, heading, and static container anchor.
export function propWorldPose(data,frame,prop,pose=worldPose(data,frame)){
  const joint=prop.attachment_joint??-1;
  let parentPosition,parentRotation;
  if(joint>=0){
    if(joint>=pose.positions.length)return null;
    parentPosition=pose.positions[joint];parentRotation=pose.rotations[joint];
  }else{
    const travel=frame.trajectory||{position_xz_cm:[0,0],heading_units:0};
    parentPosition=pose.origin;
    parentRotation=new Quaternion().setFromAxisAngle(new Vector3(0,1,0),angle(travel.heading_units+pose.anchor.heading));
  }
  return {position:new Vector3(...prop.translation_cm).applyQuaternion(parentRotation).add(parentPosition),rotation:new Quaternion(...prop.rotation_xyzw).normalize().premultiply(parentRotation)};
}
export function sampleFrame(data,time){
  const frames=data.frames;
  if(time<=frames[0].time)return frames[0];
  if(time>=frames.at(-1).time)return frames.at(-1);
  let lo=0,hi=frames.length-1;
  while(hi-lo>1){const mid=(lo+hi)>>1;if(frames[mid].time<=time)lo=mid;else hi=mid;}
  const a=frames[lo],b=frames[hi],t=(time-a.time)/(b.time-a.time);
  const lerp=(x,y)=>x+(y-x)*t;
  const vector=(x,y)=>x.map((v,i)=>lerp(v,y[i]));
  const rotation=(x,y)=>new Quaternion(...x).normalize().slerp(new Quaternion(...y).normalize(),t).toArray();
  const trajectory=a.trajectory&&b.trajectory?{position_xz_cm:vector(a.trajectory.position_xz_cm,b.trajectory.position_xz_cm),heading_units:lerp(a.trajectory.heading_units,b.trajectory.heading_units)}:null;
  return {time,rotations_xyzw:a.rotations_xyzw.map((q,i)=>rotation(q,b.rotations_xyzw[i])),translations_cm:a.translations_cm.map((v,i)=>vector(v,b.translations_cm[i])),trajectory,
    hand_selectors:a.hand_selectors,
    hand_rotations_xyzw:a.hand_rotations_xyzw&&b.hand_rotations_xyzw?a.hand_rotations_xyzw.map((hand,side)=>hand.map((q,i)=>rotation(q,b.hand_rotations_xyzw[side][i]))):undefined,
    props:(a.props||[]).map(p=>{const next=(b.props||[]).find(n=>n.name_hash===p.name_hash&&n.attachment_joint===p.attachment_joint);return next?{...p,translation_cm:vector(p.translation_cm,next.translation_cm),rotation_xyzw:rotation(p.rotation_xyzw,next.rotation_xyzw)}:p;})};
}
export function adaptCharacter(source,data){
  const target=bindPositions(data),rest=(source.rig_rest_cm||source.main_rest_cm).map(p=>new Vector3(...p));
  const ratios=[8,9,12,13,15,16,19,20].map(i=>target[i].distanceTo(target[data.parents[i]])/rest[i].distanceTo(rest[data.parents[i]])).sort((a,b)=>a-b);
  const scale=(ratios[3]+ratios[4])/2,positions=new Float32Array(source.positions_cm.length);
  const offsets=data.bind_offsets_cm.map(p=>[...p]);
  if(source.rig_parents)for(let i=22;i<source.rig_parents.length;i++){
    const parent=source.rig_parents[i],offset=rest[i].clone().sub(rest[parent]).multiplyScalar(scale);
    offsets.push(offset.toArray());target.push(offset.clone().add(target[parent]));
  }
  const p=new Vector3(),adapted=new Vector3(),influence=new Vector3();
  for(let i=0;i<source.vertex_count;i++){
    p.fromArray(source.positions_cm,i*3);adapted.set(0,0,0);
    for(let j=0;j<4;j++){
      const at=i*4+j,w=source.skin_weight[at];if(!w)continue;
      const bone=source.skin_index[at];influence.copy(p).sub(rest[bone]).multiplyScalar(scale).add(target[bone]);adapted.addScaledVector(influence,w);
    }
    adapted.multiplyScalar(.01).toArray(positions,i*3);
  }
  return {positions,scale,offsets};
}
