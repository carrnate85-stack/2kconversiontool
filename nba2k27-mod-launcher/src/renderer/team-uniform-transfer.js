(function(root,factory){const api=factory(typeof module==='object'&&module.exports?require('./team-uniforms'):root.TeamUniforms);if(typeof module==='object'&&module.exports)module.exports=api;else root.TeamUniformTransfer=api;})(typeof window==='undefined'?globalThis:window,uniforms=>{
 'use strict';
 const rules=new Map(uniforms.PACKED_FIELDS.map(f=>[f.key,f.values.length-1]));
 for(const key of uniforms.COLOR_FIELDS.map(f=>f.key))rules.set(key,0xFFFFFFFF);
 rules.set('home',7);rules.set('away',7);rules.set('logo',3);
 const object=v=>v&&typeof v==='object'&&!Array.isArray(v);
 function values(input){if(!object(input))throw Error('Invalid uniform settings.');const result={};for(const [key,v] of Object.entries(input)){if(!rules.has(key)||!Number.isInteger(v)||v<0||v>rules.get(key))throw Error('Invalid uniform setting: '+key);result[key]=v;}return result;}
 function validate(input){if(!Array.isArray(input)||input.length>128)throw Error('Invalid Team DNA uniforms.');return input.map(row=>{if(!object(row)||!object(row.match))throw Error('Invalid uniform slot match.');const match=values(row.match);if(Object.keys(match).length!==3||!['typeId','sideId','designationId'].every(k=>Object.hasOwn(match,k)))throw Error('Uniform matches require type, side and designation.');return {match,values:values(row.values)};});}
 function matches(match,uniform){return Object.entries(match).every(([key,value])=>uniform[key]===value);}
 return {values,validate,matches};
});
