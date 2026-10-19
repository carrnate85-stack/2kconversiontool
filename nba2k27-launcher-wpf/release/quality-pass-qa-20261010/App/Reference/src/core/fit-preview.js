'use strict';
const path=require('node:path'),{Worker}=require('node:worker_threads');
const {selection}=require('./outfit-presets');
const {composeStreetwear}=require('./clothing-components');
const catalog=require('../data/streetwear-components.json');
function fitAssets(value){const valid=selection(value);if(valid.pool!=='male')throw Error('The retired fit-preview renderer only supports male geometry.');const result=composeStreetwear(catalog,valid.streetId,valid.bottom,valid.shoe,valid.sock,valid.layer);if(!result||result.items.length>7)throw Error('Unsupported outfit.');return result.items;}
class FitPreview{
 constructor({cacheRoot,decoderPath}){Object.assign(this,{cacheRoot,decoderPath});this.worker=null;this.idleWorker=null;}
 cancel(){this.worker?.postMessage('cancel');}
 async render(gameRoot,value){
  const assets=fitAssets(value);if(this.worker)throw Error('The previous preview is still finishing. Please try again in a moment.');
  return new Promise((resolve,reject)=>{
   const data={gameRoot,assets,cacheRoot:this.cacheRoot,decoderPath:this.decoderPath};
   const reused=this.idleWorker;this.idleWorker=null;
   const worker=reused||new Worker(path.join(__dirname,'fit-preview-worker.js'),{workerData:data});this.worker=worker;worker.ref();
   if(!reused){worker.on('error',()=>{if(this.idleWorker===worker)this.idleWorker=null;});worker.on('exit',()=>{if(this.idleWorker===worker)this.idleWorker=null;});}
   let settled=false;
   const cleanup=()=>{worker.off('message',message);worker.off('error',error);worker.off('exit',exit);};
   const finish=(err,payload,reusable=false)=>{if(settled)return;settled=true;cleanup();if(this.worker===worker)this.worker=null;if(reusable){this.idleWorker=worker;worker.unref();}else worker.terminate();if(err)reject(err);else resolve(payload);};
   const message=m=>finish(m.error?Error(m.error):null,m.payload,true);
   const error=e=>finish(e);
   const exit=()=>finish(Error('Preview worker stopped.'));
   worker.on('message',message);worker.on('error',error);worker.on('exit',exit);
   if(reused)worker.postMessage({render:data});
  });
 }
}
module.exports={FitPreview,fitAssets};
