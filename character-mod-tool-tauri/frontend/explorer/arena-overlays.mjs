import * as THREE from './vendor/three.module.min.js';

// Court paint on asset.fx uses TEXCOORD1, independently of the asphalt's
// repeating TEXCOORD0. Alpha blends the exact authored design onto the base.
export function configureCourtGraffiti(material, definition, texture, hasUv) {
  if (!definition?.captureCourtGraffiti) return null;
  if (!texture || !hasUv) throw new Error(`Missing court paint texture/coordinates: ${material.name}`);
  const p=definition.parameter || {};
  const finite=(v,f)=>v == null || !Number.isFinite(Number(v)) ? f : Number(v);
  const transform=[finite(p.GraffitiScaleX,1),finite(p.GraffitiScaleY,1),finite(p.GraffitiOffsetX,0),finite(p.GraffitiOffsetY,0)];
  const angle=finite(p.GraffitiRotation,0)*Math.PI/180;
  const previous=material.onBeforeCompile, key=material.customProgramCacheKey();
  material.onBeforeCompile=shader=>{
    previous(shader);
    shader.uniforms.courtPaint={value:texture};
    shader.uniforms.courtPaintTransform={value:new THREE.Vector4(...transform)};
    shader.uniforms.courtPaintRotation={value:new THREE.Vector2(Math.cos(angle),Math.sin(angle))};
    shader.vertexShader='attribute vec2 graffitiUv; varying vec2 vCourtPaintUv;\n'+shader.vertexShader;
    shader.vertexShader=shader.vertexShader.replace('#include <uv_vertex>','#include <uv_vertex>\nvCourtPaintUv=graffitiUv;');
    shader.fragmentShader='varying vec2 vCourtPaintUv; uniform sampler2D courtPaint; uniform vec4 courtPaintTransform; uniform vec2 courtPaintRotation;\n'+shader.fragmentShader;
    shader.fragmentShader=shader.fragmentShader.replace('#include <color_fragment>',`#include <color_fragment>
      vec2 cp=vCourtPaintUv-vec2(.5);
      vec2 cr=courtPaintRotation;
      vec2 cq=vec2(cr.x*cp.x+cr.y*cp.y,-cr.y*cp.x+cr.x*cp.y)*courtPaintTransform.xy+vec2(.5)+courtPaintTransform.zw;
      float ci=step(0.0,cq.x)*step(cq.x,1.0)*step(0.0,cq.y)*step(cq.y,1.0);
      vec4 ink=texture2D(courtPaint,cq);
      diffuseColor.rgb=mix(diffuseColor.rgb,ink.rgb,clamp(ink.a*ci,0.0,1.0));`);
  };
  material.customProgramCacheKey=()=>key+':court-paint-uv1-v1';
  return {slot:'GraffitiAlbedoMap',texture:texture.name,coordinates:'TEXCOORD1',transform,rotationDegrees:angle*180/Math.PI};
}

// The inspected Rucker scoreboard places its alpha artwork on TEXCOORD3,
// independently of the metal atlas on TEXCOORD0. Keep the panel opaque.
export function configureScoreboardOverlay(material, definition, texture, hasOverlayUv) {
  if (!definition?.captureScoreboardOverlay) return null;
  if (!texture || !hasOverlayUv) throw new Error(`Missing scoreboard overlay texture/coordinates: ${material.name}`);
  const p = definition.parameter || {};
  const finite = (value, fallback) => value == null || !Number.isFinite(Number(value)) ? fallback : Number(value);
  const transform = [finite(p.AddOn1ScaleX,1), finite(p.AddOn1ScaleY,1), finite(p.AddOn1OffsetX,0), finite(p.AddOn1OffsetY,0)];
  const tint = p.AddOn1Modulate || [1,1,1,1];
  const color = new THREE.Color().setRGB(...tint.slice(0,3), THREE.SRGBColorSpace);
  const previous = material.onBeforeCompile;
  const previousKey = material.customProgramCacheKey();
  material.onBeforeCompile = shader => {
    previous(shader);
    shader.uniforms.scoreboardOverlay = {value:texture};
    shader.uniforms.scoreboardOverlayTransform = {value:new THREE.Vector4(...transform)};
    shader.uniforms.scoreboardOverlayTint = {value:new THREE.Vector4(color.r,color.g,color.b,finite(tint[3],1))};
    shader.vertexShader = 'attribute vec2 overlayUv; varying vec2 vScoreboardOverlayUv;\n' + shader.vertexShader;
    shader.vertexShader = shader.vertexShader.replace('#include <uv_vertex>', '#include <uv_vertex>\nvScoreboardOverlayUv=overlayUv;');
    shader.fragmentShader = 'varying vec2 vScoreboardOverlayUv; uniform sampler2D scoreboardOverlay; uniform vec4 scoreboardOverlayTransform; uniform vec4 scoreboardOverlayTint;\n' + shader.fragmentShader;
    shader.fragmentShader = shader.fragmentShader.replace('#include <color_fragment>', `#include <color_fragment>
      vec2 boardUv = vScoreboardOverlayUv * scoreboardOverlayTransform.xy + scoreboardOverlayTransform.zw;
      vec4 boardInk = texture2D(scoreboardOverlay, boardUv) * scoreboardOverlayTint;
      float boardInside = step(0.0,boardUv.x)*step(boardUv.x,1.0)*step(0.0,boardUv.y)*step(boardUv.y,1.0);
      diffuseColor.rgb = mix(diffuseColor.rgb, boardInk.rgb, clamp(boardInk.a * boardInside, 0.0, 1.0));`);
  };
  material.customProgramCacheKey = () => previousKey + ':scoreboard-overlay-uv3-v1';
  return {texture:texture.name, slot:'AddOn1AlbedoMap', coordinates:'TEXCOORD3', transform};
}
