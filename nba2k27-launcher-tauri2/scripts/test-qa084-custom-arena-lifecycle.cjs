const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
function fixture(){
 const nodes=new Map(),calls=[],profileCalls=[],toasts=[],rendered=[];let closed=0;
 function node(tag){return {tag,children:[],listeners:{},value:'',textContent:'',disabled:false,hidden:false,open:false,classList:{contains(){return this.owner.hidden;},owner:null},append(...children){this.children.push(...children);},replaceChildren(...children){this.children=children;},addEventListener(k,f){(this.listeners[k]??=[]).push(f);},insertAdjacentElement(_where,n){this.after=n;},showModal(){this.open=true;},close(){this.open=false;for(const f of this.listeners.close||[])f();},remove(){this.removed=true;}};}
 function make(tag){const n=node(tag);n.classList.owner=n;return n;}
 const body=make('body'),document={getElementById:id=>nodes.get(id),createElement:make,body};
 for(const id of ['createCustomArena','teamEditorModal','customArenaDonor','customArenaTarget','customArenaName','customArenaFile','customArenaFloor'])nodes.set(id,make(id));
 for(const[id,value]of Object.entries({customArenaDonor:'17',customArenaTarget:'999',customArenaName:'Court',customArenaFile:'arena_custom',customArenaFloor:'floor_custom'}))nodes.get(id).value=value;
 const team={source:'live-team',recordIndex:1,teamId:7,name:'Fixture',nativeArenaCatalogId:'a'.repeat(64)};
 let clone=async()=>({report:{ok:true,readBackVerified:true},playerResearch:{status:'ready'}}),profiles=async()=>({profiles:[]});
 const root={openTeamEditor(){nodes.get('teamEditorModal').hidden=false;},closeTeamEditor(){closed++;nodes.get('teamEditorModal').hidden=true;},toast:(text,error)=>toasts.push({text,error}),renderPlayerResearch:v=>rendered.push(v),modLauncher:{cloneArena:async r=>{calls.push(r);return clone(r);},customArenaProfiles:r=>{profileCalls.push(r);return profiles(r);}}};
 const sandbox={module:{exports:{}},document};vm.runInNewContext(fs.readFileSync(require('node:path').join(__dirname,'../browser-overrides/custom-arena-v2.js'),'utf8'),sandbox);sandbox.module.exports.install(root);root.openTeamEditor(team);
 const click=()=>nodes.get('createCustomArena').listeners.click[0]({preventDefault(){},stopImmediatePropagation(){}});
 return {root,team,nodes,body,calls,profileCalls,toasts,rendered,click,closed:()=>closed,setClone:f=>clone=f,setProfiles:f=>profiles=f};
}
test('normal native verified receipt renders/close once; duplicate click never dispatches twice',async()=>{
 const f=fixture();let resolve;f.setClone(()=>new Promise(r=>resolve=r));const first=f.click();await f.click();assert.equal(f.calls.length,1);assert.equal(f.nodes.get('createCustomArena').disabled,true);
 resolve({report:{ok:true,readBackVerified:true},playerResearch:{status:'ready'}});await first;assert.equal(f.closed(),1);assert.equal(f.rendered.length,1);assert.equal(f.nodes.get('createCustomArena').disabled,false);assert.equal(f.toasts[0].error,false);
});
test('canceled and failed/unverified receipts never present successful clone',async()=>{
 for(const reply of [{canceled:true},null,{}, {report:{ok:false,readBackVerified:true}},{report:{ok:true,readBackVerified:false}}]){const f=fixture();f.setClone(async()=>reply);await f.click();assert.equal(f.closed(),0);assert.equal(f.rendered.length,0);assert.ok(!f.toasts.some(x=>x.error===false));assert.equal(f.nodes.get('createCustomArena').disabled,false);}
});
test('same-object reopen, different-object visit and close discard late verified result',async()=>{
 for(const change of ['same','different','close']){const f=fixture();let resolve;f.setClone(()=>new Promise(r=>resolve=r));const pending=f.click();
  if(change==='close')f.root.closeTeamEditor();else{f.root.closeTeamEditor();f.root.openTeamEditor(change==='same'?f.team:{...f.team});}
  const closed=f.closed();resolve({report:{ok:true,readBackVerified:true},playerResearch:{old:true}});await pending;assert.equal(f.closed(),closed);assert.equal(f.rendered.length,0);assert.equal(f.toasts.length,0);
 }
});
test('late failure from prior visit cannot overwrite new visit status',async()=>{
 const f=fixture();let reject;f.setClone(()=>new Promise((_r,j)=>reject=j));const pending=f.click();f.root.closeTeamEditor();f.root.openTeamEditor(f.team);reject(Error('old transaction error'));await pending;assert.equal(f.toasts.length,0);assert.equal(f.nodes.get('createCustomArena').disabled,false);
});
test('profile-save failure is truthful after a verified native application',async()=>{
 const f=fixture();f.setClone(async()=>({report:{ok:true,readBackVerified:true},profileSaveError:'Arena applied; profile disk save failed.'}));await f.click();assert.equal(f.closed(),1);assert.deepEqual(f.toasts,[{text:'Arena applied; profile disk save failed.',error:true}]);
});
test('closed profile dialog never renders a late native read into detached UI',async()=>{
 const f=fixture();let resolve;f.setProfiles(()=>new Promise(r=>resolve=r));const pending=f.nodes.get('createCustomArena').after.listeners.click[0]();const dialog=f.body.children[0],list=dialog.children[2];dialog.close();resolve({profiles:[{id:'one',arenaName:'Late',expectedTeamName:'Old',targetArenaId:999,enabled:false}]});await pending;assert.equal(dialog.removed,true);assert.equal(list.children.length,0);
});

