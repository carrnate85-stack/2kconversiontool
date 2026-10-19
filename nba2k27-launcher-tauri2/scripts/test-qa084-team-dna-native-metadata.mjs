import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const source=fs.readFileSync(new URL('../browser-overrides/team-dna-v2.js',import.meta.url),'utf8');
function load(file){const box={module:{exports:{}}};vm.runInNewContext(fs.readFileSync(new URL(file,import.meta.url),'utf8'),box);return box.module.exports;}
const api=load('../browser-overrides/team-dna-v2.js'),metadata=load('../browser-overrides/team-metadata-v2.js');
const names=Array.from({length:36},(_,i)=>metadata.kindLabel(i)),caps={rosterTypeId:true,classicTeamEndYearRaw:true,arena:true};
const arena={name:'Court',arenaFile:'arena_court',floorFile:'floor_court',arenaId:13};
const portable={format:'nba2k27-native-team-metadata',version:1,rosterTypeName:'WNBA',classicSeasonEndYear:2017,arena:{name:arena.name,arenaFile:arena.arenaFile,floorFile:arena.floorFile}};
const plain=x=>JSON.parse(JSON.stringify(x));
function plan(value=portable,c=caps,arenas=[arena],raw=117){return plain(api.metadataPlan(value,{},c,arenas,names,metadata.yearChoices(raw)));}
assert.deepEqual(plan().values,{type:'29',year:'117',arena:'13'});
for(const [raw,year] of [[1,2001],[101,2001],[17,2017],[117,2017],[27,2027],[127,2027],[0,null],[100,2000]])assert.equal(plan({...portable,classicSeasonEndYear:year},caps,[arena],raw).values.year,String(raw));
assert.equal(plan({...portable,rosterTypeName:'COED'}).values.type,'35');
assert.deepEqual(plan(portable,{}).values,{});assert.equal(plan(portable,{}).warnings.length,3);
for(const arenas of [[],[arena,{...arena,arenaId:2}],[{...arena,floorFile:'other'}]]){assert.equal(plan(portable,caps,arenas).values.arena,undefined);assert.match(plan(portable,caps,arenas).warnings[0],/missing or ambiguous/);}
assert.equal(plan({...portable,rosterTypeName:'UNKNOWN'}).values.type,undefined);
assert.throws(()=>plan({...portable,version:2}),/Unsupported/);
assert.deepEqual(plan(null).values,{});

function harness(){
 const controls=new Map(),calls=[];
 for(const [id,value] of Object.entries({teamEditName:'Original',teamEditCity:'City',teamEditCityShort:'CT',teamEditStateShort:'ST',teamEditLogo:'logo',teamEditRosterType:'0',teamNativeYearChoice:'117',teamEditArenaModel:'4',teamDnaStatus:'',teamDnaImport:'',teamDnaExport:''}))controls.set(id,{value,textContent:'',disabled:false,dispatchEvent(e){calls.push('event:'+e.type);}});
 controls.set('teamDnaSections',{querySelectorAll:()=>[{value:'info'},{value:'arena'}]});controls.set('teamEditorModal',{classList:{contains:()=>false}});
 const team={source:'live-team',nativeArenaId:4,nativeArenaCatalogId:'native-current-catalog',editCapabilities:{...caps,infoFields:['name','cityName','cityShort','stateShort','mainLogo']}};
 let confirm=true,duringReview=()=>{},pendingResolve=null,captured;
 const imported={name:'Portable',info:{name:'Imported',rosterTypeId:6,historicYearId:17},nativeMetadata:portable,staff:[],uniforms:[]};
 const context={document:{getElementById:id=>controls.get(id)},state:{editingTeam:team,playerResearch:{arenas:[arena]}},teamDnaBusy:false,teamUniformBusy:false,teamStaffView:{},teamRotationView:{},teamUniformView:{snapshot:()=>[],stage:()=>{throw Error('unexpected uniform stage');}},openTeamEditor(t){context.state.editingTeam=t;},transferTeamDna(){},Event:class{constructor(type){this.type=type;}},window:{NativeTeamMetadata:metadata,modLauncher:{teamDna:async request=>{captured=request;if(request.action==='export')return {};return pendingResolve?await new Promise(resolve=>pendingResolve(resolve)):imported;}},ImportReview:{diff:(before,after)=>[{before,after}],confirm:async()=>{duringReview();return confirm;}}}};
 vm.createContext(context);vm.runInContext(source,context);
 return {context,controls,calls,team,get exported(){return captured;},setConfirm:v=>confirm=v,onReview:fn=>duringReview=fn,defer(fn){pendingResolve=fn;}};
}
{
 const h=harness();await h.context.transferTeamDna('import');assert.equal(h.controls.get('teamEditRosterType').value,'29');assert.equal(h.controls.get('teamNativeYearChoice').value,'117');assert.equal(h.controls.get('teamEditArenaModel').value,'13');assert.deepEqual(h.calls,['event:change']);assert.match(h.controls.get('teamDnaStatus').textContent,/Nothing has been written/);
}
for(const mutation of [h=>h.controls.get('teamNativeYearChoice').value='18',h=>h.team.nativeArenaCatalogId='replaced',h=>h.context.state.playerResearch.arenas.push({...arena,arenaId:15}),h=>h.controls.get('teamEditRosterType').disabled=true]){
 const h=harness();h.onReview(()=>mutation(h));await h.context.transferTeamDna('import');assert.equal(h.controls.get('teamEditName').value,'Original');assert.equal(h.controls.get('teamEditArenaModel').value,'4');assert.deepEqual(h.calls,[]);assert.match(h.controls.get('teamDnaStatus').textContent,/drafts changed/);
}
{
 const h=harness();h.setConfirm(false);await h.context.transferTeamDna('import');assert.equal(h.controls.get('teamEditRosterType').value,'0');assert.deepEqual(h.calls,[]);
}
{
 const h=harness();let finish;h.defer(resolve=>finish=resolve);const pending=h.context.transferTeamDna('import');h.context.openTeamEditor(h.team);finish({info:{name:'Stale'},nativeMetadata:portable,staff:[],uniforms:[]});await pending;assert.equal(h.controls.get('teamEditName').value,'Original');assert.deepEqual(h.calls,[]);
}
{
 const h=harness();h.controls.get('teamEditRosterType').value='29';h.controls.get('teamEditArenaModel').value='13';await h.context.transferTeamDna('export');const value=plain(h.exported.data.nativeMetadata);assert.deepEqual(value,portable);assert.equal(JSON.stringify(value).includes('catalog'),false);assert.equal(Object.hasOwn(value.arena,'arenaId'),false);
}
console.log('QA084 TeamDNA semantic metadata, duplicate years, unique arena, capability/epoch/draft/cancel/no-write checks PASS');
