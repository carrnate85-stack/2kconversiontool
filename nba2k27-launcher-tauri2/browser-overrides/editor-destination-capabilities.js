(function(root,factory){const api=factory();if(typeof module==='object'&&module.exports)module.exports=api;if(root)root.EditorDestinationCapabilities=api;})(typeof window==='object'?window:globalThis,()=>{
 'use strict';
 function filterPlayer(dna,before={},capabilities,available={}){
  const data={...dna,info:{},fields:{}},skipped=[];
  const writable=capabilities?.infoFields??capabilities;
  for(const [key,value] of Object.entries(dna.info||{})){
   const changed=value!==before.info?.[key];
   if((Array.isArray(writable)?!writable.includes(key):writable?.[key]===false)||available[key]===false){if(changed)skipped.push(key);continue;}
   if(changed)data.info[key]=value;
  }
  for(const [key,value] of Object.entries(dna.fields||{}))if(value!==before.fields?.[key])data.fields[key]=value;
  return {data,skipped,count:Object.keys(data.info).length+Object.keys(data.fields).length};
 }
 const staffWritable=staff=>staff?.shared!==true&&staff?.editingEnabled!==false;
 return {filterPlayer,staffWritable};
});