test('saved-profile read responses cannot resurrect a profile after a newer removal',async()=>{
 const f=fixture(),profile={id:'one',arenaName:'Court',expectedTeamName:'Fixture',targetArenaId:999,enabled:false};let reads=0,oldRead,newRead;
 f.setProfiles(async r=>{if(r.action!=='read')return {};reads++;if(reads===1)return {profiles:[profile]};return new Promise(resolve=>{if(reads===2)oldRead=resolve;else newRead=resolve;});});
 await f.nodes.get('createCustomArena').after.listeners.click[0]();const dialog=f.body.children[0],list=dialog.children[2],row=list.children[0];
 const enable=row.children[1].onclick();for(let n=0;n<8&&!oldRead;n++)await Promise.resolve();assert.ok(oldRead);
 const remove=row.children[2].onclick();for(let n=0;n<8&&!newRead;n++)await Promise.resolve();assert.ok(newRead);
 newRead({profiles:[]});await remove;oldRead({profiles:[{...profile,enabled:true}]});await enable;
 assert.equal(list.children.length,0);assert.match(list.textContent,/No saved custom arenas/);
});

const legacySchema='nba2k27-custom-arena-profiles-review-v1';
function legacyReply(){return {schema:legacySchema,legacyPresent:true,reviewToken:'c'.repeat(64),activation:false,memoryWrites:0,entries:[{id:'a'.repeat(64),index:0,valid:true,issues:[],profile:{arenaName:'First',expectedTeamName:'Team 1',targetArenaId:999,enabled:false}},{id:'b'.repeat(64),index:1,valid:false,issues:[{field:'arenaFile',message:'Invalid path stem'}],profile:null},{id:'d'.repeat(64),index:2,valid:true,issues:[],profile:{arenaName:'Third',expectedTeamName:'Team 3',targetArenaId:1001,enabled:false}}]};}
async function openLegacy(f,handler){f.setProfiles(async r=>r.action==='read'?{profiles:[],legacyReviewAvailable:true}:handler(r));await f.nodes.get('createCustomArena').after.listeners.click[0]();const dialog=f.body.children[0],list=dialog.children[2],note=dialog.children[1],reviewButton=list.children.find(n=>n.textContent==='Review WPF profiles');return {dialog,list,note,reviewButton};}
function panelParts(list){const panel=list.children.findLast(n=>n.tag==='div'&&!n.removed);return {panel,choices:panel.children.filter(n=>n.tag==='label').map(n=>n.children[0]),importButton:panel.children.find(n=>n.tag==='button')};}
test('legacy review displays invalid entries without autoselection; imports only chosen valid rows disabled',async()=>{
 const f=fixture(),review=legacyReply();const ui=await openLegacy(f,async r=>r.action==='reviewLegacy'?review:{schema:legacySchema,activation:false,memoryWrites:0,legacyUnchanged:true,importedCount:r.selectedIds.length,imported:r.selectedIds.map(id=>({sourceId:id,id:'e'.repeat(48),enabled:false}))});
 await ui.reviewButton.onclick();const {panel,choices,importButton}=panelParts(ui.list);assert.ok(choices.every(x=>x.checked===false));assert.equal(choices[1].disabled,true);assert.match(panel.children.find(n=>n.tag==='label'&&n.children[0]===choices[1]).children[1].textContent,/Invalid path/);
 await importButton.onclick();assert.equal(f.profileCalls.filter(r=>r.action==='importLegacy').length,0);assert.match(ui.note.textContent,/Select at least/);
 choices[2].checked=true;choices[1].checked=true;await importButton.onclick();const request=f.profileCalls.find(r=>r.action==='importLegacy');assert.deepEqual(JSON.parse(JSON.stringify(request)),{action:'importLegacy',reviewToken:review.reviewToken,selectedIds:['d'.repeat(64)]});assert.match(ui.note.textContent,/imported as disabled/);assert.equal(f.calls.length,0);
});
test('closed or replaced profile view discards a late legacy review',async()=>{
 for(const change of ['close','rerender']){const f=fixture();let finish;const ui=await openLegacy(f,r=>r.action==='reviewLegacy'?new Promise(resolve=>finish=resolve):{});const pending=ui.reviewButton.onclick();if(change==='close')ui.dialog.close();else{await f.nodes.get('createCustomArena').after.listeners.click[0]();ui.dialog.close();}finish(legacyReply());await pending;assert.equal(ui.list.children.filter(n=>n.tag==='div').length,0);assert.equal(f.profileCalls.filter(r=>r.action==='importLegacy').length,0);}
});
test('native source replacement rejection never reports import success or refreshes unrelated draft state',async()=>{
 const f=fixture(),ui=await openLegacy(f,async r=>{if(r.action==='reviewLegacy')return legacyReply();throw Error('Legacy profiles changed. Review them again before importing.');});await ui.reviewButton.onclick();const {choices,importButton}=panelParts(ui.list);choices[0].checked=true;await importButton.onclick();assert.match(ui.note.textContent,/Legacy profiles changed/);assert.equal(importButton.disabled,true);assert.equal(ui.reviewButton.disabled,false);assert.equal(f.profileCalls.filter(r=>r.action==='read').length,1);assert.equal(f.rendered.length,0);assert.equal(f.calls.length,0);
});
test('legacy import success requires an exact local-only selected-disabled receipt',async()=>{
 const chosen='a'.repeat(64),valid={schema:legacySchema,activation:false,memoryWrites:0,legacyUnchanged:true,importedCount:1,imported:[{sourceId:chosen,id:'e'.repeat(48),enabled:false}]};
 for(const reply of [{activation:false,memoryWrites:0},{...valid,activation:true},{...valid,memoryWrites:1},{...valid,importedCount:2},{...valid,imported:[{sourceId:'foreign',id:'e'.repeat(48),enabled:false}]},{...valid,imported:[{sourceId:chosen,id:'e'.repeat(48),enabled:true}]}]){const f=fixture(),ui=await openLegacy(f,async r=>r.action==='reviewLegacy'?legacyReply():reply);await ui.reviewButton.onclick();const {choices,importButton}=panelParts(ui.list);choices[0].checked=true;await importButton.onclick();assert.doesNotMatch(ui.note.textContent,/Selected profiles imported/);assert.equal(f.profileCalls.filter(r=>r.action==='read').length,1);}
});
test('closing during import discards the late reply and never enables or touches current editor',async()=>{
 const f=fixture();let finish;const ui=await openLegacy(f,r=>r.action==='reviewLegacy'?legacyReply():new Promise(resolve=>finish=resolve));await ui.reviewButton.onclick();const {choices,importButton}=panelParts(ui.list);choices[0].checked=true;const pending=importButton.onclick();assert.equal(importButton.disabled,true);ui.dialog.close();finish({schema:legacySchema,activation:false,memoryWrites:0,legacyUnchanged:true,importedCount:1,imported:[{sourceId:'a'.repeat(64),id:'e'.repeat(48),enabled:false}]});await pending;assert.equal(f.profileCalls.filter(r=>r.action==='read').length,1);assert.equal(f.profileCalls.filter(r=>r.action==='enable').length,0);assert.equal(f.closed(),0);assert.equal(f.rendered.length,0);
});

