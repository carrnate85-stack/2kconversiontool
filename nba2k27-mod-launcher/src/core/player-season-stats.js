'use strict';

const math=require('../renderer/player-season-math');
// Current 2DD2500 getter mappings; signed plus/minus occupies 15 bits.
const fields = Object.freeze({
  ftm:[0x20,0,11],fta:[0x20,11,11],fgm:[0x18,13,13],fga:[0x14,15,14],
  threeMade:[0x24,12,12],threeAttempted:[0x24,0,12],oreb:[0x20,22,10],
  dreb:[0x28,0,12],steals:[0x1c,0,11],blocks:[0x1c,11,11],assists:[0x38,0,12],
  turnovers:[0x28,12,11],points:[0x2c,0,14],gp:[0x14,0,7],gs:[0x38,12,7],minutes:[0x18,0,13],
  fouls:[0x1c,22,10],doubleDoubles:[0x34,25,7],tripleDoubles:[0x14,8,7],plusMinus:[0x2c,14,15,true],
});
const averages = Object.freeze(math.averages);
function decode(record) {
  if (!Buffer.isBuffer(record)||record.length!==64) throw Error('Invalid season record.');
  return Object.fromEntries(Object.entries(fields).map(([key,[offset,shift,bits,signed]])=>{let value=(record.readUInt32LE(offset)>>>shift)&(2**bits-1);if(signed&&value>=2**(bits-1))value-=2**bits;return [key,value];}));
}
function summary(totals) {
  const display=math.display(totals);return Object.fromEntries(Object.entries(display).filter(([key])=>!Object.hasOwn(fields,key)));
}
function sharedWarning(linkedPlayers) {
  if (!Array.isArray(linkedPlayers)||!linkedPlayers.length) throw Error('Season ownership is unavailable. Refresh the roster.');
  const count=new Set(linkedPlayers.map(p=>{
    if (!Number.isInteger(p.recordIndex)||p.recordIndex<0) throw Error('Invalid linked player.');
    return p.recordIndex;
  })).size;
  return count>1?`This season history is shared by ${count} player entries in the currently loaded roster. Saving will update all ${count} linked entries. Other roster files will not be changed.`:null;
}
function plan(record,changes,{linkedPlayers,confirmedShared=false}={}) {
  const before=decode(record),warning=sharedWarning(linkedPlayers);let next={...before};
  if(!changes||typeof changes!=='object'||Array.isArray(changes))throw Error('Invalid season changes.');
  const edited=new Set();
  for(const [key,value] of Object.entries(changes)){
    if(!Object.hasOwn(fields,key)&&!Object.hasOwn(averages,key)&&!Object.hasOwn(math.percentages,key)&&!['rpg','rebounds'].includes(key))throw Error('Unsupported season field: '+key);
    if(typeof value!=='number'||!Number.isFinite(value))throw Error('Invalid season value: '+key);
    if(Object.hasOwn(fields,key)){
      const bits=fields[key][2],signed=fields[key][3];if(!Number.isInteger(value)||value<(signed?-(2**(bits-1)):0)||value>(signed?2**(bits-1)-1:2**bits-1))throw Error('Season total outside the stored range: '+key);
      next[key]=value;edited.add(key);
    }
  }
  for(const [key,value] of Object.entries(changes))if(!Object.hasOwn(fields,key)){
    const total=averages[key]||math.percentages[key]?.[0];if(total&&Object.hasOwn(changes,total))throw Error('Edit the average or its total, not both: '+key);
    next=math.change(next,key,value);
  }
  for(const [key,[offset,shift,bits,signed]] of Object.entries(fields))if(!Number.isInteger(next[key])||next[key]<(signed?-(2**(bits-1)):0)||next[key]>(signed?2**(bits-1)-1:2**bits-1))throw Error('Season total outside the stored range: '+key);
  const changed=Object.keys(fields).filter(key=>before[key]!==next[key]);
  if(changed.length&&warning&&!confirmedShared)throw Error(warning+' Confirm before saving.');
  if(changed.length){
    if(next.gs>next.gp)throw Error('Games started cannot exceed games played.');
    if(next.doubleDoubles>next.gp||next.tripleDoubles>next.gp)throw Error('Double/triple-doubles cannot exceed games played.');
    for(const [made,attempted] of [['fgm','fga'],['threeMade','threeAttempted'],['ftm','fta']])if(next[made]>next[attempted])throw Error('Made shots cannot exceed attempts.');
    if(next.threeMade>next.fgm||next.threeAttempted>next.fga)throw Error('Three-pointers cannot exceed all field goals.');
  }
  const after=Buffer.from(record);
  for(const key of changed){
    const [offset,shift,bits]=fields[key],mask=((2**bits-1)<<shift)>>>0;
    after.writeUInt32LE(((after.readUInt32LE(offset)&~mask)|((next[key]&(2**bits-1))<<shift))>>>0,offset);
  }
  const warnings=[];
  if(changed.length&&next.points!==next.fgm*2+next.threeMade+next.ftm)warnings.push('Points do not match the shooting totals. Shooting totals have not been changed automatically.');
  return {before,after,totals:next,averages:summary(next),changed,sharedWarning:warning,warnings};
}
module.exports={fields,averages,decode,summary,sharedWarning,plan};
