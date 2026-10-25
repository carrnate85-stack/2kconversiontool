// QA118: explicit mappings verified against source records and decoded DDS.
// Unknown bundles/materials continue through the original conservative checks.
function verifiedTextureSpec(texture,spec){
 if(!texture||!spec)return false;
 if(texture.format!==spec.format||texture.width!==spec.width||texture.height!==spec.height)return false;
 if(spec.range===null)return texture.range===null;
 return ['min','max','mask'].every(k=>Array.isArray(texture.range?.[k])&&texture.range[k].length===4&&texture.range[k].every((v,i)=>Number.isFinite(v)&&v===spec.range[k][i]));
}
function verifiedHatArtwork(asset,records,dependencies,resources){
 const expected=accessoryMaterialMap.boundHats?.[asset]||accessoryMaterialMap.catalogArtwork?.[asset];
 if(!expected||JSON.stringify(records?.map(r=>[r.type,r.parameter,r.value]))!==JSON.stringify(expected.records))return null;
 for(const slot of expected.slots){
  if(!slot)continue;
  if(!dependencies.some(d=>d.kind===1&&d.name===slot.name))return null;
  const textures=resources.get(slot.name)?.value?.textures;
  if(!textures||textures.filter(t=>t.name===slot.textureName).length!==1)return null;
  if(slot.textureSpec&&!verifiedTextureSpec(textures.find(t=>t.name===slot.textureName),slot.textureSpec))return null;
 }
 return expected.slots.map(s=>s?{kind:1,name:s.name,textureName:s.textureName,palette:s.palette||null,textureSpec:s.textureSpec||null}:null);
}
function expandVerifiedHatArtwork(dependency,textures){
 const expected=accessoryMaterialMap.hats[dependency.name];
 if(!Array.isArray(expected))return [dependency];
 const colors=textures.filter(t=>!/(?:normal|rough|metal|mask|rdmo|coneangle|_n[._])/i.test(t.name));
 if(colors.length!==expected.length)return [dependency];
 const ordered=expected.map(name=>colors.filter(t=>t.name===name));
 if(ordered.some(matches=>matches.length!==1))return [dependency];
 return ordered.map(([texture])=>({...dependency,textureName:texture.name}));
}
function verifiedAccessoryPalette(asset,records,hash,texture){
 const expected=accessoryMaterialMap.diffuse[asset]||accessoryMaterialMap.boundDiffuse?.[asset]||accessoryMaterialMap.opaqueSleeves?.[asset]||accessoryMaterialMap.catalogMaterials?.[asset];
 if(!expected||hash!==expected.hash||texture?.name!==expected.texture)return null;
 if(!records?.some(r=>r.type===5&&r.parameter===hash&&r.value===expected.shader))return null;
 if(expected.records){
  if(JSON.stringify(records.map(r=>[r.type,r.parameter,r.value]))!==JSON.stringify(expected.records))return null;
 }else if(records.some(r=>r.type===4||r.type===9))return null;
 if(expected.textureSpec){
  if(!verifiedTextureSpec(texture,expected.textureSpec))return null;
  if(expected.range===null)return expected.palette;
 }
 const range=texture.range;
 if(!range||!['min','max','mask'].every(k=>Array.isArray(range[k])&&range[k].length===4&&range[k].every(Number.isFinite)))return null;
 if(!['min','max','mask'].every(k=>range[k].every((v,i)=>v===expected.range[k][i])))return null;
 // Source-bound palettes retain their verified shader-specific alpha policy.
 return expected.palette||{diffuse:true,schema:'verified-accessory-source-diffuse-v2'};
}
