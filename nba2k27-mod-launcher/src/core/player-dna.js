'use strict';
const crypto=require('node:crypto'),schema=require('../data/player-field-schema.json'),{maximum}=require('./player-field-edits');
const schemaId=crypto.createHash('sha256').update(JSON.stringify(schema)).digest('hex');
// The two appended hand fields do not alter any previously exported field.
const oldSockChoices=['White','Black','Uniform 1','Uniform 2','Uniform 3','Uniform Armsleeve','Uniform Legsleeve','Uniform Primary Color','Navy','Blue','Green','Yellow','Red','Gray','Purple','Orange','Tan'];
const priorSchemas=[schema,schema.filter(f=>f.group!=='playtypes')].flatMap(fields=>[fields,fields.map(f=>/^SockItems(?:Home|Away)Color$/.test(f.key)?{...f,choices:oldSockChoices}:f)]);
const compatibleSchemaIds=new Set(priorSchemas.flatMap(fields=>[fields,fields.filter(f=>!['BestHand','BestDunkHand'].includes(f.key))]).map(fields=>crypto.createHash('sha256').update(JSON.stringify(fields)).digest('hex')));
for(const id of require('../data/player-schema-legacy-ids.json'))compatibleSchemaIds.add(id);
const infoRules={firstName:'name',lastName:'name',cyberfaceId:[0,65535],positionId:[0,5],secondaryPositionId:[0,5],jerseyNumberRaw:[0,100],weightLbs:[70,500,true],heightCm:[120,260,true],wingspanCm:[120,320,true],potential:[25,110],fullNameId:[-1,32767]};
const groups=['info','appearance','audio','attributes','development','tendencies','playtypes','personality','badges','hotzones','signature','equipment','contract','stats','seasons'];
const bio=require('../renderer/player-bio');
infoRules.birthDate='date';infoRules.yearsPro=[0,31];
infoRules.fromName='origin';
const infoGroup=key=>['cyberfaceId','weightLbs','heightCm','wingspanCm'].includes(key)?'appearance':key==='fullNameId'?'audio':key==='potential'?'attributes':'info';
const object=x=>!!x&&typeof x==='object'&&!Array.isArray(x);
function validate(value){
 if(!object(value)||value.format!=='2kplayer'||![1,2].includes(value.version)||value.game!=='NBA2K27')throw Error('This is not a supported .2kplayer file.');
 if(value.version===1&&Object.hasOwn(value,'seasonHistory'))throw Error('Season history requires Player DNA version 2.');
 if(value.version===2&&!Object.hasOwn(value,'seasonHistory'))throw Error('Player DNA version 2 requires season history.');
 if(!compatibleSchemaIds.has(value.schemaId))throw Error('This player DNA uses a different field schema. Export it again with this launcher version.');
 if(!object(value.info)||!object(value.fields)||typeof value.name!=='string'||value.name.length>80||/[\u0000-\u001F]/.test(value.name))throw Error('Invalid player DNA data.');
 const info={},fields={};
 for(const [key,v] of Object.entries(value.info)){
  if(!Object.hasOwn(infoRules,key))throw Error('Unsupported player information: '+key);const rule=infoRules[key];
  if(rule==='date')bio.dateParts(v);
  else if(rule==='name'||rule==='origin'){if(typeof v!=='string'||v.length>(rule==='origin'?47:19)||/[\u0000-\u001F]/.test(v)||(rule==='origin'&&!v.trim()))throw Error('Invalid player name or origin.');}
  else if(typeof v!=='number'||!Number.isFinite(v)||(!rule[2]&&!Number.isInteger(v))||v<rule[0]||v>rule[1])throw Error('Invalid player DNA value: '+key);
  info[key]=v;
 }
 for(const [key,v] of Object.entries(value.fields)){
  const f=schema.find(f=>f.key===key);if(!f||maximum(f)<0||!Number.isInteger(v)||v<0||v>maximum(f))throw Error('Invalid player DNA field: '+key);fields[key]=v;
 }
 // Reconstruct the portable payload: no addresses, team links, files, or IDs
 // identifying the source process can reach the exported document.
 return {format:'2kplayer',version:value.version,game:'NBA2K27',schemaId,name:value.name,info,fields,...(value.version===2?{seasonHistory:require('../renderer/player-season-transfer').validate(value.seasonHistory)}:{})};
}
function parse(text){if(typeof text!=='string'||Buffer.byteLength(text)>256*1024)throw Error('Player DNA must be smaller than 256 KB.');let value;try{value=JSON.parse(text);}catch{throw Error('The .2kplayer file is not valid JSON.');}return validate(value);}
function fromPlayer(p){
 if(p?.source!=='live-roster'||!Array.isArray(p.ratingsRaw)||p.ratingsRaw.length!==schema.length)throw Error('Refresh the live roster before copying a player.');
 const info={},fields={};for(const [key,rule] of Object.entries(infoRules)){
  if(['birthDate','yearsPro'].includes(key)&&p.bioLayoutVerified!==true)continue;
  const v=key==='fromName'?p.collegeName:key==='fullNameId'?p.audio?.fullNameId:key==='jerseyNumberRaw'?(Number.isInteger(p.jerseyNumberRaw)?p.jerseyNumberRaw:p.jerseyNumber==='00'?100:typeof p.jerseyNumber==='string'&&/^\d{1,2}$/.test(p.jerseyNumber)?Number(p.jerseyNumber):typeof p.jerseyNumber==='number'?p.jerseyNumber:undefined):p[key];
  if(rule==='date'){try{const d=bio.dateParts(v);if(d.year===p.birthYear&&d.month===p.birthMonth&&d.day===p.birthDay)info[key]=v;}catch{}continue;}
  if(rule==='name'||rule==='origin'?typeof v==='string'&&v.length<=(rule==='origin'?47:19)&&(rule==='name'||!!v.trim()):typeof v==='number'&&Number.isFinite(v)&&(rule[2]||Number.isInteger(v))&&v>=rule[0]&&v<=rule[1])info[key]=v;
 }
 schema.forEach((f,index)=>{const v=p.ratingsRaw[index];if(Number.isInteger(v)&&v>=0&&v<=maximum(f))fields[f.key]=v;});
 return validate({format:'2kplayer',version:1,game:'NBA2K27',schemaId,name:[p.firstName,p.lastName].filter(Boolean).join(' '),info,fields});
}
function subset(value,selected){
 const dna=validate(value);if(!Array.isArray(selected)||!selected.length||selected.some(g=>!groups.includes(g)))throw Error('Choose at least one valid DNA section.');const include=new Set(selected);
 dna.info=Object.fromEntries(Object.entries(dna.info).filter(([key])=>include.has(infoGroup(key))));dna.fields=Object.fromEntries(Object.entries(dna.fields).filter(([key])=>include.has(schema.find(f=>f.key===key).group)));if(!include.has('seasons')){delete dna.seasonHistory;dna.version=1;}return dna;
}
module.exports={schemaId,isCompatibleSchema:id=>compatibleSchemaIds.has(id),groups,infoGroup,infoKeys:Object.freeze(Object.keys(infoRules)),validate,parse,fromPlayer,subset};
