'use strict';
const crypto=require('node:crypto');
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
// Pure fail-closed fixed-width UI edit. No process, filesystem, locale or
// game-mode mutation. Plans are bound to the exact inspected stock member.
function patchScoreboardMember(source,plan){
 if(!Buffer.isBuffer(source)||!plan||sha(source)!==plan.sourceSha256||!Array.isArray(plan.edits)||!plan.edits.length)throw Error('Scoreboard source or patch plan differs.');
 const result=Buffer.from(source),occupied=new Set();
 for(const edit of plan.edits){
  if(!Number.isSafeInteger(edit.offset)||edit.offset<0||edit.offset+8>source.length||!/^[a-f0-9]{16}$/.test(edit.before)||!/^[a-f0-9]{16}$/.test(edit.after))throw Error('Invalid scoreboard text edit.');
  if(source.subarray(edit.offset,edit.offset+8).toString('hex')!==edit.before)throw Error('Scoreboard text binding changed.');
  for(let i=edit.offset;i<edit.offset+8;i++){if(occupied.has(i))throw Error('Overlapping scoreboard edits.');occupied.add(i);}
  Buffer.from(edit.after,'hex').copy(result,edit.offset);
 }
 for(let i=0;i<source.length;i++)if(!occupied.has(i)&&source[i]!==result[i])throw Error('Unrelated scoreboard bytes changed.');
 return result;
}
module.exports={patchScoreboardMember,sha};
