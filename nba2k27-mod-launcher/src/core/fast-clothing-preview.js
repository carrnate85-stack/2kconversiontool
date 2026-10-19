'use strict';
const path=require('node:path');
const {Worker}=require('node:worker_threads');
const {selection}=require('./outfit-presets');
const accessorySlots=require('../shared/accessory-slots');
const {composeStreetwear,category}=require('./clothing-components');
const catalog=require('../data/streetwear-components.json');
const previewNames=[...new Set(catalog.presets.filter(p=>p.sourceGroup.pool==='male').flatMap(p=>p.original).filter(asset=>['top','bottom'].includes(category(asset))||/^acc_sock_/.test(asset)))];
const previewNameSet=new Set(previewNames);

function previewAssets(value){
 const valid=selection(value);
 if(valid.pool!=='male')throw Error('The mannequin preview currently supports men’s clothing.');
 const composed=composeStreetwear(catalog,valid.streetId,valid.bottom,valid.shoe,valid.sock,valid.layer,valid.accessory,valid.accessories);
 if(!composed||composed.items.length>7)throw Error('Unsupported outfit preview.');
 return composed.items.filter(asset=>['top','bottom'].includes(category(asset))||/^(?:acc_sock_|shoe_)/.test(asset)||Boolean(accessorySlots.slot(asset)));
}
class FastClothingPreview{
 constructor({cacheRoot,decoderPath}){this.cacheRoot=cacheRoot;this.decoderPath=decoderPath;this.worker=null;this.pending=new Map();this.sequence=0;}
 ensure(gameRoot){
  if(this.worker&&this.gameRoot===gameRoot)return;
  this.dispose();this.gameRoot=gameRoot;
  const worker=new Worker(path.join(__dirname,'fast-clothing-preview-worker.js'),{workerData:{gameRoot,cacheRoot:this.cacheRoot,decoderPath:this.decoderPath,previewNames}});
  this.worker=worker;worker.unref();
  worker.on('message',message=>{
   const item=this.pending.get(message.id);if(!item)return;
   if(message.progress){item.onProgress?.(message.payload);return;}
   this.pending.delete(message.id);
   if(message.error)item.reject(Error(message.error));else item.resolve(message.payload);
  });
  worker.on('error',error=>{if(this.worker===worker)this.fail(error);});
  worker.on('exit',code=>{if(this.worker===worker)this.fail(Error(`Preview worker stopped (${code}).`));});
 }
 fail(error){this.worker=null;for(const item of this.pending.values())item.reject(error);this.pending.clear();}
 warm(gameRoot){
  this.ensure(path.resolve(gameRoot));
  if(this.pending.size)return Promise.resolve({busy:true});
  const id=++this.sequence;
  return new Promise((resolve,reject)=>{this.pending.set(id,{resolve,reject});this.worker.postMessage({id,request:{warm:true}});});
 }
 prefetch(gameRoot,asset){
  if(typeof asset!=='string'||!previewNameSet.has(asset))return false;
  this.ensure(path.resolve(gameRoot));
  this.worker.postMessage({prefetch:asset});
  return true;
 }
 render(gameRoot,request,onProgress){
  const assets=previewAssets(request?.selection),knownMeshes=Array.isArray(request?.knownMeshes)?request.knownMeshes.filter(x=>/^[a-f0-9]{64}$/.test(x)).slice(0,256):[];
  const knownTextures=Array.isArray(request?.knownTextures)?request.knownTextures.filter(x=>/^[a-f0-9]{64}$/.test(x)).slice(0,256):[];
  this.ensure(path.resolve(gameRoot));
  const id=++this.sequence;
  for(const [old,item] of this.pending){item.reject(Error('Preview superseded.'));this.pending.delete(old);}
  return new Promise((resolve,reject)=>{this.pending.set(id,{resolve,reject,onProgress});this.worker.postMessage({id,request:{assets,knownMeshes,knownTextures,showMannequinEarly:request?.showMannequinEarly===true}});});
 }
 cancel(){this.worker?.postMessage({cancel:true});for(const item of this.pending.values())item.reject(Error('Preview cancelled.'));this.pending.clear();}
 dispose(){this.cancel();const worker=this.worker;this.worker=null;worker?.terminate();}
}
module.exports={FastClothingPreview,previewAssets};
