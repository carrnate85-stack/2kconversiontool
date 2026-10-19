'use strict';
const {readZipArchive,buildZipArchive}=require('./jukebox-manager');
// Localization keys, not enum hashes or synthetic camera-slot identities.
const LABELS=Object.freeze([
 {id:51,key:'070A02F4676F7A26',source:'Full Court',name:'Broadcast Low 2K18',donor:'E2D6B673633FB485',donorSource:'Pregame Shootaround'},
 {id:53,key:'724E93A6FB8B2389',source:'Half Court',name:'Press 2K17',donor:'B09970FB8F8E8E2A',donorSource:'Coach View'},
 {id:54,key:'B800A4C12E23B6BD',source:'DO NOT USE THIS CAM',name:'Press 2K27'},
 {id:52,key:'81C8B0C640438EFB',source:'Full Court Corner',name:'Beluba 2K17'},
 {id:57,key:'2AFCCD8CA86B576C',source:'Rail High',name:'Beluba 2K27',donor:'C0E7B391A0EDA7AD',donorSource:'Minigame Downtown'},
 {id:59,key:'3B94D5AAE1F235DE',source:'Side',name:'Side',locked:true},
 {id:55,key:'7DE7AF563ED79DC4',source:'Nosebleeds',name:'Nosebleeds',locked:true},
]);
function resolve(text,key){
 if(text.length<64)throw new Error('Truncated localization data.');
 const pool=Math.ceil(Number(text.readBigUInt64LE(24))/64)*64;
 if(!Number.isSafeInteger(pool)||pool<64||pool>=text.length)throw new Error('Unsupported localization string pool.');
 const needle=Buffer.alloc(8);needle.writeBigUInt64LE(BigInt('0x'+key));const found=[];
 for(let at=text.indexOf(needle,64);at>=64&&at<pool;at=text.indexOf(needle,at+1))if(at%8===0&&at%64<40)found.push(at);
 if(found.length!==1)throw new Error(`Camera localization key ${key} is not unique (${found.length}).`);
 const at=found[0],field=Math.floor(at/64)*64+40+(at%64/8)*4;
 const start=pool+text.readUInt32LE(field)*2;let end=start;
 if(start<pool||start+2>text.length)throw new Error('Camera name points outside localization data.');
 while(end+2<=text.length&&text.readUInt16LE(end)!==0)end+=2;
 if(end+2>text.length)throw new Error('Camera label is unterminated.');
 return {pool,field,start,end,value:text.subarray(start,end).toString('utf16le')};
}
function cameraNames(overrides={}){
 const names={};
 for(const label of LABELS){
   const value=overrides[label.id]===undefined?label.name:overrides[label.id];
   const max=(label.donorSource||label.source).length;
   if(typeof value!=='string'||!value.trim()||value.trim().length>max||/[^\x20-\x7e]/.test(value))throw new Error(`Camera ${label.id} name must be 1–${max} printable English characters.`);
   if(label.locked&&value.trim()!==label.name)throw new Error(`${label.name} is locked to its stock name.`);
   names[label.id]=value.trim();
 }
 return names;
}
function verifyCameraNames(archive,overrides={}){
 const names=cameraNames(overrides);
 const text=readZipArchive(archive).get('TEXT.VCLOCALIZEDATA');if(!text)throw new Error('Missing camera localization member.');
 for(const label of LABELS)if(resolve(text,label.key).value!==names[label.id])throw new Error(`Camera ${label.id} does not resolve to ${names[label.id]}.`);
 return true;
}
function buildCameraNames(archive,overrides={}){
 const names=cameraNames(overrides);
 const entries=readZipArchive(archive),original=entries.get('TEXT.VCLOCALIZEDATA');
 if(!original)throw new Error('English archive has no localization member.');const text=Buffer.from(original);
 for(const label of LABELS){
   const lookup=resolve(text,label.key);if(lookup.value!==label.source)throw new Error(`Camera ${label.id} source changed (${lookup.value}); name patch stopped safely.`);
   if(names[label.id]===label.source)continue;
   const target=label.donor?resolve(text,label.donor):lookup;
   if(label.donor&&target.value!==label.donorSource)throw new Error('Camera name donor changed.');
   // Do not overwrite suffix-shared strings, e.g. Eight Seconds Half Court.
   if(target.start>target.pool&&text.readUInt16LE(target.start-2)!==0)throw new Error('Camera name storage is shared with another string.');
   const value=Buffer.from(names[label.id],'utf16le');if(value.length>target.end-target.start)throw new Error('Camera label does not fit its verified storage.');
   text.fill(0,target.start,target.end);value.copy(text,target.start);
   if(label.donor)text.writeUInt32LE((target.start-target.pool)/2,lookup.field);
 }
 entries.set('TEXT.VCLOCALIZEDATA',text);
 const built=buildZipArchive([...entries].map(([name,data])=>({name,data})));
 verifyCameraNames(built,names);return built;
}
function namesIni(){return '[LocalizationKeys]\r\n'+LABELS.map(l=>`${l.id}=${l.key}`).join('\r\n')+'\r\n';}
module.exports={LABELS,resolve,buildCameraNames,verifyCameraNames,namesIni,cameraNames};
