'use strict';
const path=require('node:path'),fs=require('node:fs/promises');
const {pathToFileURL}=require('node:url');
const {watchController}=require('./controller-skip');
class VideoOverlay {
  constructor({BrowserWindow,screen,ipcMain,helperPath,volumeMonitor=null,waitForWindow=null,onReturn=async()=>{},log=()=>{}}){
    Object.assign(this,{BrowserWindow,screen,helperPath,volumeMonitor,waitForWindow,onReturn,log});this.window=null;this.finishing=null;this.pending=null;this.generation=0;
    ipcMain.handle('startup-video:source',event=>{this.authorize(event);return pathToFileURL(this.videoPath).href;});
    ipcMain.handle('startup-video:volume',event=>{this.authorize(event);return this.volume;});
    ipcMain.handle('startup-video:finish',(event,reason)=>{this.authorize(event);return this.finish(['ended','skip','error'].includes(reason)?reason:'skip');});
  }
  authorize(event){if(!this.window||event.sender!==this.window.webContents)throw Error('Unknown video window');}
  async showWhenGameReady(options){
    this.pending?.abort();const request=new AbortController();this.pending=request;
    this.log({event:'waiting-for-game-window'});
    try{
      const ready=await this.waitForWindow?.({gamePath:options.gamePath,signal:request.signal});
      if(!ready||request.signal.aborted||this.pending!==request){this.log({event:'video-wait-cancelled'});return;}
      this.pending=null;this.log({event:'game-window-ready'});await this.show(options);
    }finally{if(this.pending===request)this.pending=null;}
  }
  async show({videoPath,gamePath=null,preview=false,owner=null}={}){
    const generation=this.generation;
    if(this.finishing)await this.finishing;
    if(generation!==this.generation)return;
    if(this.window&&!this.window.isDestroyed()){this.window.focus();return;}
    const stat=await fs.lstat(videoPath);if(!stat.isFile()||stat.isSymbolicLink()||path.extname(videoPath).toLowerCase()!=='.mp4')throw Error('The startup MP4 is missing or invalid.');
    if(generation!==this.generation)return;
    if(this.window&&!this.window.isDestroyed()){this.window.focus();return;}
    this.videoPath=videoPath;
    this.volume={volume:0.25,muted:false,source:'fallback'};
    this.preview=preview;this.owner=owner;
    const bounds=owner&&!owner.isDestroyed()?this.screen.getDisplayMatching(owner.getBounds()).bounds:this.screen.getPrimaryDisplay().bounds;
    const win=new this.BrowserWindow({...bounds,show:false,frame:false,resizable:true,movable:false,fullscreenable:true,skipTaskbar:true,backgroundColor:'#000000',autoHideMenuBar:true,
      webPreferences:{preload:path.join(__dirname,'video-overlay-preload.js'),sandbox:true,contextIsolation:true,nodeIntegration:false,backgroundThrottling:false,autoplayPolicy:'no-user-gesture-required'}});
    this.window=win;win.setAlwaysOnTop(true,'screen-saver');
    win.webContents.setWindowOpenHandler(()=>({action:'deny'}));win.webContents.on('will-navigate',event=>event.preventDefault());
    win.webContents.on('render-process-gone',()=>{void this.finish('renderer-failed');});
    win.on('close',event=>{if(this.window===win){event.preventDefault();void this.finish('window-close');}});
    this.watchdog=setTimeout(()=>{void this.finish('timeout');},615000);this.watchdog.unref();
    try{
      await this.volumeMonitor?.start(gamePath,value=>{if(this.window!==win||win.isDestroyed())return;this.volume=value;win.webContents.send('startup-video:volume-changed',value);},{muteGame:!preview,onFailure:()=>{if(this.window===win)void this.finish('audio-helper-failed');}});
      if(win.isDestroyed()||this.window!==win||generation!==this.generation)return;
      await win.loadFile(path.join(__dirname,'video-overlay.html'));if(win.isDestroyed()||this.window!==win||generation!==this.generation)return;win.show();win.setFullScreen(true);win.focus();this.log({event:'shown',preview,fullscreen:win.isFullScreen()});
      this.controller=watchController({helper:this.helperPath,onSkip:()=>this.finish('controller-skip'),log:this.log});}
    catch(error){await this.finish('load-failed');throw error;}
  }
  async finish(reason){
    this.generation++;
    this.pending?.abort();this.pending=null;
    if(this.finishing)return this.finishing;const win=this.window;if(!win)return;
    this.finishing=(async()=>{
      clearTimeout(this.watchdog);this.log({event:'closed',reason,preview:this.preview});
      // Native controller skip/window close may happen before the page pauses.
      // Silence the intro before restoring the game's audio.
      try{if(!win.isDestroyed())win.webContents.setAudioMuted(true);}catch{}
      await this.volumeMonitor?.stop().catch(error=>this.log({event:'volume-stop-error',message:error.message}));
      await this.controller?.stop().catch(error=>this.log({event:'controller-stop-error',message:error.message}));this.controller=null;
      try{if(!this.preview&&!['launcher-exit','game-ended','launch-failed'].includes(reason))await Promise.race([this.onReturn(),new Promise(resolve=>setTimeout(resolve,1500))]);else if(this.owner&&!this.owner.isDestroyed())this.owner.focus();}
      catch(error){this.log({event:'focus-error',message:error.message});}
      finally{this.window=null;if(!win.isDestroyed())win.destroy();}
    })();
    try{await this.finishing;}finally{this.finishing=null;}
  }
}
module.exports={VideoOverlay};
