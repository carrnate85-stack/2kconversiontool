// Retime the composed pose, event markers and release together. Camera and
// synthetic ball-flight gravity remain in physical scene units.
export function retimeShot(data,timing){
  if(timing==null)return data;
  const factor=timing.multiplier;
  if(!Number.isInteger(timing.value)||timing.value<0||timing.value>4||!Number.isFinite(factor)||factor<.1||factor>10)
    throw new Error('Invalid game release-speed choice.');
  return {...data,fps:data.fps*factor,duration:data.duration/factor,
    frames:data.frames.map(frame=>({...frame,time:frame.time/factor})),
    shot_events:data.shot_events?.map(event=>({...event,time:event.time/factor})),
    preview_ball_release:{...data.preview_ball_release,time:data.preview_ball_release.time/factor},
    blend_prototype:{...data.blend_prototype,release_timing:{...timing}}};
}
