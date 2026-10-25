import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
const mappings=JSON.parse(readFileSync(new URL('./accessory-material-map.json',import.meta.url),'utf8'));
const api={accessoryMaterialMap:mappings};runInNewContext(readFileSync(new URL('./accessory-materials.js',import.meta.url),'utf8'),api);
const dep={name:'vandy_galaxytwill_tex',kind:1};
const color=n=>({name:mappings.hats[dep.name][n-1]||`vandy_galaxytwill_0${n}_c.0123456789abcdef.dds`});
test('verified numbered hat artwork preserves order and rejects ambiguity',()=>{
 const expanded=api.expandVerifiedHatArtwork(dep,[color(2),{name:'vandy_galaxytwill_01_n.0123456789abcdef.dds'},color(1)]);
 assert.equal(expanded.length,2);assert.equal(expanded[0].textureName,color(1).name);assert.equal(expanded[1].textureName,color(2).name);
 for(const textures of [[color(1)],[color(1),color(1)],[color(1),color(3)],[color(1),color(2),{name:'extra_color.dds'}]]){
  assert.equal(api.expandVerifiedHatArtwork(dep,textures)[0],dep);
 }
 const unknown={name:'unknown_bundle'};assert.equal(api.expandVerifiedHatArtwork(unknown,[color(1),color(2)])[0],unknown);
});
test('source diffuse exception requires exact sleeve, shader, texture and no color binding',()=>{
 const name='acc_armsleeve_full_2kevents_proam_s1_org_left',hash=0x8b715672;
 const records=[{type:1,value:hash},{type:5,parameter:hash,value:0xdfee4fe6}];
 const texture={name:mappings.diffuse[name].texture,range:{min:[0,0,0,1],max:[1,1,1,1],mask:[0,0,0,1]}};
 assert.equal(api.verifiedAccessoryPalette(name,records,hash,texture).diffuse,true);
 assert.equal(api.verifiedAccessoryPalette('other',records,hash,texture),null);
 assert.equal(api.verifiedAccessoryPalette(name,records,hash+1,texture),null);
 assert.equal(api.verifiedAccessoryPalette(name,[{type:5,parameter:hash,value:123}],hash,texture),null);
 for(const type of [4,9]) assert.equal(api.verifiedAccessoryPalette(name,[...records,{type}],hash,texture),null);
 assert.equal(api.verifiedAccessoryPalette(name,records,hash,{...texture,name:'normal.dds'}),null);
 assert.equal(api.verifiedAccessoryPalette(name,records,hash,{...texture,range:{...texture.range,min:[0,0,0,0]}}),null);
 assert.equal(api.verifiedAccessoryPalette(name,records,hash,{...texture,range:null}),null);
});
test('every audited diffuse mapping rejects changed source identity or tint bindings',()=>{
 assert.equal(Object.keys(mappings.diffuse).length,1449);
 for(const [asset,m] of Object.entries(mappings.diffuse)){
  const records=[{type:1,value:m.hash},{type:5,parameter:m.hash,value:m.shader}];
  const texture={name:m.texture,range:m.range};
  assert.equal(api.verifiedAccessoryPalette(asset,records,m.hash,texture)?.diffuse,true,asset);
  assert.equal(api.verifiedAccessoryPalette(asset,records,m.hash,{...texture,name:texture.name+'.changed'}),null,asset);
  assert.equal(api.verifiedAccessoryPalette(asset,records,m.hash+1,texture),null,asset);
  for(const type of [4,9])assert.equal(api.verifiedAccessoryPalette(asset,[...records,{type}],m.hash,texture),null,asset);
  for(const range of [{min:[0,0,0,0],max:[1,1,1,1],mask:[0,0,0,1]},{min:[0,0,0,1],max:[1,1,0,1],mask:[0,0,1,1]},{min:[NaN,0,0,1],max:[1,1,1,1],mask:[0,0,0,1]}])assert.equal(api.verifiedAccessoryPalette(asset,records,m.hash,{...texture,range}),null,asset);
 }
});
test('every audited hat bundle rejects missing, duplicated or additional artwork',()=>{
 assert.equal(Object.keys(mappings.hats).length,32);
 for(const [name,names] of Object.entries(mappings.hats)){
  const dependency={name,kind:1};const textures=names.map(name=>({name}));
  assert.deepEqual(Array.from(api.expandVerifiedHatArtwork(dependency,[...textures].reverse()),x=>x.textureName),names);
  for(const invalid of [textures.slice(1),[textures[0],textures[0]],textures.concat({name:'extra_color.dds'})])assert.equal(api.expandVerifiedHatArtwork(dependency,invalid)[0],dependency,name);
 }
});
test('source-bound hat slots preserve repeated images and reject stale records or resources',()=>{
 assert.equal(Object.keys(mappings.boundHats).length,866);
 let repeated=0;
 for(const [asset,m] of Object.entries(mappings.boundHats)){
  const records=m.records.map(([type,parameter,value])=>({type,parameter,value}));
  const slots=m.slots.filter(Boolean),deps=[...new Set(slots.map(s=>s.name))].map(name=>({name,kind:1}));
  const resources=new Map(deps.map(d=>[d.name,{value:{textures:[...new Set(slots.filter(s=>s.name===d.name).map(s=>s.textureName))].map(name=>({name,...slots.find(s=>s.textureName===name)?.textureSpec}))}}]));
  const result=api.verifiedHatArtwork(asset,records,deps,resources);
  assert.deepEqual(Array.from(result,s=>s?.textureName||null),m.slots.map(s=>s?.textureName||null));
  if(new Set(slots.map(s=>s.textureName)).size<slots.length)repeated++;
  assert.equal(api.verifiedHatArtwork(asset,records.slice(1),deps,resources),null);
  assert.equal(api.verifiedHatArtwork(asset,[...records,{type:9,parameter:1,value:2}],deps,resources),null);
  assert.equal(api.verifiedHatArtwork(asset,records,deps.slice(1),resources),null);
  const first=slots[0],mutated=new Map(resources);
  mutated.set(first.name,{value:{textures:[{name:first.textureName},{name:first.textureName}]}});
  assert.equal(api.verifiedHatArtwork(asset,records,deps,mutated),null);
  mutated.delete(first.name);assert.equal(api.verifiedHatArtwork(asset,records,deps,mutated),null);
  if(first.textureSpec){
   const damaged=new Map(resources);
   damaged.set(first.name,{value:{textures:resources.get(first.name).value.textures.map(t=>t.name===first.textureName?{...t,format:'UNKNOWN'}:t)}});
   assert.equal(api.verifiedHatArtwork(asset,records,deps,damaged),null,asset);
  }
 }
 assert.ok(repeated>0,'repeated source image should be present in the audit');
 assert.equal(api.verifiedHatArtwork('unknown',[],[],new Map()),null);
});
test('source-bound hat materials require their exact source record set',()=>{
 assert.equal(Object.keys(mappings.boundDiffuse).length,866);
 for(const [asset,m] of Object.entries(mappings.boundDiffuse)){
  const records=m.records.map(([type,parameter,value])=>({type,parameter,value}));
  const tex={name:m.texture,range:m.range,...m.textureSpec};
  assert.equal(Boolean(api.verifiedAccessoryPalette(asset,records,m.hash,tex)),true);
  assert.equal(api.verifiedAccessoryPalette(asset,[...records,{type:4,parameter:0xd965ece7,value:0}],m.hash,tex),null);
  assert.equal(api.verifiedAccessoryPalette(asset,records.slice(1),m.hash,tex),null);
  assert.equal(api.verifiedAccessoryPalette(asset,records,m.hash,{...tex,name:'other.dds'}),null);
 }
});

