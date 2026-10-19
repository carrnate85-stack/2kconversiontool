const { contextBridge, ipcRenderer } = require('electron');
// This window intentionally gets no file, shell, launch, or roster-edit APIs.
contextBridge.exposeInMainWorld('modLauncher', {
  arenaStatus:()=>ipcRenderer.invoke('arena:status'),
  selectArena:id=>ipcRenderer.invoke('arena:select',id),
  arenaFavorites:()=>ipcRenderer.invoke('arena:favorites'),
  setArenaFavorite:(id,favorite)=>ipcRenderer.invoke('arena:favorite',id,favorite),
  onArenaStatus:callback=>{const listener=(_event,value)=>callback(value);ipcRenderer.on('arena:status',listener);return()=>ipcRenderer.removeListener('arena:status',listener);},
  ballStatus:()=>ipcRenderer.invoke('ball:status'),
  selectBall:id=>ipcRenderer.invoke('ball:select',id),
  ballThumbnail:id=>ipcRenderer.invoke('ball:thumbnail',id),
  onBallStatus:callback=>{const listener=(_event,value)=>callback(value);ipcRenderer.on('ball:status',listener);return()=>ipcRenderer.removeListener('ball:status',listener);},
  scoreboardStatus: () => ipcRenderer.invoke('scoreboard:status'),
  selectScoreboard: id => ipcRenderer.invoke('scoreboard:select', id),
  onScoreboardStatus: callback => { const listener = (_event, value) => callback(value); ipcRenderer.on('scoreboard:status', listener); return () => ipcRenderer.removeListener('scoreboard:status', listener); },
  scanBlacktop: options => ipcRenderer.invoke('blacktop:scan',options),
  readGameRules:()=>ipcRenderer.invoke('blacktop:rules-read'),
  setGameRule:(snapshot,field,target)=>ipcRenderer.invoke('blacktop:rules-set',snapshot,field,target),
  blacktopCarryStatus:()=>ipcRenderer.invoke('blacktop:carry-status'),
  onBlacktopCarryStatus:callback=>{const listener=(_event,state)=>callback(state);ipcRenderer.on('blacktop:carry-status',listener);return()=>ipcRenderer.removeListener('blacktop:carry-status',listener);},
  listOutfitPresets: options => ipcRenderer.invoke('blacktop:outfits-list',options),
  applyBlacktop: value => ipcRenderer.invoke('blacktop:apply', value),
  clothingThumbnail: asset => ipcRenderer.invoke('blacktop:thumbnail', asset),
  hideBlacktopPanel: () => ipcRenderer.invoke('blacktop:panel-hide'),
  expandBlacktopPanel: expanded => ipcRenderer.invoke('blacktop:panel-expanded', expanded),
  sizeBlacktopPlayerMenu: rows => ipcRenderer.invoke('blacktop:panel-player-menu', rows),
  onBlacktopPanelCollapsed: callback => {
    const listener=()=>callback();ipcRenderer.on('blacktop:panel-collapsed',listener);
    return ()=>ipcRenderer.removeListener('blacktop:panel-collapsed',listener);
  },
  onBlacktopPanelShown: callback => {
    const listener = () => callback();
    ipcRenderer.on('blacktop:panel-shown', listener);
    return () => ipcRenderer.removeListener('blacktop:panel-shown', listener);
  },
});
