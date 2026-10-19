const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm'),path=require('node:path');
const source=fs.readFileSync(path.join(__dirname,'../frontend/browser-v2-readonly.js'),'utf8');
function fixture(search='?readonly=v2') {
  const normal={id:'playerEditFirstName',disabled:false},rating={id:'studioField3',disabled:false},filter={id:'ratingSearch',disabled:false},nav={id:'playerQuickSwitch',disabled:false};
  const save={id:'savePlayerEdit',textContent:'Save & verify',disabled:false},back={id:'closePlayerEditor',textContent:'Cancel',disabled:false},imp={id:'player23Import',textContent:'Import from NBA 2K23',disabled:false};
  const player={querySelectorAll:s=>s==='button'?[save,back,imp]:[normal,rating,filter,nav]},boundary={textContent:'Editable'},events={},observers=[];
  const window={modLauncher:{savePlayer:async()=>{throw Error('Must not be reached');},playerResearchStatus:async()=>({readOnly:true})},addEventListener:(name,fn)=>events[name]=fn};
  vm.runInNewContext(source,{window,location:{search},URLSearchParams,queueMicrotask:fn=>fn(),document:{getElementById:id=>({playerEditorModal:player,playerEditBoundary:boundary})[id]},MutationObserver:class{constructor(fn){observers.push(fn);}observe(){}}});
  return {window,normal,rating,filter,nav,save,back,imp,boundary,events,observers};
}
test('read-only guard preserves filters/navigation and disables edits and imports',async()=>{const f=fixture();assert.ok(f.normal.disabled&&f.rating.disabled&&f.save.disabled&&f.imp.disabled);assert.equal(f.filter.disabled,false);assert.equal(f.nav.disabled,false);assert.equal(f.back.disabled,false);assert.match(f.boundary.textContent,/read-only/);await assert.rejects(f.window.modLauncher.savePlayer({}),/read-only/);assert.deepEqual(await f.window.modLauncher.playerResearchStatus(),{readOnly:true});});
test('player reopens and dynamic panels cannot re-enable game writes',()=>{const f=fixture();f.rating.disabled=false;f.events['player-editor:open']();assert.equal(f.rating.disabled,true);f.normal.disabled=false;f.observers[0]();assert.equal(f.normal.disabled,true);});
test('legacy and preview workspaces remain unchanged without explicit V2 flag',()=>{const f=fixture('?preview=player');assert.equal(f.normal.disabled,false);assert.equal(f.save.disabled,false);assert.equal(f.imp.disabled,false);assert.equal(Object.keys(f.events).length,0);});
