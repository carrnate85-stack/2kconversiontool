(function(root){
 const clean=asset=>String(asset||'').replace(/^female_/,'');
 const label=asset=>clean(asset).replace(/^acc_sock_/,'').replace(/^acc_/,'').replaceAll('_',' ');
 const shoeBrands=['Nike','Jordan','Adidas','Under Armour','Puma','New Balance','Converse','Reebok','Anta','Li-Ning','AND1','Fila','361°','Skechers','Coach','Rigorer','Seven Four One','Peak','Qiaodan','J. Cole'];
 function category(asset,kind,displayLabel=''){
  const name=clean(asset).toLowerCase();
  if(kind==='accessories'){
   if(/^hat_(hockey|knight)_/.test(name))return 'Costume masks';
   if(name.startsWith('facemask_'))return 'Protective masks';
   if(name.startsWith('mask_gaiter_'))return 'Gaiters';
   if(name.startsWith('mask_masquerade_'))return 'Costume masks';
   if(name.startsWith('mask_'))return 'Cloth masks';
   if(/^hat_/.test(name))return 'Hats';if(/^headphone_/.test(name))return 'Headphones';if(/^piercing_/.test(name))return 'Piercings';if(/^necklace_/.test(name))return 'Necklaces';if(/^(scarf_|tie_|bowtie_|towel_neck_)/.test(name))return 'Neckwear';if(/^(bracelet_|watch_)/.test(name))return 'Wristwear';if(/^glove_/.test(name))return 'Gloves';if(/^(ring_|nailpolish_|acc_fingertape_)/.test(name))return 'Hands';if(/^(bag_|belt_|cape_|innertube_)/.test(name))return 'Body';if(/^floaties_/.test(name))return 'Arms';
   if(/^acc_(armsleeve|elbowband|elbowpad|elbowsleeve|wristband|wristtape|rubber|kinesiotape)_/.test(name))return 'Arms';
   if(/^acc_(kneebrace|kneepad|kneesleeve|kneestrap|legsleeve|anklebrace)_/.test(name))return 'Legs';
   if(name.startsWith('cyborg_arm_'))return 'Arms';if(name.startsWith('cyborg_leg_'))return 'Legs';
   if(name.startsWith('headband_'))return 'Headbands';if(name.startsWith('goggles_'))return 'Goggles';
   if(/^(glasses_|sunglasses_)/.test(name))return 'Glasses';if(name.startsWith('backpack_'))return 'Backpacks';return 'None';
  }
  if(kind==='shoes'){
   const text=String(displayLabel).toLowerCase();
   if(text.startsWith('air jordan '))return 'Jordan';
   return shoeBrands.find(brand=>text.startsWith(brand.toLowerCase()+' '))||'Other';
  }
  if(kind==='bottoms'){
   if(/^jogger_/.test(name))return 'Joggers';
   if(/^jeans_/.test(name))return 'Jeans';
   if(/^track_/.test(name))return 'Track';
   if(/^cargo_/.test(name))return 'Cargo';
   if(/^sweats_/.test(name))return 'Sweats';
   if(/^(?:basketball_|canvas_)?shorts_|^uniform_shorts$/.test(name))return 'Shorts';
   return 'Pants / Other';
  }
  if(kind==='tops'){
   if(/^(cyborg_arm_|cyborg_leg_|backpack_|headband_|glasses_|sunglasses_|goggles_)/.test(name))return 'Accessory tests';
   if(/^jersey_/.test(name))return 'Jersey';
   if(/^tshirt_ls_/.test(name))return 'Long sleeves';
   if(/^tshirt_/.test(name))return 'T-shirts';
   if(/^hoodie_/.test(name))return 'Hoodies';
   if(/^sweatshirt_/.test(name))return 'Sweatshirts';
   if(/^tanktop_/.test(name))return 'Tank tops';
   if(/^(jacket_|windbreaker_|blazer_)/.test(name))return 'Jackets';
   if(/^(shirt_|polo_|bdshirt_)/.test(name))return 'Shirts';
   return 'Other';
  }
  return 'All';
 }
 function categories(items,kind){
  if(kind==='accessories'){const present=new Set(items.map(i=>category(i.asset,kind)));return ['All',...['Arms','Legs','Headbands','Glasses','Goggles','Protective masks','Cloth masks','Gaiters','Costume masks','Hats','Headphones','Piercings','Necklaces','Neckwear','Wristwear','Gloves','Hands','Body','Backpacks'].filter(v=>present.has(v))];}
  if(!['tops','bottoms','shoes'].includes(kind))return [];
  const order=kind==='shoes'?[...shoeBrands,'Other']:kind==='bottoms'?['Joggers','Jeans','Shorts','Track','Cargo','Sweats','Pants / Other']:['T-shirts','Long sleeves','Hoodies','Sweatshirts','Tank tops','Jersey','Jackets','Shirts','Accessory tests','Other'];
  const present=new Set(items.map(i=>category(i.asset,kind,i.label)));return ['All',...order.filter(c=>present.has(c))];
 }
 const api={label,category,categories};if(typeof module==='object'&&module.exports)module.exports=api;else root.ClothingCatalogUI=api;
})(typeof window==='object'?window:globalThis);
