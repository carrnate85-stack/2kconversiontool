// Browser replacements for the only Node/disk-bound functions in WPF's
// core/fast-clothing-preview-worker.js. Everything else in that worker runs
// verbatim. The generator swaps exact, hash-pinned source regions.
//@@decode
// Archive decoding runs in Rust (fit_assets.rs, port of Nba2kFastFitDecoder);
// see loadCached below. The WPF decoder-process permit queue is not needed.
//@@manifest
async function manifest(gameRoot){
 const current=await host('index',{stamp:indexStamp});
 if(current.stamp===indexStamp&&index)return index;
 index=new Map(current.names.map(name=>[name,{}]));
 indexStamp=current.stamp;tables=null;memory.clear();memoryBytes=0;warmQueue=[];warmCursor=0;warmRequested=pendingPrefetch.size>0;prefetchQueue=[];prefetched.clear();
 return index;
}
async function cacheFile(gameRoot,name){
 if(!index.has(name))throw Error(`Preview resource unavailable: ${name}`);
 const {key}=await host('key',{name});
 return {name,key};
}
//@@loadCached
async function loadCached(gameRoot,source,kind,key,build,signal){
 const id=`${kind}-${source.key}`;
 const stored=await diskCache.get(id);
 if(stored!==undefined)return {key:source.key,value:remember(key,stored),source:'disk'};
 const bytes=new Uint8Array(await host('resource',{name:source.name},signal));
 signal.throwIfAborted();
 const target=putFile(bytes);
 try{
  const value=await build(target);
  await diskCache.put(id,value,estimate(value));
  return {key:source.key,value:remember(key,value),source:'decoded'};
 }finally{files.delete(target);}
}
async function prune(){await diskCache.prune();}
