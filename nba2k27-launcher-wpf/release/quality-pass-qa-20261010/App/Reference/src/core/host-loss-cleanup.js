'use strict';
// Headless recovery must not depend on a dialog or a forced-exit deadline.
// Unknown game state and failed restoration retain the backend/session owner.
function createHostLossCleanup({ready,cleanup,complete,report=()=>{},sleep=ms=>new Promise(resolve=>setTimeout(resolve,ms)),retryMs=1000}){
 let pending,last='';
 const status=(phase,message)=>{const key=phase+'|'+message;if(key===last)return;last=key;try{report({phase,message});}catch{}};
 return ()=>pending??=(async()=>{
  for(;;){
   try{
    if(!await ready()){status('waiting','Keeping the session active until launch/initialization completes and NBA 2K27 exits.');await sleep(retryMs);continue;}
    status('restoring','Restoring the launcher-owned session without a window.');
    await cleanup();await complete();return;
   }catch(error){status('retrying',String(error.message||error));await sleep(retryMs);}
  }
 })();
}
module.exports={createHostLossCleanup};
