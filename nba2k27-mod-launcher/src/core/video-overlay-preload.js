const {contextBridge,ipcRenderer}=require('electron');
contextBridge.exposeInMainWorld('startupVideo',{
  source:()=>ipcRenderer.invoke('startup-video:source'),finish:reason=>ipcRenderer.invoke('startup-video:finish',reason),
  volume:()=>ipcRenderer.invoke('startup-video:volume'),
  onVolume:callback=>{const listener=(_event,value)=>callback(value);ipcRenderer.on('startup-video:volume-changed',listener);return()=>ipcRenderer.removeListener('startup-video:volume-changed',listener);},
});
