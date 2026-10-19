const assert=require('node:assert/strict');
const fs=require('node:fs'),vm=require('node:vm');const sandbox={module:{exports:{}}};vm.runInNewContext(fs.readFileSync(require('node:path').join(__dirname,'../browser-overrides/custom-arena-v2.js'),'utf8'),sandbox);const {prepare}=sandbox.module.exports;
const team={source:'live-team',recordIndex:1,teamId:7,name:'Fixture',nativeArenaCatalogId:'a'.repeat(64)};
const request={recordIndex:1,teamId:7,expectedTeamName:'Fixture',donorArenaId:17,targetArenaId:999,arenaName:'Court',arenaFile:'arena_999_int',floorFile:'arena_999_int_floor',expectedTargetName:''};
const value=prepare(request,team);assert.equal(value.catalogId,team.nativeArenaCatalogId);assert(!('expectedTargetName'in value));assert(!('catalogId'in request));
for(const changed of [{...team,source:'catalog'},{...team,recordIndex:2},{...team,teamId:8},{...team,name:'Other'},{...team,nativeArenaCatalogId:'stale'},null])assert.throws(()=>prepare(request,changed));
console.log('Custom arena request binding: 7 cases passed');
