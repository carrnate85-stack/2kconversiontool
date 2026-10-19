import * as THREE from './vendor/three.module.min.js';
import {DDSLoader} from './vendor/DDSLoader.js';

const canvas=document.getElementById('previewCanvas');
const errorBox=document.getElementById('error');
const scene=new THREE.Scene();scene.background=new THREE.Color(0x10151b);
const camera=new THREE.PerspectiveCamera(42,1,0.01,100000);
const renderer=new THREE.WebGLRenderer({canvas,antialias:false,powerPreference:'high-performance'});
renderer.outputColorSpace=THREE.SRGBColorSpace;
renderer.setPixelRatio(Math.min(devicePixelRatio||1,1.5));
scene.add(new THREE.AmbientLight(0xffffff,0.7));
const key=new THREE.DirectionalLight(0xffffff,1.1);key.position.set(3,6,5);scene.add(key);
const fill=new THREE.DirectionalLight(0xd8e8ff,0.35);fill.position.set(-4,2,-3);scene.add(fill);
const model=new THREE.Group();model.rotation.x=-Math.PI/2;scene.add(model);
const bodyGroup=new THREE.Group(),clothesGroup=new THREE.Group();const accessoryGroup=new THREE.Group();model.add(bodyGroup,clothesGroup,accessoryGroup);
const meshes=new Map(),textures=new Map();
let activeParts=new Map();
let bodyKey='',yaw=0.45,pitch=0.1,distance=10,clipRadius=1,target=new THREE.Vector3(),drag=null;
// Keyboard panning belongs to the focused canvas, never the catalog/search UI.
canvas.tabIndex=0;
canvas.setAttribute('aria-label','3D clothing mannequin. Drag to orbit, scroll to zoom, hold W or S to pan vertically.');
const panKeys=new Set();let panFrame=0,panTime=0;
function stopPan(){panKeys.clear();cancelAnimationFrame(panFrame);panFrame=0;panTime=0;}
function panStep(time){
 if(!document.hasFocus()||document.activeElement!==canvas||document.hidden){stopPan();return;}
 const dt=panTime?Math.min((time-panTime)/1000,0.05):0;panTime=time;
 const direction=Number(panKeys.has('w'))-Number(panKeys.has('s'));
 if(direction&&dt){target.y+=direction*clipRadius*0.7*dt;draw();}
 if(panKeys.size)panFrame=requestAnimationFrame(panStep);else stopPan();
}
canvas.addEventListener('keydown',event=>{
 const key=event.key.toLowerCase();
 if(!['w','s'].includes(key)||event.ctrlKey||event.altKey||event.metaKey||document.activeElement!==canvas||document.querySelector('dialog[open]'))return;
 event.preventDefault();panKeys.add(key);if(!panFrame)panFrame=requestAnimationFrame(panStep);
});
window.addEventListener('keyup',event=>{panKeys.delete(event.key.toLowerCase());if(!panKeys.size)stopPan();});
canvas.addEventListener('blur',stopPan);window.addEventListener('blur',stopPan);
document.addEventListener('visibilitychange',()=>{if(document.hidden)stopPan();});
window.addEventListener('pagehide',stopPan);
function draw(){
 const width=Math.max(1,canvas.clientWidth),height=Math.max(1,canvas.clientHeight);
 if(canvas.width!==Math.round(width*renderer.getPixelRatio())||canvas.height!==Math.round(height*renderer.getPixelRatio()))renderer.setSize(width,height,false);
 // Bound the depth range to this outfit. The previous 0.01..100000 range
 // discarded useful precision at mannequin distances, causing surface noise.
 camera.near=Math.max(clipRadius*0.001,distance-clipRadius*1.1);
 camera.far=Math.max(camera.near*2,distance+clipRadius*1.1);
 camera.aspect=width/height;camera.updateProjectionMatrix();
 const horizontal=Math.cos(pitch)*distance;
 camera.position.set(target.x+Math.sin(yaw)*horizontal,target.y+Math.sin(pitch)*distance,target.z+Math.cos(yaw)*horizontal);
 camera.lookAt(target);renderer.render(scene,camera);
}
new ResizeObserver(draw).observe(canvas);
const typed=(value,Type)=>value instanceof Type?value:ArrayBuffer.isView(value)?new Type(value.buffer,value.byteOffset,value.byteLength/Type.BYTES_PER_ELEMENT):new Type(value||[]);
function geometry(key,payload){
 if(meshes.has(key)){const entry=meshes.get(key);meshes.delete(key);meshes.set(key,entry);return entry;}
 if(!Array.isArray(payload))throw Error('A cached garment mesh is missing. Reopen this preview.');
 const items=payload.map(source=>{
  const shape=new THREE.BufferGeometry();
  shape.setAttribute('position',new THREE.BufferAttribute(typed(source.positions,Float32Array),3));
  if(source.uvs?.length)shape.setAttribute('uv',new THREE.BufferAttribute(typed(source.uvs,Float32Array),2));
  shape.setIndex(new THREE.BufferAttribute(typed(source.indices,Uint32Array),1));shape.computeVertexNormals();
  for(const [index,group] of (source.groups||[]).entries())shape.addGroup(group.start,group.count,index);
  return {shape,groups:source.groups||[]};
 });
 meshes.set(key,items);
 const pinned=new Set([bodyKey,key,...[...activeParts.values()].map(part=>part.meshKey)]);
 while(meshes.size>32){const oldest=[...meshes.keys()].find(candidate=>!pinned.has(candidate));if(!oldest)break;for(const item of meshes.get(oldest))item.shape.dispose();meshes.delete(oldest);}
 return items;
}
function dds(key,item){
 if(!key)return null;
 if(textures.has(key)){const entry=textures.get(key);textures.delete(key);textures.set(key,entry);return entry.texture;}
 if(!item)return null;
 try{
  const raw=atob(item.bytes),bytes=new Uint8Array(raw.length);
  for(let i=0;i<raw.length;i++)bytes[i]=raw.charCodeAt(i);
  const parsed=item.previewRgba?null:new DDSLoader().parse(bytes.buffer,true);
  if(item.previewRgba&&(!Number.isInteger(item.width)||!Number.isInteger(item.height)||bytes.length!==item.width*item.height*4))return null;
  if(parsed&&(!parsed.format||!parsed.width||!parsed.height||parsed.isCubemap))return null;
  let texture;
  if(item.previewRgba){texture=new THREE.DataTexture(bytes,item.width,item.height,THREE.RGBAFormat);texture.minFilter=texture.magFilter=THREE.LinearFilter;}
  else if((parsed.width<4||parsed.height<4)&&item.range?.mask.every(x=>x===1)&&item.range.min.every((x,i)=>x===item.range.max[i])){
   // Exact constant TXTR channels need no BC7 sub-block upload. The shader
   // reconstructs the declared RGBA, including transparent optical surfaces.
   texture=new THREE.DataTexture(new Uint8Array([255,255,255,255]),1,1,THREE.RGBAFormat);texture.minFilter=texture.magFilter=THREE.NearestFilter;
  }else if(parsed.width<4||parsed.height<4){
   // Stock palette masks can be 1x1 DXT1 blocks. WebGL rejects a compressed
   // texture smaller than a block, so expand its first texel to plain RGBA.
   if(String.fromCharCode(...bytes.slice(84,88))!=='DXT1'||bytes.length<136)return null;
   const color16=offset=>bytes[offset]|(bytes[offset+1]<<8);
   const rgb=value=>[Math.round(((value>>11)&31)*255/31),Math.round(((value>>5)&63)*255/63),Math.round((value&31)*255/31)];
   const first=rgb(color16(128)),second=rgb(color16(130));
   const colors=[first,second,first.map((v,i)=>Math.round((2*v+second[i])/3)),first.map((v,i)=>Math.round((v+2*second[i])/3))];
   const index=bytes[132]&3,color=colors[index];
   texture=new THREE.DataTexture(new Uint8Array([...color,255]),1,1,THREE.RGBAFormat);
   texture.minFilter=texture.magFilter=THREE.NearestFilter;
  }else texture=new THREE.CompressedTexture(parsed.mipmaps,parsed.width,parsed.height,parsed.format);
  texture.colorSpace=item.range?THREE.NoColorSpace:THREE.SRGBColorSpace;
  texture.userData.range=item.range||null;
  if(texture.isCompressedTexture){texture.minFilter=parsed.mipmapCount>1?THREE.LinearMipmapLinearFilter:THREE.LinearFilter;texture.magFilter=THREE.LinearFilter;}
  texture.wrapS=texture.wrapT=THREE.RepeatWrapping;texture.needsUpdate=true;
  textures.set(key,{texture,bytes:bytes.length});
  let total=[...textures.values()].reduce((sum,x)=>sum+x.bytes,0);
  const pinned=new Set([key,...[...activeParts.values()].flatMap(part=>[part.textureKey,part.emissionTextureKey,...(part.materialTextureKeys?.flat()||[]),part.shoeTextureKeys?.left,part.shoeTextureKeys?.right,part.jerseyBigLogoKey,part.jerseyPatchKey,part.jerseyNumberAtlasKey,...[1,2,3,4,5,6].map(slot=>part[`overlayKey${slot===1?'':slot}`])])]);
  while(textures.size>48||total>128*1024*1024){const oldest=[...textures.keys()].find(candidate=>!pinned.has(candidate));if(!oldest)break;const previous=textures.get(oldest);previous.texture.dispose();total-=previous.bytes;textures.delete(oldest);}
  return texture;
 }catch{return null;}
}
function colorShader(material,palette,range,artworks,emission=null){
 if(palette?.diffuse)palette=null;
 const active=artworks.filter(item=>item.texture&&item.placement);
 if(!palette&&!range&&!active.length&&!emission)return;
 material.onBeforeCompile=shader=>{
  shader.uniforms.previewMin={value:new THREE.Vector4(...(range?.min||[0,0,0,0]))};
  shader.uniforms.previewMax={value:new THREE.Vector4(...(range?.max||[1,1,1,1]))};
  shader.uniforms.previewMask={value:new THREE.Vector4(...(range?.mask||[0,0,0,0]))};
  const declarations=[],steps=[];
  if(emission){
   const r=emission.texture.userData.range;
   shader.uniforms.previewEmission={value:emission.texture};shader.uniforms.previewEmissionMin={value:new THREE.Vector4(...r.min)};shader.uniforms.previewEmissionMax={value:new THREE.Vector4(...r.max)};
   shader.uniforms.previewEmissionColor={value:new THREE.Color().setRGB(...emission.palette.color.map(x=>x/255),THREE.SRGBColorSpace)};shader.uniforms.previewEmissionGain={value:emission.palette.intensity};
   declarations.push('uniform sampler2D previewEmission;uniform vec4 previewEmissionMin;uniform vec4 previewEmissionMax;uniform vec3 previewEmissionColor;uniform float previewEmissionGain;');
   shader.fragmentShader=shader.fragmentShader.replace('#include <emissivemap_fragment>','vec4 sourceEmission=mix(previewEmissionMin,previewEmissionMax,texture2D(previewEmission,vMapUv));totalEmissiveRadiance=previewEmissionColor*previewEmissionGain*sourceEmission.r*sourceEmission.a;');
  }
  active.forEach(({texture,placement},index)=>{
   const prefix=`previewArt${index}`,artRange=texture.userData.range;
   shader.uniforms[prefix]={value:texture};
   shader.uniforms[`${prefix}Min`]={value:new THREE.Vector4(...(artRange?.min||[0,0,0,0]))};
   shader.uniforms[`${prefix}Max`]={value:new THREE.Vector4(...(artRange?.max||[1,1,1,1]))};
   shader.uniforms[`${prefix}Mask`]={value:new THREE.Vector4(...(artRange?.mask||[0,0,0,0]))};
   shader.uniforms[`${prefix}Scale`]={value:new THREE.Vector2(...placement.scale)};
   shader.uniforms[`${prefix}Offset`]={value:new THREE.Vector2(...placement.offset)};
   shader.uniforms[`${prefix}Angle`]={value:THREE.MathUtils.degToRad(placement.rotation)};
   declarations.push(`uniform sampler2D ${prefix}; uniform vec4 ${prefix}Min; uniform vec4 ${prefix}Max; uniform vec4 ${prefix}Mask; uniform vec2 ${prefix}Scale; uniform vec2 ${prefix}Offset; uniform float ${prefix}Angle;`);
   steps.push(`{vec2 shifted=vMapUv+${prefix}Offset-vec2(0.5);float c=cos(${prefix}Angle),s=sin(${prefix}Angle);vec2 artworkUv=vec2(c*shifted.x-s*shifted.y,s*shifted.x+c*shifted.y)*${prefix}Scale+vec2(0.5);if(all(greaterThanEqual(artworkUv,vec2(0.0)))&&all(lessThanEqual(artworkUv,vec2(1.0)))){vec4 artwork=texture2D(${prefix},artworkUv);vec4 decodedArtwork=mix(${prefix}Min,${prefix}Max,artwork);artwork=mix(artwork,decodedArtwork,${prefix}Mask);sampledDiffuseColor.rgb=mix(sampledDiffuseColor.rgb,artwork.rgb,artwork.a);}}`);
  });
  if(palette){const linear=rgb=>new THREE.Color().setRGB(...rgb.map(x=>x/255),THREE.SRGBColorSpace);shader.uniforms.previewPrimary={value:linear(palette.primary)};shader.uniforms.previewSecondary={value:linear(palette.secondary)};shader.uniforms.previewTertiary={value:linear(palette.tertiary||[255,255,255])};}
  shader.fragmentShader=shader.fragmentShader.replace('#include <map_pars_fragment>','#include <map_pars_fragment>\nuniform vec4 previewMin; uniform vec4 previewMax; uniform vec4 previewMask;'+(palette?'\nuniform vec3 previewPrimary; uniform vec3 previewSecondary; uniform vec3 previewTertiary;':'')+'\n'+declarations.join('\n')+'\nvec3 previewSrgbToLinear(vec3 c){return mix(c/12.92,pow((c+0.055)/1.055,vec3(2.4)),step(vec3(0.04045),c));}')
   .replace('#include <map_fragment>',`#ifdef USE_MAP\nvec4 sampledDiffuseColor = texture2D(map,vMapUv);\nvec4 decodedColor = mix(previewMin,previewMax,sampledDiffuseColor);\nsampledDiffuseColor=mix(sampledDiffuseColor,decodedColor,previewMask);\n${palette?'sampledDiffuseColor.rgb=sampledDiffuseColor.r*previewPrimary+sampledDiffuseColor.g*previewSecondary+sampledDiffuseColor.b*previewTertiary;':range?'sampledDiffuseColor.rgb=previewSrgbToLinear(sampledDiffuseColor.rgb);':''}\n${steps.join('\n')}\ndiffuseColor*=sampledDiffuseColor;\n#endif`);
 };
 material.customProgramCacheKey=()=>`clothing-preview-${Boolean(palette)}-${Boolean(range)}-${active.length}-${Boolean(emission)}`;
}
function makeMeshes(key,payload,palettes,texture,neutral=false,overlays=[],placements=[],shoeTextures=null,materialTextures=null,emission=null){
 return geometry(key,payload).map((item,i)=>{
  const materials=item.groups.map((group,j)=>{
   const map=neutral?null:materialTextures?materialTextures[i]?.[j]:shoeTextures?(/^R_/i.test(group.material)?shoeTextures.right:shoeTextures.left):texture;
   if(materialTextures&&!map)throw Error('A separate accessory material texture is missing.');
   const material=new THREE.MeshStandardMaterial({color:neutral?0x929da8:map?0xffffff:0xb1b5bc,map,roughness:0.86,side:THREE.DoubleSide,
    polygonOffset:!neutral,polygonOffsetFactor:-1,polygonOffsetUnits:-1});
   // The neutral mannequin has no alpha textures. Hair cards would otherwise
   // become opaque gray sheets; the game's explicitly hidden surface stays hidden.
   if(neutral&&/^(?:hidden_shader|[lr]_armpithair_shader|eyelashes_shader)$/i.test(group.material))material.visible=false;
   if(palettes?.[i]?.[j]?.transparent){material.transparent=true;material.depthWrite=false;material.roughness=0.2;}
   const glow=emission?{texture:emission.texture,palette:emission.palettes[i][j]}:null;
   if(map)colorShader(material,palettes?.[i]?.[j],map.userData.range,overlays.map((art,index)=>({texture:art,placement:placements[index]?.[i]?.[j]})),glow);
   return material;
  });
  if(!materials.length)materials.push(new THREE.MeshStandardMaterial({color:neutral?0x929da8:0xb1b5bc,side:THREE.DoubleSide}));
  return new THREE.Mesh(item.shape,materials);
 });
}
function dropPart(part){
 for(const object of part.objects){object.parent?.remove(object);if(object.userData.ownGeometry)object.geometry.dispose();for(const material of Array.isArray(object.material)?object.material:[object.material])material?.dispose();}
}
function jerseyDecals(objects,bigLogo,patchLogo,numberAtlas,number,asset){
 if(!bigLogo&&!patchLogo&&!numberAtlas)return [];
 model.updateWorldMatrix(true,true);
 const bounds=new THREE.Box3();for(const object of objects)bounds.expandByObject(object);
 if(bounds.isEmpty())return [];
 const size=bounds.getSize(new THREE.Vector3()),center=bounds.getCenter(new THREE.Vector3());
 const front=bounds.max.z+Math.max(size.z*0.025,0.001);
 const make=(texture,width,x,y,depth)=>{
  const geometry=new THREE.PlaneGeometry(width,width);
  const uv=geometry.getAttribute('uv');for(let i=0;i<uv.count;i++)uv.setY(i,1-uv.getY(i));
  const material=new THREE.MeshBasicMaterial({map:texture,transparent:true,side:THREE.DoubleSide,depthWrite:false});
  if(texture.userData.range)colorShader(material,null,texture.userData.range,[]);
  const mesh=new THREE.Mesh(geometry,material);mesh.position.set(x,y,front+depth);mesh.userData.ownGeometry=true;scene.add(mesh);return mesh;
 };
 const result=[];
 if(bigLogo)result.push(make(bigLogo,size.x*0.50,center.x,bounds.max.y-size.y*0.31,0));
 if(patchLogo)result.push(make(patchLogo,size.x*0.105,center.x+size.x*0.24,bounds.max.y-size.y*0.15,size.z*0.004));
 if(numberAtlas&&/^\d{1,2}$/.test(number)){
  const digits=[...number],width=size.x*0.30;
  const numberDrop=/_023_1996_core_away_/.test(asset)?0.68:/_022_1995_core_away_/.test(asset)?0.62:0.51;
  for(let index=0;index<digits.length;index++){
   const digit=Number(digits[index]),geometry=new THREE.PlaneGeometry(width,width);
   const uv=geometry.getAttribute('uv');
   for(let i=0;i<uv.count;i++)uv.setXY(i,(digit+uv.getX(i))/10,1-uv.getY(i));
   const material=new THREE.MeshBasicMaterial({map:numberAtlas,transparent:true,side:THREE.DoubleSide,depthWrite:false});
   if(numberAtlas.userData.range)colorShader(material,null,numberAtlas.userData.range,[]);
   const mesh=new THREE.Mesh(geometry,material);
   mesh.position.set(center.x+(index-(digits.length-1)/2)*size.x*0.21,bounds.max.y-size.y*numberDrop,front+size.z*0.008);
   mesh.userData.ownGeometry=true;scene.add(mesh);result.push(mesh);
  }
 }
 return result;
}
function fit(){
 const bounds=new THREE.Box3().setFromObject(model);
 if(bounds.isEmpty())return;
 bounds.getCenter(target);const size=bounds.getSize(new THREE.Vector3());clipRadius=size.length()/2;
 distance=Math.max(size.x,size.y,size.z)/(2*Math.tan(THREE.MathUtils.degToRad(camera.fov/2)))*1.25;
 draw();
}
function known(){parent.postMessage({type:'fit-preview:known',meshes:[...meshes.keys()],textures:[...textures.keys()]},'*');}
window.addEventListener('message',event=>{
 if(event.source!==parent||event.data?.type!=='fit-preview:result')return;
 try{
  const started=performance.now();
  const payload=event.data.payload;
  if(bodyKey!==payload.mannequin.key){
   for(const object of [...bodyGroup.children]){bodyGroup.remove(object);for(const material of Array.isArray(object.material)?object.material:[object.material])material?.dispose();}
   bodyKey=payload.mannequin.key;
   bodyGroup.add(...makeMeshes(bodyKey,payload.mannequin.meshes,null,null,true));
   fit();
  }
  const previous=activeParts,next=new Map(),occurrences=new Map();let reused=0;
  for(const part of payload.parts){
   const occurrence=occurrences.get(part.asset)||0;occurrences.set(part.asset,occurrence+1);
   const identity=`${part.asset}\0${occurrence}`;
   const overlaySlots=[1,2,3,4,5,6],overlaySuffix=slot=>slot===1?'':slot;
   const signature=`${part.meshKey}:${part.textureKey}:${part.emissionTextureKey||''}:${JSON.stringify(part.emissionPalettes||[])}:${JSON.stringify(part.materialTextureKeys||[])}:${part.shoeTextureKeys?.left||''}:${part.shoeTextureKeys?.right||''}:${part.jerseyBigLogoKey||''}:${part.jerseyPatchKey||''}:${part.jerseyNumberAtlasKey||''}:${part.jerseyNumber||''}:${overlaySlots.map(slot=>part[`overlayKey${overlaySuffix(slot)}`]).join(':')}:${JSON.stringify(part.palettes)}`;
   const old=previous.get(identity);
   if(old?.signature===signature&&(!part.texture||textures.has(part.textureKey))&&(!part.emissionTextureKey||textures.has(part.emissionTextureKey))&&(!part.materialTextureKeys||part.materialTextureKeys.flat().every(key=>textures.has(key)))&&(!part.shoeTextureKeys||['left','right'].every(side=>textures.has(part.shoeTextureKeys[side])))&&(!part.jerseyBigLogo||textures.has(part.jerseyBigLogoKey))&&(!part.jerseyPatch||textures.has(part.jerseyPatchKey))&&(!part.jerseyNumberAtlas||textures.has(part.jerseyNumberAtlasKey))&&overlaySlots.every(slot=>!part[`overlay${overlaySuffix(slot)}`]||textures.has(part[`overlayKey${overlaySuffix(slot)}`]))){next.set(identity,old);reused++;continue;}
   if(old)dropPart(old);
   const texture=dds(part.textureKey,part.texture),overlays=overlaySlots.map(slot=>dds(part[`overlayKey${overlaySuffix(slot)}`],part[`overlay${overlaySuffix(slot)}`]));
   const shoeTextures=part.shoeTextureKeys?Object.fromEntries(['left','right'].map(side=>[side,dds(part.shoeTextureKeys[side],part.shoeTextures?.[side])])):null;
   const placements=overlaySlots.map(slot=>part[`overlayPlacements${overlaySuffix(slot)}`]);
   const materialTextures=part.materialTextureKeys?.map((groups,i)=>groups.map((key,j)=>dds(key,part.materialTextures?.[i]?.[j])));
   const emission=part.emissionTextureKey?{texture:dds(part.emissionTextureKey,part.emissionTexture),palettes:part.emissionPalettes}:null;if(emission&&!emission.texture)throw Error('The emission texture is missing.');
   const objects=makeMeshes(part.meshKey,part.meshes,part.palettes,texture,false,overlays,placements,shoeTextures,materialTextures,emission);
   if(part.accessorySlot){
    if(part.placement!=='body-space')throw Error('Unsupported accessory attachment.');
    accessoryGroup.add(...objects);
   }else clothesGroup.add(...objects);
   const bigLogo=dds(part.jerseyBigLogoKey,part.jerseyBigLogo),patchLogo=dds(part.jerseyPatchKey,part.jerseyPatch);
   const numberAtlas=dds(part.jerseyNumberAtlasKey,part.jerseyNumberAtlas);
   objects.push(...jerseyDecals(objects,bigLogo,patchLogo,numberAtlas,part.jerseyNumber,part.asset));
   const entry={signature,meshKey:part.meshKey,textureKey:part.textureKey,emissionTextureKey:part.emissionTextureKey,materialTextureKeys:part.materialTextureKeys,shoeTextureKeys:part.shoeTextureKeys,jerseyBigLogoKey:part.jerseyBigLogoKey,jerseyPatchKey:part.jerseyPatchKey,jerseyNumberAtlasKey:part.jerseyNumberAtlasKey,objects};
   overlaySlots.forEach(slot=>{entry[`overlayKey${overlaySuffix(slot)}`]=part[`overlayKey${overlaySuffix(slot)}`];});
   next.set(identity,entry);activeParts.set(identity,entry);
  }
  for(const [identity,part] of previous)if(!next.has(identity))dropPart(part);
  activeParts=next;
  const clipBounds=new THREE.Box3().setFromObject(model);
  clipRadius=clipBounds.getSize(new THREE.Vector3()).length()/2+clipBounds.getCenter(new THREE.Vector3()).distanceTo(target);
  errorBox.classList.remove('visible');draw();known();
  parent.postMessage({type:'fit-preview:displayed',progress:Boolean(payload.progress),parts:payload.parts.length,accessories:accessoryGroup.children.length,resources:{geometries:meshes.size,textures:textures.size,activeParts:activeParts.size},assets:payload.parts.map(part=>part.asset),reused,displayMs:Math.round(performance.now()-started)},'*');
 }catch(error){errorBox.textContent='Could not display this outfit: '+error.message;errorBox.classList.add('visible');parent.postMessage({type:'fit-preview:display-error',message:error.message},'*');}
});
canvas.addEventListener('pointerdown',event=>{canvas.focus({preventScroll:true});drag={x:event.clientX,y:event.clientY,yaw,pitch};canvas.setPointerCapture(event.pointerId);});
canvas.addEventListener('pointermove',event=>{if(!drag)return;yaw=drag.yaw-(event.clientX-drag.x)*0.008;pitch=THREE.MathUtils.clamp(drag.pitch+(event.clientY-drag.y)*0.008,-1.35,1.35);draw();});
canvas.addEventListener('pointerup',()=>{drag=null;});
canvas.addEventListener('wheel',event=>{event.preventDefault();distance*=Math.exp(event.deltaY*0.001);distance=THREE.MathUtils.clamp(distance,0.1,100000);draw();},{passive:false});
window.addEventListener('error',event=>{errorBox.textContent='The 3D renderer could not start: '+event.message;errorBox.classList.add('visible');});
parent.postMessage({type:'fit-preview:ready'},'*');draw();

window.addEventListener('pagehide',()=>{for(const part of activeParts.values())dropPart(part);activeParts.clear();for(const object of bodyGroup.children)for(const material of Array.isArray(object.material)?object.material:[object.material])material.dispose();bodyGroup.clear();for(const items of meshes.values())for(const item of items)item.shape.dispose();meshes.clear();for(const item of textures.values())item.texture.dispose();textures.clear();renderer.dispose();});
