'use strict';

async function gameStopped({presence,running}){
  if(presence()===true)return false;
  if(await running('NBA2K27.exe',{fresh:true}))return false;
  return presence()!==true;
}

function shareCleanup(cleanup){
  let pending=null;
  return ()=>{
    // Final shutdown is irreversible once successful. Host EOF after an orderly
    // close must not repeat the entire completed restoration batch.
    if(!pending)pending=Promise.resolve().then(cleanup).catch(error=>{pending=null;throw error;});
    return pending;
  };
}
module.exports={gameStopped,shareCleanup};
