'use strict';
const fs=require('node:fs/promises'),path=require('node:path');
const OLD='6A974D13:368C3000',CURRENT='6AA2013D:36558000';
// Mirrors native helper build guards. Native structural/ownership checks remain
// mandatory; recognizing a PE identity is NOT permission to skip those checks.
const features={session:['Game session',[OLD,CURRENT]],roster:['Roster / Team editor',[OLD,CURRENT]],seasons:['Season history',[CURRENT]],camera:['Cameras',[OLD,CURRENT]],arena:['Arena',[OLD,CURRENT]],scoreboard:['Scoreboard',[OLD,CURRENT]],outfits:['Outfits',[OLD,CURRENT]],ball:['Ball selection',[CURRENT]],rules:['Game rules',[CURRENT]],crowd:['Crowd removal',[CURRENT]],debug:['Debug menus',[CURRENT]],scaler:['Scaler',[OLD,CURRENT]]};
function identity(bytes){
 if(bytes.length<64||bytes.readUInt16LE(0)!==0x5a4d)throw Error('Not a Windows executable.');const pe=bytes.readUInt32LE(60);
 if(pe<64||pe+84>bytes.length||bytes.readUInt32LE(pe)!==0x4550||bytes.readUInt16LE(pe+4)!==0x8664||bytes.readUInt16LE(pe+24)!==0x20b)throw Error('Unsupported executable header.');
 return [bytes.readUInt32LE(pe+8),bytes.readUInt32LE(pe+80)].map(n=>n.toString(16).toUpperCase().padStart(8,'0')).join(':');
}
function featureFor(channel,args=[]){
 if(['game:launch-without-eac','session:start'].includes(channel))return 'session';
 if(['player:save','team:save','team:staff-save','team-uniform:save','team-uniform:link','arena:clone'].includes(channel))return 'roster';
 if(channel==='team:rotations'&&args[0]?.action==='save')return 'roster';
 if(channel==='player:seasons'&&['save','apply-import','add-older','remove-oldest'].includes(args[0]?.action))return 'seasons';
 if(channel==='roster:transfer'&&args[0]?.action==='apply')return 'roster';
 if(channel==='scaler:apply')return 'scaler';
 if(channel==='camera:start'||channel==='camera:install-names')return 'camera';
 if(channel==='arena:select')return 'arena';if(channel==='ball:select')return 'ball';if(channel==='scoreboard:select')return 'scoreboard';
 if(channel==='blacktop:apply')return 'outfits';if(channel==='blacktop:rules-set')return 'rules';
 return null;
}
class GameCompatibility{
 constructor(){this.key=null;this.result=null;}
 async inspect(gameRoot){
  const file=path.resolve(gameRoot||'.','NBA2K27.exe');let build=null,reason='';
  try{const stat=await fs.stat(file);if(!stat.isFile())throw Error('NBA2K27.exe is not a file.');const key=[file,stat.size,stat.mtimeMs,stat.ctimeMs].join('|');if(key===this.key&&this.result)return this.result;
   const handle=await fs.open(file,'r');try{const bytes=Buffer.alloc(4096);const read=await handle.read(bytes,0,bytes.length,0);build=identity(bytes.subarray(0,read.bytesRead));}finally{await handle.close();}this.key=key;
  }catch(error){this.key=null;reason=error.code==='ENOENT'?'Choose the installed NBA 2K27 folder.':error.message;}
  const rows=Object.entries(features).map(([key,[label,builds]])=>({key,label,supported:builds.includes(build),reason:builds.includes(build)?'Build recognized; native safety checks still apply.':reason||'This game build is not supported by this feature. No new edit will be attempted.'}));
  return this.result={build,checkedAt:new Date().toISOString(),features:rows,known:rows.some(r=>r.supported),message:reason||(!rows.some(r=>r.supported)?'Unrecognized game update. Library management and recovery remain available.':'Compatibility checked against the native helper build guards.')};
 }
 async assert(channel,args,gameRoot){const feature=featureFor(channel,args);if(!feature)return;const status=await this.inspect(gameRoot),row=status.features.find(r=>r.key===feature);if(!row.supported)throw Error(row.label+': '+row.reason);}
}
module.exports={GameCompatibility,identity,featureFor,features,OLD,CURRENT};
