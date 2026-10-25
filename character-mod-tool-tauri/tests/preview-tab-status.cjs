const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');
const esbuild=require('../build-tools/node_modules/esbuild');
const finish=require('./test-watchdog.cjs')('Preview tab initialization');
const baseline=process.argv.includes('--baseline'),failures=[];
const source=esbuild.transformSync(fs.readFileSync(path.join(__dirname,'../web/ui/app.ts'),'utf8'),{loader:'ts',target:'es2022'}).code;
const tick=()=>new Promise(resolve=>setTimeout(resolve,10));
function setup(loaded){
  const statuses=[],requests=[],elements=new Map();let loads=0;
  const state={activeView:'full-swap',viewRequestId:0,engineLabel:'engine',health:{report:{}}};
  const c=vm.createContext({state,$:id=>{if(!elements.has(id))elements.set(id,{classList:{toggle(){}},setAttribute(){}});return elements.get(id);},
    window:{modelPreview:loaded?{resize(){}}:undefined},requestAnimationFrame:callback=>callback(),
    setPreviewStatus:value=>statuses.push(value),ensureModelPreviewRenderer:()=>new Promise((resolve,reject)=>requests.push({resolve,reject})),
    loadStagedPreview:()=>{loads++;return Promise.resolve();},refreshBodyFit(){},refreshHeadbandOptions(){},loadRosterLanguage(){},
  });
  vm.runInContext(source.slice(source.indexOf('function setActiveView('),source.indexOf('function rosterPlayerName(')),c);
  return{c,state,statuses,requests,loads:()=>loads};
}
async function check(label,run){try{await run();}catch(error){if(!baseline)throw new Error(label+': '+error.message,{cause:error});failures.push({label,error:error.message});}}
(async()=>{
  await check('A loaded renderer does not acquire a permanent Loading status',async()=>{
    const e=setup(true);e.c.setActiveView('preview');e.requests[0].resolve();await tick();assert.equal(e.statuses.length,0);assert.equal(e.loads(),1);
  });
  await check('Leaving Preview retires an old initializer success',async()=>{
    const e=setup(false);e.c.setActiveView('preview');e.c.setActiveView('body');e.requests[0].resolve();await tick();assert.equal(e.loads(),0);
  });
  await check('Leaving Preview retires an old initializer error',async()=>{
    const e=setup(false);e.c.setActiveView('preview');e.c.setActiveView('body');e.requests[0].reject(Error('Obsolete module failure'));await tick();assert(!e.statuses.includes('Obsolete module failure'));
  });
  await check('Preview-Body-Preview accepts only the latest continuation',async()=>{
    const e=setup(false);e.c.setActiveView('preview');e.c.setActiveView('body');e.c.setActiveView('preview');e.requests[1].resolve();e.requests[0].resolve();await tick();assert.equal(e.loads(),1);
  });
  await check('The current initialization error is still shown',async()=>{
    const e=setup(false);e.c.setActiveView('preview');e.requests[0].reject(Error('Current module failure'));await tick();assert(e.statuses.includes('Current module failure'));
  });
  if(baseline){assert(failures.length);console.log(JSON.stringify({baseline:true,failures},null,2));}
  else console.log('Preview tab: current status, latest initialization, stale success/error and current error passed.');
})().catch(error=>{console.error(error);process.exitCode=1}).finally(finish);