test('full-range texture exceptions reject missing metadata, changed format, size and range',()=>{
 let plain=0,variableAlpha=0;
 for(const [asset,m] of Object.entries(mappings.boundDiffuse).filter(([,m])=>m.textureSpec)){
  const records=m.records.map(([type,parameter,value])=>({type,parameter,value}));
  const texture={name:m.texture,...m.textureSpec};
  assert.ok(api.verifiedAccessoryPalette(asset,records,m.hash,texture));
  for(const mutation of [{format:'BC7_SRGB'},{width:texture.width+1},{height:texture.height+1},{range:undefined},{range:{min:[0,0,0,1],max:[1,1,1,1],mask:[0,0,0,1]}}]){
   if(JSON.stringify(mutation.range)===JSON.stringify(texture.range)&&Object.hasOwn(mutation,'range'))continue;
   assert.equal(api.verifiedAccessoryPalette(asset,records,m.hash,{...texture,...mutation}),null,asset);
  }
  if(m.range===null)plain++;
  else if(m.range.min[3]!==m.range.max[3])variableAlpha++;
 }
 assert.equal(plain,24);assert.equal(variableAlpha,6);
});

test('WNBA opaque sleeves require exact records, shader, DDS identity and range',()=>{
 assert.equal(Object.keys(mappings.opaqueSleeves).length,12);
 for(const [asset,m] of Object.entries(mappings.opaqueSleeves)){
  const records=m.records.map(([type,parameter,value])=>({type,parameter,value}));
  const texture={name:m.texture,...m.textureSpec};
  const result=api.verifiedAccessoryPalette(asset,records,m.hash,texture);
  assert.equal(result.opaqueColor,true);assert.equal(result.diffuse,true);assert.equal(result.transparent,undefined);
  assert.equal(api.verifiedAccessoryPalette(asset+'-unknown',records,m.hash,texture),null);
  assert.equal(api.verifiedAccessoryPalette(asset,records,m.hash+1,texture),null);
  for(const rs of [records.slice(1),[...records,{type:4,parameter:0,value:0}],records.map(r=>r.type===5?{...r,value:0}:r),records.map(r=>r.type===2?{...r,value:r.value+1}:r)])assert.equal(api.verifiedAccessoryPalette(asset,rs,m.hash,texture),null);
  for(const mutation of [{name:'other.dds'},{format:'BC7_SRGB'},{width:512},{height:512},{range:null},{range:{...texture.range,mask:[0,0,0,1]}}])assert.equal(api.verifiedAccessoryPalette(asset,records,m.hash,{...texture,...mutation}),null);
 }
});

