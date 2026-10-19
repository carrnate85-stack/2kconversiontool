(function(root){
 const fields=[
  {key:'mvp',label:'Most Valuable Player',offset:20,bit:7,width:1},
  {key:'champion',label:'NBA Champion',offset:43,bit:6,width:1},
  {key:'finalsMvp',label:'Finals MVP',offset:43,bit:7,width:1},
  {key:'roy',label:'Rookie of the Year',offset:27,bit:5,width:1},
  {key:'sixthMan',label:'Sixth Man of the Year',offset:27,bit:6,width:1},
  {key:'dpoy',label:'Defensive Player of the Year',offset:27,bit:7,width:1},
  {key:'mip',label:'Most Improved Player',offset:47,bit:7,width:1},
  {key:'allStar',label:'All-Star',offset:39,bit:7,width:2,max:2,split:true,choices:['None','Starter','Reserve']},
  {key:'allNba',label:'All-NBA',offset:23,bit:5,width:2,max:3},
  {key:'allDefense',label:'All-Defensive',offset:47,bit:5,width:2,max:2},
  {key:'allRookie',label:'All-Rookie',offset:27,bit:2,width:2,max:2},
 ];
 function decode(bytes){if(bytes.length!==64)throw Error('Invalid season record.');return Object.fromEntries(fields.map(f=>[f.key,f.split?((bytes[39]>>7)&1)|(((bytes[27]>>4)&1)<<1):(bytes[f.offset]>>f.bit)&(2**f.width-1)]));}
 function patch(bytes,changes){if(!changes||typeof changes!=='object'||Array.isArray(changes))throw Error('Invalid awards.');const after=Uint8Array.from(bytes);if(after.length!==64)throw Error('Invalid season record.');for(const [key,value] of Object.entries(changes)){const f=fields.find(f=>f.key===key);if(!f||!Number.isInteger(value)||value<0||value>(f.max??1))throw Error('Unsupported award or tier: '+key);if(f.split){after[39]=(after[39]&127)|(value===1?128:0);after[27]=(after[27]&239)|(value===2?16:0);}else{const mask=(2**f.width-1)<<f.bit;after[f.offset]=(after[f.offset]&~mask)|(value<<f.bit);}}return after;}
 const api={fields,decode,patch};if(typeof module!=='undefined')module.exports=api;else root.playerSeasonAwards=api;
})(globalThis);
