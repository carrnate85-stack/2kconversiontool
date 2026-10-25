import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {runInNewContext} from 'node:vm';
const viewer=readFileSync(new URL('../../frontend/public/fit-preview/fast-viewer.js',import.meta.url),'utf8');
const functionSource=viewer.slice(viewer.indexOf('function configureArtworkPalette('),viewer.indexOf('function makeMeshes('));
class Vector{constructor(...v){this.values=v;}}
class Color{setRGB(...v){this.values=v;return this;}}
const context={THREE:{Color,Vector2:Vector,Vector4:Vector,SRGBColorSpace:'srgb',MathUtils:{degToRad:n=>n*Math.PI/180}}};
runInNewContext(functionSource,context);
test('generated viewer wires source RGB logo regions into uniforms, shader and cache key',()=>{
 const artwork={texture:{userData:{range:null}},placement:{scale:[2,3],offset:[0.1,0.2],rotation:90},palette:[[45,45,45],[115,206,181],[229,229,229]]};
 const material={};context.colorShader(material,null,null,[artwork]);
 const shader={uniforms:{},fragmentShader:'#include <map_pars_fragment>\n#include <map_fragment>'};material.onBeforeCompile(shader);
 assert.deepEqual(Array.from(shader.uniforms.previewArt0Red.value.values),[45/255,45/255,45/255,'srgb']);
 assert.deepEqual(Array.from(shader.uniforms.previewArt0Green.value.values),[115/255,206/255,181/255,'srgb']);
 assert.match(shader.fragmentShader,/artwork.rgb=artwork.r\*previewArt0Red\+artwork.g\*previewArt0Green\+artwork.b\*previewArt0Blue/);
 assert.match(shader.fragmentShader,/mix\(sampledDiffuseColor.rgb,artwork.rgb,artwork.a\)/);
 const plain={};context.colorShader(plain,null,null,[{...artwork,palette:null}]);assert.notEqual(plain.customProgramCacheKey(),material.customProgramCacheKey());
 const raw={uniforms:{},fragmentShader:'#include <map_pars_fragment>\n#include <map_fragment>'};plain.onBeforeCompile(raw);assert.equal(raw.uniforms.previewArt0Red,undefined);
});
test('viewer tracks artwork color changes in part reuse and passes all six palettes',()=>{
 assert.ok(viewer.includes('JSON.stringify(part.overlayPalettes||[])'));
 assert.ok(viewer.includes('palette:overlayPalettes[index]'));
 assert.ok(viewer.includes('emission,part.overlayPalettes||[])'));
});

test('uncompressed RGB masks recover original weights while ranged masks stay untouched',()=>{
 const palette={primary:[20,40,60],secondary:[80,100,120],tertiary:[140,160,180],fullRangeMask:true};
 const artwork={texture:{userData:{range:null}},placement:{scale:[1,1],offset:[0,0],rotation:0},palette:[[1,2,3],[4,5,6],[7,8,9]]};
 const compile=(range,art)=>{const material={};context.colorShader(material,palette,range,[art]);const shader={uniforms:{},fragmentShader:'#include <map_pars_fragment>\n#include <map_fragment>'};material.onBeforeCompile(shader);return {material,shader};};
 const plain=compile(null,artwork);
 assert.match(plain.shader.fragmentShader,/sampledDiffuseColor.rgb=previewLinearToSrgb\(sampledDiffuseColor.rgb\)/);
 assert.match(plain.shader.fragmentShader,/artwork.rgb=previewLinearToSrgb\(artwork.rgb\)/);
 const range={min:[0,0,0,0],max:[1,1,1,1],mask:[1,1,1,0]};
 const ranged=compile(range,{...artwork,texture:{userData:{range}}});
 assert.doesNotMatch(ranged.shader.fragmentShader,/sampledDiffuseColor.rgb=previewLinearToSrgb/);
 assert.doesNotMatch(ranged.shader.fragmentShader,/artwork.rgb=previewLinearToSrgb/);
 const mixed=compile(null,{...artwork,texture:{userData:{range}}});
 assert.notEqual(mixed.material.customProgramCacheKey(),plain.material.customProgramCacheKey());
 assert.match(plain.shader.fragmentShader,/mix\(sampledDiffuseColor.rgb,artwork.rgb,artwork.a\)/);
 const legacy={};context.colorShader(legacy,{...palette,fullRangeMask:undefined},null,[artwork]);
 const legacyShader={uniforms:{},fragmentShader:'#include <map_pars_fragment>\n#include <map_fragment>'};legacy.onBeforeCompile(legacyShader);
 assert.doesNotMatch(legacyShader.fragmentShader,/sampledDiffuseColor.rgb=previewLinearToSrgb/);
 assert.notEqual(legacy.customProgramCacheKey(),plain.material.customProgramCacheKey());
});

test('verified opaque color ignores source alpha without changing other materials',()=>{
 const range={min:[0,0,0.791297913,0],max:[1,1,1,1],mask:[0,0,1,0]};
 const compile=(palette,r=range)=>{const material={};context.colorShader(material,palette,r,[]);const shader={uniforms:{},fragmentShader:'#include <map_pars_fragment>\n#include <map_fragment>'};material.onBeforeCompile(shader);return {material,shader};};
 const opaque=compile({diffuse:true,opaqueColor:true}),ordinary=compile({diffuse:true});
 assert.match(opaque.shader.fragmentShader,/sampledDiffuseColor.a=1.0;diffuseColor\*=sampledDiffuseColor/);
 assert.match(opaque.shader.fragmentShader,/sampledDiffuseColor.rgb=previewSrgbToLinear/);
 assert.doesNotMatch(ordinary.shader.fragmentShader,/sampledDiffuseColor.a=1.0/);
 assert.notEqual(opaque.material.customProgramCacheKey(),ordinary.material.customProgramCacheKey());
 assert.equal(opaque.material.transparent,undefined);assert.equal(opaque.material.alphaTest,undefined);
 assert.match(compile({diffuse:true,opaqueColor:true},null).shader.fragmentShader,/sampledDiffuseColor.a=1.0/);
});
