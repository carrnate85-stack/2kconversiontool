'use strict';
const fs=require('node:fs/promises');
const path=require('node:path');
const crypto=require('node:crypto');
// Session-scoped desired outfits. No native requests are issued by this store.
class BlacktopMatchChoices {
  constructor(file){this.file=file;}
  async record(snapshot, player, items, restore=false) {
    if(!Number.isSafeInteger(snapshot.pid)||snapshot.pid<=0||!/^\d+$/.test(snapshot.started)||
       !Number.isSafeInteger(player.roster)||player.roster<=0||!Number.isInteger(player.slot)||player.slot<0||player.slot>=10)
      throw new Error('Unverified Blacktop session/player identity.');
    if(!Array.isArray(items)||items.length<1||items.length>7||
       items.some(n=>typeof n!=='string'||(n!=='NO SHIRT'&&!/^[A-Za-z0-9_]{1,255}$/.test(n))))
      throw new Error('Unverified Blacktop clothing names.');
    let data;
    try {data=JSON.parse(await fs.readFile(this.file,'utf8'));}
    catch(e){if(e.code!=='ENOENT')throw e;}
    if(!data||data.pid!==snapshot.pid||data.started!==snapshot.started)
      data={schema:'blacktop-match-choices-v1',pid:snapshot.pid,started:snapshot.started,players:[]};
    if(data.schema!=='blacktop-match-choices-v1'||!Array.isArray(data.players))throw new Error('Invalid match choices record.');
    // Remove displaced lineup occupants as well as this player's prior choice.
    data.players=data.players.filter(p=>p.roster!==player.roster&&p.slot!==player.slot);
    if(!restore)data.players.push({choiceId:crypto.randomUUID(),roster:player.roster,slot:player.slot,name:player.name,
      items:[...items],status:'preview-verified-not-applied-to-match'});
    if(data.players.length>10)throw new Error('Too many Blacktop choices.');
    await fs.mkdir(path.dirname(this.file),{recursive:true});
    const temp=`${this.file}.${crypto.randomUUID()}.tmp`;
    try {
      const f=await fs.open(temp,'wx');
      try {await f.writeFile(JSON.stringify(data,null,2));await f.sync();}finally{await f.close();}
      await fs.rename(temp,this.file);
    } finally {await fs.unlink(temp).catch(()=>{});}
  }
}
module.exports={BlacktopMatchChoices};
