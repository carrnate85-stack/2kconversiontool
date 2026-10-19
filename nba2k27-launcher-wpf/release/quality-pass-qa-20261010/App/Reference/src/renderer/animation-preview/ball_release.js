import {Vector3,Quaternion} from './vendor/three.core.min.js';
import {sampleFrame,worldPose,propWorldPose} from './motion.js';

const BALL='7432485b',GRAVITY=981;
export function recordedBall(data,time){
  const frame=sampleFrame(data,time),prop=(frame.props||[]).find(p=>p.name_hash===BALL);
  return prop?propWorldPose(data,frame,prop):null;
}
function actionFor(data){
  const name=(data.source_clip||data.name||'').replace(/^ANM_2K\d+_/i,'');
  if(/(?:^|_)(FAKE|CATCH|RECEIVE|RECV|PREROUTINE|WAIT|PRE_STANCE)(?:_|\.|$)/i.test(name))return null;
  if(/^DUNK_/i.test(name))return 'dunk';
  if(/^(SIG_JUMPER|FT_SHOT|JUMP_SHOT)_/i.test(name))return 'shot';
  if(/^(PASS|TOUCH_PASS)_/i.test(name))return 'pass';
  return null;
}
export function buildReleasePlan(data){
  if(data.schema==='nba2k27-spatial-clip-v1')return null;
  const setting=data.preview_ball_release;
  if(setting&&(!['preview','recorded'].includes(setting.mode)||(setting.time!=null&&(!Number.isFinite(setting.time)||setting.time<0||setting.time>data.frames.at(-1).time))))throw new Error('Invalid saved ball release preview.');
  const action=actionFor(data),samples=[];
  for(const frame of data.frames){
    const pose=worldPose(data,frame),prop=(frame.props||[]).find(p=>p.name_hash===BALL);
    if(!prop)continue;
    const ball=propWorldPose(data,frame,prop,pose);if(!ball)continue;
    samples.push({time:frame.time,position:ball.position,rotation:ball.rotation,
      distance:Math.min(ball.position.distanceTo(pose.positions[9]),ball.position.distanceTo(pose.positions[13])),
      reach:ball.position.distanceTo(pose.positions[3])});
  }
  if(samples.length<3)return null;
  // Captured prop tracks sometimes include complete flight; never replace it
  // automatically. A sustained gap from both wrists distinguishes flight from
  // the small ball-to-palm offset. Dribbles aren't release candidates at all.
  let farStart=null,authoredFlight=false;
  for(const s of samples){if(s.distance>75){farStart??=s.time;if(s.time-farStart>=.10)authoredFlight=true;}else farStart=null;}
  const manual=setting?.time!=null;
  if(!manual&&(!action||authoredFlight))return {mode:setting?.mode||'preview',kind:authoredFlight?'authored':'recorded',time:null};
  const candidates=samples.filter(s=>s.time<=samples.at(-1).time*.85);
  let selected=candidates[0];
  for(const s of candidates)if((action==='pass'?s.reach:s.position.y)>(action==='pass'?selected.reach:selected.position.y))selected=s;
  const time=manual?setting.time:selected.time,release=recordedBall(data,time);
  const before=recordedBall(data,Math.max(samples[0].time,time-.06));
  const dt=time-Math.max(samples[0].time,time-.06);
  const velocity=dt>0?release.position.clone().sub(before.position).divideScalar(dt):new Vector3();
  const frame=sampleFrame(data,time),pose=worldPose(data,frame);
  let direction=new Vector3(0,0,1).applyQuaternion(pose.rotations[0]);direction.y=0;direction.normalize();
  let target,targetTime;
  if(action==='shot'&&data.blend_prototype){
    // Preview rim: 6.75 m ahead, 3.05 m high. Solve a forward, descending
    // shot arc rather than assuming a fixed vertical launch speed.
    target=release.position.clone().addScaledVector(direction,675);target.y=305;
    const apex=Math.max(release.position.y+125,target.y+100);
    velocity.y=Math.sqrt(2*GRAVITY*(apex-release.position.y));
    const flightTime=velocity.y/GRAVITY+Math.sqrt(2*(apex-target.y)/GRAVITY);
    velocity.x=(target.x-release.position.x)/flightTime;
    velocity.z=(target.z-release.position.z)/flightTime;
    targetTime=time+flightTime;
  }else if(action==='dunk'){
    velocity.x=direction.x*100;velocity.z=direction.z*100;velocity.y=Math.min(-100,velocity.y);
  }else if(action==='pass'){
    const speed=700;
    velocity.x=direction.x*speed;velocity.z=direction.z*speed;velocity.y=GRAVITY*.5/2;
  }else{
    velocity.x=direction.x*450;velocity.z=direction.z*450;velocity.y=Math.max(650,Math.min(900,velocity.y));
  }
  // Synthetic flight stays separate from native props and exportable keys.
  return {mode:setting?.mode||'preview',kind:manual?'manual':'estimated',action:action||'shot',time,
    position:release.position,rotation:release.rotation,velocity,target,targetTime};
}
export function releaseBallPose(plan,time){
  if(!plan||plan.mode==='recorded'||plan.time==null||time<=plan.time)return null;
  const dt=time-plan.time,position=plan.position.clone().addScaledVector(plan.velocity,dt);
  position.y-=GRAVITY*dt*dt/2;
  const impact=(plan.velocity.y+Math.sqrt(plan.velocity.y**2+2*GRAVITY*Math.max(0,plan.position.y-12)))/GRAVITY;
  if(dt>impact){
    // Settle on the floor after impact rather than tunnelling below the court.
    position.copy(plan.position).addScaledVector(plan.velocity,impact);position.y=12;
  }
  const spin=new Quaternion().setFromAxisAngle(new Vector3(1,0,0),dt*8);
  return {position,rotation:plan.rotation.clone().multiply(spin)};
}
