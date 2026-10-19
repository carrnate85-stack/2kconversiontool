'use strict';
const {validateValues}=require('./team-staff');
const sections=['info','arena','staff','rotations','uniforms'];
const uniformTransfer=require('../renderer/team-uniform-transfer');
const textRules={name:23,cityName:17,cityShort:6,stateShort:6,mainLogo:35};
const object=v=>v&&typeof v==='object'&&!Array.isArray(v);
function text(v,max){if(typeof v!=='string'||v.length>max||/[\x00-\x1f\x7f]/.test(v))throw Error('Invalid Team DNA text.');return v;}
function validate(v){
 if(!object(v)||v.format!=='2kteam'||v.version!==1||v.game!=='NBA2K27'||!object(v.info)||!Array.isArray(v.staff)||v.staff.length>32)throw Error('This is not a supported Team DNA file.');
 const info={};for(const [key,value] of Object.entries(v.info)){if(Object.hasOwn(textRules,key))info[key]=text(value,textRules[key]);else if(['rosterTypeId','historicYearId'].includes(key)&&Number.isInteger(value)&&value>=0&&value<=(key==='rosterTypeId'?31:127))info[key]=value;else throw Error('Unsupported Team DNA field: '+key);}
 let arena=null;if(v.arena!==null&&v.arena!==undefined){if(!object(v.arena))throw Error('Invalid Team DNA arena.');arena={name:text(v.arena.name,32),arenaFile:text(v.arena.arenaFile,37),floorFile:text(v.arena.floorFile,39)};for(const stem of [arena.arenaFile,arena.floorFile])if(!/^[A-Za-z0-9_ -]+$/.test(stem))throw Error('Invalid arena or floor asset stem.');}
 const slots=new Set(),staff=v.staff.map(row=>{if(!object(row)||!Number.isInteger(row.slot)||row.slot<0||row.slot>=32||slots.has(row.slot))throw Error('Invalid or duplicate Team DNA staff slot.');slots.add(row.slot);return {slot:row.slot,values:validateValues(row.values)};});
 // Reconstruct portable data only: no source Team ID, staff ID, pointers,
 // process metadata, cache paths, or local files are transferable.
 let rotations=null;if(v.rotations!==null&&v.rotations!==undefined){if(!Array.isArray(v.rotations)||v.rotations.length!==48||v.rotations.some(row=>!Array.isArray(row)||row.length!==5||new Set(row).size!==5))throw Error('Team DNA rotations need 48 five-player segments.');rotations=v.rotations.map(row=>row.map(name=>text(name,39)));}
 const uniforms=uniformTransfer.validate(v.uniforms??[]);
 return {format:'2kteam',version:1,game:'NBA2K27',name:text(v.name,80),info,arena,staff,rotations,uniforms};
}
function parse(s){if(typeof s!=='string'||Buffer.byteLength(s)>512*1024)throw Error('Team DNA must be smaller than 512 KB.');let v;try{v=JSON.parse(s);}catch{throw Error('The Team DNA file is not valid JSON.');}return validate(v);}
function subset(v,selected){const r=validate(v);if(!Array.isArray(selected)||!selected.length||selected.some(s=>!sections.includes(s)))throw Error('Choose at least one Team DNA section.');if(!selected.includes('info'))r.info={};if(!selected.includes('arena'))r.arena=null;if(!selected.includes('staff'))r.staff=[];if(!selected.includes('rotations'))r.rotations=null;if(!selected.includes('uniforms'))r.uniforms=[];return r;}
function fromTeam(team,arena,staff=[]){if(team?.source!=='live-team')throw Error('Refresh live teams before exporting Team DNA.');const info={};for(const key of [...Object.keys(textRules),'rosterTypeId','historicYearId'])if(team[key]!==undefined)info[key]=team[key];
 // Unknown values are preserved in-game but cannot be offered as valid edits.
 const rows=staff.map(row=>({slot:row.slot,values:Object.fromEntries(Object.entries(row.values).filter(([key,v])=>{try{validateValues({[key]:v});return true;}catch{return false;}}))}));
 return validate({format:'2kteam',version:1,game:'NBA2K27',name:team.name,info,arena:arena?.arenaFile&&arena?.floorFile?{name:arena.name,arenaFile:arena.arenaFile,floorFile:arena.floorFile}:null,staff:rows});
}
module.exports={sections,validate,parse,subset,fromTeam};
