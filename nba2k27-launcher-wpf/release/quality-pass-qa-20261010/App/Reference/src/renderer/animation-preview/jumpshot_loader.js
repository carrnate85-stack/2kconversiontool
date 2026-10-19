// Serialize decoder work, coalesce rapid choices, and discard stale results.
export function liveShotLoader({load,apply,onError,onBusy,delay=220}){
  let revision=0,pending=null,active=false,timer=null,disposed=false;
  async function run(){
    if(disposed||active||!pending?.ready)return;
    const job=pending;pending=null;active=true;onBusy?.(true);
    try{const result=await load(job.selection);if(!disposed&&job.revision===revision)apply(result);}
    catch(error){if(!disposed&&job.revision===revision)onError?.(error);}
    finally{active=false;if(pending?.ready)void run();else if(!pending)onBusy?.(false);}
  }
  function schedule(selection,immediate=false){
    if(disposed)return;
    clearTimeout(timer);const job={selection,revision:++revision,ready:immediate};pending=job;
    onBusy?.(true);
    if(immediate)void run();
    else timer=setTimeout(()=>{if(pending===job){job.ready=true;void run();}},delay);
  }
  return {schedule,dispose(){disposed=true;++revision;clearTimeout(timer);pending=null;}};
}

export function clipCache(fetchClip,limit=12){
  const cache=new Map();
  const load=async function(field,value){
    const key=`${field}:${value}`;
    if(cache.has(key)){const value=cache.get(key);cache.delete(key);cache.set(key,value);return value;}
    const promise=Promise.resolve().then(()=>fetchClip(field,value));cache.set(key,promise);
    while(cache.size>limit)cache.delete(cache.keys().next().value);
    try{return await promise;}catch(error){if(cache.get(key)===promise)cache.delete(key);throw error;}
  };
  load.clear=()=>cache.clear();
  return load;
}
