'use strict';
// NikeIdStyle model-name field observed in this build; reject changed layouts.
function readStyleName(bytes,id) {
 if(!Buffer.isBuffer(bytes)||bytes.length<272||!/^\d{4}$/.test(id))throw Error('Invalid shoe style');
 // +0x14 is this record's ID. +0x24 can be a related style ID or zero,
 // e.g. 1072 names Foamposite One Weatherman but references base 1013.
 if(bytes.readUInt32LE(20)!==Number(id)||bytes.readUInt32LE(16)!==0xf86ea2b6||!bytes.subarray(0,16).every(x=>x===0))throw Error('Shoe style identity changed');
 const field=bytes.subarray(216,272),end=field.indexOf(0);
 // The name is NUL terminated. Subsequent bytes may be metadata, not padding:
 // styles 2029/7629/8008 have nonzero fields after the short name.
 if(end<3||field.subarray(0,end).some(x=>x>127))throw Error('Shoe name field changed');
 const slug=field.subarray(0,end).toString('ascii');
 if(!/^[a-z][a-z0-9]*(?:_[a-z0-9]+)+$/.test(slug))throw Error('Unrecognized shoe model name');
 return slug;
}
const brands={nike:'Nike',adidas:'Adidas',jordan:'Jordan',reebok:'Reebok',converse:'Converse',underarmour:'Under Armour',lining:'Li-Ning',newbalance:'New Balance',puma:'Puma',anta:'Anta',threesixone:'361°',and1:'AND1',peak:'Peak',qiaodan:'Qiaodan',rigorer:'Rigorer',sevenfourone:'Seven Four One',fila:'Fila',skechers:'Skechers',jcole:'J. Cole'};
const compounds={airjordan:'Air Jordan',airforce:'Air Force',foampositeone:'Foamposite One',airpenny:'Air Penny',airmax:'Air Max',lebronsoldier:'LeBron Soldier',lebron:'LeBron',airmoreuptempo:'Air More Uptempo',airshakendestrukt:'Air Shake Ndestrukt',kobeadexodus:'Kobe AD Exodus',zoomfreak:'Zoom Freak',airzoomgtrun:'Air Zoom GT Run',zoomgtjump:'Zoom GT Jump',zoomgtcut:'Zoom GT Cut',lbjnxxtgen:'LBJ NXXT Gen',jamorant:'Ja Morant',gthustle:'GT Hustle',blackmamba:'Black Mamba',protrope:'Protro PE',airforce1lowtravisscott:'Air Force 1 Low Travis Scott',travisscott:'Travis Scott',cosmicunity:'Cosmic Unity',zoombbnxt:'Zoom BB NXT',crazylightboostprimeknit:'Crazy Light Boost Primeknit',adizerorose:'Adizero Rose',adizeroselect:'Adizero Select',hardenstepback:'Harden Stepback',extendedplay:'Extended Play',hardenvol:'Harden Vol.',allstarbbshift:'All Star BB Shift',allstarbbprototypecx:'All Star BB Prototype CX',allstartrilliantcx:'All Star Trilliant CX',allstarbbevo:'All Star BB Evo',allstarpro:'All Star Pro',chucktaylor:'Chuck Taylor',proleather:'Pro Leather',fastbreak:'Fastbreak',wayofwadeallcity:'Way of Wade All City',wayofwade:'Way of Wade',hesilow:'Hesi Low',twowxy:'TWO WXY',drosesonofchi:'D Rose Son of Chi',donissue:'DON Issue',traeyoung:'Trae Young',fearofgodbasketball:'Fear of God Basketball',fearofgod:'Fear of God',topten:'Top Ten',aebelievethat:'AE Believe That',crazyenergy:'Crazy Energy',exhibitselect:'Exhibit Select',crazyiiinfinity:'Crazy IIInfinity',blacktopbattleground:'Blacktop Battleground',shaqattaq:'Shaq Attaq',pumpdtime:'Pump D-Time',prolegacy:'Pro Legacy',omnilite:'Omni Lite',twilightzonepump:'Twilight Zone Pump',angelreese:'Angel Reese',zpumprise:'ZPump Rise',raftermid:'Rafter Mid'};
const orderedCompounds=Object.entries(compounds).sort(([a],[b])=>b.length-a.length);
function humanizeModel(slug){
 const [brand,...parts]=slug.split('_');
 const words=parts.map(part=>{
  for(const [key,value]of orderedCompounds)if(part.includes(key)){part=part.replaceAll(key,' '+value+' ');}
  return part.replace(/([a-zA-Z])(\d)/g,'$1 $2').replace(/(\d)([a-zA-Z])/g,'$1 $2');
 }).join(' ').replace(/\s+/g,' ').trim().split(' ').map(w=>({kd:'KD',pg:'PG',ad:'AD',em:'EM',bb:'BB',cx:'CX',kt:'KT',ae:'AE',mb:'MB',tmac:'T-Mac',lbj:'LBJ',gt:'GT'}[w.toLowerCase()]||w.charAt(0).toUpperCase()+w.slice(1))).join(' ').replace(/\b2 K (\d)/g,'2K$1');
 return ((brands[brand]||brand.charAt(0).toUpperCase()+brand.slice(1))+' '+words).replace(/^Jordan Air Jordan\b/,'Air Jordan').replace(/\b2 K\b/g,'2K');
}
function buildLabels(catalog,models){
 const assets=new Set([...catalog.presets.filter(p=>p.sourceGroup.pool==='male').flatMap(p=>p.shoes||[]),...(catalog.additionalShoes||[]).map(s=>s.asset)]);
 const labels={};for(const asset of assets){const m=/^shoe_(\d{4})_(.+)$/.exec(asset);if(m&&models[m[1]])labels[asset]=humanizeModel(models[m[1]].slug)+' · '+m[2].replaceAll('_',' ');}
 return labels;
}
module.exports={readStyleName,humanizeModel,buildLabels};
