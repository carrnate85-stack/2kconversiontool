'use strict';

// Catalog separation is not a claim that every new mix is render-compatible.
// Keep every original item and its order; co-occurring layers are preserved
// conservatively until removal has been tested in game.
function category(asset) {
  const name = asset.replace(/^female_/, '').toLowerCase();
  if (/^shoes?_/.test(name)) return 'shoes';
  if (/^acc_/.test(name)||require('../shared/accessory-slots').slot(name)) return 'accessories';
  if (/^(cyborg_arm_|cyborg_leg_|backpack_|facemask_|mask_|hat_hockey_|hat_knight_|headband_|glasses_|sunglasses_|goggles_)/.test(name)) return 'accessories';
  if (/^(hoodie_|jacket_|tshirt_|shirt_|tanktop_|jersey_|sweatshirt_|windbreaker_|croptop_|polo_|vest_|sweater_|coat_|blazer_|bdshirt_|cardigan_|flannel_)/.test(name)) return 'top';
  if (name === 'uniform_shorts' || /^(warmup_|jogger_|jeans_|track_|pants_|shorts_|canvas_shorts_|basketball_shorts_|sweats_|cargo_|chino_|business_)/.test(name)) return 'bottom';
  return 'unknown';
}
function splitPreset(preset) {
  const original = preset.items.map(item => typeof item === 'string' ? item : item.asset);
  if (!original.every(item => typeof item === 'string' && item.length > 0)) throw new Error('Invalid clothing catalog item.');
  const parts = { top: [], bottom: [], shoes: [], accessories: [], unknown: [] };
  original.forEach(asset => parts[category(asset)].push(asset));
  return { id: preset.id, sourceGroup: { index: preset.groupIndex ?? null, hash: preset.groupHash ?? null, pool: preset.genderPool ?? null },
    original, top: parts.top[0] || null, bottom: parts.bottom[0] || null,
    layers: parts.top.slice(1), shoes: parts.shoes, accessories: parts.accessories, unknown: parts.unknown,
    layerEvidence: parts.top.length > 1 ? 'observed-together-preserve' : 'none',
    separable: parts.top.length >= 1 && parts.bottom.length === 1 && parts.unknown.length === 0,
    liveMixReady: original.length === 2 && parts.top.length === 1 && parts.bottom.length === 1 };
}
function buildComponentCatalog(presets) {
  const separated = presets.map(splitPreset);
  const pieces = new Map();
  for (const preset of separated) for (const asset of preset.original) {
    if (!pieces.has(asset)) pieces.set(asset, { asset, category: category(asset), presets: [] });
    const ids = pieces.get(asset).presets; if (!ids.includes(preset.id)) ids.push(preset.id);
  }
  return { schema: 'nba2k27-clothing-components-v1', pieces: [...pieces.values()], presets: separated };
}
function composeLoadedPair(outfits, top, bottom) {
  if (typeof top !== 'string' || typeof bottom !== 'string') return undefined;
  // Native layer repeats these checks against the live, verified group.
  const simple = outfits.filter(o => o.items?.length === 2);
  const exact = simple.find(o => o.items[0] === top && o.items[1] === bottom);
  if (exact) return exact;
  if (!simple.some(o => o.items[0] === top) || !simple.some(o => o.items[1] === bottom)) return undefined;
  return { id: 'composed', pointer: 0, items: [top, bottom], composed: true };
}
const layerIndexes=new WeakMap();
function layerChoices(catalog,preset) {
  if(!preset)return [];
  if(!layerIndexes.has(catalog)) {
    const index=new Map();
    for(const p of catalog.presets)if(p.layers?.length<=1){const key=p.sourceGroup.pool+'\0'+p.top;
      if(!index.has(key))index.set(key,new Set());index.get(key).add(p.layers[0]||'none');}
    layerIndexes.set(catalog,index);
  }
  return [...(layerIndexes.get(catalog).get(preset.sourceGroup.pool+'\0'+preset.top)||[])];
}
function layerItems(catalog,preset,layer) {
  const original=[...preset.original],current=preset.layers||[];
  if(!layer)return original;
  if(typeof layer!=='string'||current.length>1||!layerChoices(catalog,preset).includes(layer))return undefined;
  if(layer===(current[0]||'none'))return original;
  if(current.length) {
    const index=original.indexOf(current[0]);if(index<0)return undefined;
    if(layer==='none')original.splice(index,1);else original[index]=layer;
  }else {
    const donor=catalog.presets.find(p=>p.top===preset.top&&p.sourceGroup.pool===preset.sourceGroup.pool&&p.layers?.[0]===layer);
    // Keep the observed layer placement relative to the main top or footwear.
    // Existing non-layer items retain their ordering.
    const donorPosition=donor.original.indexOf(layer);
    const afterTop=donorPosition===donor.original.indexOf(donor.top)+1;
    if(!afterTop&&donorPosition!==donor.original.length-1)return undefined;
    original.splice(afterTop?original.indexOf(preset.top)+1:original.length,0,layer);
  }
  return original;
}
const accessorySlots=require('../shared/accessory-slots');
const accessoryIndexes=new WeakMap();
function accessoryChoices(catalog) {
 if(!accessoryIndexes.has(catalog))accessoryIndexes.set(catalog,[...(catalog.accessoryTest?.added||[]),...(catalog.additionalAccessories||[])].map(a=>({asset:a.asset,label:accessorySlots.label(a.asset),experimental:Boolean(a.experimental),family:a.family,slot:accessorySlots.slot(a.asset)})));
 return accessoryIndexes.get(catalog);
}
function normalizeAccessories(catalog,value) {
  const legacy=catalog.accessoryTest?.added.find(p=>p.id===value.streetId);
  return legacy?{...value,streetId:'g1-o0',accessory:value.accessory===undefined?legacy.asset:value.accessory}:value;
}
function composeStreetwear(catalog, id, bottom, shoe, sock,layer,accessory,accessories) {
  const preset = catalog.presets.find(p => p.id === id);
  if (!preset || typeof bottom !== 'string') return undefined;
  const items = layerItems(catalog,preset,layer);if(!items)return undefined;
  if(shoe==='roster'){
    if(!Array.isArray(preset.shoes)||preset.shoes.length>1)return undefined;
    if(preset.shoes.length===1){const position=items.indexOf(preset.shoes[0]);if(position<0)return undefined;items.splice(position,1);}
  }else if (shoe != null && shoe !== '') {
    if (typeof shoe !== 'string' || preset.shoes?.length !== 1
      || !(catalog.presets.some(p => p.sourceGroup.pool === preset.sourceGroup.pool && p.shoes?.includes(shoe))
        || (catalog.additionalShoes||[]).some(s=>s.asset===shoe&&s.pools.includes(preset.sourceGroup.pool)))) return undefined;
    const position=items.indexOf(preset.shoes[0]);if(position<0)return undefined;items[position]=shoe;
  }
  if (sock != null && sock !== '') {
    const socks=(preset.accessories||[]).filter(s=>/^(female_)?acc_sock_/.test(s));
    if(typeof sock!=='string'||socks.length>1||!catalog.presets.some(p=>p.sourceGroup.pool===preset.sourceGroup.pool
      && p.accessories?.includes(sock)&&/^(female_)?acc_sock_/.test(sock)))return undefined;
    if(socks.length===1){const position=items.indexOf(socks[0]);if(position<0)return undefined;items[position]=sock;}
    else {if(items.length>=7)return undefined;items.push(sock);}
  }
  if (bottom !== preset.bottom) {
    if (!preset.separable || !catalog.presets.some(p => p.separable && p.sourceGroup.pool === preset.sourceGroup.pool && p.bottom === bottom)) return undefined;
    const index = items.indexOf(preset.bottom); if (index < 0) return undefined; items[index] = bottom;
  }
  if(accessories!==undefined){
    const values=accessorySlots.normalize(accessories,accessoryChoices(catalog));
    if(!values||!['male','female'].includes(preset.sourceGroup.pool)||values.some(a=>(a.startsWith('female_')?'female':'male')!==preset.sourceGroup.pool))return undefined;
    const allowed=new Set(accessoryChoices(catalog).map(a=>a.asset));
    for(let i=items.length-1;i>=0;i--)if(allowed.has(items[i]))items.splice(i,1);
    items.push(...values);accessories=values;
  }else if(accessory!==undefined){
    const allowed=new Set(accessoryChoices(catalog).map(a=>a.asset));
    if(typeof accessory!=='string'||!['male','female'].includes(preset.sourceGroup.pool)||(accessory!==''&&(!allowed.has(accessory)||(accessory.startsWith('female_')?'female':'male')!==preset.sourceGroup.pool)))return undefined;
    for(let i=items.length-1;i>=0;i--)if(allowed.has(items[i]))items.splice(i,1);
    if(accessory)items.push(accessory);
  }
  if(items.length<2||items.length>7)return undefined;
  return { id: `street:${id}`, pointer: 0, items, composed: true, streetId: id, bottom, ...(shoe ? {shoe} : {}),...(sock?{sock}:{}),...(layer?{layer}:{}),...(accessories!==undefined?{accessories}:accessory!==undefined?{accessory}:{}) };
}
const randomPools=new WeakMap();
function randomStreetwear(catalog,pool,random=Math.random,accessories,rosterShoes=false) {
  if(!['male','female'].includes(pool))return undefined;
  // Every selectable garment in the player's own pool participates.
  // Legacy accessory demonstration presets are not independent clothing choices.
  if(!randomPools.has(catalog))randomPools.set(catalog,new Map());
  const cache=randomPools.get(catalog),key=pool+':'+(accessories?.length||0)+':'+Boolean(rosterShoes);
  if(!cache.has(key)){
   const presets=catalog.presets.filter(p=>p.sourceGroup.pool===pool&&p.separable&&!p.manualOnly&&!p.id.startsWith('accessory-test-'));
   const compatible=presets.filter(p=>(rosterShoes?Array.isArray(p.shoes)&&p.shoes.length<=1:p.shoes?.length===1)&&(p.accessories||[]).filter(s=>/^(female_)?acc_sock_/.test(s)).length<=1
     && (p.original.length-(rosterShoes?p.shoes.length:0)+((p.accessories||[]).some(s=>/^(female_)?acc_sock_/.test(s))?0:1)+(accessories?.length||0)<=7));
   const tops=[...new Map(compatible.map(p=>[p.top,p])).values()];
   const bottoms=[...new Set(presets.map(p=>p.bottom))];
   const socks=[...new Set(presets.flatMap(p=>p.accessories||[]).filter(s=>/^(female_)?acc_sock_/.test(s)))];
   const shoes=[...new Set([...presets.flatMap(p=>p.shoes||[]),...(catalog.additionalShoes||[]).filter(s=>s.pools.includes(pool)).map(s=>s.asset)])];
   cache.set(key,{tops,bottoms,socks,shoes});
  }
  const {tops,bottoms,socks,shoes}=cache.get(key);
  if([tops,bottoms,socks,shoes].some(a=>!a.length))return undefined;
  const pick=a=>{const value=random();if(!Number.isFinite(value)||value<0||value>=1)throw Error('Invalid random source');return a[Math.floor(value*a.length)];};
  const top=pick(tops),bottom=pick(bottoms),sock=pick(socks),shoe=rosterShoes?'roster':pick(shoes);
  return composeStreetwear(catalog,top.id,bottom,shoe,sock,undefined,undefined,accessories);
}
module.exports = { category, splitPreset, buildComponentCatalog, composeLoadedPair, composeStreetwear,layerChoices,layerItems,randomStreetwear,accessoryChoices,normalizeAccessories };