test('legacy review/import duplicate callbacks and detached old panel cannot send additional writes',async()=>{
 const f=fixture();let finishReview,finishImport,reviews=0;const ui=await openLegacy(f,r=>{if(r.action==='reviewLegacy'){reviews++;if(reviews===1)return new Promise(resolve=>finishReview=resolve);return legacyReply();}return new Promise(resolve=>finishImport=resolve);});
 const firstReview=ui.reviewButton.onclick();await ui.reviewButton.onclick();assert.equal(f.profileCalls.filter(r=>r.action==='reviewLegacy').length,1);finishReview(legacyReply());await firstReview;
 const old=panelParts(ui.list);old.choices[0].checked=true;const firstImport=old.importButton.onclick();await old.importButton.onclick();assert.equal(f.profileCalls.filter(r=>r.action==='importLegacy').length,1);await ui.reviewButton.onclick();assert.equal(f.profileCalls.filter(r=>r.action==='reviewLegacy').length,1);
 finishImport({activation:true,memoryWrites:1});await firstImport;assert.equal(ui.reviewButton.disabled,false);await ui.reviewButton.onclick();assert.equal(old.panel.removed,true);await old.importButton.onclick();assert.equal(f.profileCalls.filter(r=>r.action==='importLegacy').length,1);
});
test('malformed legacy review receipt never creates selectable import authority',async()=>{
 for(const review of [{...legacyReply(),schema:'foreign'},{...legacyReply(),reviewToken:'invalid'},{...legacyReply(),entries:Array(51).fill(legacyReply().entries[0])}]){const f=fixture(),ui=await openLegacy(f,async()=>review);await ui.reviewButton.onclick();assert.equal(ui.list.children.filter(n=>n.tag==='div').length,0);assert.equal(f.profileCalls.filter(r=>r.action==='importLegacy').length,0);assert.match(ui.note.textContent,/review changed/);}
});
