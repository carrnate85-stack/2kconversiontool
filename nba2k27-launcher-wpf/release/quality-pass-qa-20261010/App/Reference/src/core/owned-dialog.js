// Restore the owning launcher window after native dialog completion/cancellation.
async function ownedDialog(owner,action){
 try{return await action();}
 finally{
  if(owner&&!owner.isDestroyed()){
   owner.setEnabled(true);
   owner.focus();
   owner.webContents?.focus();
  }
 }
}
module.exports={ownedDialog};
