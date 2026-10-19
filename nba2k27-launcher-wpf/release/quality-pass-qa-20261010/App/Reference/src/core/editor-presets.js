'use strict';
const fs=require('node:fs/promises'),path=require('node:path'),crypto=require('node:crypto');
const schema=require('../data/player-field-schema.json'),{maximum}=require('./player-field-edits');
const {validateValues,controls}=require('./team-staff');
const coachingKeys=new Set(controls.filter(f=>!['Identity','Contract'].includes(f.group)&&!f.text&&!['faceId','soundId','wage'].includes(f.key)).map(f=>f.key));
const kinds=['signature','equipment','coaching'];
function validate(v){
 if(!v||v.format!=='2k-editor-preset'||v.version!==1||!kinds.includes(v.kind)||typeof v.name!=='string'||!v.name.trim()||v.name.length>60||/[\x00-\x1f\x7f]/.test(v.name)||!v.values||typeof v.values!=='object'||Array.isArray(v.values)||!Object.keys(v.values).length)throw Error('Invalid editor preset.');
 const values={};if(v.kind==='coaching'){validateValues(v.values);for(const [key,value] of Object.entries(v.values)){if(!coachingKeys.has(key))throw Error('Staff identity and contracts cannot be included in coaching presets.');values[key]=value;}}
 else for(const [key,value] of Object.entries(v.values)){const f=schema.find(f=>f.key===key);if(!f||f.group!==v.kind||maximum(f)<0||!Number.isInteger(value)||value<0||value>maximum(f))throw Error('Invalid preset field: '+key);values[key]=value;}
 return {format:'2k-editor-preset',version:1,kind:v.kind,name:v.name.trim(),values};
}
class EditorPresets{
 constructor(directory){this.directory=path.resolve(directory);this.busy=false;}
 async safeRoot(){for(let dir=this.directory;;dir=path.dirname(dir)){const stat=await fs.lstat(dir).catch(e=>{if(e.code==='ENOENT')return null;throw e;});if(stat?.isSymbolicLink())throw Error('Preset storage cannot use linked folders.');if(path.dirname(dir)===dir)break;}await fs.mkdir(this.directory,{recursive:true});}
 async read(id){if(typeof id!=='string'||!/^[a-f0-9-]{36}$/.test(id))throw Error('Invalid preset ID.');await this.safeRoot();const file=path.join(this.directory,id+'.json'),stat=await fs.lstat(file);if(!stat.isFile()||stat.isSymbolicLink()||stat.size>32*1024)throw Error('Invalid preset file.');return validate(JSON.parse(await fs.readFile(file,'utf8')));}
 async request(r){if(!r||!kinds.includes(r.kind)||!['list','read','save'].includes(r.action))throw Error('Invalid preset request.');
  if(r.action==='read'){const v=await this.read(r.id);if(v.kind!==r.kind)throw Error('Preset category does not match.');return v;}
  if(r.action==='list'){await this.safeRoot();const rows=[];for(const file of await fs.readdir(this.directory)){if(!/^[a-f0-9-]{36}\.json$/.test(file))continue;try{const id=file.slice(0,-5),v=await this.read(id);if(v.kind===r.kind)rows.push({id,name:v.name});}catch{}}return rows.sort((a,b)=>a.name.localeCompare(b.name));}
  if(this.busy)throw Error('A preset is already being saved.');this.busy=true;
  let temp;try{const v=validate({...r,format:'2k-editor-preset',version:1}),text=JSON.stringify(v,null,2)+'\n';if(Buffer.byteLength(text)>32*1024)throw Error('Preset is too large.');await this.safeRoot();const id=crypto.randomUUID();temp=path.join(this.directory,id+'.tmp');const handle=await fs.open(temp,'wx');try{await handle.writeFile(text);await handle.sync();}finally{await handle.close();}await fs.rename(temp,path.join(this.directory,id+'.json'));return {id,name:v.name};}finally{if(temp)await fs.unlink(temp).catch(()=>{});this.busy=false;}
 }
}
module.exports={EditorPresets,validate,coachingKeys,kinds};
