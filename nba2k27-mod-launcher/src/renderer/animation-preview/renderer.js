import * as THREE from './vendor/three.module.min.js';
import {adaptCharacter,anchorFor,worldPose,propWorldPose} from './motion.js';
const turn=units=>units*Math.PI*2/65536;
const UP=new THREE.Vector3(0,1,0);
const colors=[new THREE.Color(.025,.25,.68),new THREE.Color(.82,.19,.045),new THREE.Color(.06,.50,.23),new THREE.Color(.50,.13,.63)];

function kitMaterial(color){
  const mat=new THREE.MeshStandardMaterial({color:new THREE.Color(.38,.18,.095),roughness:.78});
  mat.onBeforeCompile=shader=>{
    shader.uniforms.kitColor={value:color};
    shader.vertexShader=shader.vertexShader.replace('#include <common>','#include <common>\nattribute vec3 nativeRest; varying vec3 vNativeRest;').replace('#include <begin_vertex>','#include <begin_vertex>\nvNativeRest=nativeRest;');
    shader.fragmentShader=shader.fragmentShader.replace('#include <common>','#include <common>\nvarying vec3 vNativeRest; uniform vec3 kitColor;');
    shader.fragmentShader=shader.fragmentShader.replace('#include <color_fragment>',`#include <color_fragment>
      float shirt=step(-3.0,vNativeRest.y)*(1.0-step(46.0,vNativeRest.y))*(1.0-step(18.0,abs(vNativeRest.x)));
      float shorts=step(-35.0,vNativeRest.y)*(1.0-step(-3.0,vNativeRest.y));
      diffuseColor.rgb=mix(diffuseColor.rgb,kitColor,shirt);
      diffuseColor.rgb=mix(diffuseColor.rgb,kitColor*0.32,shorts);`);
  };
  mat.customProgramCacheKey=()=> 'nba2k27-kit-v1';return mat;
}
function bodyGeometry(source,data,adapted){
  const geometry=new THREE.BufferGeometry();
  geometry.setAttribute('position',new THREE.BufferAttribute(adapted.positions,3));
  geometry.setAttribute('nativeRest',new THREE.Float32BufferAttribute(source.positions_cm,3));
  geometry.setAttribute('skinIndex',new THREE.Uint16BufferAttribute(source.skin_index,4));
  geometry.setAttribute('skinWeight',new THREE.Float32BufferAttribute(source.skin_weight,4));
  geometry.setIndex(source.indices);
  for(const g of source.groups)geometry.addGroup(g.start,g.count,g.material);
  geometry.computeVertexNormals();return geometry;
}
function ballObject(){
  const group=new THREE.Group();
  const ball=new THREE.Mesh(new THREE.SphereGeometry(.12,24,16),new THREE.MeshStandardMaterial({color:0xca781e,roughness:.82}));ball.castShadow=true;group.add(ball);
  for(let plane=0;plane<3;plane++){
    const points=[];for(let i=0;i<96;i++){const a=i*Math.PI*2/96,x=.1208*Math.cos(a),y=.1208*Math.sin(a);points.push(new THREE.Vector3(...(plane===0?[x,y,0]:plane===1?[x,0,y]:[0,x,y])));}
    const line=new THREE.LineLoop(new THREE.BufferGeometry().setFromPoints(points),new THREE.LineBasicMaterial({color:0x20150e}));group.add(line);
  }
  return group;
}
function propMarker(hash){
  const group=new THREE.Group();group.name=`Prop ${hash}`;
  // Geometry is not present in motion records. Show the authored transform
  // with a neutral marker and local XYZ axes instead of guessing its shape.
  group.add(new THREE.LineSegments(new THREE.EdgesGeometry(new THREE.BoxGeometry(.10,.10,.10)),new THREE.LineBasicMaterial({color:0xf6d272})));
  group.add(new THREE.AxesHelper(.22));
  return group;
}
export function buildActor(data,source,index=0){
  const human=data.schema!=='nba2k27-spatial-clip-v1';if(!human)source=null;
  const color=colors[(data.actor_index??index)%colors.length];
  const anchor=new THREE.Group(),movement=new THREE.Group();anchor.add(movement);
  const adapted=source?adaptCharacter(source,data):null;
  const names=source?.rig_names||data.bone_names,parents=source?.rig_parents||data.parents;
  const offsets=adapted?.offsets||data.bind_offsets_cm;
  const bones=names.map((name,i)=>{const bone=new THREE.Bone();bone.name=name;bone.position.fromArray(offsets[i]).multiplyScalar(.01);return bone;});
  for(let i=0;i<bones.length;i++)if(parents[i]>=0)bones[parents[i]].add(bones[i]);
  for(let i=0;i<bones.length;i++)if(parents[i]<0)movement.add(bones[i]);anchor.updateMatrixWorld(true);
  const skeleton=new THREE.Skeleton(bones);skeleton.calculateInverses();
  let mesh=null;
  if(source){
    const materials=[new THREE.MeshStandardMaterial({color:new THREE.Color(.38,.18,.095),roughness:.78}),kitMaterial(color),new THREE.MeshStandardMaterial({color:new THREE.Color(.90,.85,.76),roughness:.4}),new THREE.MeshStandardMaterial({color:new THREE.Color(.13,.04,.025),roughness:.75})];
    mesh=new THREE.SkinnedMesh(bodyGeometry(source,data,adapted),materials);mesh.bind(skeleton,new THREE.Matrix4());mesh.frustumCulled=false;mesh.castShadow=true;movement.add(mesh);
  }
  const simple=[],skin=new THREE.MeshStandardMaterial({color:new THREE.Color(.38,.18,.095),roughness:.8}),jersey=new THREE.MeshStandardMaterial({color,roughness:.8});
  const add=(joint,geometry,mat,position,scale)=>{const obj=new THREE.Mesh(geometry,mat);if(position)obj.position.copy(position);if(scale)obj.scale.copy(scale);obj.castShadow=true;bones[joint].add(obj);simple.push(obj);return obj;};
  if(human){
  add(5,new THREE.SphereGeometry(.12,20,14),skin,new THREE.Vector3(0,.07,0),new THREE.Vector3(.85,.98,.88));
  add(3,new THREE.SphereGeometry(1,18,12),jersey,new THREE.Vector3(0,-.065,0),new THREE.Vector3(.22,.22,.13));
  add(0,new THREE.SphereGeometry(1,18,12),jersey,null,new THREE.Vector3(.19,.16,.13));
  for(const joint of [7,8,11,12,14,15,18,19,16,20]){
    const child=data.parents.findIndex(p=>p===joint);if(child<0)continue;
    const vector=new THREE.Vector3(...data.bind_offsets_cm[child]).multiplyScalar(.01),length=vector.length(),r=(joint===14||joint===18)? .082:(joint===15||joint===19)? .065:.048;
    const obj=add(joint,new THREE.CapsuleGeometry(r,Math.max(.01,length-2*r),6,12),joint===14||joint===18?jersey:skin,vector.clone().multiplyScalar(.5));obj.quaternion.setFromUnitVectors(UP,vector.normalize());
  }
  }else for(let joint=0;joint<bones.length;joint++){
    add(joint,new THREE.SphereGeometry(.025,8,6),jersey);
    bones[joint].add(new THREE.AxesHelper(.15));
  }
  const helper=new THREE.SkeletonHelper(movement);helper.material.depthTest=false;helper.material.transparent=true;helper.material.opacity=.8;
  const ball=ballObject();movement.add(ball);
  const props=new Map([['7432485b',ball]]);
  for(const frame of data.frames)for(const prop of frame.props||[])if(!props.has(prop.name_hash)){
    const marker=propMarker(prop.name_hash);movement.add(marker);props.set(prop.name_hash,marker);
  }
  return {data,human,anchor,movement,bones,skeleton,mesh,simple,helper,ball,props,handJoints:source?.hand_joints||[]};
}
export function poseActor(actor,frame,release=null){
  const a=anchorFor(actor.data),travel=frame.trajectory||{position_xz_cm:[0,0],heading_units:0};
  actor.anchor.position.set(a.position[0]*.01,0,a.position[1]*.01);actor.anchor.rotation.y=turn(a.heading);
  actor.movement.position.set(travel.position_xz_cm[0]*.01,0,travel.position_xz_cm[1]*.01);actor.movement.rotation.y=turn(travel.heading_units);
  for(let i=0;i<actor.data.parents.length;i++){
    actor.bones[i].position.fromArray(actor.data.bind_offsets_cm[i]).add(new THREE.Vector3(...frame.translations_cm[i])).multiplyScalar(.01);
    actor.bones[i].quaternion.fromArray(frame.rotations_xyzw[i]).normalize();
  }
  for(const hand of actor.handJoints){
    const q=hand.pose_node==null?null:frame.hand_rotations_xyzw?.[hand.side]?.[hand.pose_node];
    if(q)actor.bones[hand.joint].quaternion.fromArray(q).normalize();
    else actor.bones[hand.joint].quaternion.identity();
  }
  const ball=(frame.props||[]).find(p=>p.name_hash==='7432485b');
  for(const object of actor.props.values())object.visible=false;
  for(const prop of frame.props||[]){
    const object=actor.props.get(prop.name_hash),joint=prop.attachment_joint??-1,parent=joint<0?actor.movement:actor.bones[joint];
    if(!object||!parent)continue;
    object.visible=true;if(object.parent!==parent)parent.add(object);
    object.position.fromArray(prop.translation_cm).multiplyScalar(.01);object.quaternion.fromArray(prop.rotation_xyzw).normalize();
  }
  actor.anchor.updateMatrixWorld(true);
  if(ball&&release){
    actor.anchor.add(actor.ball);
    actor.ball.position.copy(actor.anchor.worldToLocal(release.position.clone().multiplyScalar(.01)));
    actor.ball.quaternion.copy(actor.anchor.getWorldQuaternion(new THREE.Quaternion()).invert().multiply(release.rotation));
    actor.anchor.updateMatrixWorld(true);
  }
  actor.skeleton.update();
}
export function disposeActor(actor){
  const geometries=new Set(),materials=new Set();
  for(const root of [actor.anchor,actor.helper])root.traverse(o=>{if(o.geometry)geometries.add(o.geometry);for(const m of Array.isArray(o.material)?o.material:o.material?[o.material]:[])materials.add(m);});
  geometries.forEach(g=>g.dispose());materials.forEach(m=>m.dispose());actor.skeleton.dispose();
}

