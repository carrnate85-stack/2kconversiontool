'use strict';

// Electron does not await async before-quit listeners. Gate the final quit,
// and share preparation when close/quit requests arrive together.
function createExitHandler({ canQuit=async()=>true, prepare, quit, forceQuit=quit, forceOnTimeout=true, onError, onTimeout=()=>{}, timeoutMs=15000 }) {
  let ready = false, pending = null;
  return event => {
    if (ready) return;
    event.preventDefault();
    if (pending) return pending;
    let timer;
    pending = Promise.resolve().then(async()=>{
      // User dialogs and the live-game guard are not shutdown work.
      if(await canQuit()===false)return false;
      // A restoring worker must retain ownership, even when it is slow. Keep
      // the same pending close so another click cannot overlap preparation.
      if(!forceOnTimeout){
        timer=setTimeout(()=>{void Promise.resolve().then(()=>onTimeout(Error('Launcher cleanup is still pending.'))).catch(()=>{});},timeoutMs);
        return prepare();
      }
      const timeout=new Promise((_,reject)=>{timer=setTimeout(()=>{const error=Error('Launcher cleanup timed out.');error.code='EXIT_TIMEOUT';reject(error);},timeoutMs);});
      return Promise.race([Promise.resolve().then(prepare),timeout]);
    }).then(allowed => {
      if (allowed === false) return;
      ready = true;
      quit();
    }).catch(async error=>{
      if(error?.code!=='EXIT_TIMEOUT')return onError(error);
      try{await Promise.race([Promise.resolve().then(()=>onTimeout(error)),new Promise(resolve=>setTimeout(resolve,500))]);}
      finally{ready=true;forceQuit();}
    }).finally(() => { clearTimeout(timer);pending = null; });
    return pending;
  };
}
module.exports = { createExitHandler };
