'use strict';
const fs=require('node:fs/promises');
const path=require('node:path');
// Samples the existing authenticated provider status; does not synthesize audio
// requests or modify game state. The provider retains only its latest 200 events.
class AudioRouteDiagnostics {
  constructor(file) {this.file=file;this.sequence=0;this.total=0;this.events=[];this.truncated=false;this.accessoryEvents=[];this.accessoryTotal=0;}
  async sample(provider) {
    const status=await provider.request({operation:'status'});
    const rows=Array.isArray(status.recentActivity)?status.recentActivity:[];
    const first=rows[0]?.sequence;
    if(first>this.sequence+1)this.truncated=true;
    for(const row of rows) {
      if(!Number.isSafeInteger(row.sequence)||row.sequence<=this.sequence)continue;
      this.sequence=row.sequence;
      if(typeof row.logicalPath==='string'&&/(?:^|[/\\])(?:png\d+_(?:geo|item|config)_|signatureconfigs_(?:all|editplayer)\.iff$|fpplayer_global\.iff$)/i.test(row.logicalPath)){
        this.accessoryTotal++;
        this.accessoryEvents.push({sequence:row.sequence,time:row.timestamp,operation:row.operation,path:row.logicalPath.slice(0,512),bytes:Number(row.bytesRead)||0,ok:row.ok===true,error:row.error?String(row.error).slice(0,512):null});
      }
      if(typeof row.logicalPath!=='string'||!/(?:newaudio|jukebox|\.wav$)/i.test(row.logicalPath))continue;
      this.total++;
      this.events.push({sequence:row.sequence,time:row.timestamp,operation:row.operation,path:row.logicalPath.slice(0,512),bytes:Number(row.bytesRead)||0,ok:row.ok===true,error:row.error?String(row.error).slice(0,512):null});
    }
    this.events=this.events.slice(-120);
    this.accessoryEvents=this.accessoryEvents.slice(-120);
    const result={schema:'nba2k27-audio-route-diagnostics-v1',updatedAt:new Date().toISOString(),providerPid:provider.pid,
      providerReachable:true,indexedAssets:status.indexedAssets,observedAudioEvents:this.total,historyMayBeIncomplete:this.truncated,
      note:'Bounded status sample. No events does not prove the game never requested a resource; stock routes and overwritten history remain possible.',events:this.events,
      observedAccessoryEvents:this.accessoryTotal,accessoryEvents:this.accessoryEvents};
    await this.save(result);this.lastResult=result;return result;
  }
  async failed(providerPid) {
    await this.save({...this.lastResult,schema:'nba2k27-audio-route-diagnostics-v1',
      updatedAt:new Date().toISOString(),providerPid,providerReachable:false,
      observedAudioEvents:this.total,historyMayBeIncomplete:true,events:this.events,
      note:'Provider status could not be read. Retained events are historical, not evidence of a currently working route.'});
  }
  async save(result) {
    await fs.mkdir(path.dirname(this.file),{recursive:true});
    const temporary=this.file+'.tmp';
    await fs.writeFile(temporary,JSON.stringify(result,null,2)+'\n');await fs.rename(temporary,this.file);
  }
}
module.exports={AudioRouteDiagnostics};