test('catalog RGB materials require exact source identity and decoded texture metadata',()=>{
 assert.equal(Object.keys(mappings.catalogMaterials).length,963);
 for(const [asset,m] of Object.entries(mappings.catalogMaterials)){
  const records=m.records.map(([type,parameter,value])=>({type,parameter,value}));
  const texture={name:m.texture,...m.textureSpec};
  assert.equal(api.verifiedAccessoryPalette(asset,records,m.hash,texture),m.palette,asset);
  assert.equal(m.palette.opaqueColor,true);
  assert.equal(api.verifiedAccessoryPalette(asset+'-unknown',records,m.hash,texture),null);
  assert.equal(api.verifiedAccessoryPalette(asset,records,m.hash+1,texture),null);
  for(const rs of [records.slice(1),[...records,{type:9,parameter:1,value:2}],records.map(r=>r.type===5?{...r,value:0}:r)])assert.equal(api.verifiedAccessoryPalette(asset,rs,m.hash,texture),null,asset);
  for(const mutation of [{name:'other.dds'},{format:'UNKNOWN'},{width:texture.width+1},{height:texture.height+1},{range:undefined}])assert.equal(api.verifiedAccessoryPalette(asset,records,m.hash,{...texture,...mutation}),null,asset);
 }
});

test('catalog logo slots keep source ordering and reject missing or changed artwork',()=>{
 assert.equal(Object.keys(mappings.catalogArtwork).length,336);
 for(const [asset,m] of Object.entries(mappings.catalogArtwork)){
  const records=m.records.map(([type,parameter,value])=>({type,parameter,value}));
  const slots=m.slots.filter(Boolean),deps=[...new Set(slots.map(s=>s.name))].map(name=>({name,kind:1}));
  const resources=new Map(deps.map(d=>[d.name,{value:{textures:[...new Set(slots.filter(s=>s.name===d.name).map(s=>s.textureName))].map(name=>({name,...slots.find(s=>s.textureName===name).textureSpec}))}}]));
  const result=api.verifiedHatArtwork(asset,records,deps,resources);
  assert.deepEqual(Array.from(result,s=>s?.textureName||null),m.slots.map(s=>s?.textureName||null),asset);
  assert.deepEqual(Array.from(result,s=>s?.palette||null),m.slots.map(s=>s?.palette||null),asset);
  assert.equal(api.verifiedHatArtwork(asset,records.slice(1),deps,resources),null,asset);
  assert.equal(api.verifiedHatArtwork(asset,records,deps.slice(1),resources),null,asset);
  const first=slots[0],textures=resources.get(first.name).value.textures;
  for(const mutation of [textures.concat(textures.find(t=>t.name===first.textureName)),textures.filter(t=>t.name!==first.textureName),textures.map(t=>t.name===first.textureName?{...t,width:t.width+1}:t),textures.map(t=>t.name===first.textureName?{...t,range:undefined}:t)]){
   const changed=new Map(resources);changed.set(first.name,{value:{textures:mutation}});
   assert.equal(api.verifiedHatArtwork(asset,records,deps,changed),null,asset);
  }
 }
});
