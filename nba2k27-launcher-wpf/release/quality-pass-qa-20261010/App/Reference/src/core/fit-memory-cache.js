'use strict';
const {serialize}=require('node:v8');
class FitMemoryCache{
 constructor(limit=96*1024*1024){this.limit=limit;this.bytes=0;this.items=new Map();}
 get(key){const e=this.items.get(key);if(!e)return;this.items.delete(key);this.items.set(key,e);return structuredClone(e.value);}
 set(key,value){const bytes=serialize(value).byteLength;const old=this.items.get(key);if(old){this.bytes-=old.bytes;this.items.delete(key);}if(bytes>this.limit)return;
  while(this.bytes+bytes>this.limit){const key=this.items.keys().next().value;this.bytes-=this.items.get(key).bytes;this.items.delete(key);}
  this.items.set(key,{value:structuredClone(value),bytes});this.bytes+=bytes;
 }
 clear(){this.items.clear();this.bytes=0;}
}
module.exports={FitMemoryCache};
