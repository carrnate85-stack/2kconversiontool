'use strict';
const path=require('node:path');
const crypto=require('node:crypto');
const fsp=require('node:fs/promises');
const {promisify}=require('node:util');
const {execFile}=require('node:child_process');
const run=promisify(execFile);
const {findManifestEntry}=require('./jukebox-manager');
const {BallLiveSelector,savedState}=require('./ball-live-selector');
const ballPreviews=require('../assets/blacktop-ball-previews/catalog.json');
const liveExclusions=require('../data/ball-live-exclusions.json');
const ballPreviewById=new Map(ballPreviews.items.map(item=>[item.id,item]));
const excludedLiveBalls=new Set(liveExclusions.ids);

const BALL_TARGET='balls/ball_gameplay_official_nba_nba_official.iff';
const STOCK_DEFAULT='balls/ball_gameplay_official_default.iff';
const BALL_TARGETS=Object.freeze([STOCK_DEFAULT,BALL_TARGET]);
const MAX_MANIFEST_BYTES=64*1024*1024;
const MAX_BALL_BYTES=64*1024*1024;
const SESSION_SCHEMA='blacktop-ball-selection-session-v1';
const PAUSED_BALL_MESSAGE='Live ball swaps are temporarily paused while Blacktop stability is investigated. Your choice is saved, but the installed ball stays unchanged.';
const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const optional=target=>fsp.readFile(target).catch(error=>error.code==='ENOENT'?null:Promise.reject(error));

