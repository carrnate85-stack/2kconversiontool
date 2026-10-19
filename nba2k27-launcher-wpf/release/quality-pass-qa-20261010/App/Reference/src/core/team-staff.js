'use strict';
const schema=require('../data/staff-field-schema.json');
const controls=require('../data/staff-controls.json');
const fs=require('node:fs/promises'),path=require('node:path'),crypto=require('node:crypto');
const {execFile}=require('node:child_process');
const {promisify}=require('node:util');
const run=promisify(execFile);
const STRIDE=456;
const scalarFields=[
 ['BenchDepth',0x158],['LineupFormFactor',0x159],['RunPlayTendency',0x15a],
 ['DoubleTeamTendency',0x15b],['ZoneUsage',0x15c],['ShotInsideTendency',0x15d],
 ['ShotCloseTendency',0x15e],['ShotMediumTendency',0x15f],['ShotThreeTendency',0x160],
 ['DriveTendency',0x161],['PostUpTendency',0x162],['AlleyOopTendency',0x163],
 ['Tempo',0x164],['HelpDefense',0x165],['Motivation',0x166],['Intensity',0x167],
 ['BasketballIQ',0x168],['WorkEthic',0x169],['Offense',0x16a],['Defense',0x16b],
 ['Business',0x16c],['Training',0x16d],['Charisma',0x16e],['Analytics',0x16f],
];
const systems=['Balanced','Grit and Grind','Pace and Space','Perimeter Centric','Post Centric','Triangle','Seven Seconds','Defense'];
function name(b,at){
 let end=at;while(end<at+40&&b.readUInt16LE(end)!==0)end+=2;
 if(end===at+40)throw Error('Unterminated staff name.');
 const s=b.subarray(at,end).toString('utf16le');if(/[\x00-\x1f\x7f\ufffd]/u.test(s))throw Error('Invalid staff name.');return s;
}
function label(key){return key.replace(/Badge$/,'').replaceAll('_',' ').replace(/([a-z])([A-Z])/g,'$1 $2');}
function getValue(b,f){if(f.text)return name(b,f.offset);let v=0;for(let i=0;i<f.bits;i++){const bit=f.offset*8+f.shift+i;if(b[bit>>3]&(1<<(bit&7)))v+=2**i;}return f.signed&&v>=32768?v-65536:v;}
function validateValues(values){
 if(!values||typeof values!=='object'||Array.isArray(values)||Object.keys(values).length>controls.length)throw Error('Invalid staff changes.');
 for(const [key,v] of Object.entries(values)){const f=controls.find(f=>f.key===key);if(!f)throw Error('Unsupported staff field: '+key);
  if(f.text){if(typeof v!=='string'||v.length>=f.bytes/2||/[\x00-\x1f\x7f]|[\uD800-\uDFFF]/u.test(v))throw Error('Invalid staff name.');}
  else if(!Number.isInteger(v)||v<(f.min??0)||v>(f.max??2**f.bits-1)||(f.choices&&v>=f.choices.length&&!(f.key.endsWith('SystemId')&&v===15)))throw Error('Invalid staff value: '+key);
 }return {...values};
}
function patch(raw,values){const b=Buffer.from(raw,'base64');if(b.length!==STRIDE)throw Error('Invalid staff snapshot.');validateValues(values);
 for(const [key,v] of Object.entries(values)){const f=controls.find(f=>f.key===key);if(f.text){b.fill(0,f.offset,f.offset+f.bytes);b.write(v,f.offset,f.bytes,'utf16le');}else{const n=v<0?v+65536:v;for(let i=0;i<f.bits;i++){const bit=f.offset*8+f.shift+i,mask=1<<(bit&7);b[bit>>3]=(b[bit>>3]&~mask)|(Math.floor(n/2**i)%2?mask:0);}}}
 if(!name(b,0x50)&&!name(b,0x78))throw Error('A staff member needs a name.');return b;
}
function decode(raw,slot,index){
 if(typeof raw!=='string'||!/^[A-Za-z0-9+/]{608}$/.test(raw))throw Error('Invalid staff record encoding.');
 const b=Buffer.from(raw,'base64');if(b.length!==STRIDE||b.toString('base64')!==raw)throw Error('Incomplete staff record.');
 if(!Number.isInteger(slot)||slot<0||slot>31||!Number.isInteger(index)||index<0||index>=8192||(b[0x182]&31)!==slot)throw Error('Invalid staff identity.');
 const firstName=name(b,0x50),lastName=name(b,0x78);if(!firstName&&!lastName)throw Error('Unnamed staff record.');
 const system=(b.readUInt32LE(0x130)>>>20)&15;
 const active=(b.readUInt32LE(0x130)>>>24)&15;
 const displaySystem=n=>n===15?'None / unset':schema.systems[n]?systems[n]:`Unknown (${n})`;
 return {slot,recordIndex:index,firstName,lastName,name:[firstName,lastName].filter(Boolean).join(' '),
  role:schema.roles[slot],values:Object.fromEntries(controls.map(f=>[f.key,getValue(b,f)])),controls,uniqueId:b.readUInt16LE(0x120),faceId:b.readUInt16LE(0x12a)&0x7fff,
  soundId:b.readInt16LE(0x128),playerUniqueId:b.readUInt32LE(0xa8),wage:b.readUInt32LE(0x124),
  growthPotential:b[0x131],yearsInLeague:(b.readUInt32LE(0x134)>>>7)&127,yearsWithTeam:b[0x134]&127,
  preferredSystem:displaySystem(system),activeSystem:displaySystem(active),
  attributes:scalarFields.map(([key,offset])=>({key,label:label(key),raw:b[offset]})),
  proficiency:systems.map((label,i)=>({key:schema.systems[i],label,raw:b[0x170+i]})),
  badges:schema.badges.filter(f=>f.key!=='NONEBadge').map(f=>{
   const n=Math.ceil((f.shift+f.bits)/8);let packed=0;for(let i=0;i<n;i++)packed+=b[f.offset+i]*2**(8*i);
   return {key:f.key,label:label(f.key),raw:(packed>>>f.shift)&((1<<f.bits)-1),bits:f.bits};
  })};
}
class TeamStaffService {
 constructor(manager,{execute=run,backupRoot}={}){this.manager=manager;this.execute=execute;this.backupRoot=backupRoot;this.busy=false;this.snapshot=null;}
 async read(input){
  if(this.busy)throw Error('Staff inspection is already running.');
  if(!input||!Number.isInteger(input.recordIndex)||!Number.isInteger(input.teamId)||typeof input.expectedName!=='string')throw Error('Invalid staff inspection request.');
  const state=this.manager.state,live=(state.teams||[]).find(t=>t.source==='live-team'&&t.recordIndex===input.recordIndex&&t.teamId===input.teamId&&t.name===input.expectedName);
  if(state.status!=='ready'||!live||!Number.isInteger(state.pid)||state.pid<=0)throw Error('Load this team’s roster and refresh live teams first.');
  this.busy=true;
  this.snapshot=null;
  try{
   const {stdout}=await this.execute(this.manager.helperPath,['--team-staff','--pid',String(state.pid),'--record-index',String(input.recordIndex),'--team-id',String(input.teamId),'--expected-name',input.expectedName],{windowsHide:true,timeout:30000,maxBuffer:128*1024,encoding:'utf8'});
   if(this.manager.state!==state)throw Error('The live roster changed during inspection. Refresh and try again.');
   const r=JSON.parse(stdout);
   if(r.ok!==true||r.schema!=='nba2k27-team-staff-v1'||r.readOnly!==true||r.writes!==0||r.recordIndex!==input.recordIndex||r.teamId!==input.teamId||r.expectedName!==input.expectedName||!Array.isArray(r.rows)||r.rows.length>32)throw Error('Invalid guarded staff report.');
   const slots=new Set(),indices=new Set();const staff=r.rows.map(row=>{if(slots.has(row.slot)||indices.has(row.recordIndex))throw Error('Duplicate staff report.');slots.add(row.slot);indices.add(row.recordIndex);return decode(row.raw,row.slot,row.recordIndex);});
   this.snapshot={state,input:{...input},rows:r.rows};
   return {staff,readOnly:true,writes:0,editingEnabled:true,status:'Stage mapped settings, then Save & verify. IDs and assignment links stay unchanged; save the roster in-game to retain edits. Raw attribute and badge levels are shown without tier conversion.'};
  }finally{this.busy=false;}
 }
 async save(input){
  if(this.busy)throw Error('A staff operation is already running.');const s=this.snapshot,state=this.manager.state;
  if(state.status!=='ready'||!s||s.state!==state||s.input.recordIndex!==input?.recordIndex||s.input.teamId!==input.teamId||s.input.expectedName!==input.expectedName)throw Error('Refresh this team’s staff before saving.');
  const row=s.rows.find(r=>r.slot===input.slot&&r.recordIndex===input.staffIndex);if(!row)throw Error('The selected staff assignment changed.');
  const current=decode(row.raw,row.slot,row.recordIndex).values;
  if(!input.expected||Object.keys(input.values||{}).some(key=>input.expected[key]!==current[key]))throw Error('The staff values changed. Refresh before saving.');
  const after=patch(row.raw,input.values),before=Buffer.from(row.raw,'base64');if(before.equals(after))return {saved:true,writes:0,readBackVerified:true};
  if(!this.backupRoot)throw Error('Staff recovery storage is unavailable.');this.busy=true;
  try{
   await fs.mkdir(this.backupRoot,{recursive:true});const backup=path.join(this.backupRoot,'staff-'+Date.now()+'-'+crypto.randomUUID()+'.json');
   const h=await fs.open(backup,'wx');try{await h.writeFile(JSON.stringify({schema:'nba2k27-staff-recovery-v1',pid:state.pid,...s.input,...row,after:after.toString('base64')}));await h.sync();}finally{await h.close();}
   if(this.manager.state!==state)throw Error('The roster changed before saving. Refresh staff.');
   const args=['--team-staff-write','--pid',String(state.pid),'--record-index',String(input.recordIndex),'--team-id',String(input.teamId),'--expected-name',input.expectedName,'--staff-index',String(row.recordIndex),'--slot',String(row.slot),'--before',row.raw,'--after',after.toString('base64')];
   const {stdout}=await this.execute(this.manager.helperPath,args,{windowsHide:true,timeout:30000,maxBuffer:128*1024,encoding:'utf8'});const r=JSON.parse(stdout);
   if(r.ok!==true||r.schema!=='nba2k27-team-staff-write-v1'||r.readBackVerified!==true||r.staffIndex!==row.recordIndex||r.teamId!==input.teamId||r.recordIndex!==input.recordIndex||!Number.isInteger(r.writes)||r.writes<1||r.writes>STRIDE)throw Error('Staff save could not be verified. Refresh staff before retrying.');
   row.raw=after.toString('base64');return {...r,saved:true,backup};
  }finally{this.busy=false;}
 }
}
module.exports={TeamStaffService,decode,schema,STRIDE,scalarFields,controls,validateValues,patch,getValue};
