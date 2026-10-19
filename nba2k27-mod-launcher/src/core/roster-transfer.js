'use strict';
const crypto = require('node:crypto');
const dna = require('./player-dna');
const schema = require('../data/player-field-schema.json');
const {validatePlayerEditInput, validateTeamEditInput} = require('./player-research');
const {maximum}=require('./player-field-edits');
const MAX_BYTES = 64 * 1024 * 1024;
const teamKeys = ['name','cityName','cityShort','stateShort','mainLogo','rosterTypeId','historicYearId','arenaId'];
const pick = (value, keys) => Object.fromEntries(keys.map(key => [key,value[key]]));
function coverage(players){
  const expected=schema.filter(f=>maximum(f)>=0),missingByField={},missingByInfo={};
  let incompletePlayers=0;
  for(const {data} of players){
    let incomplete=false;
    for(const f of expected)if(!Object.hasOwn(data.fields,f.key)){missingByField[f.key]=(missingByField[f.key]||0)+1;incomplete=true;}
    for(const key of dna.infoKeys)if(!Object.hasOwn(data.info,key)){missingByInfo[key]=(missingByInfo[key]||0)+1;incomplete=true;}
    if(incomplete)incompletePlayers++;
  }
  return {kind:'mapped-fields-only',completeNativeBackup:false,playerCount:players.length,
    expectedMappedFieldsPerPlayer:expected.length,expectedInfoFieldsPerPlayer:dna.infoKeys.length,
    incompletePlayers,missingByField,missingByInfo,
    excluded:['native-roster-file','player-creation-and-deletion','team-membership','lineups-and-rotations','season-history','award-history','staff','unmapped-native-fields'],
    warning:'Partial mapped-field export, not a complete roster backup. Missing values are unavailable, not zero. Keep an in-game roster save.'};
}
function identity(player, teams) {
  const team = teams.find(t => t.source === 'live-team' && t.recordIndex === player.teamIndex);
  return {firstName:player.firstName,lastName:player.lastName,cyberfaceId:player.cyberfaceId,teamId:team?.teamId ?? null};
}
function checkedIdentity(value) {
  if (!value || typeof value.firstName !== 'string' || typeof value.lastName !== 'string'
    || [value.firstName,value.lastName].some(n => n.length > 19 || /[\x00-\x1f]/.test(n))
    || !Number.isInteger(value.cyberfaceId) || value.cyberfaceId < 0 || value.cyberfaceId > 65535
    || (value.teamId !== null && (!Number.isInteger(value.teamId) || value.teamId < 0 || value.teamId > 65535))) throw Error('Invalid roster player identity.');
  return pick(value,['firstName','lastName','cyberfaceId','teamId']);
}
function validate(value) {
  if (!value || value.format !== '2kroster' || value.version !== 1 || value.game !== 'NBA2K27' || value.schemaId !== dna.schemaId)
    throw Error('Unsupported .2kroster format or field schema. Export again with this launcher.');
  if (!Array.isArray(value.players) || value.players.length > 8192 || !Array.isArray(value.teams) || value.teams.length > 1024 || !value.players.length)
    throw Error('Invalid roster record counts.');
  const players = value.players.map(row => {
    const data = dna.validate(row?.data);
    if (data.version !== 1) throw Error('Bulk season-history transfer is not supported. Use Player DNA for existing season rows.');
    return {match:checkedIdentity(row.match),data};
  });
  const teams = value.teams.map(row => {
    if (!row || !Number.isInteger(row.teamId) || row.teamId < 0 || row.teamId > 65535 || !row.data) throw Error('Invalid roster team.');
    for (const key of teamKeys) {
      const v=row.data[key];
      if (['rosterTypeId','historicYearId','arenaId'].includes(key) ? !Number.isInteger(v) : typeof v !== 'string' || /[\x00-\x1f]/.test(v)) throw Error('Invalid team field: '+key);
    }
    const checked=validateTeamEditInput({...row.data,recordIndex:0,teamId:row.teamId,expectedName:row.data.name});
    return {teamId:row.teamId,data:pick(checked,teamKeys)};
  });
  // Recompute coverage rather than trusting claims supplied by an imported file.
  return {format:'2kroster',version:1,game:'NBA2K27',schemaId:dna.schemaId,players,teams,coverage:coverage(players)};
}
function parse(text) {
  if (typeof text !== 'string' || Buffer.byteLength(text) > MAX_BYTES) throw Error('Roster files must be smaller than 64 MB.');
  let value;try {value=JSON.parse(text);} catch {throw Error('The .2kroster file is not valid JSON.');}return validate(value);
}
function snapshot(state) {
  if (state.status !== 'ready') throw Error('Refresh a live roster first.');
  const teams=(state.teams||[]).filter(t=>t.source==='live-team');
  const players=(state.players||[]).filter(p=>p.source==='live-roster');
  // Fail rather than silently dropping a record from a whole-roster export.
  return validate({format:'2kroster',version:1,game:'NBA2K27',schemaId:dna.schemaId,
    players:players.map(p=>({match:identity(p,teams),data:dna.fromPlayer(p)})),
    teams:teams.map(t=>({teamId:t.teamId,data:pick(t,teamKeys)}))});
}
function fingerprint(state) {
  return crypto.createHash('sha256').update(JSON.stringify({pid:state.pid,players:state.players?.filter(p=>p.source==='live-roster'),teams:state.teams?.filter(t=>t.source==='live-team')})).digest('hex');
}
function plan(input,state,sections,includeTeams=false) {
  if (!state || state.status !== 'ready' || !Number.isInteger(state.pid) || state.pid <= 0)
    throw Error('Refresh a live roster before previewing an import.');
  const value=validate(input);dna.subset(value.players[0].data,sections);
  if (sections.includes('seasons')) throw Error('Bulk seasons are not supported.');
  const live=(state.players||[]).filter(p=>p.source==='live-roster'),teams=(state.teams||[]).filter(t=>t.source==='live-team');
  const key=x=>JSON.stringify(x),targets=new Map(),sources=new Map();
  for (const p of live) {const k=key(identity(p,teams));targets.set(k,[...(targets.get(k)||[]),p]);}
  for (const row of value.players) {const k=key(row.match);sources.set(k,(sources.get(k)||0)+1);}
  const entries=[];
  for (const row of value.players) {
    const k=key(row.match),matches=targets.get(k)||[],entry={kind:'player',name:row.data.name,status:'unchanged',changes:[]};entries.push(entry);
    if(sources.get(k)>1||matches.length>1){entry.status='ambiguous';continue;}
    if(!matches.length){entry.status='missing';continue;}
    const p=matches[0];
    try {
      const before=dna.fromPlayer(p),selected=dna.subset(row.data,sections),info={...before.info,...selected.info};
      const edits=Object.entries(selected.fields).filter(([f,v])=>before.fields[f]!==v).map(([f,v])=>({index:schema.findIndex(s=>s.key===f),expected:p.ratingsRaw[schema.findIndex(s=>s.key===f)],value:v}));
      entry.changes=[...Object.entries(selected.info).filter(([f,v])=>before.info[f]!==v).map(([field,to])=>({field,from:before.info[field],to})),...edits.map(e=>({field:schema[e.index].key,from:e.expected,to:e.value}))];
      if(!entry.changes.length)continue;
      // Reject malformed destination records now, before the first batch write.
      for(const f of ['positionId','secondaryPositionId','jerseyNumberRaw','potentialRaw','heightRaw','wingspanRaw'])if(!Number.isInteger(p[f]))throw Error('Destination '+f+' is unavailable.');
      const bioExpected={...(info.birthDate!==undefined?{expectedBirthYear:p.birthYear,expectedBirthMonth:p.birthMonth,expectedBirthDay:p.birthDay}:{}),...(info.yearsPro!==undefined?{expectedYearsPro:p.yearsPro}:{})};
      entry.edit=validatePlayerEditInput({...info,...bioExpected,recordIndex:p.recordIndex,expectedFirstName:p.firstName,expectedLastName:p.lastName,expectedCyberfaceId:p.cyberfaceId,ratingEdits:edits});
      require('../renderer/player-bio').plan(entry.edit,p);
      entry.before={match:row.match,data:before};
      entry.recovery={match:{...row.match,firstName:info.firstName,lastName:info.lastName,cyberfaceId:info.cyberfaceId},data:before};
      entry.status='change';
    }catch(error){entry.status='blocked';entry.reason=error.message;delete entry.edit;}
  }
  if(includeTeams)for(const row of value.teams){
    const matches=teams.filter(t=>t.teamId===row.teamId),entry={kind:'team',name:row.data.name,status:'unchanged',changes:[]};entries.push(entry);
    if(matches.length>1||value.teams.filter(t=>t.teamId===row.teamId).length>1){entry.status='ambiguous';continue;}
    if(!matches.length){entry.status='missing';continue;}const t=matches[0];
    entry.changes=teamKeys.filter(k=>row.data[k]!==t[k]).map(field=>({field,from:t[field],to:row.data[field]}));
    if(entry.changes.length){entry.edit=validateTeamEditInput({...row.data,recordIndex:t.recordIndex,teamId:t.teamId,expectedName:t.name});entry.status='change';}
  }
  return {entries,fingerprint:fingerprint(state),coverage:value.coverage};
}
function preview(plan) {
  const rows=plan.entries.map(({kind,name,status,changes,reason})=>({kind,name,status,changes,reason}));
  return {rows,changes:rows.filter(r=>r.status==='change').length,skipped:rows.filter(r=>['missing','ambiguous','blocked'].includes(r.status)).length,coverage:plan.coverage};
}
module.exports={MAX_BYTES,validate,parse,snapshot,plan,preview,fingerprint};
