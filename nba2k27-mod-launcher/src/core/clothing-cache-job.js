'use strict';
class ClothingCacheJob {
  constructor(thumbnails,onStatus=()=>{}) {
    this.thumbnails=thumbnails;this.onStatus=onStatus;this.running=null;this.cancelled=false;
    this.state={phase:'idle',total:0,completed:0,ready:0,missing:0,failed:0};
  }
  publish(patch) {Object.assign(this.state,patch);this.onStatus({...this.state});return {...this.state};}
  start(gameRoot) {
    if(this.running)return {...this.state};
    this.cancelled=false;
    this.publish({phase:'indexing',total:0,completed:0,ready:0,missing:0,failed:0,batch:0,batches:0,current:'',error:''});
    this.running=this.run(gameRoot).catch(error=>this.publish({phase:'failed',error:String(error.message).slice(0,400)}))
      .finally(()=>{this.running=null;});
    return {...this.state};
  }
  cancel() {if(this.running){this.cancelled=true;this.publish({phase:'cancelling'});}return {...this.state};}
  async run(gameRoot) {
    const index=await this.thumbnails.index(gameRoot);
    const unique=new Map();
    for(const [asset,entry] of index.entries){
      const key=entry.previewKind==='shoe-model'?entry.sourcePath:asset;
      if(!unique.has(key))unique.set(key,asset);
    }
    const assets=[...unique.values()],batchSize=64,batches=Math.ceil(assets.length/batchSize);
    this.publish({total:assets.length,batches,batch:0,phase:this.cancelled?'cancelling':'running'});
    for(let start=0;start<assets.length&&!this.cancelled;start+=batchSize){
    let next=start;const end=Math.min(start+batchSize,assets.length);
    this.publish({batch:Math.floor(start/batchSize)+1});
    const worker=async()=>{while(!this.cancelled && next<end) {
      const asset=assets[next++];
      this.publish({current:asset});
      try {
        const result=await this.thumbnails.get(gameRoot,asset,{background:true});
        if(result.state==='ready')this.state.ready++;
        else if(result.state==='missing')this.state.missing++;
        else this.state.failed++;
      } catch {this.state.failed++;}
      this.publish({completed:this.state.completed+1});
    }};
    // Small bounded batches; browse conversions retain queue priority.
    // Cancellation starts no new work; cached results are reused on restart.
    await Promise.all(Array.from({length:4},()=>worker()));
    await new Promise(resolve=>setImmediate(resolve));
    }
    return this.publish({phase:this.cancelled?'cancelled':'complete',current:''});
  }
}
module.exports={ClothingCacheJob};
