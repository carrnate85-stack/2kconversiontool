import {Quaternion} from './vendor/three.core.min.js';
import {sampleFrame,validateMotion,worldPose,propWorldPose} from './motion.js';
import {retimeShot} from './jumpshot_timing.js';

// Prototype mask: spine, neck, head and arms. Native joint weights are pending.
export const UPPER_BODY=Object.freeze(Array.from({length:13},(_,i)=>i+1));
const mix=(a,b,t)=>a.map((v,i)=>v+(b[i]-v)*t);
const rotation=(a,b,t)=>new Quaternion(...a).normalize().slerp(new Quaternion(...b).normalize(),t).toArray();
const BALL='7432485b';
function ballWrist(data,clock){
  const frame=sampleFrame(data,clock[1]),prop=frame.props?.find(p=>p.name_hash===BALL);
  if(!prop)return 9;
  const pose=worldPose(data,frame),ball=propWorldPose(data,frame,prop,pose);
  return ball.position.distanceTo(pose.positions[9])<=ball.position.distanceTo(pose.positions[13])?9:13;
}
function attachedProps(data,frame,wrist){
  return (frame.props||[]).map(prop=>{
    if(prop.name_hash!==BALL||(prop.attachment_joint??-1)>=0)return prop;
    const pose=worldPose(data,frame),ball=propWorldPose(data,frame,prop,pose),inverse=pose.rotations[wrist].clone().invert();
    return {...prop,attachment_joint:wrist,
      translation_cm:ball.position.clone().sub(pose.positions[wrist]).applyQuaternion(inverse).toArray(),
      rotation_xyzw:inverse.multiply(ball.rotation).toArray()};
  });
}

export function anchors(data){
  const duration=data.frames.at(-1).time;
  const event=kind=>data.shot_events?.find(e=>e.type===kind)?.time;
  const points=[0,event(33),event(14),event(34),duration];
  if(!points.every(Number.isFinite)||points.some((v,i)=>v<0||(i&&v<=points[i-1])))
    throw new Error('This shot lacks ordered setup, release and finish markers. Choose another shot.');
  return points;
}

export function mapShotTime(time,target,source){
  if(time<=target[0])return source[0];
  for(let i=1;i<target.length;i++){
    if(time===target[i])return source[i];
    if(time<target[i])return source[i-1]+(source[i]-source[i-1])*(time-target[i-1])/(target[i]-target[i-1]);
  }
  return source.at(-1);
}

export function composeShot(base,release1,release2,ratio,timing=null){
  if(!Number.isFinite(ratio)||ratio<0||ratio>1)throw new Error('Blend ratio must be between 0 and 1.');
  const secondSource=release2;
  if(release2==null){release2=release1;ratio=0;}
  [base,release1,release2].forEach(validateMotion);
  const clock=anchors(base),aClock=anchors(release1),bClock=anchors(release2);
  const aWrist=ballWrist(release1,aClock),bWrist=ballWrist(release2,bClock);
  const frames=base.frames.map(lower=>{
    const a=sampleFrame(release1,mapShotTime(lower.time,clock,aClock));
    const b=sampleFrame(release2,mapShotTime(lower.time,clock,bClock));
    const result={...lower,rotations_xyzw:lower.rotations_xyzw.map(q=>[...q]),translations_cm:lower.translations_cm.map(v=>[...v])};
    for(const joint of UPPER_BODY){
      result.rotations_xyzw[joint]=rotation(a.rotations_xyzw[joint],b.rotations_xyzw[joint],ratio);
      result.translations_cm[joint]=mix(a.translations_cm[joint],b.translations_cm[joint],ratio);
    }
    if(a.hand_rotations_xyzw&&b.hand_rotations_xyzw)
      result.hand_rotations_xyzw=a.hand_rotations_xyzw.map((hand,side)=>hand.map((q,i)=>rotation(q,b.hand_rotations_xyzw[side][i],ratio)));
    // Attachment-local transforms keep an attached ball on the composed hands.
    // Incompatible attachment tracks select the dominant release explicitly.
    const aProps=attachedProps(release1,a,aWrist),bProps=attachedProps(release2,b,bWrist);
    const dominant=ratio<=.5?aProps:bProps,secondary=ratio<=.5?bProps:aProps;
    result.props=dominant.map(p=>{
      const other=secondary.find(n=>n.name_hash===p.name_hash&&n.attachment_joint===p.attachment_joint);
      const weight=ratio<=.5?ratio:1-ratio;
      return other?{...p,translation_cm:mix(p.translation_cm,other.translation_cm,weight),rotation_xyzw:rotation(p.rotation_xyzw,other.rotation_xyzw,weight)}:{...p};
    });
    return result;
  });
  return retimeShot({...base,name:'Jump shot blend prototype',frames,
    preview_ball_release:{mode:'preview',time:clock[2]},
    blend_prototype:{ratio,upper_body_joints:[...UPPER_BODY],base_clock:clock,
      sources:[base,release1,secondSource].map(d=>d?.roster_selection??null),
      status:'Prototype: event-aligned releases on the base clock; native joint weights and complete timing composition not yet reproduced.'}},timing);
}
