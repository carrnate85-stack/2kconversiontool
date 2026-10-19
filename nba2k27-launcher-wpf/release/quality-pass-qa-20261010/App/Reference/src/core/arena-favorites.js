'use strict';
const fs=require('node:fs/promises'),path=require('node:path');
const {choices}=require('../data/blacktop-arenas.json');
class ArenaFavorites {
 constructor(directory){this.file=path.join(directory,'arena-favorites.json');this.queue=Promise.resolve();}
 async read(){
  try{const value=JSON.parse(await fs.readFile(this.file,'utf8'));if(!Array.isArray(value))throw Error('Invalid arena favorites');return [...new Set(value.filter(id=>choices.some(c=>c.id===id)))];}
  catch(e){if(e.code==='ENOENT')return [];throw Error('Could not read arena favorites. Your saved file was left unchanged.');}
 }
 set(id,favorite){
  if(!choices.some(c=>c.id===id)||typeof favorite!=='boolean')return Promise.reject(Error('Invalid arena favorite.'));
  const job=this.queue.then(async()=>{const ids=new Set(await this.read());favorite?ids.add(id):ids.delete(id);const next=[...ids];
   await fs.mkdir(path.dirname(this.file),{recursive:true});const temp=this.file+'.tmp';
   try{await fs.writeFile(temp,JSON.stringify(next)+'\n','utf8');await fs.rename(temp,this.file);}finally{await fs.rm(temp,{force:true}).catch(()=>{});}
   return next;
  });this.queue=job.catch(()=>{});return job;
 }
}
module.exports={ArenaFavorites};
