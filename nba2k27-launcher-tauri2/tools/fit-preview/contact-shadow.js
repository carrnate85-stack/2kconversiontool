// One procedural two-triangle ellipse. No texture, shadow map or offscreen pass.
function createContactShadow() {
 const geometry=new THREE.PlaneGeometry(1,1);
 const material=new THREE.ShaderMaterial({
  transparent:true,depthWrite:false,
  vertexShader:'varying vec2 shadowUv; void main(){shadowUv=uv;gl_Position=projectionMatrix*modelViewMatrix*vec4(position,1.0);}',
  fragmentShader:'varying vec2 shadowUv; void main(){float r=length((shadowUv-0.5)*2.0);float a=(1.0-smoothstep(0.1,1.0,r))*0.23;gl_FragColor=vec4(0.02,0.025,0.035,a);}'
 });
 const mesh=new THREE.Mesh(geometry,material);
 mesh.rotation.x=-Math.PI/2;mesh.visible=false;mesh.renderOrder=-1;
 return mesh;
}
const contactShadow=createContactShadow();
scene.add(contactShadow);
function positionContactShadow(bounds) {
 if(bounds.isEmpty()){contactShadow.visible=false;return;}
 const size=bounds.getSize(new THREE.Vector3()),center=bounds.getCenter(new THREE.Vector3());
 const height=Math.max(size.y,0.001);
 contactShadow.position.set(center.x,bounds.min.y-height*0.001,center.z);
 contactShadow.scale.set(Math.max(size.x*1.1,height*0.42),Math.max(size.z*1.5,height*0.27),1);
 contactShadow.visible=true;
 contactShadow.updateMatrixWorld(true);
}
