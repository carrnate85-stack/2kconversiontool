'use strict';
// Drain in-flight jobs before reporting failure so cleanup cannot race a writer.
async function boundedWork(items, limit, action) {
  let next=0, failure;
  await Promise.all(Array.from({length:Math.min(limit,items.length)},async()=>{
    while(!failure && next<items.length){const index=next++;try{await action(items[index],index);}catch(error){failure ||= error;}}
  }));
  if(failure)throw failure;
}
module.exports={boundedWork};
