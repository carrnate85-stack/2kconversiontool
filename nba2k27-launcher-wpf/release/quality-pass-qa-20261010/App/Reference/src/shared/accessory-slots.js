(function(root){
 const slots=[['headwear','Hat'],['headphones','Headphones'],['headband','Headband'],['eyewear','Eyewear'],['faceMask','Face mask'],['leftPiercing','Left piercing'],['rightPiercing','Right piercing'],['neckwear','Neckwear'],['bodywear','Shoulder bag'],['waist','Waist'],['backpack','Backpack'],['leftArm','Left arm'],['rightArm','Right arm'],['leftWrist','Left wrist'],['rightWrist','Right wrist'],['leftHand','Left hand'],['rightHand','Right hand'],['leftLeg','Left leg'],['rightLeg','Right leg']];
 function slot(asset){
  asset=asset.replace(/^female_/,'');
  if(/^(facemask_|mask_|hat_hockey_|hat_knight_)/.test(asset))return 'faceMask';if(/^headband_/.test(asset))return 'headband';if(/^(glasses_|sunglasses_|goggles_)/.test(asset))return 'eyewear';
  if(/^hat_/.test(asset))return 'headwear';if(/^headphone_/.test(asset))return 'headphones';
  if(/^(necklace_|scarf_|tie_|bowtie_|towel_neck_)/.test(asset))return 'neckwear';
  if(/^bag_shoulder_/.test(asset))return 'bodywear';if(/^(belt_|innertube_)/.test(asset))return 'waist';
  if(/^(backpack_|jetpack_|wings_|cape_)/.test(asset))return 'backpack';
  const side=/_(left|right)$/.exec(asset)?.[1];if(!side&&/^nailpolish_(left|right)_/.test(asset))return /^nailpolish_left_/.test(asset)?'leftHand':'rightHand';if(!side)return null;
  if(/^piercing_/.test(asset))return side+'Piercing';if(/^(bracelet_|watch_)/.test(asset))return side+'Wrist';if(/^(glove_|ring_|acc_fingertape_)/.test(asset))return side+'Hand';
  if(/^(cyborg_arm_|acc_(armsleeve|elbowband|elbowpad|elbowsleeve|wristband|wristtape|rubber|kinesiotape)_|arm_band)/.test(asset))return side+'Arm';
  if(/^floaties_arm_/.test(asset))return side+'Arm';
  if(/^(cyborg_leg_|acc_(kneebrace|kneepad|kneesleeve|kneestrap|legsleeve|anklebrace)_)/.test(asset))return side+'Leg';
  return null;
 }
 function normalize(values,allowed){
  if(!Array.isArray(values)||values.length>7)return null;
  const selected=new Map(),names=new Set(allowed.map(a=>typeof a==='string'?a:a.asset));
  for(const asset of values){if(typeof asset!=='string'||!names.has(asset)||!slot(asset)||selected.has(slot(asset)))return null;selected.set(slot(asset),asset);}
  return slots.map(([key])=>selected.get(key)).filter(Boolean);
 }
 function label(asset){
  asset=asset.replace(/^female_/,'');
  const words={blk:'Black',wht:'White',slv:'Silver',gld:'Gold',blu:'Blue',grn:'Green',org:'Orange',gry:'Grey',crm:'Cream',pnk:'Pink',pur:'Purple',blkblu:'Black/Blue',blkgrn:'Black/Green',blkorg:'Black/Orange',blkyel:'Black/Yellow',solid:'',provinggrounds:'Proving Grounds',antisocialclub:'Anti Social Club','2kevents':'2K Events','2kseason1':'Season 1',wemby:'Wemby','3d':'3D'};
  return asset.replace(/^hat_hockey_/,'Hockey_mask_').replace(/^hat_knight_/,'Knight_').replace(/^mask_gaiter_/,'Gaiter_').replace(/^mask_masquerade_/,'Masquerade_').replace(/^mask_/,'Cloth_mask_').replace(/^acc_/,'').replace(/^cyborg_(arm|leg)_/,'cyborg_').replace(/^(headband_|backpack_|facemask_)/,'').replace(/_(left|right)$/,'').split('_').map(w=>Object.hasOwn(words,w)?words[w]:w.charAt(0).toUpperCase()+w.slice(1)).filter(Boolean).join(' ');
 }
 function baseCount(p,sock,layer){
  if(!p)return 7;let count=p.extras+2;
  if(sock&&!p.socks?.length)count++;
  if(layer==='none')count-=p.layers?.length||0;else if(layer&&!p.layers?.length)count++;
  return count;
 }
 const api={slots,slot,normalize,baseCount,label};if(typeof module==='object'&&module.exports)module.exports=api;else root.AccessorySlots=api;
})(typeof window==='object'?window:globalThis);
