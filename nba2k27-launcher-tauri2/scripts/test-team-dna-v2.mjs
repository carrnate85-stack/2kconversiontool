import assert from 'node:assert/strict';
import vm from 'node:vm';
import fs from 'node:fs';
import {createRequire} from 'node:module';
const box={module:{exports:{}}};vm.runInNewContext(fs.readFileSync(new URL('../browser-overrides/team-dna-v2.js',import.meta.url),'utf8'),box);const api=box.module.exports;
assert.deepEqual(JSON.parse(JSON.stringify(api.infoPlan({name:'new',rosterTypeId:6,historicYearId:117},{name:'old',rosterTypeId:0}, {infoFields:['name']}))),{values:{name:'new'},skipped:['rosterTypeId','historicYearId']});
assert.throws(()=>api.infoPlan({name:7},{name:'old'}));
const match={typeId:1,sideId:0,designationId:2},u={match,values:{home:2}};
assert.equal(api.uniformPlan([u],[u,u]).rows.length,0);
assert.throws(()=>api.uniformPlan([u,u],[u]),/duplicate/);
function harness(){
 const controls=new Map();for(const [id,value] of Object.entries({teamDnaStatus:'',teamEditName:'Original',teamEditCity:'City',teamEditCityShort:'CT',teamEditStateShort:'ST',teamEditLogo:'logo',teamDnaImport:'',teamDnaExport:''}))controls.set(id,{value,textContent:'',disabled:false});
 controls.set('teamDnaSections',{querySelectorAll:()=>[{value:'info'},{value:'staff'},{value:'rotations'},{value:'uniforms'}]});
 controls.set('teamEditorModal',{classList:{contains:()=>false}});
 const team={source:'live-team',teamId:3,editCapabilities:{infoFields:['name','cityName','cityShort','stateShort','mainLogo']}},calls=[];
 const imported={format:'2kteam',name:'Source',info:{name:'Imported',rosterTypeId:6},staff:[{slot:0,values:{}}],rotations:[['A']],uniforms:[u],arena:{name:'Unsupported'}};
 let confirm=true,resolveFile=null,failRotation=false,duringReview=()=>{};
 const staff={load:async()=>{},snapshot:()=>[],prepareStage:async()=>({before:[],after:[]}),validateStage:()=>{},commitStage:()=>calls.push('staff')};
 const rotations={portable:async()=>[],prepareStage:async()=>{if(failRotation)throw Error('Automatic rotation');return {before:[],after:[]};},validateStage:()=>{},commitStage:()=>calls.push('rotations')};
 const uniforms={snapshot:()=>[u],stage:()=>{calls.push('uniforms');return [];}};
 const context={document:{getElementById:id=>controls.get(id)},state:{editingTeam:team},teamDnaBusy:false,teamUniformBusy:false,teamStaffView:staff,teamRotationView:rotations,teamUniformView:uniforms,openTeamEditor(t){context.state.editingTeam=t;},transferTeamDna(){},window:{modLauncher:{teamDna:async()=>resolveFile?await new Promise(resolve=>resolveFile(resolve)):imported},ImportReview:{diff:()=>[],confirm:async()=>{duringReview();return confirm;}}},console};
 vm.createContext(context);vm.runInContext(fs.readFileSync(new URL('../browser-overrides/team-dna-v2.js',import.meta.url),'utf8'),context);
 return {context,controls,calls,team,onReview:fn=>duringReview=fn,setConfirm:v=>confirm=v,setFailRotation:v=>failRotation=v,defer(fn){resolveFile=fn;}};
}
{
 const h=harness();await h.context.transferTeamDna('import');assert.deepEqual(h.calls,['staff','rotations','uniforms']);assert.equal(h.controls.get('teamEditName').value,'Imported');assert.match(h.controls.get('teamDnaStatus').textContent,/rosterTypeId/);assert.match(h.controls.get('teamDnaStatus').textContent,/Nothing has been written/);
}
{
 const h=harness();h.setConfirm(false);await h.context.transferTeamDna('import');assert.deepEqual(h.calls,[]);assert.equal(h.controls.get('teamEditName').value,'Original');
}
{
 const h=harness();h.setFailRotation(true);await h.context.transferTeamDna('import');assert.deepEqual(h.calls,[]);assert.equal(h.controls.get('teamEditName').value,'Original');assert.match(h.controls.get('teamDnaStatus').textContent,/Automatic rotation/);
}
{
 const h=harness();let finish;h.defer(resolve=>finish=resolve);const pending=h.context.transferTeamDna('import');h.context.openTeamEditor(h.team);finish({info:{name:'Stale'},staff:[],uniforms:[]});await pending;assert.deepEqual(h.calls,[]);assert.equal(h.controls.get('teamEditName').value,'Original');
}
{
 const h=harness();h.onReview(()=>h.controls.get('teamEditName').value='New local draft');await h.context.transferTeamDna('import');assert.deepEqual(h.calls,[]);assert.equal(h.controls.get('teamEditName').value,'New local draft');assert.match(h.controls.get('teamDnaStatus').textContent,/drafts changed/);
}
console.log('TeamDNA bounded plans, cancellation, atomic preflight and same-object reentry: passed');