export class MotionStage{
  constructor(canvas){
    this.renderer=new THREE.WebGLRenderer({canvas,antialias:true,alpha:false});this.renderer.setPixelRatio(Math.min(devicePixelRatio,2));this.renderer.shadowMap.enabled=true;this.renderer.shadowMap.type=THREE.PCFShadowMap;
    this.renderer.outputColorSpace=THREE.SRGBColorSpace;this.renderer.toneMapping=THREE.ACESFilmicToneMapping;this.renderer.toneMappingExposure=1.0;
    this.scene=new THREE.Scene();this.scene.background=new THREE.Color(0x1b2737);this.scene.fog=new THREE.Fog(0x1b2737,28,75);
    this.camera=new THREE.PerspectiveCamera(42,1,.01,150);this.target=new THREE.Vector3(0,1,0);this.radius=5;this.theta=.45;this.phi=1.16;
    this.scene.add(new THREE.HemisphereLight(0xcbdfff,0x474847,2.25));
    const key=new THREE.DirectionalLight(0xffe8d6,3.2);key.position.set(4,8,5);key.castShadow=true;key.shadow.mapSize.set(2048,2048);Object.assign(key.shadow.camera,{left:-16,right:16,top:16,bottom:-16,near:.1,far:40});key.shadow.bias=-.00015;this.scene.add(key);
    const fill=new THREE.DirectionalLight(0x8fbfff,1.1);fill.position.set(-5,3,-4);this.scene.add(fill);
    this.floor=new THREE.Mesh(new THREE.PlaneGeometry(150,150),new THREE.MeshStandardMaterial({color:0x273344,roughness:1}));this.floor.rotation.x=-Math.PI/2;this.floor.receiveShadow=true;this.scene.add(this.floor);
    this.grid=new THREE.GridHelper(80,80,0x657c98,0x42536c);this.grid.material.transparent=true;this.grid.material.opacity=.28;this.scene.add(this.grid);
    this.actors=[];this.setupControls(canvas);this.resize(canvas.clientWidth,canvas.clientHeight);this.updateCamera();
  }
  resize(w,h){if(!w||!h)return;this.renderer.setSize(w,h,false);this.camera.aspect=w/h;this.camera.updateProjectionMatrix();}
  updateCamera(){const s=Math.sin(this.phi);this.camera.position.set(this.radius*s*Math.sin(this.theta),this.radius*Math.cos(this.phi),this.radius*s*Math.cos(this.theta)).add(this.target);this.camera.lookAt(this.target);}
  setupControls(canvas){
    this.controls=new AbortController();const options={signal:this.controls.signal};
    let drag=null;
    canvas.addEventListener('contextmenu',e=>e.preventDefault(),options);
    canvas.addEventListener('pointerdown',e=>{drag={x:e.clientX,y:e.clientY,button:e.button};canvas.setPointerCapture(e.pointerId);},options);
    canvas.addEventListener('pointermove',e=>{
      if(!drag)return;const dx=e.clientX-drag.x,dy=e.clientY-drag.y;drag.x=e.clientX;drag.y=e.clientY;
      if(drag.button===2||e.shiftKey){const right=new THREE.Vector3(1,0,0).applyQuaternion(this.camera.quaternion),up=new THREE.Vector3(0,1,0).applyQuaternion(this.camera.quaternion);this.target.addScaledVector(right,-dx*this.radius*.0015).addScaledVector(up,dy*this.radius*.0015);}
      else {this.theta-=dx*.006;this.phi=THREE.MathUtils.clamp(this.phi-dy*.006,.08,Math.PI-.08);}this.updateCamera();
    },options);
    const end=()=>drag=null;canvas.addEventListener('pointerup',end,options);canvas.addEventListener('pointercancel',end,options);
    canvas.addEventListener('wheel',e=>{e.preventDefault();this.radius=THREE.MathUtils.clamp(this.radius*Math.exp(e.deltaY*.001),.5,80);this.updateCamera();},{...options,passive:false});
  }
  load(actors,source){
    for(const actor of this.actors){this.scene.remove(actor.anchor,actor.helper);disposeActor(actor);}
    this.actors=actors.map((data,i)=>buildActor(data,source,i));for(const a of this.actors)this.scene.add(a.anchor,a.helper);
    this.refreshBounds(true);this.fit();
  }
  refreshBounds(placeFloor=false){
    this.bounds=new THREE.Box3();this.playerBounds=new THREE.Box3();
    for(const {data} of this.actors){const origin=worldPose(data,data.frames[0]).positions[0];for(const frame of data.frames){
      const pose=worldPose(data,frame),root=pose.positions[0].clone();for(const p of pose.positions){
        this.playerBounds.expandByPoint(p.clone().sub(root).add(origin).multiplyScalar(.01));
        this.bounds.expandByPoint(p.clone().multiplyScalar(.01));
      }
      for(const prop of frame.props||[]){
        const value=propWorldPose(data,frame,prop,pose);if(value)this.bounds.expandByPoint(value.position.multiplyScalar(.01));
      }
    }}
    this.playerBounds.expandByScalar(.2);this.bounds.expandByScalar(.2);this.recordedBounds=this.bounds.clone();if(placeFloor){const floor=this.bounds.min.y+.12;this.floor.position.y=floor;this.grid.position.y=floor+.002;}
  }
  fit(){
    if(!this.bounds||this.bounds.isEmpty())return;
    this.target.copy(this.bounds.getCenter(new THREE.Vector3()));const size=this.bounds.getSize(new THREE.Vector3());
    const fov=this.camera.fov*Math.PI/180;this.radius=Math.max(size.y,size.x/Math.min(1,this.camera.aspect),size.z)*.65/Math.tan(fov/2)+1;
    this.radius=Math.max(3.5,this.radius);this.theta=.45;this.phi=1.16;this.updateCamera();
  }
  focusPose(){
    const box=new THREE.Box3();for(const a of this.actors)for(const b of a.bones)box.expandByPoint(b.getWorldPosition(new THREE.Vector3()));
    if(box.isEmpty())return;box.expandByScalar(.2);this.target.copy(box.getCenter(new THREE.Vector3()));
    const size=box.getSize(new THREE.Vector3());this.radius=Math.max(3.3,Math.max(size.y,size.x/this.camera.aspect,size.z)*.7/Math.tan(this.camera.fov*Math.PI/360));this.updateCamera();
  }
  focusPlayerMotion(){
    if(!this.playerBounds||this.playerBounds.isEmpty()){this.focusPose();return;}
    const actor=this.actors[0],box=this.playerBounds.clone();
    if(actor){const initial=worldPose(actor.data,actor.data.frames[0]).positions[0].multiplyScalar(.01),current=actor.bones[0].getWorldPosition(new THREE.Vector3());box.translate(current.sub(initial));}
    this.target.copy(box.getCenter(new THREE.Vector3()));const size=box.getSize(new THREE.Vector3());
    this.radius=Math.max(3.3,Math.max(size.y,size.x/this.camera.aspect,size.z)*.8/Math.tan(this.camera.fov*Math.PI/360));this.updateCamera();
  }
  followActor(){
    const actor=this.actors[0];if(!actor)return;
    const position=actor.bones[0].getWorldPosition(new THREE.Vector3());
    if(this.followPosition){
      // Follow the player, not prop flight; preserve user pan, orbit and zoom.
      this.target.add(position.clone().sub(this.followPosition));
      this.updateCamera();
    }
    this.followPosition=position;
  }
  focusHand(){
    const actor=this.actors[0];if(!actor)return;
    if(!actor.human){this.focusPose();return;}
    const ball=actor.ball.visible?actor.ball.getWorldPosition(new THREE.Vector3()):null;
    const wrist=ball&&actor.bones[13].getWorldPosition(new THREE.Vector3()).distanceTo(ball)<actor.bones[9].getWorldPosition(new THREE.Vector3()).distanceTo(ball)?13:9;
    const side=wrist===9?0:1,box=new THREE.Box3();box.expandByPoint(actor.bones[wrist].getWorldPosition(new THREE.Vector3()));
    for(const hand of actor.handJoints)if(hand.side===side)box.expandByPoint(actor.bones[hand.joint].getWorldPosition(new THREE.Vector3()));
    this.target.copy(box.getCenter(new THREE.Vector3()));this.radius=.8;this.updateCamera();
  }
  display(mode,bones){for(const a of this.actors){if(a.mesh)a.mesh.visible=mode==='mesh';for(const o of a.simple)o.visible=!a.human||mode==='simple'||(mode==='mesh'&&!a.mesh);a.helper.visible=!a.human||mode==='skeleton'||bones;}}
  dispose(){
    if(this.disposed)return;this.disposed=true;this.controls?.abort();
    for(const actor of this.actors){this.scene.remove(actor.anchor,actor.helper);disposeActor(actor);}this.actors=[];
    const geometry=new Set(),materials=new Set(),textures=new Set();
    this.scene.traverse(object=>{object.shadow?.dispose();if(object.geometry)geometry.add(object.geometry);for(const material of (Array.isArray(object.material)?object.material:[object.material]))if(material){materials.add(material);for(const value of Object.values(material))if(value?.isTexture)textures.add(value);}});
    if(this.scene.background?.isTexture)textures.add(this.scene.background);if(this.scene.environment?.isTexture)textures.add(this.scene.environment);
    geometry.forEach(value=>value.dispose());textures.forEach(value=>value.dispose());materials.forEach(value=>value.dispose());
    this.scene.clear();this.renderer.dispose();this.renderer.forceContextLoss();
  }
  render(){if(!this.disposed)this.renderer.render(this.scene,this.camera);}
  inspect(){return {three_revision:THREE.REVISION,triangles:this.renderer.info.render.triangles,actors:this.actors.map(a=>({name:a.data.name,joints:a.bones.map(b=>b.getWorldPosition(new THREE.Vector3()).multiplyScalar(100).toArray()),ball:a.ball.visible?a.ball.getWorldPosition(new THREE.Vector3()).multiplyScalar(100).toArray():null,mesh_vertices:a.mesh?.geometry.attributes.position.count||0,vertex_probe:a.mesh?a.mesh.localToWorld(a.mesh.getVertexPosition(0,new THREE.Vector3())).multiplyScalar(100).toArray():null}))};}
}
