'use strict';
const codec=require('./native-roster-file');
const reader=require('./native-roster-reader');
class NativeRosterExport{
 constructor({verify,read=reader.read}){this.verify=verify;this.read=read;this.busy=false;this.listed=null;}
 async run(work){if(this.busy)throw Error('A roster export is already running.');this.busy=true;try{return await work();}finally{this.busy=false;}}
 async list(){return this.run(async()=>{
  this.listed=null;const session=await this.verify(),slots=[];
  for(let slot=1;slot<=100;slot++){const row=await this.read(slot);if(row.type===1)slots.push(row);}
  const after=await this.verify();if(JSON.stringify(session)!==JSON.stringify(after))throw Error('Game session changed. Refresh the saved roster list.');
  this.listed={session,slots};return slots.map(({slot,name})=>({slot,name}));
 });}
 async export(slot){return this.run(async()=>{
  const listed=this.listed,chosen=listed?.slots.find(row=>row.slot===slot);
  if(!chosen)throw Error('Refresh and select a saved NBA roster first.');
  const session=await this.verify();if(JSON.stringify(session)!==JSON.stringify(listed.session))throw Error('Game restarted. Refresh the saved roster list.');
  const before=await this.read(slot);if(JSON.stringify(before)!==JSON.stringify(chosen))throw Error('Saved roster changed. Refresh the list.');
  const blob=await this.read(slot,true),after=await this.read(slot);
  if(JSON.stringify(before)!==JSON.stringify(after)||before.stamp!==blob.stamp)throw Error('Saved roster changed during export. Try again after saving finishes.');
  if(JSON.stringify(session)!==JSON.stringify(await this.verify()))throw Error('Game session changed during export.');
  return {name:before.name,bytes:codec.create({bundle:blob.bundle,name:before.name,gameBuildSha256:session.gameBuildSha256})};
 });}
}
module.exports={NativeRosterExport};
