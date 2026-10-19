'use strict';
// Pure transaction planning against a fresh native snapshot. No process or file
// writes. The native commit boundary must independently recheck this snapshot.
const crypto=require('node:crypto'),transfer=require('../renderer/player-season-transfer');
const statsCodec=require('./player-season-stats'),awards=require('../renderer/player-season-awards');
const PLAYER_STRIDE=0x4f8,DIRECTORY=0x130,REGULAR_SLOTS=25,ALL_SLOTS=33,REF_MASK=0x1fe000;
const normalize=v=>v.normalize('NFKD').replace(/\p{M}/gu,'').trim().toLowerCase();
const fail=message=>{throw Error(message);};
function count(raw){return (raw.readUInt32LE(60)>>>13)&255;}
function setCount(raw,value){if(!Number.isInteger(value)||value<0||value>255)fail('Season reference count overflow.');raw.writeUInt32LE(((raw.readUInt32LE(60)&~REF_MASK)|(value<<13))>>>0,60);}
function clearForPool(raw,next){
 const out=Buffer.from(raw);out.fill(0,0,48);out.writeUInt32LE((out.readUInt32LE(48)&0xf0000000)>>>0,48);out.writeUInt32LE(0,52);
 out.writeUInt32LE((out.readUInt32LE(56)&0xf8000000)>>>0,56);out.writeUInt32LE((out.readUInt32LE(60)&0xf0000000)>>>0,60);
 out.writeUInt32LE((next<<16)>>>0,16);return out;
}
function snapshot(value){
 if(!value||!Buffer.isBuffer(value.players)||!value.players.length||value.players.length%PLAYER_STRIDE||value.players.length/PLAYER_STRIDE>8192||!Buffer.isBuffer(value.stats)||!value.stats.length||value.stats.length%64||value.stats.length/64>=32768||!Buffer.isBuffer(value.pool)||value.pool.length!==64)fail('Unsupported native season snapshot.');
 const playerCount=value.players.length/PLAYER_STRIDE,statCount=value.stats.length/64;
 if(!Number.isInteger(value.recordIndex)||value.recordIndex<0||value.recordIndex>=playerCount||!Array.isArray(value.teams)||!value.teams.length||value.teams.length>1024)fail('Invalid destination player/team directory.');
 const teams=new Map(),pointers=new Set();for(const t of value.teams){
  if(!t||!Number.isInteger(t.index)||t.index<0||t.index>=1024||teams.has(t.index)||typeof t.name!=='string'||!t.name.trim()||t.name.length>100||/[\u0000-\u001f]/.test(t.name))fail('Invalid destination team.');
  let pointer;try{pointer=BigInt(t.pointer);}catch{fail('Invalid native team pointer.');}
  if(pointer<=0n||pointer>0x7fffffffffffn)fail('Invalid native team pointer.');
  if(t.displayName!==undefined&&(typeof t.displayName!=='string'||!t.displayName.trim()||t.displayName.length>180||/[\u0000-\u001f]/.test(t.displayName)))fail('Invalid destination team label.');
  if(pointers.has(pointer.toString()))fail('Duplicate native team pointer.');pointers.add(pointer.toString());teams.set(t.index,{index:t.index,name:t.name,pointer,...(t.displayName!==undefined?{displayName:t.displayName}:{})});
 }
 const references=Array(statCount).fill(0),rows=Array.from({length:statCount},(_,i)=>value.stats.subarray(i*64,(i+1)*64));
 for(let p=0;p<playerCount;p++){
  if(value.players.readBigInt64LE(p*PLAYER_STRIDE+0xe0)!==0n)fail('Private season histories are not supported.');
  for(let s=0;s<ALL_SLOTS;s++){const i=value.players.readInt16LE(p*PLAYER_STRIDE+DIRECTORY+s*2);if(i===-1)continue;if(i<0||i>=statCount)fail('Invalid native season reference.');references[i]++;}
 }
 for(let i=0;i<statCount;i++)if(references[i]!==count(rows[i]))fail('Native season reference counts disagree.');
 const free=[],seen=new Set();for(let i=value.pool.readUInt16LE(0);i!==65535;i=rows[i].readUInt16LE(18)){
  if(i>=statCount||references[i]!==0||seen.has(i))fail('Invalid native free-season chain.');seen.add(i);free.push(i);
  if(!rows[i].equals(clearForPool(rows[i],rows[i].readUInt16LE(18))))fail('Free season record is not initialized.');
 }
 if(free.length+references.filter(v=>v>0).length!==statCount)fail('Native free-season chain is incomplete.');
 const existing=new Map();let origin=null;
 for(let slot=0;slot<REGULAR_SLOTS;slot++){
  const index=value.players.readInt16LE(value.recordIndex*PLAYER_STRIDE+DIRECTORY+slot*2);if(index<0)continue;
  const year=1900+rows[index][17];if(existing.has(year)||origin!==null&&origin!==year+slot)fail('Destination regular-season ordering is ambiguous.');origin=year+slot;existing.set(year,{year,slot,index,raw:rows[index]});
 }
 return {playerCount,statCount,teams,references,rows,free,existing,origin};
}
function resolveTeam(teams,name,mapped,year,role){
 if(name===null)return null;
 if(mapped!==undefined){const team=teams.get(mapped);if(!team)fail('Selected destination team is no longer available.');return team;}
 const matches=[...teams.values()].filter(t=>normalize(t.name)===normalize(name));
 if(matches.length!==1){const error=Error(`Choose the destination ${role} team for ${year}: ${name}.`);error.code='TEAM_MAPPING_REQUIRED';error.year=year;error.role=role;error.sourceName=name;error.choices=[...teams.values()].map(({index,name,displayName})=>({index,name,...(displayName!==undefined?{displayName}:{})}));throw error;}
 return matches[0];
}
function plan(value,source,{teamMappings={},normalTeamsOnly=false,preserveTotals={}}={}){
 const input=transfer.validate(source);if(!input.length)fail('Choose at least one season to import.');
 if(!teamMappings||typeof teamMappings!=='object'||Array.isArray(teamMappings))fail('Invalid destination team mappings.');
 const years=new Set();for(const row of input){if(years.has(row.year))fail('Choose one aggregate row per season year.');years.add(row.year);}
 if(Object.keys(teamMappings).some(year=>!years.has(Number(year))))fail('Team mapping does not belong to a selected year.');
 const state=snapshot(value),origin=Math.max(state.origin??Math.max(...years),...years);
 if(!preserveTotals||typeof preserveTotals!=='object'||Array.isArray(preserveTotals)||Object.keys(preserveTotals).some(year=>!years.has(Number(year))))fail('Invalid preserved season fields.');
 for(const row of input){
  const fields=preserveTotals[row.year]??[];
  if(!Array.isArray(fields)||fields.length>20||new Set(fields).size!==fields.length||fields.some(key=>!Object.hasOwn(statsCodec.fields,key)))fail('Invalid preserved season fields.');
  const old=state.existing.get(row.year),baseline=old?statsCodec.decode(old.raw):null;
  for(const key of fields)row.totals[key]=baseline?.[key]??0;
 }
 transfer.validate(input);
 const targetTeams=normalTeamsOnly?new Map([...state.teams].filter(([,team])=>team.displayName?.endsWith(' · Normal'))):state.teams;
 if(!targetTeams.size)fail('No verified normal destination teams are available. Refresh the roster before importing stats.');
 // Never discard unselected history to make a career fit. Gaps are preserved.
 for(const year of [...state.existing.keys(),...years])if(origin-year<0||origin-year>=REGULAR_SLOTS)fail('Selected and existing history spans more than the 25 regular-season slots. No seasons will be dropped.');
 const players=Buffer.from(value.players),stats=Buffer.from(value.stats),pool=Buffer.from(value.pool),review=[],warnings=[];
 for(const row of input)if(preserveTotals[row.year]?.length)warnings.push(`${row.year}: unavailable ${preserveTotals[row.year].join(', ')} ${state.existing.has(row.year)?'preserved from the verified destination snapshot':'initialized to zero for this new season'}. Existing awards are preserved when none are supplied.`);
 const base=value.recordIndex*PLAYER_STRIDE+DIRECTORY;players.fill(255,base,base+REGULAR_SLOTS*2);
 for(const row of state.existing.values())players.writeInt16LE(row.index,base+(origin-row.year)*2);
 let usedFree=0;
 for(const row of input){
  const mapping=teamMappings[row.year]??{};if(!mapping||typeof mapping!=='object'||Array.isArray(mapping)||Object.keys(mapping).some(k=>!['primaryTeamIndex','secondaryTeamIndex'].includes(k)))fail('Invalid season team mapping.');
  const primary=resolveTeam(targetTeams,row.team,mapping.primaryTeamIndex,row.year,'primary');
  const secondary=resolveTeam(targetTeams,row.secondaryTeam??null,mapping.secondaryTeamIndex,row.year,'secondary');
  if(primary&&secondary&&primary.index===secondary.index){if(normalize(row.team)!==normalize(row.secondaryTeam))fail('Different source teams cannot map to the same destination team.');warnings.push(`${row.year}: the source repeats the same team in both entries; both entries are preserved.`);}
  const old=state.existing.get(row.year),template=old?Buffer.from(old.raw):null;
  let next=template||Buffer.alloc(64);next=statsCodec.plan(next,row.totals,{linkedPlayers:[{recordIndex:value.recordIndex}],confirmedShared:true}).after;
  if(row.awards)next=Buffer.from(awards.patch(next,row.awards));
  next.writeBigInt64LE(primary.pointer,0);next.writeBigInt64LE(secondary?.pointer??0n,8);next[17]=row.year-1900;
  const changed=!old||!next.equals(old.raw),shared=!!old&&state.references[old.index]>1;
  let index=old?.index,operation='unchanged';
  if(changed){
   if(!old||shared){
    index=state.free[usedFree++];if(index===undefined)fail('No free season records remain. Existing history will not be evicted.');
    if(!old){const freeTemplate=Buffer.from(state.rows[index]);next=statsCodec.plan(freeTemplate,row.totals,{linkedPlayers:[{recordIndex:value.recordIndex}],confirmedShared:true}).after;if(row.awards)next=Buffer.from(awards.patch(next,row.awards));next.writeBigInt64LE(primary.pointer,0);next.writeBigInt64LE(secondary?.pointer??0n,8);next[17]=row.year-1900;}
    next.writeUInt16LE(65535,18);setCount(next,1);
    if(shared){const before=Buffer.from(old.raw);setCount(before,state.references[old.index]-1);before.copy(stats,old.index*64);operation='copy-shared';}else operation='create';
   }else operation='update';
   next.copy(stats,index*64);
  }
  const slot=origin-row.year;players.writeInt16LE(index,base+slot*2);
  const label=team=>({index:team.index,name:team.name,...(team.displayName!==undefined?{displayName:team.displayName}:{})});
  const oldTeam=old?[...state.teams.values()].find(t=>t.pointer===old.raw.readBigUInt64LE(0)):null;
  review.push({year:row.year,slot,operation,primaryTeam:label(primary),secondaryTeam:secondary?label(secondary):null,totals:{...row.totals},awards:row.awards?{...row.awards}:null,before:old?{totals:statsCodec.decode(old.raw),awards:awards.decode(old.raw),team:oldTeam?.displayName||oldTeam?.name||'Unavailable'}:null});
  if(row.totals.points!==row.totals.fgm*2+row.totals.threeMade+row.totals.ftm)warnings.push(`${row.year}: source points differ from shooting totals; the original values are preserved.`);
 }
 if(usedFree)pool.writeUInt16LE(state.free[usedFree]??65535,0);
 // Revalidate the resulting whole directory/pool: all ownership must agree.
 snapshot({...value,players,stats,pool});
 const changes=[];for(let i=0;i<state.statCount;i++){const before=value.stats.subarray(i*64,(i+1)*64),after=stats.subarray(i*64,(i+1)*64);if(!before.equals(after))changes.push({table:'stats',offset:i*64,before:Buffer.from(before),after:Buffer.from(after)});}
 if(!pool.equals(value.pool))changes.push({table:'pool',offset:0,before:Buffer.from(value.pool.subarray(0,2)),after:Buffer.from(pool.subarray(0,2))});
 if(!players.equals(value.players))changes.push({table:'players',offset:base,before:Buffer.from(value.players.subarray(base,base+50)),after:Buffer.from(players.subarray(base,base+50))});
 const digest=crypto.createHash('sha256').update(String(value.recordIndex)+':');for(const b of [value.players,value.stats,value.pool])digest.update(b);digest.update(JSON.stringify([...state.teams.values()].map(t=>({...t,pointer:t.pointer.toString()}))));
 return {snapshotId:digest.digest('hex'),originYear:origin,review,warnings,allocated:usedFree,changes,after:{players,stats,pool}};
}
module.exports={PLAYER_STRIDE,DIRECTORY,REGULAR_SLOTS,ALL_SLOTS,count,setCount,clearForPool,snapshot,plan};