function title(value){return String(value||'').split('_').filter(Boolean).map(word=>word.length<4&&/^nba|wnba$/i.test(word)?word.toUpperCase():word[0].toUpperCase()+word.slice(1)).join(' ');}
function describeBall(logicalPath){
 const stem=path.posix.basename(logicalPath,'.iff').replace(/^ball_(?:gameplay_)?(?:official_)?/i,'');
 const parts=stem.split('_').filter(Boolean);let group='Other';
 if(logicalPath===BALL_TARGET)return {label:'NBA Official',group:'League'};
 if(logicalPath===STOCK_DEFAULT)return {label:'2K Default',group:'League'};
 if(/^wnba_/.test(stem))return {label:title(parts.slice(1).join('_')||'WNBA Official'),group:'League'};
 const groups={allstar:'All-Star',animal:'Animals',camo:'Camo',cloth:'Materials',country:'Countries',emoji:'Emoji',event:'Events',gem:'Gems',holiday:'Holiday',nba:'League',wnba:'League',pattern:'Patterns',season:'Seasons',solid:'Colors',team:'Teams'};
 const key=parts[0]?.toLowerCase();if(groups[key]){group=groups[key];parts.shift();}
 return {label:title(parts.join('_')||stem),group};
}
function previewCandidates(logicalPath){
 const stem=path.posix.basename(logicalPath,'.iff').replace(/^ball_(?:gameplay_)?(?:official_)?/i,''),parts=stem.split('_').filter(Boolean),category=parts[0],rest=parts.slice(1).join('_'),last=parts.at(-1),reversed=parts.length===2?`${parts[1]}_${parts[0]}_ball`:'';
 const names=[stem,`ball_${stem}`,`${stem}_ball`,rest,rest&&`ball_${category}_${rest}`,rest&&`${category}_${rest}_ball`,rest&&`${rest}_ball`,rest&&`ball_${rest}`,rest&&`ai_ball_texture_${rest}`,`ai_ball_texture_${stem}`,reversed];
 if(category==='food')names.unshift(`${last}_fruit_ball`);
 if(category==='cloth')names.unshift(`ai_ball_texture_${rest}`);
 if(category==='country'){const country=parts.slice(parts[1]==='flag'?2:1).join('_');names.push(`country_${country}_ball`,`${country}_ball`);}
 if(category==='animal')names.push(`${last}_ball`,`${last}_skin_ball`,`${last}_scale_ball`);
 return [...new Set(names.filter(Boolean).map(name=>`store/thumbnails/${name}.iff`))];
}
async function plain(target){
 const absolute=path.resolve(target),root=path.parse(absolute).root;let current=root;
 for(const segment of absolute.slice(root.length).split(path.sep).filter(Boolean)){
  current=path.join(current,segment);const stat=await fsp.lstat(current).catch(error=>error.code==='ENOENT'?null:Promise.reject(error));
  if(stat?.isSymbolicLink())throw Error('Ball selection refuses linked files or folders.');
 }
}
async function atomicWrite(target,bytes){
 await plain(target);await fsp.mkdir(path.dirname(target),{recursive:true});const temporary=target+'.'+crypto.randomUUID()+'.tmp';
 try{await fsp.writeFile(temporary,bytes,{flag:'wx'});await fsp.rename(temporary,target);}finally{await fsp.rm(temporary,{force:true}).catch(()=>{});}
}
class BallSelectionManager{
 constructor({statePath,decoderPath,helperPath,directory=path.dirname(statePath),settings,publish=()=>{},decode=null,thumbnailService=null,isGameRunning=async()=>false,liveSelector=null,liveSubstitutionEnabled=true,requireExplicitBallSelection=false}){
  this.statePath=path.resolve(statePath);this.decoderPath=decoderPath;this.settings=settings;this.publish=publish;
  this.decode=decode||this.decodeStock.bind(this);this.thumbnailService=thumbnailService;this.isGameRunning=isGameRunning;this.liveSubstitutionEnabled=liveSubstitutionEnabled;this.requireExplicitBallSelection=requireExplicitBallSelection;this.selected='current';this.loaded=false;this.catalogCache=null;this.session=null;this.tail=Promise.resolve();this.blacktopSelection=false;
  this.sessionRoot=path.join(path.dirname(this.statePath),'ball-selection-session');this.marker=path.join(this.sessionRoot,'session.json');
  this.state={selected:'current',phase:'saved',message:'Current installed ball will be used.'};
  this.liveSelector=liveSelector||new BallLiveSelector({directory,helperPath,publish:value=>this.emit({phase:value.phase,message:value.message})});
 }
 async load(){if(this.loaded)return;this.loaded=true;try{const value=JSON.parse(await fsp.readFile(this.statePath,'utf8'));if(value?.schema==='blacktop-ball-selection-v1'&&typeof value.selected==='string')this.selected=value.selected;}catch(error){if(error.code!=='ENOENT'&&error.name!=='SyntaxError')throw error;}if(this.requireExplicitBallSelection&&this.selected!=='current'){this.selected='current';await this.persist();}}
 savedStatus(){return this.liveSubstitutionEnabled?savedState(this.selected):{phase:'saved',message:PAUSED_BALL_MESSAGE};}
 async catalog(gameRoot){
  const manifest=path.join(path.resolve(gameRoot),'manifest'),stat=await fsp.lstat(manifest);if(!stat.isFile()||stat.isSymbolicLink()||stat.size<1||stat.size>MAX_MANIFEST_BYTES)throw Error('The NBA 2K27 ball catalog is unavailable.');
  const stamp=`${manifest}:${stat.size}:${stat.mtimeMs}`;if(this.catalogCache?.stamp===stamp)return this.catalogCache.choices;
  const text=await fsp.readFile(manifest,'utf8'),lines=text.split(/\r?\n/),thumbnails=new Set(lines.map(line=>line.slice(0,line.indexOf(',')).replaceAll('\\','/').toLowerCase()).filter(name=>/^store\/thumbnails\/[a-z0-9_.-]+\.iff$/.test(name))),seen=new Set(),choices=[{id:'current',label:'Current installed',group:'Installed'}];
  for(const line of lines){const name=line.slice(0,line.indexOf(',')).replaceAll('\\','/');if(!/^balls\/ball_[a-z0-9_.-]+\.iff$/i.test(name))continue;const id=name.toLowerCase();if(seen.has(id)||excludedLiveBalls.has(id))continue;seen.add(id);const preview=previewCandidates(id).find(candidate=>thumbnails.has(candidate)),local=ballPreviewById.get(id);choices.push({id,...describeBall(id),...(preview?{preview}: {}),...(local?{thumbnail:local.thumbnail,...(local.renderNote?{renderNote:local.renderNote}:{})}:{})});}
  choices.splice(1,choices.length-1,...choices.slice(1).sort((a,b)=>a.group.localeCompare(b.group)||a.label.localeCompare(b.label)||a.id.localeCompare(b.id)));
  this.catalogCache={stamp,choices};return choices;
 }
 async view(){
  await this.load();if(this.state.phase==='saved'||!this.liveSubstitutionEnabled&&['restored','ended','current'].includes(this.state.phase))this.state={...this.state,...this.savedStatus()};const settings=await this.settings();const choices=await this.catalog(settings.gameRoot);
  if(!choices.some(choice=>choice.id===this.selected)){this.selected='current';await this.persist();}
  return {...this.state,selected:this.selected,choices};
 }
 emit(patch){this.state={...this.state,...patch,selected:this.selected};const value={...this.state};this.publish(value);return value;}
 async persist(){await fsp.mkdir(path.dirname(this.statePath),{recursive:true});const temporary=this.statePath+'.'+crypto.randomUUID()+'.tmp';try{await fsp.writeFile(temporary,JSON.stringify({schema:'blacktop-ball-selection-v1',selected:this.selected},null,2),'utf8');await fsp.rename(temporary,this.statePath);}finally{await fsp.rm(temporary,{force:true}).catch(()=>{});}}
 async select(id){
  const operation=this.tail.then(async()=>{await this.load();const settings=await this.settings(),choices=await this.catalog(settings.gameRoot),choice=choices.find(item=>item.id===id);if(!choice)throw Error('Unknown Blacktop ball.');this.selected=choice.id;await this.persist();
   if(!this.session||!this.liveSubstitutionEnabled)return this.emit(this.savedStatus());
   const value=this.liveSelector.set(choice.id,this.blacktopSelection);
   return this.emit({phase:value.phase,message:value.message});
  });this.tail=operation.catch(()=>{});return operation;
  }
 async thumbnail(id){const settings=await this.settings(),choices=await this.catalog(settings.gameRoot),choice=choices.find(item=>item.id===id);if(!choice)throw Error('Unknown Blacktop ball.');if(!choice.preview||!this.thumbnailService)return {state:'missing'};return this.thumbnailService.get(settings.gameRoot,choice.preview);}
 async decodeStock(gameRoot,logicalPath,outputPath){const entry=await findManifestEntry(gameRoot,logicalPath),decoder=typeof this.decoderPath==='function'?this.decoderPath():this.decoderPath;await run(decoder,[path.join(gameRoot,entry.archive),String(entry.offset),String(entry.size),path.join(gameRoot,'data','oodle','oo2core_9_win64.dll'),outputPath],{windowsHide:true,timeout:120000,maxBuffer:1024*1024});}
 async stagedBytes(logicalPath){
  if(!this.session)throw Error('No active ball-selection session.');const key=crypto.createHash('sha256').update(logicalPath).digest('hex').slice(0,20),file=path.join(this.sessionRoot,`${key}.iff`);
  const existing=await optional(file);if(existing?.length>0&&existing.length<=MAX_BALL_BYTES)return existing;
  await plain(this.sessionRoot);await fsp.mkdir(this.sessionRoot,{recursive:true});const temporary=file+'.'+crypto.randomUUID()+'.tmp';
  try{await this.decode(this.session.gameRoot,logicalPath,temporary);const stat=await fsp.lstat(temporary);if(!stat.isFile()||stat.isSymbolicLink()||stat.size<1||stat.size>MAX_BALL_BYTES)throw Error('The selected stock ball failed validation.');const bytes=await fsp.readFile(temporary);await atomicWrite(file,bytes);return bytes;}
  finally{await fsp.rm(temporary,{force:true}).catch(()=>{});}
 }
 async readJournal(){
  const raw=await optional(this.marker);if(!raw)return null;const value=JSON.parse(raw);
  if(value?.schema!==SESSION_SCHEMA||!path.isAbsolute(value.modsRoot||'')||value.target!==BALL_TARGET||!(value.before===null||/^[a-f0-9]{64}$/.test(value.before))||!Array.isArray(value.afters)||value.afters.length>128||value.afters.some(hash=>!/^[a-f0-9]{64}$/.test(hash))||typeof value.createdBalls!=='boolean')throw Error('Invalid Blacktop ball recovery journal. Originals retained.');
  return value;
 }
 async writeJournal(value){await atomicWrite(this.marker,Buffer.from(JSON.stringify(value,null,2)));}
 async installLoose(logicalPath){
  if(!this.session)throw Error('No active ball-selection session.');const bytes=await this.stagedBytes(logicalPath),after=sha(bytes),target=path.join(this.session.modsRoot,...BALL_TARGET.split('/'));await plain(target);
  let journal=await this.readJournal();
  if(!journal){const ballsFolder=path.dirname(target),createdBalls=!(await fsp.stat(ballsFolder).catch(error=>error.code==='ENOENT'?null:Promise.reject(error))),original=await optional(target),before=original?sha(original):null;
   if(original){const backup=path.join(this.sessionRoot,'original.iff'),saved=await optional(backup);if(saved&&sha(saved)!==before)throw Error('Blacktop ball backup conflict.');if(!saved)await atomicWrite(backup,original);}
   journal={schema:SESSION_SCHEMA,modsRoot:this.session.modsRoot,target:BALL_TARGET,createdBalls,before,afters:[after]};
  }else{const current=await optional(target),currentHash=current?sha(current):null;if(currentHash!==journal.before&&!journal.afters.includes(currentHash))throw Error('The installed ball changed outside the launcher; its backup was preserved.');if(!journal.afters.includes(after))journal.afters.push(after);}
  await this.writeJournal(journal);await atomicWrite(target,bytes);
 }
 async restoreLoose({allowRunning=false}={}){
  const journal=await this.readJournal();if(!journal)return;if(!allowRunning&&await this.isGameRunning())throw Error('Close NBA 2K27 before restoring the Blacktop ball.');
  const target=path.join(journal.modsRoot,...BALL_TARGET.split('/'));await plain(target);const current=await optional(target),currentHash=current?sha(current):null;
  if(currentHash!==journal.before&&!journal.afters.includes(currentHash))throw Error('The installed ball changed outside the launcher; it and its backup were left untouched.');
  if(journal.before){const backup=await optional(path.join(this.sessionRoot,'original.iff'));if(!backup||sha(backup)!==journal.before)throw Error('Blacktop ball backup failed verification; no files were restored.');await atomicWrite(target,backup);}else if(current)await fsp.rm(target);
  if(journal.createdBalls)await fsp.rmdir(path.dirname(target)).catch(error=>{if(!['ENOENT','ENOTEMPTY','EEXIST'].includes(error.code))throw error;});
  await fsp.rm(this.marker,{force:true});await fsp.rm(path.join(this.sessionRoot,'original.iff'),{force:true});
 }
 async prepare({gameRoot,modsRoot}){
  const selectionActive=this.blacktopSelection;await this.cleanup({resetObservation:false});this.blacktopSelection=selectionActive;await this.load();const choices=await this.catalog(gameRoot);if(!choices.some(choice=>choice.id===this.selected)){this.selected='current';await this.persist();}
  const root=path.resolve(modsRoot);await plain(root);this.session={gameRoot:path.resolve(gameRoot),modsRoot:root};
  this.emit(this.savedStatus());
  return {forcedOverrides:[],providerAliases:{},mutableProviderAliases:[]};
 }
 async arm(gameRoot){
  if(!this.session)throw Error('Prepare the Blacktop ball before launch.');
  if(!this.liveSubstitutionEnabled)return this.emit(this.savedStatus());
  return this.liveSelector.arm(path.resolve(gameRoot),this.selected,this.blacktopSelection);
 }
 observeBlacktop(value){
  if(typeof value?.selection!=='boolean')return Promise.resolve(this.state);
  const operation=this.tail.then(async()=>{
   if(value.selection){
    if(this.blacktopSelection)return this.state;this.blacktopSelection=true;
    if(this.session&&this.liveSubstitutionEnabled)this.liveSelector.set(this.selected,true);
    return this.state;
   }
   if(!this.blacktopSelection)return this.state;this.blacktopSelection=false;if(this.session&&this.liveSubstitutionEnabled)this.liveSelector.set(this.selected,false);
   // Leaving the selection screen may mean match loading, not leaving Blacktop.
   // Keep the last status until the native mode/resource observer reports it.
   return this.state;
  });this.tail=operation.catch(error=>this.emit({phase:'error',message:`Blacktop ball handoff failed: ${error.message}`}));return operation;
 }
 async cleanup({resetObservation=true}={}){await this.liveSelector.stop();if(this.liveSelector.child)throw Error('Ball restoration is still running. Close the game before retrying cleanup.');if(resetObservation)this.blacktopSelection=false;this.session=null;await this.restoreLoose();const entries=await fsp.readdir(this.sessionRoot).catch(error=>error.code==='ENOENT'?[]:Promise.reject(error));if(!entries.length)await fsp.rmdir(this.sessionRoot).catch(error=>{if(!['ENOENT','ENOTEMPTY','EEXIST'].includes(error.code))throw error;});}
}
module.exports={BallSelectionManager,BALL_TARGET,BALL_TARGETS,STOCK_DEFAULT,describeBall,previewCandidates};
