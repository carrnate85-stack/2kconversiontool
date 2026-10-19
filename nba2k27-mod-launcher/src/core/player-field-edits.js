'use strict';
const schema=require('../data/player-field-schema.json');
function maximum(f){return f.key==='PotentialRaw'||f.readOnly===true?-1:Math.min(2**f.bits-1,f.max??(f.money?200000000:f.choices?f.choices.length-1:f.group==='badges'?5:f.group==='tendencies'?100:2**f.bits-1));}
function validate(changes,live){
 if(changes===undefined)return [];
 if(!Array.isArray(changes)||changes.length>schema.length)throw Error('Invalid player field changes.');
 const seen=new Set();return changes.map(c=>{
  if(!c||!Number.isInteger(c.index)||!schema[c.index]||seen.has(c.index))throw Error('Invalid or duplicate player field.');seen.add(c.index);
  const f=schema[c.index],max=maximum(f);
  if(!Number.isInteger(c.value)||c.value<0||c.value>max||!Number.isInteger(c.expected)||c.expected<0||c.expected>=2**f.bits)throw Error('Invalid value for '+f.label+'.');
  if(live&&(!Array.isArray(live)||live.length!==schema.length||live[c.index]!==c.expected))throw Error('Player fields changed. Refresh before saving.');
  return {index:c.index,expected:c.expected,value:c.value};
 });
}
module.exports={validate,maximum};
