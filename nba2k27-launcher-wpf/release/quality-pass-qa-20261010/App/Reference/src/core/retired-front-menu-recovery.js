'use strict';
// Cleanup only. Cannot install assets, launch helpers, or patch game memory.
const fs=require('node:fs/promises'),path=require('node:path'),crypto=require('node:crypto');
const KNOWN=new Map([
 ['gooey_scenes/mainmenu_2k27_whatsnew_description.iff','9f45337a76ce0f68f5b7f3e7d185900cb151dffc4772f9d7e3b1d656605ee5ea'],
 ['gooey/nba_2k27_mainmenu.iff','916a760a9e8085005b2df674ef7b6e202d90894a27b04910dbedf864bf110862'],
]);
async function optional(file){try{return await fs.readFile(file);}catch(e){if(e.code==='ENOENT')return null;throw e;}}
async function plain(file){let current=path.parse(path.resolve(file)).root;for(const part of path.resolve(file).slice(current.length).split(path.sep).filter(Boolean)){current=path.join(current,part);const stat=await fs.lstat(current).catch(e=>{if(e.code==='ENOENT')return null;throw e;});if(stat?.isSymbolicLink())throw Error('Retired front-menu recovery refuses linked paths.');}}
class RetiredFrontMenuRecovery {
 constructor({directory,isGameRunning}){this.marker=path.join(directory,'whats-new-session.json');this.isGameRunning=isGameRunning;}
 async restore(){
  await plain(this.marker);const raw=await optional(this.marker);if(!raw)return;
  if(raw.length>16384)throw Error('Invalid retired front-menu journal.');
  if(await this.isGameRunning())throw Error('Close NBA 2K27 before cleaning up retired front-menu files.');
  const s=JSON.parse(raw),entries=s.schema===1?[{target:s.target,hash:s.hash,logical:[...KNOWN.keys()][0]}]:s.schema===2?s.entries:null;
  if(typeof s.modsRoot!=='string'||!path.isAbsolute(s.modsRoot)||!Array.isArray(entries)||!entries.length||entries.length>2||entries.some(e=>!e)||new Set(entries.map(e=>e.logical)).size!==entries.length)throw Error('Invalid retired front-menu journal; files left untouched.');
  for(const e of entries){
   if(KNOWN.get(e.logical)!==e.hash||e.target!==path.join(s.modsRoot,...e.logical.split('/')))throw Error('Invalid retired front-menu target; files left untouched.');
   await plain(e.target);const current=await optional(e.target);
   if(current&&crypto.createHash('sha256').update(current).digest('hex')!==e.hash)throw Error('Retired front-menu file changed externally; left untouched.');
  }
  for(const e of entries)if(await optional(e.target))await fs.unlink(e.target);
  await fs.unlink(this.marker);
 }
}
module.exports={RetiredFrontMenuRecovery};
