'use strict';
const fs=require('node:fs/promises');
const path=require('node:path');

// Diagnostic only: never delays a Blacktop selection and never contains
// game memory or player data. One bounded file belongs to one launcher process.
class SelectionStatusLog {
  constructor({directory,version,pid=process.pid,maxBytes=1024*1024}) {
    this.file=path.join(directory,`blacktop-selections-${version}-${pid}.jsonl`);
    this.version=version;this.pid=pid;this.maxBytes=maxBytes;
    this.bytes=null;this.last=new Map();this.tail=Promise.resolve();
  }
  record(kind,value) {
    if(!['arena','ball','scoreboard','outfit','crowd','wnba'].includes(kind))throw Error('Invalid selection status kind.');
    const event={kind,selected:String(value?.selected||''),phase:String(value?.phase||''),message:String(value?.message||'').slice(0,500)};
    const key=JSON.stringify(event);
    if(this.last.get(kind)===key)return this.tail;
    this.last.set(kind,key);
    this.tail=this.tail.catch(()=>{}).then(async()=>{
      if(this.bytes===null){await fs.mkdir(path.dirname(this.file),{recursive:true});this.bytes=(await fs.stat(this.file).catch(error=>error.code==='ENOENT'?{size:0}:Promise.reject(error))).size;}
      const line=JSON.stringify({...event,time:new Date().toISOString(),version:this.version,pid:this.pid})+'\n';
      const size=Buffer.byteLength(line);
      if(this.bytes+size>this.maxBytes)return;
      await fs.appendFile(this.file,line,'utf8');this.bytes+=size;
    }).catch(error=>{if(this.last.get(kind)===key)this.last.delete(kind);throw error;});
    return this.tail;
  }
}
module.exports={SelectionStatusLog};
