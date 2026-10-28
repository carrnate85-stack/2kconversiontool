import * as THREE from './vendor/three.module.min.js';

export function numberRects(glyphs, value) {
  if (!/^[0-9]{1,2}$/.test(value)) return [];
  const selected=[...value].map(digit=>glyphs[Number(digit)]);
  if(selected.some(g=>!g || !Number.isFinite(g.aspect)))return [];
  const widths=selected.map(g=>g.aspect*.5),total=widths.reduce((a,b)=>a+b,0);
  let left=.5-total/2;
  return selected.map((g,i)=>{const rect=[left,widths[i],g.x,g.width];left+=widths[i];return rect;});
}

export function configureUniformOverlays(material, items, textures, number) {
  const part=material.name==='uniform_jersey_mat'?'jersey':material.name==='uniform_shorts_mat'?'shorts':null;
  if(!part)return;
  const layers=[];
  items.forEach((item,i)=>{
    const overlay=item.uniformOverlay,texture=textures[i];
    if(!texture || overlay?.part!==part)return;
    if(overlay.kind==='logo')layers.push({overlay,texture});
    if(overlay.kind==='number')for(const placement of overlay.placements||[])layers.push({overlay:{...overlay,...placement},texture});
  });
  // Keep within WebGL's texture sampler budget, even on unusual uniforms.
  const used=layers.slice(0,10),uniforms={};
  const previous=material.onBeforeCompile,previousKey=material.customProgramCacheKey();
  const numbers=[];
  let declarations='',body='';
  used.forEach(({overlay,texture},i)=>{
    const name=`uniformArt${i}`,angle=(overlay.rotate||0)*Math.PI/180;
    const range=texture.userData.range;
    uniforms[name]={value:texture};
    uniforms[`${name}Transform`]={value:new THREE.Vector4(...overlay.transform)};
    uniforms[`${name}Rotation`]={value:new THREE.Vector2(Math.cos(angle),Math.sin(angle))};
    uniforms[`${name}Min`]={value:new THREE.Vector4(...(range?.min||[0,0,0,0]))};
    uniforms[`${name}Max`]={value:new THREE.Vector4(...(range?.max||[1,1,1,1]))};
    uniforms[`${name}Mask`]={value:new THREE.Vector4(...(range?.mask||[0,0,0,0]))};
    declarations+=`uniform sampler2D ${name};uniform vec4 ${name}Transform;uniform vec2 ${name}Rotation;uniform vec4 ${name}Min;uniform vec4 ${name}Max;uniform vec4 ${name}Mask;\n`;
    body+=`{vec2 p=vMapUv-vec2(.5)-${name}Transform.zw;
      vec2 r=${name}Rotation;
      vec2 q=vec2(r.x*p.x+r.y*p.y,-r.y*p.x+r.x*p.y)*${name}Transform.xy+vec2(.5);`;
    if(overlay.kind==='number'){
      uniforms[`${name}Count`]={value:0};
      for(let digit=0;digit<2;digit++){
        uniforms[`${name}Glyph${digit}`]={value:new THREE.Vector4()};
        declarations+=`uniform vec4 ${name}Glyph${digit};`;
      }
      declarations+=`uniform int ${name}Count;`;
      body+=`q.x+=${Number(overlay.shear||0).toFixed(8)}*(q.y-.5);`;
      for(let digit=0;digit<2;digit++)body+=`
        if(${name}Count>${digit} && q.y>=0. && q.y<=1. && q.x>=${name}Glyph${digit}.x && q.x<=${name}Glyph${digit}.x+${name}Glyph${digit}.y){
          vec2 atlas=vec2(${name}Glyph${digit}.z+(q.x-${name}Glyph${digit}.x)/${name}Glyph${digit}.y*${name}Glyph${digit}.w,q.y);
          vec4 art=texture2D(${name},atlas);
          art=mix(art,mix(${name}Min,${name}Max,art),${name}Mask);
          diffuseColor.rgb=mix(diffuseColor.rgb,art.rgb,clamp(art.a,0.,1.));
        }`;
      numbers.push({name,glyphs:overlay.glyphs});
    } else body+=`
      if(q.x>=0. && q.x<=1. && q.y>=0. && q.y<=1.){
        vec4 art=texture2D(${name},q);
        art=mix(art,mix(${name}Min,${name}Max,art),${name}Mask);
        diffuseColor.rgb=mix(diffuseColor.rgb,art.rgb,clamp(art.a,0.,1.));
      }`;
    body+='}\n';
  });
  const update=value=>{
    for(const {name,glyphs} of numbers){
      const rects=numberRects(glyphs,value);
      uniforms[`${name}Count`].value=rects.length;
      rects.forEach((rect,i)=>uniforms[`${name}Glyph${i}`].value.fromArray(rect));
    }
  };
  update(number);
  material.userData.updateUniformNumber=update;
  material.userData.uniformArtwork=used.map(({overlay,texture})=>({kind:overlay.kind,side:overlay.side||'',texture:texture.name}));
  if(!used.length)return;
  material.onBeforeCompile=shader=>{
    previous(shader);
    Object.assign(shader.uniforms,uniforms);
    shader.fragmentShader=declarations+'\n'+shader.fragmentShader;
    // Base range hooks can replace map_fragment, so insert just before alphamap.
    shader.fragmentShader=shader.fragmentShader.replace('#include <alphamap_fragment>',`${body}\n#include <alphamap_fragment>`);
  };
  material.customProgramCacheKey=()=>`${previousKey}:uniform-art-v1:${used.map(l=>l.overlay.kind).join(',')}`;
}
