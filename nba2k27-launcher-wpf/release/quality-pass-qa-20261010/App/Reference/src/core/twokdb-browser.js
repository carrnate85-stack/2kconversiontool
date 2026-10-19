'use strict';
const twokdb=require('./twokdb-import'),auction=require('./twokauction-import');
let busy=false;
async function read(BrowserWindow,owner,input){
 let host;try{host=new URL(input).hostname;}catch{throw Error('Paste a 2KDB or 2KAuction player link.');}
 const isAuction=host==='2kauction.com',importer=isAuction?auction:twokdb;
 const address=importer.url(input),id=new URL(address).pathname.split('/').pop(),source=isAuction?'2KAuction':'2KDB',origin=new URL(address).origin;
 const allowed=isAuction?['2kauction.com','challenges.cloudflare.com']:['2kdb.net','img.2kdb.net','challenges.cloudflare.com'];
 if(busy)throw Error('A ratings import is already open.');busy=true;let win;
 try{
  win=new BrowserWindow({width:1100,height:800,show:true,title:source+' — read player data',webPreferences:{nodeIntegration:false,contextIsolation:true,sandbox:true,partition:isAuction?'twokauction-import':'twokdb-import'}});
  win.webContents.setWindowOpenHandler(()=>({action:'deny'}));
  win.webContents.session.setPermissionRequestHandler((wc,permission,callback)=>callback(false));
  win.webContents.session.on('will-download',(event)=>event.preventDefault());
  win.webContents.session.webRequest.onBeforeRequest((details,callback)=>{let allow=false;try{const u=new URL(details.url);allow=['data:','blob:'].includes(u.protocol)||(u.protocol==='https:'&&allowed.includes(u.hostname));}catch{}callback({cancel:!allow});});
  win.webContents.on('will-navigate',(event,target)=>{try{if(new URL(target).origin!==origin)event.preventDefault();}catch{event.preventDefault();}});
  await win.loadURL(address);
  const until=Date.now()+120000;
  while(!win.isDestroyed()&&Date.now()<until){
   const scripts=await win.webContents.executeJavaScript('Array.from(document.scripts).filter(s=>!s.src).map(s=>s.textContent).join("\\n").slice(0,4000001)');
   try{return importer.map(importer.extract(scripts,isAuction?address:id),address);}catch(e){if(scripts.length>4000000)throw e;}
   await new Promise(resolve=>setTimeout(resolve,1500));
  }
  throw Error(source+' import closed or timed out. No player values were changed.');
 }finally{if(win&&!win.isDestroyed())win.destroy();busy=false;}
}
module.exports={read};
