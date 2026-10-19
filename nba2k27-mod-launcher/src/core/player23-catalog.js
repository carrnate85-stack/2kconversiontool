'use strict';
const fs=require('node:fs/promises'),path=require('node:path'),crypto=require('node:crypto');
const transfer=require('../renderer/player-season-transfer'),awards=require('../renderer/player-season-awards');
const dna=require('./player-dna'),schema=require('../data/player-field-schema.json');
const sectionKeys=['info','appearance','attributes','tendencies','personality','hotzones'],fieldGroups=new Map(schema.map(field=>[field.key,field.group]));
function sectionCounts(data){return Object.fromEntries(sectionKeys.map(key=>[key,data?Object.keys(data.info).filter(name=>dna.infoGroup(name)===key).length+Object.keys(data.fields).filter(name=>fieldGroups.get(name)===key).length:0]));}
const MAX_BYTES=32*1024*1024,hash=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const categories=new Set(['current','historic','all_time','other','historic_unassigned','unassigned']);
const integer=(v,max,label)=>{if(!Number.isInteger(v)||v<0||v>max)throw Error('Invalid '+label+'.');return v;};
const text=(v,max,label)=>{if(typeof v!=='string'||v.length>max||/[\u0000-\u001f]/.test(v))throw Error('Invalid '+label+'.');return v;};
const array=(v,max,label)=>{if(!Array.isArray(v)||v.length>max)throw Error('Invalid '+label+'.');return v;};
const unique=rows=>{const map=new Map();for(const row of rows){if(map.has(row.id))throw Error('Duplicate source ID.');map.set(row.id,row);}return map;};
function validate(value){
 if(!value||!['2k23-season-catalog','2k23-player-catalog'].includes(value.format)||value.version!==1||value.game!=='NBA2K23')throw Error('Choose a supported .2k23catalog or .2k23stats catalog, not a raw ratings export.');
 const unified=value.format==='2k23-player-catalog',fieldKeys=unified?array(value.fieldKeys,schema.length,'source field keys'):[];
 if(unified&&(!dna.isCompatibleSchema(value.targetSchemaId)||new Set(fieldKeys).size!==fieldKeys.length||fieldKeys.some(key=>{const f=schema.find(f=>f.key===key);return !f||key==='PotentialRaw'||key==='AgilityAbilityRaw'||!(['attributes','tendencies','hotzones'].includes(f.group)||f.group==='personality'&&f.bits===1||['BestHand','BestDunkHand'].includes(key));})))throw Error('Unsupported source-to-target player mapping.');
 const source=value.source;
 if(!source||!/^\w{64}$/.test(source.sha256)||!/^[a-f0-9]{64}$/.test(source.sha256)||source.playerSchema!=='48362c10f664d79bb3fc8c81d54a72288d80debc52d2efd5253e690547af2eaa'||source.historySchema!=='df2519eb509843ef0df97ecb8666131875c75a6deeeba22db51c2e3f6790ee36')throw Error('Unsupported 2K23 season schema or source fingerprint.');
 const teams=array(value.teams,1024,'source teams').map(t=>{
  if(!t||!categories.has(t.category))throw Error('Invalid source team category.');
  return {id:integer(t.id,65534,'team ID'),name:text(t.name,100,'team name'),city:text(t.city,100,'team city'),category:t.category,classicEndYearRaw:integer(t.classicEndYearRaw,255,'classic year')};
 });
 const teamMap=unique(teams);if(!teams.length)throw Error('Catalog has no teams.');
 const records=array(value.records,65535,'source seasons').map(row=>{
  if(!row||!teamMap.has(row.primaryTeamId)||row.secondaryTeamId!==null&&!teamMap.has(row.secondaryTeamId))throw Error('Unresolved source season team.');
  const totals=transfer.validate([{year:row.year,team:'source',totals:row.totals}])[0].totals;
  if(!row.awards||typeof row.awards!=='object'||Array.isArray(row.awards)||Object.keys(row.awards).length!==awards.fields.length||awards.fields.some(f=>!Object.hasOwn(row.awards,f.key)))throw Error('Incomplete source season awards.');
  awards.patch(new Uint8Array(64),row.awards);
  return {id:integer(row.id,65534,'season ID'),year:row.year,primaryTeamId:row.primaryTeamId,secondaryTeamId:row.secondaryTeamId,totals,awards:Object.fromEntries(awards.fields.map(f=>[f.key,row.awards[f.key]]))};
 });
 const recordMap=unique(records),references=new Set();
 const players=array(value.players,8192,'source players').map(p=>{
  if(!p)throw Error('Invalid source player.');
  const history=array(p.history,25,'player history'),contexts=array(p.teams,1024,'player team context'),groups=array(p.categories,6,'player categories');
  if(new Set(history).size!==history.length||new Set(contexts).size!==contexts.length||new Set(groups).size!==groups.length||!groups.length||groups.some(v=>!categories.has(v))||contexts.some(id=>!teamMap.has(id)))throw Error('Invalid source player context.');
  const years=new Set();for(const id of history){const row=recordMap.get(id);if(!row||years.has(row.year))throw Error('Missing or duplicate source player season.');years.add(row.year);references.add(id);}
  const result={id:integer(p.id,8191,'player ID'),firstName:text(p.firstName,100,'first name'),lastName:text(p.lastName,100,'last name'),categories:[...groups],teams:[...contexts],history:[...history]};
  if(unified){
   if(!p.data||!p.data.info||!Array.isArray(p.data.values)||p.data.values.length!==fieldKeys.length)throw Error('Incomplete source player values.');
   const data=dna.validate({format:'2kplayer',version:1,game:'NBA2K27',schemaId:dna.schemaId,name:[result.firstName,result.lastName].join(' ').trim().slice(0,80),info:p.data.info,fields:Object.fromEntries(fieldKeys.map((key,index)=>[key,p.data.values[index]]))});
   // Never permit game-specific appearance, sound or source directory IDs.
   if(Object.keys(data.info).some(key=>!['firstName','lastName','potential','positionId','secondaryPositionId','jerseyNumberRaw','weightLbs','birthDate','yearsPro'].includes(key)))throw Error('Unsupported source player detail.');
   result.data={info:data.info,values:[...p.data.values],warnings:array(p.data.warnings??[],30,'player warnings').map(w=>text(w,300,'player warning'))};
  }return result;
 });
 unique(players);if(!players.length||references.size!==records.length)throw Error('Unreferenced source seasons or empty player catalog.');
 return {format:value.format,version:1,game:'NBA2K23',source:{sha256:source.sha256,playerSchema:source.playerSchema,historySchema:source.historySchema},teams,records,players,...(unified?{targetSchemaId:dna.schemaId,fieldKeys:[...fieldKeys],unsupported:array(value.unsupported??[],30,'unsupported source sections').map(w=>text(w,300,'unsupported source section'))}:{})};
}
function parse(bytes){
 if(typeof bytes==='string')bytes=Buffer.from(bytes);if(!Buffer.isBuffer(bytes)||bytes.length<2||bytes.length>MAX_BYTES)throw Error('Stats catalogs must be between 2 bytes and 32 MB.');
 let value;try{value=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(bytes));}catch{throw Error('The .2k23stats file is not valid UTF-8 JSON.');}return validate(value);
}
const normalize=value=>value.normalize('NFKD').replace(/\p{M}/gu,'').toLowerCase().replace(/\s+/g,' ').trim();
function indexed(catalog){
 const teams=new Map(catalog.teams.map(t=>[t.id,t])),records=new Map(catalog.records.map(r=>[r.id,r]));
 const players=catalog.players.map(p=>{
  const rows=p.history.map(id=>records.get(id)),name=[p.firstName,p.lastName].filter(Boolean).join(' '),context=p.teams.map(id=>teams.get(id)).map(t=>[t.city,t.name].filter(Boolean).join(' '));
  return {...p,name,context,seasonCount:rows.length,firstYear:rows.length?Math.min(...rows.map(r=>r.year)):null,lastYear:rows.length?Math.max(...rows.map(r=>r.year)):null,search:normalize([name,...context,...p.categories].join(' '))};
 });return {teams,records,players,fieldKeys:catalog.fieldKeys||[],unsupported:catalog.unsupported||[],unified:catalog.format==='2k23-player-catalog'};
}
function search(index,request={}){
 const q=normalize(text(request.query??'',200,'search query')),offset=integer(request.offset??0,8192,'search offset'),limit=integer(request.limit??40,100,'page size');
 if(limit<1||request.category!==undefined&&!categories.has(request.category))throw Error('Invalid catalog search.');
 const matches=index.players.filter(p=>(index.unified||p.seasonCount)&&(!request.category||p.categories.includes(request.category))&&q.split(' ').every(term=>p.search.includes(term)));
 return {total:matches.length,offset,limit,players:matches.slice(offset,offset+limit).map(({search,history,data,...p})=>p)};
}
function playerHistory(index,id){
 const p=index.players.find(p=>p.id===integer(id,8191,'source player ID'));if(!p)throw Error('Source player is no longer available.');
 const data=p.data?dna.validate({format:'2kplayer',version:1,game:'NBA2K27',schemaId:dna.schemaId,name:p.name.slice(0,80),info:p.data.info,fields:Object.fromEntries(index.fieldKeys.map((key,i)=>[key,p.data.values[i]]))}):null;
 return {player:(({search,history,data,...value})=>value)(p),data,sectionCounts:sectionCounts(data),warnings:[...index.unsupported,...(p.data?.warnings||[])],seasons:p.history.map(id=>{const row=index.records.get(id);return {...row,primaryTeam:index.teams.get(row.primaryTeamId),secondaryTeam:row.secondaryTeamId===null?null:index.teams.get(row.secondaryTeamId),scoringWarning:row.totals.points!==2*row.totals.fgm+row.totals.threeMade+row.totals.ftm};}).sort((a,b)=>b.year-a.year)};
}
async function readStable(file){
 const handle=await fs.open(file,'r');try{
  const before=await handle.stat();if(!before.isFile()||before.size<2||before.size>MAX_BYTES)throw Error('Stats catalog file size is invalid.');
  const bytes=Buffer.alloc(before.size);let at=0;while(at<bytes.length){const r=await handle.read(bytes,at,bytes.length-at,at);if(!r.bytesRead)throw Error('Stats catalog was truncated.');at+=r.bytesRead;}
  const after=await handle.stat();if(before.size!==after.size||before.mtimeMs!==after.mtimeMs||before.ctimeMs!==after.ctimeMs)throw Error('Stats catalog changed while reading.');return bytes;
 }finally{await handle.close();}
}
class Player23Catalog {
 constructor(storageRoot,{bundledPath=path.join(__dirname,'../assets/player23-catalog/players.2k23catalog')}={}){this.root=path.resolve(storageRoot);this.directory=path.join(this.root,'Library','Player Imports');this.bundledPath=bundledPath;this.active=null;this.pending=null;}
 async checkDirectories(){for(const folder of [this.root,path.join(this.root,'Library'),this.directory]){try{const s=await fs.lstat(folder);if(!s.isDirectory()||s.isSymbolicLink())throw Error('Player import storage must be a regular directory.');}catch(e){if(e.code!=='ENOENT')throw e;}}}
 async directoryReady(){for(const folder of [this.root,path.join(this.root,'Library'),this.directory]){await fs.mkdir(folder,{recursive:true});const s=await fs.lstat(folder);if(!s.isDirectory()||s.isSymbolicLink())throw Error('Player import storage must be a regular directory.');}}
 async regular(file){try{const s=await fs.lstat(file);if(!s.isFile()||s.isSymbolicLink())throw Error('Player import catalog must be a regular file.');return true;}catch(e){if(e.code==='ENOENT')return false;throw e;}}
 summary(){return this.active?{loaded:true,bundled:this.active.bundled===true,catalogId:this.active.id,sourceSha256:this.active.catalog.source.sha256,players:this.active.catalog.players.length,playersWithHistory:this.active.index.players.filter(p=>p.seasonCount).length,seasons:this.active.catalog.records.length,unified:this.active.index.unified,mappedFields:this.active.index.fieldKeys.length}:{loaded:false};}
 async builtIn(){if(!this.bundledPath)return {loaded:false};if(!await this.regular(this.bundledPath))throw Error('The built-in 2K23 catalog is missing. Repair or update the App folder.');const bytes=await readStable(this.bundledPath),catalog=parse(bytes);this.active={id:hash(bytes),catalog,index:indexed(catalog),bundled:true};return this.summary();}
 async load(){
  if(this.active)return this.summary();if(this.pending)return this.pending;
  this.pending=(async()=>{await this.checkDirectories();const pointer=path.join(this.directory,'selected.json');if(!await this.regular(pointer))return this.builtIn();
   const selected=JSON.parse((await readStable(pointer)).toString());if(!/^[a-f0-9]{64}$/.test(selected.catalogId))throw Error('Invalid selected source catalog.');
   const file=path.join(this.directory,selected.catalogId+'.2k23catalog');if(!await this.regular(file))throw Error('Selected source catalog is missing.');
   const bytes=await readStable(file);if(hash(bytes)!==selected.catalogId)throw Error('Stored source catalog fingerprint changed.');
   const catalog=parse(bytes);this.active={id:selected.catalogId,catalog,index:indexed(catalog)};return this.summary();
  })();try{return await this.pending;}finally{this.pending=null;}
 }
 async import(file){
  if(this.pending)throw Error('Wait for catalog loading to finish.');
  // Validate completely before changing the stored selection.
  const catalog=parse(await readStable(file)),bytes=Buffer.from(JSON.stringify(catalog)),id=hash(bytes);
  await this.directoryReady();const target=path.join(this.directory,id+'.2k23catalog');
  if(await this.regular(target)){if(hash(await readStable(target))!==id)throw Error('Existing catalog fingerprint mismatch.');}
  else await fs.writeFile(target,bytes,{flag:'wx'});
  const pointer=path.join(this.directory,'selected.json'),temporary=path.join(this.directory,'selection-'+crypto.randomUUID()+'.tmp');
  await this.regular(pointer);try{await fs.writeFile(temporary,JSON.stringify({catalogId:id}),{flag:'wx'});await this.directoryReady();await this.regular(pointer);await fs.rename(temporary,pointer);}finally{await fs.unlink(temporary).catch(e=>{if(e.code!=='ENOENT')throw e;});}
  this.active={id,catalog,index:indexed(catalog)};return this.summary();
 }
 async checked(request){await this.load();if(!this.active||request.catalogId!==this.active.id)throw Error('Source catalog changed. Reopen the player search.');return this.active.index;}
 async search(request){return {catalogId:request.catalogId,...search(await this.checked(request),request)};}
 async player(request){return structuredClone({catalogId:request.catalogId,...playerHistory(await this.checked(request),request.playerId)});}
 async selection(request){
  const selected=playerHistory(await this.checked(request),request.playerId);
  const base=selected.data||{format:'2kplayer',version:1,game:'NBA2K27',schemaId:dna.schemaId,name:selected.player.name.slice(0,80),info:{},fields:{}};
  const sections=array(request.sections,20,'import sections');if(!sections.length||sections.some(s=>!dna.groups.includes(s)))throw Error('Choose at least one import section.');
  let data;
  if(sections.includes('seasons')){
   const ids=array(request.seasonIds,25,'selected seasons');if(!ids.length||new Set(ids).size!==ids.length)throw Error('Choose unique source seasons.');
   const available=new Map(selected.seasons.map(row=>[row.id,row]));
   const history=ids.map(id=>{const r=available.get(id);if(!r)throw Error('Selected season does not belong to this source player.');return {year:r.year,team:r.primaryTeam.name,secondaryTeam:r.secondaryTeam?.name??null,totals:r.totals,...(request.includeAwards===false?{}:{awards:r.awards})};});
   data=dna.subset({...base,version:2,seasonHistory:history},sections);
  }else data=dna.subset(base,sections);
  return {data,warnings:selected.warnings,sourcePlayer:selected.player,catalogId:request.catalogId};
 }
}
module.exports={MAX_BYTES,validate,parse,indexed,search,playerHistory,Player23Catalog};
