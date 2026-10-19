'use strict';
(function(root){
 const slots=typeof module==='object'&&module.exports?require('./accessory-slots'):root.AccessorySlots;
 const pick=(items,current,random)=>{const alternatives=items.filter(item=>item!==current),pool=alternatives.length?alternatives:items;return pool[Math.floor(random()*pool.length)];};
 function clothes(selection,library,random=Math.random){
  const catalog=(library.catalog||[]).filter(p=>p.pool==='male');
  const previous=catalog.find(p=>p.id===selection.streetId);
  if(!previous)throw Error('Choose a supported top before randomizing clothes.');
  // Resolve "Style’s original shoes" before changing top, so the actual shoes
  // stay the same, not just the empty dropdown value.
  const shoes=selection.shoe?[selection.shoe]:(previous.shoes||[]);
  const accessories=[...(selection.accessories||(selection.accessory?[selection.accessory]:[]))].slice(0,3);
  const socks=[...new Set(catalog.flatMap(p=>p.socks||[]))];
  const layerFor=p=>p.layerChoices?.includes('none')?'none':'';
  const candidates=[...new Map(catalog.filter(p=>p.separable&&!p.experimental&&(p.socks||[]).length<=1&&
   (shoes.length===1?(p.shoes||[]).length===1:JSON.stringify(p.shoes||[])===JSON.stringify(shoes))&&
   slots.baseCount(p,socks.length?socks[0]:'',layerFor(p))+accessories.length<=7
  ).map(p=>[p.top,p])).values()];
  if(!candidates.length)throw Error('No random clothing combination can preserve these shoes and accessories.');
  const alternatives=candidates.filter(p=>p.top!==previous.top);
  const top=pick(alternatives.length?alternatives:candidates,null,random);
  const bottoms=[...new Set(catalog.filter(p=>p.separable).map(p=>p.bottom))];
  return {...selection,streetId:top.id,bottom:pick(bottoms,selection.bottom,random)||top.bottom,
   sock:pick(socks,selection.sock||previous.socks?.[0],random)||'',
   shoe:shoes.length===1?shoes[0]:(selection.shoe||''),layer:layerFor(top),accessory:'',accessories};
 }
 function accessories(selection,library,random=Math.random){
  const top=(library.catalog||[]).find(p=>p.id===selection.streetId);
  const limit=Math.max(0,Math.min(3,7-slots.baseCount(top,selection.sock,selection.layer)));
  const previous=new Set(selection.accessories||(selection.accessory?[selection.accessory]:[]));
  const groups=new Map();
  for(const item of library.accessories||[]){const id=item.asset,key=typeof id==='string'&&!id.startsWith('female_')&&slots.slot(id);if(!key)continue;if(!groups.has(key))groups.set(key,new Set());groups.get(key).add(id);}
  const keys=[...groups.keys()];
  for(let i=keys.length-1;i>0;i--){const j=Math.floor(random()*(i+1));[keys[i],keys[j]]=[keys[j],keys[i]];}
  const selected=keys.slice(0,limit).map(key=>{const all=[...groups.get(key)],fresh=all.filter(id=>!previous.has(id));return pick(fresh.length?fresh:all,null,random);});
  return {...selection,accessory:'',accessories:slots.normalize(selected,library.accessories||[])||[]};
 }
 const api={clothes,accessories};if(typeof module==='object'&&module.exports)module.exports=api;else root.MannequinRandom=api;
})(typeof window==='object'?window:globalThis);
