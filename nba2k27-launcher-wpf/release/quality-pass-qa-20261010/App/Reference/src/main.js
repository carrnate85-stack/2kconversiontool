const path = require('node:path');
const crypto = require('node:crypto');
const fsp = require('node:fs/promises');
const { pathToFileURL } = require('node:url');
const { app, BrowserWindow, clipboard, dialog, ipcMain: electronIpcMain, Menu, shell, screen, nativeImage, protocol, globalShortcut } = require('electron');
const {resolveStorage,resolvePortableStorage,ensureNativeLocationHint,nativePointerFile,appDataRoot}=require('./core/launcher-storage');
const {ownedDialog}=require('./core/owned-dialog');
const {ensureRuntimeResources}=require('./core/runtime-resources');
const {createPreviewImageResolver}=require('./core/preview-image-pack');
const {findImportedPreviewPack,importPreviewImagePack}=require('./core/preview-image-import');
protocol.registerSchemesAsPrivileged([{scheme:'preview-image',privileges:{standard:true,secure:true,supportFetchAPI:true,corsEnabled:true}}]);
function trustedIpcSender(sender) {
  return Boolean(sender) && [mainWindow?.webContents, rosterWindow?.webContents, blacktopPanel?.window?.webContents].includes(sender);
}
let nativeClosing=false;
const gameCompatibility=new (require('./core/game-compatibility').GameCompatibility)();
async function assertGameFeature(channel,args){if(require('./core/game-compatibility').featureFor(channel,args))await gameCompatibility.assert(channel,args,(await readSettings()).gameRoot);}
const ipcMain={handle(channel,listener){
  electronIpcMain.handle(channel,async(event,...args)=>{
    if(!trustedIpcSender(event.sender))throw new Error('Unknown launcher window.');
    if(nativeClosing)throw new Error('Launcher cleanup is in progress. Wait for it to finish.');
    await assertGameFeature(channel,args);
    if(nativeClosing)throw new Error('Launcher cleanup is in progress. Wait for it to finish.');
    if(!trustedIpcSender(event.sender))throw new Error('Launcher window closed during the compatibility check.');
    return listener(event,...args);
  });
}};
const commandSelfTestIndex=process.argv.indexOf('--self-test');
const commandSelfTestOutput=commandSelfTestIndex>=0?process.argv[commandSelfTestIndex+1]:null;
// Folder releases keep every persistent launcher file beside the executable.
// Development runs retain the old resolver so they never mutate a user's
// portable library by accident.
const launcherStorage=app.isPackaged
  ?resolvePortableStorage({executable:process.execPath,appData:app.getPath('appData'),selfTestOutput:commandSelfTestOutput})
  :resolveStorage({appData:app.getPath('appData'),selfTestOutput:commandSelfTestOutput});
if(app.isPackaged&&launcherStorage.mode==='blocked')throw launcherStorage.error;
if(launcherStorage.mode==='organized'){
  const fs=require('node:fs');
  fs.mkdirSync(launcherStorage.data,{recursive:true});
  fs.mkdirSync(launcherStorage.chromium,{recursive:true});
  fs.mkdirSync(launcherStorage.logs,{recursive:true});
}
if(launcherStorage.mode==='organized'&&launcherStorage.placement!=='portable'){
  try{ensureNativeLocationHint(app.getPath('appData'),launcherStorage);}catch(error){console.warn('Could not refresh portable runtime location hint:',error.message);}
}
app.setPath('userData',launcherStorage.data);
if(launcherStorage.mode==='organized')app.setPath('sessionData',launcherStorage.chromium);
if(launcherStorage.mode==='organized')app.setAppLogsPath(launcherStorage.logs);
if(launcherStorage.placement==='portable')app.setPath('crashDumps',launcherStorage.logs);
const storagePath=(category,...parts)=>path.join(launcherStorage.mode==='organized'?launcherStorage[category]:app.getPath('userData'),...parts);
const musicPath=(...parts)=>launcherStorage.mode==='organized'?storagePath('music',...parts):storagePath('data','jukebox',...parts);
const logsPath=(...parts)=>launcherStorage.mode==='organized'?storagePath('logs',...parts):storagePath('data','diagnostics',...parts);
const launcherHealth=new (require('./core/launcher-health').LauncherHealth)({directory:logsPath(),version:app.getVersion(),lane:process.env.WPF_SMOKE==='1'?'qa':'normal'});
app.recordPerformance=(name,ms)=>launcherHealth.record(name,ms).catch(error=>console.warn('Performance log:',error.message));
app.finalizePerformance=async()=>{if(app.shutdownStartedAt)await app.recordPerformance('shutdown',Date.now()-app.shutdownStartedAt);await launcherHealth.tail;};
const {SelectionStatusLog}=require('./core/selection-status-log');
const selectionStatusLog=new SelectionStatusLog({directory:logsPath(),version:app.getVersion()});
const cachePath=(...parts)=>launcherStorage.mode==='organized'?storagePath('cache',...parts):storagePath('data',...parts);
const instanceLock=app.requestSingleInstanceLock();
if(!instanceLock)app.quit();
let runtimeResourceRoot=null;
function runtimePath(...parts){return path.join(runtimeResourceRoot||process.resourcesPath,...parts);}
const {GameCapture}=require('./core/game-capture');
const gameCapture=new GameCapture({directory:logsPath(),helperPath:()=>app.isPackaged?runtimePath('diagnostics','NBA2K27GameCapture.exe'):path.join(__dirname,'../diagnostics-helper/dist/NBA2K27GameCapture.exe'),onStatus:state=>send('diagnostics:status',state)});
// Let Chromium composite the popup normally; retain an explicit troubleshooting fallback.
if (process.argv.includes('--software-rendering')) {
  app.disableHardwareAcceleration();
  app.commandLine.appendSwitch('disable-gpu');
}
const { defaults, fileKind, normalizeModProfiles, validateSettings } = require('./core/paths');
const { scanMods } = require('./core/catalog');
const { PROVIDERS, providerStatus } = require('./core/providers');
const { SessionManager, processIsRunning } = require('./core/session');
const { Mp4Library } = require('./core/mp4-library');
const { VideoOverlay } = require('./core/video-overlay');
const { GameVolume } = require('./core/game-volume');
const { waitForGameWindow } = require('./core/game-window');
const {SharedPresentationSession}=require('./core/shared-presentation');
let videoOverlay=null;
const presentationManager = new Mp4Library({isGameRunning:()=>processIsRunning('NBA2K27.exe'),isPlaying:()=>Boolean(videoOverlay?.window),onProgress:progress=>send('presentation:progress',progress),libraryDirectory:launcherStorage.mode==='organized'?launcherStorage.videos:path.join(app.getPath('userData'),'videos')});
let sharedPresentationSession=null;
const {RetiredFrontMenuRecovery}=require('./core/retired-front-menu-recovery');
let retiredFrontMenuRecovery=null;
const { JukeboxSession } = require('./core/jukebox-session');
const { useFreestylePlaylist } = require('./core/freestyle-playlist');
const { waitAudioReadiness } = require('./core/audio-readiness');
const { offlineProviderDiagnostic, resolveAsset } = require('./core/independent-provider');
const {
  productionProviderDiagnostic,
  runBinaryProviderSelfTest,
  startProductionProvider,
} = require('./core/production-provider');
const { runBridgeAudit, sha256File } = require('./core/bridge-audit');
const { runOfflineDelegationSelfTest } = require('./core/delegating-device');
const { runNativeAbiHarness } = require('./core/native-abi-harness');
const { runCrossModuleHarness } = require('./core/cross-module-harness');
const { inspectObserveOnlyAddon } = require('./core/observe-only-addon');
const { ObserveOnlyProbeServer } = require('./core/probe-server');
const { CameraManager } = require('./core/camera-manager');
const { ScalerManager } = require('./core/scaler-manager');
const { FreestyleUnlocks } = require('./core/freestyle-unlocks');
const { DebugMenus } = require('./core/debug-menus');
const { BlacktopCrowd } = require('./core/blacktop-crowd');
let blacktopCrowd=null;
ipcMain.handle('blacktop:crowd-status',event=>{requireLauncherSender(event);return blacktopCrowd?.view()||{enabled:true,canRestore:false,phase:'configured',message:'Crowd control leaves the original setting unchanged.'};});
ipcMain.handle('blacktop:crowd-set',(event,enabled)=>{requireLauncherSender(event);if(!blacktopCrowd)throw Error('Launcher is initializing.');return blacktopCrowd.saveEnabled(enabled);});
const { BlacktopPresentation } = require('./core/blacktop-presentation');
let blacktopPresentation=null;
ipcMain.handle('blacktop:presentation-status',event=>{requireLauncherSender(event);return blacktopPresentation?.view()||{options:{drone:false,dj:false,mc:false},phase:'disabled',message:'Removal options are off.'};});
ipcMain.handle('blacktop:presentation-set',(event,value)=>{requireLauncherSender(event);if(!blacktopPresentation)throw Error('Launcher is initializing.');return blacktopPresentation.saveOptions(value);});
let debugMenus=null;
ipcMain.handle('unlocks:debug-status',event=>{requireLauncherSender(event);return debugMenus?.view()||{enabled:false,phase:'disabled',message:'Debug menus are off.'};});
ipcMain.handle('unlocks:debug-set',async(event,enabled)=>{
  requireLauncherSender(event);
  if(typeof enabled!=='boolean'||!debugMenus)throw Error('Invalid debug menu request.');
  if(enabled&&!debugMenus.enabled){
    const answer=await dialog.showMessageBox(mainWindow,{type:'warning',title:'Experimental debug menus',message:'Steam must be in Offline Mode.',detail:'Experimental: exposes built-in debug menus and restricted options. These options may cause instability or affect saves. Use a backup/test roster. Only use the launcher’s offline/non-EAC Play flow. This toggle resets to off whenever the launcher opens.',buttons:['Cancel','Steam is offline — enable'],defaultId:0,cancelId:0,noLink:true});
    if(answer.response!==1)return debugMenus.view();
  }
  return debugMenus.setEnabled(enabled);
});
const { BlacktopUnlocks } = require('./core/blacktop-unlocks');
let blacktopUnlocks=null;
const { BallScaleRecovery } = require('./core/ball-scale-recovery');
let ballScaleRecovery=null;
let scalerManager=null;
let freestyleUnlocks=null;
ipcMain.handle('unlocks:freestyle-status',event=>{requireLauncherSender(event);return freestyleUnlocks?.view()||{phase:'saved',message:'Saved — applies on your next launch.'};});
function requireScalerSender(event){if(!mainWindow||event.sender!==mainWindow.webContents)throw new Error('Scaler is available in the launcher.');}
ipcMain.handle('scaler:status',event=>{requireScalerSender(event);return scalerManager.view();});
ipcMain.handle('scaler:save',(event,value)=>{requireScalerSender(event);return scalerManager.save(value);});
ipcMain.handle('scaler:apply',event=>{requireScalerSender(event);return scalerManager.applySaved();});
const { launchWithoutEac } = require('./core/steam-launcher');
const { resolveProviderLayout } = require('./core/provider-layout-resolver');
const { VersionProxySession } = require('./core/version-proxy-session');
const { MultiConfigManager } = require('./core/multi-config-manager');
const { PlayerResearchManager } = require('./core/player-research');
const { ModProfileManager } = require('./core/mod-profile-manager');
const { ModTagStore, canonicalRoot } = require('./core/mod-tags');
const {planModInstall,installModPlan,resolveCatalogFile}=require('./core/mod-installer');
const {prepareModSources}=require('./core/mod-import-source');
const { ArchiveJukeboxManager: JukeboxManager } = require('./core/archive-jukebox');
const { inferAudioMetadata } = require('./core/audio-metadata');
const { ArtworkLookup } = require('./core/artwork-lookup');
const { BlacktopManager, resolveBlacktopLayoutPath } = require('./core/blacktop-manager');
const {BlacktopSession}=require('./core/blacktop-session');
let blacktopSession=null;
let outfitPresets;
function savedOutfits() {
  if(!outfitPresets){const {OutfitPresets}=require('./core/outfit-presets');outfitPresets=new OutfitPresets(launcherStorage.mode==='organized'?storagePath('outfits','outfit-presets.json'):storagePath('data','blacktop','outfit-presets.json'));}
  return outfitPresets;
}
let clothingThumbnails;
let clothingCacheJob;
let fastClothingPreview;
function wardrobePreviews() {
  if (!clothingThumbnails) {const {ClothingThumbnails}=require('./core/clothing-thumbnails');clothingThumbnails = new ClothingThumbnails({ cacheRoot:launcherStorage.mode==='organized'?cachePath('Blacktop','thumbnails'):cachePath('blacktop','thumbnail-cache'), decoderPath:fastFitDecoderPath(), texconvPath:texconvExecutable() });}
  return clothingThumbnails;
}
function wardrobeCacheJob() {
  if(!clothingCacheJob){const {ClothingCacheJob}=require('./core/clothing-cache-job');clothingCacheJob=new ClothingCacheJob(wardrobePreviews(),state=>send('blacktop:cache-status',state));}
  return clothingCacheJob;
}
function mannequinPreview(){
  if(!fastClothingPreview){const {FastClothingPreview}=require('./core/fast-clothing-preview');fastClothingPreview=new FastClothingPreview({cacheRoot:launcherStorage.mode==='organized'?cachePath('Blacktop','fit-previews'):cachePath('blacktop','fit-preview-cache'),decoderPath:fastFitDecoderPath()});}
  return fastClothingPreview;
}
function requireLauncherSender(event) {
  if(event.sender!==mainWindow?.webContents)throw new Error('This action is only available in the launcher.');
}
async function healthReport(){
 const settings=await readSettings();
 return {...await launcherHealth.view(),compatibility:await gameCompatibility.inspect(settings.gameRoot),measuredAt:new Date().toISOString()};
}
ipcMain.handle('launcher:health',event=>{requireLauncherSender(event);return healthReport();});
ipcMain.handle('launcher:copy-health',async event=>{requireLauncherSender(event);const settings=await readSettings(),report=await healthReport();const safe=require('./core/support-bundle').redactValue(report,'',[settings.gameRoot,settings.modsRoot,launcherStorage.root,app.getPath('userData')]);clipboard.writeText(JSON.stringify(safe,null,2));return true;});
ipcMain.handle('launcher:performance',(_event,name,ms)=>{if(typeof name!=='string'||!name.startsWith('catalog.'))throw Error('Unsupported timing event.');return app.recordPerformance(name,ms);});
if(app.launcherUpdates){
 app.assertUpdateReady=()=>{if(!initializationFinished||launchInProgress||versionProxySession?.background)throw Error('Wait for launcher initialization or game launch to finish.');};
 ipcMain.handle('launcher:build-info',event=>{requireLauncherSender(event);return app.launcherUpdates.info();});
 ipcMain.handle('launcher:update-choose',async event=>{
  requireLauncherSender(event);await app.launcherUpdates.assertClosed();
  const chosen=await dialog.showOpenDialog(mainWindow,{title:'Choose downloaded launcher update folder',properties:['openDirectory']});
  if(chosen.canceled)return null;return app.launcherUpdates.prepare(chosen.filePaths[0]);
 });
 ipcMain.handle('launcher:update-rollback',async event=>{requireLauncherSender(event);return app.launcherUpdates.prepare(app.launcherUpdates.p.previous);});
 ipcMain.handle('launcher:update-cancel',event=>{requireLauncherSender(event);return app.launcherUpdates.cancel();});
 ipcMain.handle('launcher:update-restart',async event=>{requireLauncherSender(event);if(!app.launcherUpdates.pending)throw Error('Choose an update first.');await app.launcherUpdates.assertClosed();app.launcherUpdates.installRequested=true;setTimeout(()=>app.quit(),100);return true;});
}
ipcMain.handle('blacktop:cache-start',async event=>{requireLauncherSender(event);return wardrobeCacheJob().start((await readSettings()).gameRoot);});
ipcMain.handle('blacktop:cache-cancel',event=>{requireLauncherSender(event);return wardrobeCacheJob().cancel();});
ipcMain.handle('blacktop:cache-status',event=>{requireLauncherSender(event);return {...wardrobeCacheJob().state};});
let blacktopManager;
const { BlacktopPanel } = require('./core/blacktop-panel');
const { DirectGameRules } = require('./core/direct-game-rules');
const { BlacktopAutoOpen } = require('./core/blacktop-auto-open');
let blacktopAutoOpen;
const { F8Shortcut } = require('./core/f8-shortcut');
const { PanelDiagnostics } = require('./core/panel-diagnostics');
let blacktopPanel;
let directGameRules;
function gameRules(){return directGameRules??=new DirectGameRules({
  helperPath:app.isPackaged?runtimePath('unlocks','NBA2K27FreestyleTeams.exe'):path.join(__dirname,'..','unlocks-helper','NBA2K27FreestyleTeams.exe'),
  settings:readSettings,
});}
const { ScoreboardManager } = require('./core/scoreboard-manager');
const {ArenaManager}=require('./core/arena-manager');
const {ArenaFavorites}=require('./core/arena-favorites');
const {BallSelectionManager}=require('./core/ball-selection-manager');
const {BallThumbnails}=require('./core/ball-thumbnails');
let arenaFavorites;
function favoriteArenas(){return arenaFavorites??=new ArenaFavorites(path.join(app.getPath('userData'),'blacktop'));}
let arenaManager;
let ballSelectionManager;
function balls(){
  if(!ballSelectionManager)ballSelectionManager=new BallSelectionManager({
    statePath:path.join(app.getPath('userData'),'blacktop','ball-selection.json'),decoderPath:()=>vczDecoderPath(),settings:readSettings,
    // QA-only reactivation. Require a deliberate choice in this app run so a
    // previously saved ball cannot silently apply when the test starts.
    liveSubstitutionEnabled:true,
    requireExplicitBallSelection:true,
    helperPath:()=>app.isPackaged?runtimePath('unlocks','NBA2K27FreestyleTeams.exe'):path.join(__dirname,'..','unlocks-helper','NBA2K27FreestyleTeams.exe'),
    isGameRunning:()=>processIsRunning('NBA2K27.exe'),
    thumbnailService:new BallThumbnails({cacheRoot:launcherStorage.mode==='organized'?cachePath('Blacktop','ball-thumbnails'):cachePath('blacktop','ball-thumbnails'),decoderPath:()=>vczDecoderPath(),texconvPath:()=>texconvExecutable()}),
    publish:value=>{launcherHealth.observe('ball:status',value);selectionStatusLog.record('ball',value).catch(error=>console.warn('Ball status diagnostic:',error.message));for(const win of [mainWindow,blacktopPanel?.window])if(win&&!win.isDestroyed())win.webContents.send('ball:status',value);},
  });
  return ballSelectionManager;
}
function arenas(){
  if(!arenaManager)arenaManager=new ArenaManager({
    helperPath:()=>app.isPackaged?runtimePath('arena','NBA2K27Arena.exe'):path.join(__dirname,'..','arena-helper','NBA2K27Arena.exe'),
    catalogPath:()=>app.isPackaged?runtimePath('arena','blacktop-arenas.json'):path.join(__dirname,'data','blacktop-arenas.json'),
    settings:readSettings,publish:value=>{launcherHealth.observe('arena:status',value);selectionStatusLog.record('arena',value).catch(error=>console.warn('Arena status diagnostic:',error.message));for(const win of [mainWindow,blacktopPanel?.window])if(win&&!win.isDestroyed())win.webContents.send('arena:status',value);}
  });return arenaManager;
}
ipcMain.handle('arena:status',event=>{requirePanelSender(event);return arenas().view();});
ipcMain.handle('arena:select',async(event,id)=>{requirePanelSender(event);return arenas().select(id);});
ipcMain.handle('arena:favorites',event=>{requirePanelSender(event);return favoriteArenas().read();});
ipcMain.handle('arena:favorite',async(event,id,favorite)=>{requirePanelSender(event);return favoriteArenas().set(id,favorite);});
ipcMain.handle('ball:status',event=>{requirePanelSender(event);return balls().view();});
ipcMain.handle('ball:select',(event,id)=>{requirePanelSender(event);return balls().select(id);});
ipcMain.handle('ball:thumbnail',(event,id)=>{requirePanelSender(event);return balls().thumbnail(id);});
const { ScoreboardLayoutSession } = require('./core/scoreboard-layout-session');
let scoreboardLayouts;
const {createHighschoolDigitProbe}=require('./core/highschool-digit-probe');
let highschoolDigitProbe;
let scoreboardManager;
let scoreboardReady;
function scoreboards() {
  if (!scoreboardReady) {
    scoreboardReady = waitForInitialization().then(() => {
     scoreboardManager = new ScoreboardManager({
      helperPath: app.isPackaged ? runtimePath('scoreboard', 'NBA2K27Scoreboard.exe') : path.join(__dirname, '..', 'scoreboard-helper', 'NBA2K27Scoreboard.exe'),
      directory: path.join(app.getPath('userData'), 'blacktop', 'scoreboard'), settings: readSettings,
      layoutsReady:()=>Boolean(scoreboardLayouts?.ready),
      publish: value => {
        selectionStatusLog.record('scoreboard',value).catch(error=>console.warn('Scoreboard status diagnostic:',error.message));
        launcherHealth.observe('scoreboard:status',value);
        for (const win of [mainWindow, blacktopPanel?.window]) if (win && !win.isDestroyed()) win.webContents.send('scoreboard:status', value);
      },
     });
     return scoreboardManager.load().then(() => { scoreboardManager.start(); return scoreboardManager; });
    }).catch(error => { scoreboardReady=null; throw error; });
  }
  return scoreboardReady;
}
ipcMain.handle('scoreboard:status', async event => { requirePanelSender(event); return (await scoreboards()).refreshStatus(); });
ipcMain.handle('scoreboard:select', async (event, id) => { requirePanelSender(event); return (await scoreboards()).select(id); });
function requirePanelSender(event) {
  if (event.sender !== mainWindow?.webContents && event.sender !== blacktopPanel?.window?.webContents) throw new Error('Unknown panel sender.');
}
ipcMain.handle('blacktop:panel-open', event => { requirePanelSender(event); blacktopPanel?.register(); blacktopPanel?.show(); return blacktopPanel?.status(); });
ipcMain.handle('blacktop:panel-hide', event => { requirePanelSender(event); blacktopPanel?.hide(); });
ipcMain.handle('blacktop:panel-expanded', (event, expanded) => {
  if (event.sender !== blacktopPanel?.window?.webContents) throw new Error('Unknown panel sender.');
  blacktopPanel.setExpanded(expanded === true);
  return { expanded: blacktopPanel.expanded === true };
});
ipcMain.handle('blacktop:panel-player-menu', (event, rows) => {
  if(event.sender !== blacktopPanel?.window?.webContents)throw new Error('Unknown panel sender.');
  blacktopPanel.setPlayerMenuRows(rows);
});
ipcMain.handle('blacktop:panel-status', event => { requirePanelSender(event); return blacktopPanel?.status(); });
ipcMain.handle('blacktop:rules-read',event=>{
  if(event.sender!==blacktopPanel?.window?.webContents)throw Error('Game Rules are available in the F8 overlay.');
  return gameRules().read();
});
ipcMain.handle('blacktop:rules-set',(event,snapshot,field,target)=>{
  if(event.sender!==blacktopPanel?.window?.webContents)throw Error('Game Rules are available in the F8 overlay.');
  if(versionProxySession?.state?.phase!=='loader-active'||!versionProxySession.background||versionProxySession.cancelled)
    throw Error('Launch an active offline session through this launcher before changing Game Rules.');
  return gameRules().set(snapshot,field,target);
});

function blacktopTools() {
  if (!blacktopManager) blacktopManager = new BlacktopManager({
    helperPath: app.isPackaged
      ? runtimePath('roster-research', 'NBA2K27Blacktop.exe')
      : path.join(__dirname, '..', '..', 'nba2k27-roster-research', 'dist', 'NBA2K27Blacktop.exe'),
    layoutPath: resolveBlacktopLayoutPath({ isPackaged: app.isPackaged, resourcesPath: runtimeResourceRoot||process.resourcesPath, sourceDir: __dirname }),
    statePath: path.join(app.getPath('userData'), 'blacktop', 'original-outfits.json'),
    onAutoStatus: value => selectionStatusLog.record('outfit',{selected:'automatic',...value}),
  });
  return blacktopManager;
}
ipcMain.handle('blacktop:outfits-list', (event,options)=>{requirePanelSender(event);return savedOutfits().list({omitCatalog:options?.omitCatalog===true});});
ipcMain.handle('blacktop:outfits-save', (event,value)=>{requireLauncherSender(event);return savedOutfits().save(value);});
ipcMain.handle('blacktop:outfits-delete', (event,id)=>{requireLauncherSender(event);return savedOutfits().remove(id);});
ipcMain.handle('blacktop:outfits-export',async(event,value)=>{
  requireLauncherSender(event);const {encodeOutfit}=require('./core/outfit-share');const document=encodeOutfit(value);
  const name=JSON.parse(document).name.replace(/[<>:"/\\|?*]/g,'_').replace(/[. ]+$/g,'')||'Outfit';
  const result=await ownedDialog(mainWindow,()=>dialog.showSaveDialog(mainWindow,{title:'Export outfit',defaultPath:'Outfit - '+name+'.2koutfit',filters:[{name:'NBA 2K outfit',extensions:['2koutfit']}],properties:['showOverwriteConfirmation']}));
  if(result.canceled||!result.filePath)return null;
  await fsp.writeFile(result.filePath,document,'utf8');return {exported:true};
});
ipcMain.handle('blacktop:outfits-import',async event=>{
  requireLauncherSender(event);
  const result=await ownedDialog(mainWindow,()=>dialog.showOpenDialog(mainWindow,{title:'Import outfit',properties:['openFile'],filters:[{name:'NBA 2K outfit',extensions:['2koutfit']}]}));
  if(result.canceled||!result.filePaths[0])return null;
  const {readOutfitFile}=require('./core/outfit-share');return savedOutfits().save(await readOutfitFile(result.filePaths[0]));
});
ipcMain.handle('blacktop:scan', async (event,options) => {requirePanelSender(event);return blacktopTools().scan((await readSettings()).gameRoot,{omitCatalog:options?.omitCatalog===true});});
ipcMain.handle('blacktop:carry-status',event=>{requirePanelSender(event);return blacktopSession?.state;});
ipcMain.handle('blacktop:apply', async (event, value) => {
  requirePanelSender(event);
  const chosen=String(value?.outfitPresetId||value?.streetId||value?.outfitId||'manual');
  try {
    if(value?.outfitPresetId) {
      const preset=await savedOutfits().resolve(value.outfitPresetId);
      value={token:value.token,slot:value.slot,omitCatalog:value.omitCatalog,streetId:preset.streetId,bottom:preset.bottom,shoe:preset.shoe,sock:preset.sock,layer:preset.layer,accessories:preset.accessories,accessory:preset.accessory};
    }
    const result=await blacktopTools().apply((await readSettings()).gameRoot, value);
    const report=result?.report||{};
    selectionStatusLog.record('outfit',{selected:chosen,phase:report.matchCarryover||'unknown',
      message:`slot=${value?.slot}; consumed=${report.consumed===true}; pointer=${report.pointerMatches===true}; uniform=${report.uniformMatches===true}; unchanged=${report.unchanged===true}`})
      .catch(error=>console.warn('Outfit status diagnostic:',error.message));
    return result;
  } catch(error) {
    selectionStatusLog.record('outfit',{selected:chosen,phase:'error',message:`slot=${value?.slot}; ${error.message}`})
      .catch(logError=>console.warn('Outfit status diagnostic:',logError.message));
    throw error;
  }
});
ipcMain.handle('blacktop:thumbnail', async (event, asset) => {
  requirePanelSender(event);
  return wardrobePreviews().get((await readSettings()).gameRoot, asset);
});
ipcMain.handle('blacktop:thumbnail-cached', async (event, asset) => {
  requireLauncherSender(event);
  return wardrobePreviews().getCached((await readSettings()).gameRoot, asset);
});
ipcMain.handle('blacktop:fit-preview',async(event,request)=>{
  requireLauncherSender(event);
  const gameRoot=(await readSettings()).gameRoot;
  if(!gameRoot)throw Error('Select the NBA 2K27 folder in Settings first.');
  const token=Number.isSafeInteger(request?.previewToken)&&request.previewToken>0?request.previewToken:0;
  return mannequinPreview().render(gameRoot,request,payload=>{if(!event.sender.isDestroyed())event.sender.send('blacktop:fit-preview-progress',{token,payload});});
});
ipcMain.handle('blacktop:fit-preview-warm',async event=>{
  requireLauncherSender(event);
  const gameRoot=(await readSettings()).gameRoot;
  if(!gameRoot)return {warmed:false};
  return mannequinPreview().warm(gameRoot);
});
ipcMain.handle('blacktop:fit-preview-prefetch',async(event,asset)=>{
  requireLauncherSender(event);
  const gameRoot=(await readSettings()).gameRoot;
  return Boolean(gameRoot&&mannequinPreview().prefetch(gameRoot,asset));
});
ipcMain.handle('blacktop:fit-preview-cancel',event=>{requireLauncherSender(event);mannequinPreview().cancel();});

let mainWindow;
let startupWindow;
let rosterWindow;
let rosterWeb;
let initializationFinished=false;
let initializationFailure=null;
let releaseInitialization;
const initializationBarrier=new Promise(resolve=>{releaseInitialization=resolve;});
async function waitForInitialization(){
  if(!initializationFinished)await initializationBarrier;
  if(initializationFailure)throw initializationFailure;
}
const rosterWebEvent=Object.freeze({browser:true});
const rosterPreviewOwners = new WeakSet();
const rosterHandlers=new Map();
let rosterTransferSession;
const seasonOperationGate=new (require('./core/player-season-operation-gate').PlayerSeasonOperationGate)();
app.on('second-instance',()=>{if(mainWindow&&!mainWindow.isDestroyed()){if(mainWindow.isMinimized())mainWindow.restore();mainWindow.show();mainWindow.focus();}});
function registerRoster(channel,handler){
 const guarded=async(event,...args)=>{rosterOwner(event);if(nativeClosing)throw Error('Launcher cleanup is in progress.');await assertGameFeature(channel,args);rosterOwner(event);if(nativeClosing)throw Error('Launcher cleanup is in progress.');if(rosterTransferSession?.busy&&channel!=='roster:transfer'&&!['roster:launcher','roster:bootstrap','animation:close','animation:warm','animation:progress-status'].includes(channel))throw Error('Wait for the roster import to finish, or stop it from the import panel.');return seasonOperationGate.run(channel,args,()=>handler(event,...args));};
 rosterHandlers.set(channel,guarded);ipcMain.handle(channel,guarded);
}
let playerFolderSession;
function playerFolders(){return playerFolderSession??=new (require('./core/player-folder-session').PlayerFolderSession)({markerPath:path.join(app.getPath('userData'),'player-folder-session.json'),isGameRunning:()=>processIsRunning('NBA2K27.exe')});}
function rosterSender(event) { return event===rosterWebEvent || rosterPreviewOwners.has(event) || [mainWindow, rosterWindow].some(win => win && !win.isDestroyed() && event.sender === win.webContents); }
function rosterOwner(event) { if (!rosterSender(event)) throw new Error('Open the roster editor from the launcher.'); return event===rosterWebEvent||rosterPreviewOwners.has(event)?mainWindow:BrowserWindow.fromWebContents(event.sender); }
let animationPreview;
const startupAnimationOwner={};let animationWarmTimer;
function animations() {
 return animationPreview ||= new (require('./core/player-animation-preview').PlayerAnimationPreview)({
  runtimeRoot: app.isPackaged ? path.join(process.resourcesPath, 'animation-preview') : path.join(__dirname, '../vendor/animation-preview/runtime'),
  cacheRoot: cachePath('player-animation-preview'), settings: readSettings, idleMs:300000,
 });
}
const animationOwners = new WeakSet();
function animationOwner(event) {
 const win = rosterOwner(event), owner = rosterPreviewOwners.has(event) ? event : event.sender;
 if (!rosterPreviewOwners.has(event) && !animationOwners.has(owner)) {
  animationOwners.add(owner);
  const closeOwner = () => void animationPreview?.closeOwner(owner).catch(error => shutdownWarning('animation window cleanup', error));
  owner.once('destroyed', closeOwner); owner.once('render-process-gone', closeOwner); win.once('closed', closeOwner);
 }
 return owner;
}
registerRoster('animation:open', event => animations().open(animationOwner(event)));
registerRoster('animation:warm', (event,enabled) => animations().setWarm(animationOwner(event),enabled));
function scheduleAnimationWarmup() {
 if(process.env.WPF_SMOKE==='1')return;
 clearTimeout(animationWarmTimer);
 animationWarmTimer=setTimeout(async()=>{
  try {
   if(nativeHostLost||nativeClosing||workspaceCloseAllowed||animationPreview?.closing||launchInProgress||await processIsRunning('NBA2K27.exe'))return;
   const {gameRoot}=await readSettings();if(!gameRoot||!await fsp.stat(path.join(gameRoot,'NBA2K27.exe')).then(s=>s.isFile()).catch(()=>false))return;
   if(nativeHostLost||nativeClosing||workspaceCloseAllowed||animationPreview?.closing||launchInProgress)return;
   const service=animations();service.setWarm(startupAnimationOwner,true);
   const lease=service.warmOwners.get(startupAnimationOwner);await lease?.promise;
   service.setWarm(startupAnimationOwner,false);
  }catch(error){console.warn('Animation background preparation:',error.message);}
 },5000);animationWarmTimer.unref?.();
}
registerRoster('animation:catalog', (event, id) => {
 rosterOwner(event);
 const owner = rosterPreviewOwners.has(event) ? event : event.sender;
 return animations().capabilities(owner, id, progress => {
  if (rosterPreviewOwners.has(event)) { const session=animationPreview?.sessions.get(id);if(session?.owner===owner)session.progress=progress; }
  else if (!event.sender.isDestroyed()) event.sender.send('animation:progress', { session: id, ...progress });
 });
});
registerRoster('animation:request', (event, payload) => { rosterOwner(event); return animations().request(rosterPreviewOwners.has(event)?event:event.sender, payload?.op, payload?.args); });
registerRoster('animation:close', (event, id) => { rosterOwner(event); return animations().close(rosterPreviewOwners.has(event)?event:event.sender, id); });
registerRoster('animation:progress-status', (event,id) => { rosterOwner(event);return {session:id,...animations().session(rosterPreviewOwners.has(event)?event:event.sender,id).progress}; });
let workspaceCloseAllowed=false;
function closingFeedback(active,message='Closing launcher…'){
  for(const win of [mainWindow,rosterWindow])if(win&&!win.isDestroyed()){
    try{win.webContents.send('launcher:closing',{active,message});}catch(error){void shutdownWarning('closing feedback',error);}
  }
}
function protectWorkspaceClose(win) {
  let checking=false;
  win.on('close',event=>{
    if(workspaceCloseAllowed)return;
    const other=[mainWindow,rosterWindow].some(candidate=>candidate&&candidate!==win&&!candidate.isDestroyed());
    if(other)return;
    event.preventDefault();if(checking)return;checking=true;
    closingFeedback(true,'Checking the game before closing…');
    (async()=>{
      try {
        if(gameCapture.gamePresence()===true||await processIsRunning('NBA2K27.exe',{fresh:true})||gameCapture.gamePresence()===true) {
          closingFeedback(false);
          await dialog.showMessageBox(win,{type:'info',title:'Game session active',message:'Close NBA 2K27 first.',detail:'Keep this window open so the launcher can finish restoring your game session.',buttons:['OK']});
        } else {
          if(rosterWeb){closingFeedback(false);const answer=await dialog.showMessageBox(win,{type:'question',title:'Close launcher?',message:'Closing the launcher disconnects the browser roster editor.',detail:'Save your roster edits first. Browser tabs do not keep the launcher running.',buttons:['Keep open','Close launcher'],defaultId:0,cancelId:0});if(answer.response!==1)return;}
          // Keep the last visible window alive while before-quit performs its
          // bounded cleanup. Closing it first leaves an invisible process group
          // if a local server takes too long to stop.
          app.quit();
        }
      } catch { closingFeedback(false);await dialog.showMessageBox(win,{type:'warning',message:'Could not check the game session. Please close the game and try again.'}); }
      finally{checking=false;}
    })();
  });
}
registerRoster('roster:launcher',event=>{rosterOwner(event);if(!mainWindow||mainWindow.isDestroyed())createWindow();else{mainWindow.restore();mainWindow.show();mainWindow.focus();}});
ipcMain.handle('roster:open', async (event, section = 'players') => {
  rosterOwner(event);
  rosterWeb ||= new (require('./core/roster-web-server').RosterWebServer)({root:__dirname,
   invoke:(channel,args,owner)=>{if(owner)rosterPreviewOwners.add(owner);return rosterHandlers.get(channel)(owner||rosterWebEvent,...args);},
   readAnimationPayload:(token,owner)=>animationPreview?.readToken(token,owner),
   closePreviewOwner:owner=>animationPreview?.closeOwner(owner),
  });
  const url = new URL(await rosterWeb.start());
  url.searchParams.set('section', section === 'teams' ? 'teams' : 'players');
  await shell.openExternal(url.toString());
});
registerRoster('roster:bootstrap', event => { rosterOwner(event); return {version:app.getVersion(),playerResearch:playerResearchManager.view()}; });
let catalog = null;
let cameraManager = null;
let versionProxySession = null;
let multiConfigManager = null;
let playerResearchManager = null;
let modProfileManager = null;
let modTagStore = null;
let jukeboxManager = null;
let jukeboxSession = null;
let launchInProgress = false;
let nativeHostLost = false;
let nativeStartupSettled = false;
function requireNativeHost(){if(nativeHostLost)throw Error('The launcher window host was lost. Session recovery is running; no new game launch will be sent.');}
function requireJukeboxEditable(){
  if(launchInProgress||jukeboxSession?.active||jukeboxSession?.busy)throw new Error('Close NBA 2K27 before editing or changing the active playlist. Preview playback remains available.');
}
const artworkLookup = new ArtworkLookup();
const session = new SessionManager(
  (state) => send('session:status', state),
  async (settings) => {
    await cameraManager.start(settings.modsRoot);
    playerResearchManager.startAutomaticArenas({ modsRoot: settings.modsRoot });
  },
);
let lastProbeEvidenceKey = '';
let activeProbeProvider = null;
let autoFallbackActivation = null;
let autoFallbackSessionId = null;
let autoFallbackError = null;
let providerLayoutState = {
  status: 'pending',
  message: 'Waiting for the automatic local build scan.',
  report: null,
};
let providerLayoutPromise = null;
const MAX_LIVE_PROBE_ASSET_BYTES = 64 * 1024 * 1024;
const MAX_JUKEBOX_IMPORT_BYTES = 128 * 1024 * 1024;
const MAX_JUKEBOX_BATCH_FILES = 100;
const JUKEBOX_AUDIO_EXTENSIONS = new Set(['.mp3', '.wav', '.m4a', '.aac', '.flac', '.ogg', '.opus']);
const jukeboxAudioSelections = new Map();

async function inspectJukeboxAudio(sourcePath) {
  const resolved = path.resolve(sourcePath || '');
  if (!JUKEBOX_AUDIO_EXTENSIONS.has(path.extname(resolved).toLowerCase())) throw new Error(`Unsupported audio file: ${path.basename(resolved)}`);
  const stat = await fsp.stat(resolved);
  if (!stat.isFile() || stat.size < 1 || stat.size > MAX_JUKEBOX_IMPORT_BYTES) {
    throw new Error(`${path.basename(resolved)} must be smaller than 128 MB.`);
  }
  const handle = await fsp.open(resolved, 'r');
  try {
    const head = Buffer.alloc(Math.min(stat.size, 1024 * 1024));
    await handle.read(head, 0, head.length, 0);
    const tail = Buffer.alloc(Math.min(stat.size, 128));
    await handle.read(tail, 0, tail.length, Math.max(0, stat.size - tail.length));
    return { resolved, stat, metadata: inferAudioMetadata(path.basename(resolved), head, tail) };
  } finally { await handle.close(); }
}

async function stopProbeProvider() {
  const provider = activeProbeProvider;
  activeProbeProvider = null;
  if (provider) await provider.stop().catch(() => false);
}

function probeEvidencePath() {
  return path.join(app.getPath('userData'), 'observe-only-lifecycle.json');
}

async function persistProbeEvidence(state) {
  const lifecycle = state?.lifecycle;
  if (!lifecycle) return;
  const stage = lifecycle.orderlyShutdown
    ? 'shutdown'
    : (lifecycle.processExitConfirmed
      ? 'process-exit'
      : (lifecycle.liveRouteFailure
        ? 'live-route-failed'
        : (lifecycle.liveRoute?.passed
          ? 'live-route'
          : (lifecycle.registrationFailure
            ? 'registration-failed'
            : (lifecycle.registration?.passed
              ? 'registration'
              : (lifecycle.heartbeatCount > 0 ? 'heartbeat' : 'initialize'))))));
  const key = `${lifecycle.sessionId}:${stage}`;
  if (key === lastProbeEvidenceKey) return;
  lastProbeEvidenceKey = key;
  const evidence = {
    schema: 'nba2k27-controlled-lifecycle-v16',
    stage,
    recordedAt: new Date().toISOString(),
    pid: lifecycle.pid,
    initializedAt: lifecycle.initializedAt,
    lastHeartbeatAt: lifecycle.lastHeartbeatAt,
    shutdownAt: lifecycle.shutdownAt,
    exitDetectedAt: lifecycle.exitDetectedAt,
    heartbeatCount: lifecycle.heartbeatCount,
    orderlyShutdown: lifecycle.orderlyShutdown,
    processExitConfirmed: lifecycle.processExitConfirmed,
    registrationProbeArmed: lifecycle.registrationProbeArmed,
    registration: lifecycle.registration,
    liveRouteArmed: lifecycle.liveRouteArmed,
    liveRouteState: lifecycle.liveRouteState,
    pathDiagnostic: lifecycle.pathDiagnostic,
    pathDiagnostics: lifecycle.pathDiagnostics,
    liveRoute: lifecycle.liveRoute,
    liveRouteFailure: lifecycle.liveRouteFailure,
    callbacksInFlight: lifecycle.callbacksInFlight,
    resourceHandles: lifecycle.resourceHandles,
    safety: {
      hooksInstalled: false,
      deviceRegistrationEnabled: Boolean(lifecycle.deviceRegistrationEnabled),
      temporaryNondefaultRegistrationAttempted: Boolean(lifecycle.registration),
      temporaryDefaultDelegationAttempted: Boolean(lifecycle.registration),
      resourceReplacementEnabled: Boolean(lifecycle.resourceReplacementEnabled),
      gameMemoryWritten: Boolean(lifecycle.registration),
      gameCodePatched: false,
    },
  };
  await fsp.mkdir(path.dirname(probeEvidencePath()), { recursive: true });
  await fsp.writeFile(probeEvidencePath(), `${JSON.stringify(evidence, null, 2)}\n`, 'utf8');
}

function probeStatusView(state) {
  return {
    ...state,
    autoFallback: {
      enabled: true,
      starting: Boolean(autoFallbackActivation),
      error: autoFallbackError,
    },
  };
}

function publishProbeStatus(state) {
  send('probe:status', probeStatusView(state));
  persistProbeEvidence(state).catch(() => {});
  const lifecycle = state?.lifecycle;
  if (lifecycle?.registration?.passed || lifecycle?.registrationFailure
    || lifecycle?.liveRoute?.passed || lifecycle?.liveRouteFailure
    || lifecycle?.orderlyShutdown || lifecycle?.processExitConfirmed) {
    stopProbeProvider().catch(() => {});
  }
  if (state?.armed && lifecycle?.active && !state.registrationProbeArmed
    && !state.liveRouteArmed && !lifecycle.liveRoute
    && !lifecycle.liveRouteFailure
    && autoFallbackSessionId !== lifecycle.sessionId) {
    autoFallbackSessionId = lifecycle.sessionId;
    activateStockFirstFallback().catch(() => {});
  }
}

const observeOnlyProbe = new ObserveOnlyProbeServer(publishProbeStatus);

async function activateStockFirstFallback() {
  if (autoFallbackActivation) return autoFallbackActivation;
  autoFallbackError = null;
  autoFallbackActivation = (async () => {
    const initial = observeOnlyProbe.status;
    if (!initial.armed || !initial.lifecycle?.active) {
      throw new Error('Waiting for a valid NBA 2K27 connection.');
    }
    if (initial.registrationProbeArmed) {
      throw new Error('The controlled path-aware test is active.');
    }
    if (initial.liveRouteArmed) return initial;
    await stopProbeProvider();
    const settings = await readSettings();
    const provider = await startProductionProvider({
      modsRoot: settings.modsRoot,
      hostExecutable: providerHostExecutable(),
    });
    activeProbeProvider = provider;
    try {
      const providerState = await provider.request({ operation: 'status' });
      if (!Number.isInteger(providerState.indexedAssets)
        || providerState.indexedAssets < 1) {
        throw new Error('The mods folder contains no files for the local fallback.');
      }
      const current = observeOnlyProbe.status;
      if (!current.armed || !current.lifecycle?.active
        || current.lifecycle.sessionId !== initial.lifecycle.sessionId) {
        throw new Error('The game session ended while the local provider was starting.');
      }
      observeOnlyProbe.armLiveRoute({ endpoint: provider.endpoint, assets: [] });
      return await observeOnlyProbe.waitForLiveRouteActivation();
    } catch (error) {
      await stopProbeProvider();
      throw error;
    }
  })();
  publishProbeStatus(observeOnlyProbe.status);
  try {
    return probeStatusView(await autoFallbackActivation);
  } catch (error) {
    autoFallbackError = error?.message || String(error);
    throw error;
  } finally {
    autoFallbackActivation = null;
    publishProbeStatus(observeOnlyProbe.status);
  }
}

function settingsPath() {
  return path.join(app.getPath('userData'), 'settings.json');
}

function providerHostExecutable() {
  return app.isPackaged
    ? runtimePath('provider-host', 'NBA2K27ProviderHost.exe')
    : path.join(__dirname, '..', 'provider-host', 'publish', 'NBA2K27ProviderHost.exe');
}

function observeOnlyAddonPath() {
  return app.isPackaged
    ? runtimePath('observe-only-probe', 'nba2k27_offline_id_probe.addon64')
    : path.join(__dirname, '..', '..', 'nba2k27-electron-offline-id-hook', 'dist', 'nba2k27_offline_id_probe.addon64');
}

function nativeAbiHarnessPath() {
  return app.isPackaged
    ? runtimePath('native-abi-harness', 'NBA2K27NativeAbiHarness.exe')
    : path.join(__dirname, '..', 'native-abi-harness', 'dist', 'NBA2K27NativeAbiHarness.exe');
}

function crossModuleHarnessDirectory() {
  return app.isPackaged
    ? runtimePath('cross-module-harness')
    : path.join(__dirname, '..', 'cross-module-harness', 'dist');
}

function cameraTemplateDirectory() {
  return app.isPackaged
    ? runtimePath('camera-tools')
    : path.join(__dirname, '..', '..', 'nba2k27-camera-tool', 'dist');
}

function providerResolverExecutable() {
  return app.isPackaged
    ? runtimePath('provider-resolver', 'NBA2K27ProviderResolver.exe')
    : path.join(__dirname, '..', '..', 'nba2k27-version-proxy', 'dist', 'NBA2K27ProviderResolver.exe');
}

function sessionProxyPath() {
  return app.isPackaged
    ? runtimePath('session-loader', 'version.dll')
    : path.join(__dirname, '..', '..', 'nba2k27-version-proxy', 'dist-scoreboard-text', 'version.dll');
}

function vczDecoderPath() {
  return app.isPackaged
    ? runtimePath('vcz-decoder', 'Nba2kVczDecoder.exe')
    : path.join(__dirname, '..', '..', 'nba2k27-electron-exporter', 'src', 'helpers', 'Nba2kVczDecoder.exe');
}

function fastFitDecoderPath() {
  return app.isPackaged
    ? runtimePath('fast-fit-decoder', 'Nba2kFastFitDecoder.exe')
    : path.join(__dirname, '..', 'fit-helper', 'Nba2kFastFitDecoder.exe');
}

function texconvExecutable() {
  return app.isPackaged
    ? runtimePath('texture-tools', 'texconv.exe')
    : path.join(__dirname, '..', '..', 'dist', 'CharacterModTool', '_internal', 'tools', 'texconv.exe');
}

function rosterResearchExecutable() {
  return app.isPackaged
    ? runtimePath('roster-research', 'NBA2K27RosterResearch.exe')
    : path.join(__dirname, '..', '..', 'nba2k27-roster-research', 'dist', 'NBA2K27RosterResearch.exe');
}

function playerFaceIdCatalogPath() {
  return path.join(__dirname, 'data', 'player-face-id-catalog.json');
}

async function refreshProviderLayout(settings, { force = false } = {}) {
  const gameExecutable = path.join(settings.gameRoot, 'NBA2K27.exe');
  if (!force && providerLayoutState.gameExecutable === gameExecutable
    && ['ready', 'blocked', 'error'].includes(providerLayoutState.status)) {
    return providerLayoutState;
  }
  if (providerLayoutPromise) return providerLayoutPromise;
  providerLayoutState = {
    status: 'scanning',
    gameExecutable,
    message: 'Resolving the provider layout from the local executable…',
    report: null,
  };
  providerLayoutPromise = (async () => {
    try {
      const report = await resolveProviderLayout({
        resolverExecutable: providerResolverExecutable(),
        gameExecutable,
        cacheDirectory: launcherStorage.mode==='organized'?cachePath('provider-layouts'):storagePath('data','provider-layouts'),
        useCache: !force,
      });
      providerLayoutState = {
        status: report.ok ? 'ready' : 'blocked',
        gameExecutable,
        message: report.ok
          ? 'Current build resolved automatically and passed every structural check.'
          : `Automatic resolver stopped safely: ${report.failure}`,
        report,
      };
    } catch (error) {
      providerLayoutState = {
        status: 'error',
        gameExecutable,
        message: error?.message || String(error),
        report: null,
      };
    }
    return providerLayoutState;
  })();
  try {
    return await providerLayoutPromise;
  } finally {
    providerLayoutPromise = null;
  }
}

let settingsCache=null;
let settingsWriteTail=Promise.resolve();
async function readSettings() {
  if(settingsCache)return structuredClone(settingsCache);
  let workspaceRoot;
  if (process.env.PORTABLE_EXECUTABLE_DIR) {
    workspaceRoot = path.resolve(process.env.PORTABLE_EXECUTABLE_DIR, '..', '..');
  } else if (app.isPackaged) {
    workspaceRoot = path.resolve(path.dirname(process.execPath), '..', '..', '..');
  } else {
    workspaceRoot = path.resolve(__dirname, '..', '..');
  }
  const base = defaults(workspaceRoot);
  try {
    const value = JSON.parse(await fsp.readFile(settingsPath(), 'utf8'));
    settingsCache=normalizeModProfiles({ ...base, ...(value && typeof value === 'object' ? value : {}), provider: 'local', startCameras: true });
  } catch (error) {
    if (error.code !== 'ENOENT' && error.name !== 'SyntaxError') throw error;
    settingsCache=normalizeModProfiles(base);
  }
  return structuredClone(settingsCache);
}

function writeSettings(patch) {
  const operation=settingsWriteTail.then(async()=>{
    const settings=normalizeModProfiles({...(await readSettings()),...(patch||{}),provider:'local',startCameras:true});
    const target=settingsPath(),temporary=`${target}.${process.pid}.${crypto.randomUUID()}.tmp`;let handle;
    await fsp.mkdir(path.dirname(target),{recursive:true});
    try{handle=await fsp.open(temporary,'wx');await handle.writeFile(`${JSON.stringify(settings,null,2)}\n`,'utf8');await handle.sync();await handle.close();handle=null;await fsp.rename(temporary,target);}
    finally{if(handle)await handle.close().catch(()=>{});await fsp.rm(temporary,{force:true}).catch(()=>{});}
    settingsCache=settings;return structuredClone(settings);
  });
  settingsWriteTail=operation.catch(()=>{});
  return operation;
}

function send(channel, payload) {
  launcherHealth.observe(channel,payload);
  if (mainWindow && !mainWindow.isDestroyed()) mainWindow.webContents.send(channel, payload);
}

function closeStartupWindow() {
  const window=startupWindow;
  startupWindow=null;
  if(window&&!window.isDestroyed())window.close();
}

function showStartupWindow() {
  startupWindow=new BrowserWindow({
    icon:path.join(__dirname,'assets','app-icon.ico'),
    width:420,height:240,resizable:false,maximizable:false,
    backgroundColor:'#0c1922',title:'Starting 2k Launcher',show:false,
    autoHideMenuBar:true,
    webPreferences:{contextIsolation:true,nodeIntegration:false,sandbox:true},
  });
  startupWindow.webContents.setWindowOpenHandler(()=>({action:'deny'}));
  startupWindow.webContents.on('will-navigate',event=>event.preventDefault());
  startupWindow.once('ready-to-show',()=>{
    if(startupWindow&&!startupWindow.isDestroyed()&&(!mainWindow||!mainWindow.isVisible()))startupWindow.show();
  });
  startupWindow.on('closed',()=>{startupWindow=null;});
  startupWindow.loadFile(path.join(__dirname,'renderer','startup-splash.html'));
}

function prepareBlacktopPanel() {
  if (!blacktopPanel || blacktopPanel.disposed) {
    const diagnostics = new PanelDiagnostics({ directory: logsPath(), version: app.getVersion() });
    const shortcut = new F8Shortcut({
      helperPath: app.isPackaged ? runtimePath('roster-research', 'NBA2K27F8Watcher.exe')
        : path.join(__dirname, '..', '..', 'nba2k27-roster-research', 'dist', 'NBA2K27F8Watcher.exe'),
      getGameRoot: async () => (await readSettings()).gameRoot,
      onError: error => { blacktopPanel?.report(error, 'f8-listener'); },
    });
    blacktopPanel = new BlacktopPanel({ BrowserWindow, globalShortcut: shortcut, screen, sourceDir: __dirname,
      onTrace: record => diagnostics.write(record),
      // Focusing the Electron app raises the main launcher above the game.
      // BlacktopPanel focuses its own expanded window after the user opens it.
      onPresent: () => { if (blacktopAutoOpen) blacktopAutoOpen.entered = true; },
      onHide: async () => { await blacktopTools().focusGame((await readSettings()).gameRoot); },
      onError: error => send('blacktop:panel-error', error.message) });
    blacktopPanel.register();
    blacktopPanel.prepare();
    blacktopAutoOpen = new BlacktopAutoOpen({ settings: readSettings,
      detect: gameRoot => blacktopTools().invoke('detect', gameRoot), panel: blacktopPanel,
      onObservation: value => balls().observeBlacktop(value),
      onError: (stage,error) => selectionStatusLog.record('outfit',{selected:'automatic',phase:'auto-error',message:`${stage}: ${error.message}`})
        .catch(logError=>console.warn('Outfit status diagnostic:',logError.message)),
      prepareLineup: (gameRoot,value) => blacktopTools().autoRandomSelection(gameRoot,value) });
    blacktopAutoOpen.start();
  }
}

function createWindow({preparePanel=true}={}) {
  if(preparePanel)prepareBlacktopPanel();
  mainWindow = new BrowserWindow({
    icon: path.join(__dirname, 'assets', 'app-icon.ico'),
    width: 1320,
    height: 860,
    minWidth: 1060,
    minHeight: 680,
    backgroundColor: '#090b10',
    title: '2k Launcher',
    autoHideMenuBar: true,
    show: false,
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
    },
  });
  Menu.setApplicationMenu(null);
  const mainPage=path.join(__dirname, 'renderer', 'index.html');
  const mainUrl=pathToFileURL(mainPage).href;
  mainWindow.webContents.setWindowOpenHandler(()=>({action:'deny'}));
  mainWindow.webContents.on('will-attach-webview',event=>event.preventDefault());
  mainWindow.webContents.on('will-navigate',(event,target)=>{if(target!==mainUrl)event.preventDefault();});
  mainWindow.loadFile(mainPage);
  mainWindow.once('ready-to-show', () => {mainWindow.show();closeStartupWindow();});
  mainWindow.on('unresponsive',()=>void shutdownWarning('main window unresponsive',Error('The launcher renderer stopped responding.')));
  mainWindow.webContents.on('render-process-gone',(_event,details)=>void shutdownWarning('main renderer stopped',Error(`${details.reason}, exit ${details.exitCode}`)));
  mainWindow.webContents.on('did-fail-load',(_event,code,description,url,isMainFrame)=>{if(isMainFrame)void shutdownWarning('main window load failed',Error(`${code} ${description} ${url}`));});
  protectWorkspaceClose(mainWindow);
  mainWindow.on('closed', () => { mainWindow = null; });
}

ipcMain.handle('app:bootstrap', async () => {
  await waitForInitialization();
  const settings = await readSettings();
  const validation = await validateSettings(settings);
  const providers = await Promise.all(Object.keys(PROVIDERS).map((id) => providerStatus(id, settings)));
  const cameras = await cameraManager.view(settings.modsRoot);
  const providerLayout = await refreshProviderLayout(settings);
  const jukebox = await jukeboxManager.view(settings.modsRoot);
  const presentation = await presentationView(settings.modsRoot);
  return {
    version: app.getVersion(), settings, validation, providers,
    session: session.state,
    loader: versionProxySession?.state || null,
    probe: probeStatusView(observeOnlyProbe.status), cameras, providerLayout,
    playerResearch: playerResearchManager.view(), jukebox, presentation,
  };
});

ipcMain.handle('settings:save', async (_event, patch) => {
  if(presentationManager.busy&&(patch?.modsRoot!==undefined||patch?.gameRoot!==undefined||patch?.startupVideoEnabled!==undefined))throw Error('Finish or cancel the video import before changing its profile or playback setting.');
  if(patch?.cachePreparedPlaylists!==undefined&&typeof patch.cachePreparedPlaylists!=='boolean')throw new Error('Invalid playlist cache preference.');
  if(patch?.startupVideoEnabled!==undefined&&typeof patch.startupVideoEnabled!=='boolean')throw new Error('Invalid presentation preference.');
  const previous=await readSettings();
  if(launcherStorage.placement==='game'&&patch?.gameRoot!==undefined&&path.resolve(patch.gameRoot).toLowerCase()!==launcherStorage.gameRoot.toLowerCase())throw Error('This launcher library is still stored inside the current NBA 2K27 folder. Finish its AppData migration before changing the game path.');
  const settings = await writeSettings(patch);
  if(settings.cachePreparedPlaylists===false&&!await processIsRunning('NBA2K27.exe')){
    await jukeboxManager.clearLaunchCache(settings.modsRoot);
    await jukeboxSession.clearConversionCache(0);
  }
  if(previous.gameRoot!==settings.gameRoot)providerLayoutState = { status: 'pending', message: 'Game path changed; waiting to rescan.', report: null };
  return { settings, validation: await validateSettings(settings) };
});

async function presentationView(modsRoot) {
  return presentationManager.view(modsRoot);
}

ipcMain.handle('presentation:view',async event=>{requireLauncherSender(event);return presentationView((await readSettings()).modsRoot);});
ipcMain.handle('presentation:choose-movie',async event=>{
  requireLauncherSender(event);
  const settings=await readSettings();
  if(launchInProgress)throw Error('Wait for game launch to finish.');
  const result=await ownedDialog(mainWindow,()=>dialog.showOpenDialog(mainWindow,{title:'Add startup MP4',properties:['openFile'],filters:[{name:'MP4 videos',extensions:['mp4']}]}));
  if(result.canceled||!result.filePaths[0])return null;
  if(launchInProgress)throw Error('Wait for game launch to finish.');
  if((await readSettings()).modsRoot!==settings.modsRoot)throw Error('The mod profile changed. Please add the video again.');
  await presentationManager.importMovie(settings.modsRoot,result.filePaths[0],{addToLibrary:true});
  // Importing media must not opt the user into startup playback.
  const saved=await readSettings();
  return {presentation:await presentationView(saved.modsRoot),settings:saved};
});
ipcMain.handle('presentation:library-edit',async(event,action,id,name)=>{
  requireLauncherSender(event);if(launchInProgress)throw Error('Wait for game launch to finish.');
  const settings=await readSettings();
  const presentation=await presentationManager.editLibrary(settings.modsRoot,action,id,name);
  return {presentation,settings};
});
ipcMain.handle('presentation:preview',async(event,id)=>{
  requireLauncherSender(event);
  if(launchInProgress||!videoOverlay)throw Error('Wait for launcher initialization or game launch to finish.');
  const videoPath=await presentationManager.previewFile(id);
  if(launchInProgress)throw Error('Wait for game launch to finish.');
  await videoOverlay.show({videoPath,gamePath:path.join((await readSettings()).gameRoot,'NBA2K27.exe'),preview:true,owner:mainWindow});
  return true;
});
ipcMain.handle('presentation:cancel',event=>{requireLauncherSender(event);presentationManager.cancel();});
ipcMain.handle('presentation:open-folder',async event=>{
  requireLauncherSender(event);const settings=await readSettings();const value=await presentationManager.view(settings.modsRoot);
  const folder=launcherStorage.mode==='organized'?launcherStorage.videos:value.directory;
  await fsp.mkdir(folder,{recursive:true});const error=await shell.openPath(folder);if(error)throw new Error(error);return folder;
});

ipcMain.handle('resolver:status', async () => refreshProviderLayout(await readSettings()));
ipcMain.handle('resolver:rescan', async () => refreshProviderLayout(await readSettings(), { force: true }));
ipcMain.handle('resolver:copy', async () => {
  const current = await refreshProviderLayout(await readSettings());
  clipboard.writeText(JSON.stringify(current, null, 2));
  return true;
});

ipcMain.handle('settings:choose', async (_event, kind) => {
  const settings = await readSettings();
  const directory = kind === 'gameRoot' || kind === 'modsRoot';
  const titles = {
    gameRoot: 'Choose the NBA 2K27 game folder',
    modsRoot: 'Choose the live mods folder',
  };
  if (!titles[kind]) throw new Error('Unknown path setting.');
  const result = await dialog.showOpenDialog(mainWindow, {
    title: titles[kind],
    defaultPath: settings[kind] || settings.gameRoot,
    properties: directory ? ['openDirectory'] : ['openFile'],
  });
  return result.canceled ? null : result.filePaths[0];
});
ipcMain.handle('support:export',async event=>{
  requireLauncherSender(event);
  const settings=await readSettings(),validation=await validateSettings(settings);
  const result=await ownedDialog(mainWindow,()=>dialog.showSaveDialog(mainWindow,{title:'Export redacted support bundle',defaultPath:`2k-launcher-support-${app.getVersion()}.json`,filters:[{name:'2k Launcher support bundle',extensions:['json']}],properties:['showOverwriteConfirmation']}));
  if(result.canceled||!result.filePath)return null;
  const support=await require('./core/support-bundle').createSupportBundle({userData:app.getPath('userData'),diagnosticDirectory:logsPath(),version:app.getVersion(),settings,validation});
  await fsp.writeFile(result.filePath,support.text,'utf8');return {filePath:result.filePath,files:support.files,truncated:support.truncated};
});
ipcMain.handle('diagnostics:status',event=>{requireLauncherSender(event);return gameCapture.status();});
ipcMain.handle('diagnostics:capture',async event=>{requireLauncherSender(event);return gameCapture.capture();});
ipcMain.handle('diagnostics:folder',async event=>{requireLauncherSender(event);await shell.openPath(logsPath());return logsPath();});

ipcMain.handle('mods:scan', async () => {
  const settings = await readSettings();
  catalog = await modTagStore.apply(await scanMods(settings.modsRoot));
  return catalog;
});

ipcMain.handle('mods:open-item',async(event,value)=>{
  requireLauncherSender(event);const settings=await readSettings();const target=await resolveCatalogFile(settings.modsRoot,catalog,value?.fileId);
  if(value?.action==='folder'){shell.showItemInFolder(target);return true;}
  if(value?.action!=='file')throw Error('Unknown mod-library open action.');const error=await shell.openPath(target);if(error)throw Error(error);return true;
});

ipcMain.handle('mods:install',async event=>{
  requireLauncherSender(event);if(await processIsRunning('NBA2K27.exe'))throw Error('Close NBA 2K27 before installing mods.');const settings=await readSettings();
  const kind=await ownedDialog(mainWindow,()=>dialog.showMessageBox(mainWindow,{type:'question',title:'Install mods',message:'Choose what you want to install.',detail:'ZIP and RAR archives are inspected before extraction. A folder is scanned recursively.',buttons:['Files or archives','Folder','Cancel'],defaultId:0,cancelId:2,noLink:true}));
  if(kind.response===2)return {cancelled:true};const mode=kind.response===1?'folder':'files';
  const selection=await ownedDialog(mainWindow,()=>dialog.showOpenDialog(mainWindow,{title:mode==='folder'?'Choose a mod folder':'Choose mod files, ZIPs, or RARs',properties:mode==='folder'?['openDirectory']:['openFile','multiSelections'],filters:mode==='folder'?undefined:[{name:'Mod packages',extensions:['zip','rar','iff','bin','dds']},{name:'All files',extensions:['*']}]}));
  if(selection.canceled||!selection.filePaths.length)return {cancelled:true};
  const prepared=await prepareModSources({paths:selection.filePaths,mode,temporaryRoot:app.getPath('temp')});
  try{
   const plan=await planModInstall({gameRoot:settings.gameRoot,modsRoot:settings.modsRoot,sources:prepared.sources});
   if(plan.issues.length)return {cancelled:false,installed:0,issues:plan.issues,ignored:plan.ignored?.length||0};let replace=false;
   if(plan.replacements){const answer=await ownedDialog(mainWindow,()=>dialog.showMessageBox(mainWindow,{type:'warning',title:'Replace existing mod files?',message:`${plan.replacements} destination${plan.replacements===1?' already contains a mod':'s already contain mods'}.`,detail:'The package was routed from its verified folder structure, the stock manifest, and safe player-file rules. Replaced files cannot be recovered by the launcher.',buttons:['Cancel','Replace and install'],defaultId:0,cancelId:0,noLink:true}));if(answer.response!==1)return {cancelled:true};replace=true;}
   const result=await installModPlan(plan,{replace});catalog=await modTagStore.apply(await scanMods(settings.modsRoot));return {...result,cancelled:false,issues:[],ignored:plan.ignored?.length||0,archives:prepared.archiveCount,folders:prepared.folderCount,catalog};
  }finally{await prepared.cleanup();}
});

ipcMain.handle('mods:save-tags', async (_event, value) => {
  const settings = await readSettings();
  if (!catalog || canonicalRoot(catalog.root) !== canonicalRoot(settings.modsRoot)) {
    catalog = await modTagStore.apply(await scanMods(settings.modsRoot));
  }
  const fileId = String(value?.fileId || '').replaceAll('\\', '/').toLowerCase();
  if (!catalog.files.some((file) => file.id === fileId)) throw new Error('That mod file is no longer present. Scan the profile and try again.');
  await modTagStore.set(settings.modsRoot, fileId, value?.tags);
  catalog = await modTagStore.apply(catalog);
  return catalog;
});

ipcMain.handle('provider:diagnose', async (_event, logicalPath) => {
  const settings = await readSettings();
  const resolver = await offlineProviderDiagnostic(settings.modsRoot, logicalPath);
  const channel = await productionProviderDiagnostic({
    modsRoot: settings.modsRoot,
    hostExecutable: providerHostExecutable(),
    logicalPath: resolver.logicalPath,
  });
  return { resolver, channel };
});

ipcMain.handle('bridge:audit', async (_event, logicalPath) => {
  const settings = await readSettings();
  const resolver = await offlineProviderDiagnostic(settings.modsRoot, logicalPath);
  return runBridgeAudit({
    gameRoot: settings.gameRoot,
    modsRoot: settings.modsRoot,
    logicalPath: resolver.logicalPath,
    nativeHarnessPath: nativeAbiHarnessPath(),
    crossModuleHarnessDirectory: crossModuleHarnessDirectory(),
    observeOnlyAddonPath: observeOnlyAddonPath(),
  });
});
ipcMain.handle('probe:start', async (_event, options = {}) => {
  await observeOnlyProbe.stop();
  await stopProbeProvider();
  const registrationProbe = options?.registrationProbe === true;
  autoFallbackSessionId = null;
  autoFallbackError = null;
  if (!registrationProbe) return probeStatusView(await observeOnlyProbe.start());
  const settings = await readSettings();
  const resolver = await offlineProviderDiagnostic(
    settings.modsRoot, String(options?.logicalPath || 'png10000.iff'));
  const asset = await resolveAsset(settings.modsRoot, resolver.logicalPath);
  const secondRequested = asset.logicalPath.toLowerCase() === 'loc/english.iff'
    ? 'char/sig/png10000.iff'
    : 'loc/english.iff';
  const secondResolver = await offlineProviderDiagnostic(
    settings.modsRoot, secondRequested);
  const secondAsset = await resolveAsset(settings.modsRoot, secondResolver.logicalPath);
  if (asset.logicalPath.toLowerCase() === secondAsset.logicalPath.toLowerCase()) {
    throw new Error('The path-aware test requires two distinct local assets.');
  }
  if ([asset, secondAsset].some((item) =>
    item.size <= 0 || item.size > MAX_LIVE_PROBE_ASSET_BYTES)) {
    throw new Error(`Live provider test assets must be from 1 byte to ${MAX_LIVE_PROBE_ASSET_BYTES / 1024 / 1024} MB.`);
  }
  const [sha256, secondSha256] = await Promise.all([
    sha256File(asset.absolutePath),
    sha256File(secondAsset.absolutePath),
  ]);
  const provider = await startProductionProvider({
    modsRoot: settings.modsRoot,
    hostExecutable: providerHostExecutable(),
  });
  activeProbeProvider = provider;
  try {
    const [metadata, secondMetadata] = await Promise.all([
      provider.request({ operation: 'query', logicalPath: asset.logicalPath }),
      provider.request({ operation: 'query', logicalPath: secondAsset.logicalPath }),
    ]);
    if (metadata.size !== asset.size || secondMetadata.size !== secondAsset.size) {
      throw new Error('Provider metadata changed during arming.');
    }
    return await observeOnlyProbe.start({
      registrationProbe: true,
      providerConfig: {
        endpoint: provider.endpoint,
        assets: [
          { logicalPath: asset.logicalPath, size: asset.size, sha256 },
          {
            logicalPath: secondAsset.logicalPath,
            size: secondAsset.size,
            sha256: secondSha256,
          },
        ],
      },
    });
  } catch (error) {
    await stopProbeProvider();
    throw error;
  }
});
ipcMain.handle('probe:activate-live-route', async () => {
  autoFallbackSessionId = observeOnlyProbe.status.lifecycle?.sessionId || null;
  return activateStockFirstFallback();
});
ipcMain.handle('probe:stop', async () => {
  const status = await observeOnlyProbe.stop();
  await stopProbeProvider();
  autoFallbackError = null;
  return probeStatusView(status);
});
ipcMain.handle('probe:status', async () => probeStatusView(observeOnlyProbe.status));
ipcMain.handle('shell:open-probe', async () => {
  const probePath = observeOnlyAddonPath();
  const addon = await inspectObserveOnlyAddon(probePath);
  if (!addon.verified) throw new Error(addon.reason || 'The observe-only add-on is not verified.');
  shell.showItemInFolder(probePath);
  return true;
});

ipcMain.handle('camera:status', async () => {
  const settings = await readSettings();
  return cameraManager.view(settings.modsRoot);
});
ipcMain.handle('camera:save', async (_event, payload) => {
  const settings = await readSettings();
  await cameraManager.saveLibrary(payload);
  return cameraManager.view(settings.modsRoot);
});
ipcMain.handle('camera:start', async (_event, payload) => {
  const settings = await readSettings();
  if (payload) await cameraManager.saveLibrary(payload);
  return cameraManager.start(settings.modsRoot);
});
ipcMain.handle('camera:stop', async () => {
  const settings = await readSettings();
  return cameraManager.stop(settings.modsRoot);
});
ipcMain.handle('camera:install-names', async () => {
  const settings = await readSettings();
  await cameraManager.installNamesMod(settings.modsRoot);
  return cameraManager.view(settings.modsRoot);
});

registerRoster('player-research:status', async () => playerResearchManager.view());
let rosterAutoAttach;
registerRoster('roster:auto-attach', event => {
  rosterOwner(event);
  rosterAutoAttach ||= new (require('./core/roster-auto-attach').RosterAutoAttach)({
    running: () => processIsRunning('NBA2K27.exe'),
    view: () => playerResearchManager.view(),
    scan: async () => {
      const settings = await readSettings();
      return playerResearchManager.scan({ gameRoot: settings.gameRoot, modsRoot: settings.modsRoot });
    },
  });
  return rosterAutoAttach.poll();
});
let playerArtBusy = false;
let rosterArtwork = null;
let playerHeaderLabels = null;
let badgeIcons = null;
registerRoster('player-research:header-labels', async (event, request) => {
  if (!rosterSender(event) || !Number.isInteger(request?.recordIndex) || typeof request.playerKey !== 'string' || request.playerKey.length>256) return null;
  const player=playerResearchManager.view().players?.find(p=>p.source==='live-roster'&&p.recordIndex===request.recordIndex&&JSON.stringify([p.firstName||'',p.lastName||'',p.cyberfaceId??null])===request.playerKey);
  if(!player?.archetypeKey)return null;
  const settings=await readSettings();
  if(!playerHeaderLabels)playerHeaderLabels=new (require('./core/player-header-labels').PlayerHeaderLabels)({cacheRoot:launcherStorage.mode==='organized'?cachePath('Player Editor','header-labels'):cachePath('player-editor','header-labels'),decoderPath:vczDecoderPath()});
  try{return {archetypeName:await playerHeaderLabels.get(settings.gameRoot,player.archetypeKey)};}catch{return null;}
});
registerRoster('player-research:artwork', async (event, request) => {
  if (!rosterSender(event) || !request || !['portrait','logo'].includes(request.kind)
      || typeof request.key !== 'string' || !request.key.length || request.key.length > 256) throw new Error('Invalid artwork request.');
  const key = crypto.createHash('sha256').update(request.kind + ':' + request.key).digest('hex');
  const file = path.join(app.getPath('userData'), 'player-editor-artwork.json');
  const read = async () => { try { if ((await fsp.stat(file)).size > 32*1024*1024) throw new Error('Artwork library exceeds its size limit.'); return JSON.parse(await fsp.readFile(file,'utf8')); } catch(error) { if(error.code==='ENOENT') return {}; throw error; } };
  if (!request.choose) {
    const manual=(await read())[key];if(manual)return manual;
    const settings=await readSettings();if(!settings.gameRoot)return null;
    const state=playerResearchManager.view();
    const player=request.preview===true ? {firstName:'Joel',lastName:'Embiid',cyberfaceId:null,teamName:'Philadelphia 76ers',preview:true}
      : state.players?.find(p=>p.recordIndex===request.recordIndex&&JSON.stringify([p.firstName||'',p.lastName||'',p.cyberfaceId??null])===request.playerKey);
    if(!player)return null;
    let asset;
    if(request.kind==='portrait'){
      const fields=require('./data/player-field-schema.json');
      asset=player.preview?4090:Array.isArray(player.ratingsRaw)&&player.ratingsRaw.length===fields.length?player.ratingsRaw[fields.findIndex(f=>f.key==='PhotoId')]:null;
    }else{
      const team=state.teams?.find(t=>t.recordIndex===player.teamIndex);
      asset=player.preview?'logo000':/^logo\d/i.test(team?.mainLogo||'')?team.mainLogo
        :Number.isInteger(team?.teamId)&&team.teamId>=0&&team.teamId<=65535?'logo'+String(team.teamId).padStart(3,'0'):null;
    }
    if(!rosterArtwork)rosterArtwork=new (require('./core/roster-artwork').RosterArtwork)({cacheRoot:launcherStorage.mode==='organized'?cachePath('Player Editor','art-cache'):cachePath('player-editor','art-cache'),decoderPath:vczDecoderPath(),texconvPath:texconvExecutable()});
    return rosterArtwork.get(settings.gameRoot,request.kind==='logo'?'background':request.kind,asset);
  }
  if (playerArtBusy) throw new Error('An artwork selection is already open.');
  playerArtBusy = true;
  try {
    const owner=rosterOwner(event);
    const result = await ownedDialog(owner, () => dialog.showOpenDialog(owner, { title: request.kind==='portrait'?'Choose player portrait':'Choose team logo', properties:['openFile'], filters:[{name:'Images',extensions:['png','jpg','jpeg','webp']}] }));
    if(result.canceled || !result.filePaths[0]) return null;
    const source=result.filePaths[0];if((await fsp.stat(source)).size>16*1024*1024) throw new Error('Choose an image smaller than 16 MB.');
    let image=require('electron').nativeImage.createFromBuffer(await fsp.readFile(source));if(image.isEmpty())throw new Error('This image could not be opened.');
    const size=image.getSize();if(size.width>512||size.height>512)image=image.resize(size.width>=size.height?{width:512}:{height:512});
    const value=image.toDataURL();if(value.length>2*1024*1024)throw new Error('Choose a smaller image.');
    const library=await read();library[key]=value;const data=JSON.stringify(library);if(Buffer.byteLength(data)>32*1024*1024)throw new Error('The artwork library is full.');
    await fsp.writeFile(file+'.tmp',data);await fsp.rename(file+'.tmp',file);return value;
  } finally { playerArtBusy=false; }
});
registerRoster('player-research:badge-icons', async (event, request) => {
  if(!rosterSender(event)||!Array.isArray(request?.labels)||request.labels.length>64)throw new Error('Invalid badge icon request.');
  const settings=await readSettings();if(!settings.gameRoot)return {};
  if(!badgeIcons)badgeIcons=new (require('./core/badge-icons').BadgeIcons)({cacheRoot:launcherStorage.mode==='organized'?cachePath('Player Editor','badge-icons'):cachePath('player-editor','badge-icons'),decoderPath:vczDecoderPath(),texconvPath:texconvExecutable()});
  try{return await badgeIcons.get(settings.gameRoot,request.labels);}catch(error){console.warn('Badge icons unavailable:',error.message);return {};}
});
registerRoster('player-research:preview-audio', async (event, request) => {
  rosterOwner(event);
  const settings = await readSettings();
  if (!settings.gameRoot) throw new Error('Select the NBA 2K27 folder first.');
  return require('./core/player-audio-preview').preview(settings.gameRoot, request);
});
registerRoster('player-research:scan', async () => {
  const settings = await readSettings();
  return playerResearchManager.scan({ gameRoot: settings.gameRoot, modsRoot: settings.modsRoot });
});
registerRoster('player-research:export', async event => {
  const selection = await dialog.showSaveDialog(rosterOwner(event), {
    title: 'Export live NBA 2K27 player catalog',
    defaultPath: 'NBA2K27-Player-Cyberface-Catalog.json',
    filters: [{ name: 'JSON catalog', extensions: ['json'] }],
  });
  if (selection.canceled || !selection.filePath) return null;
  return playerResearchManager.exportCatalog(selection.filePath);
});
let playerDnaBusy=false;
let nativeRosterExportService, nativeRosterExportBusy=false;
registerRoster('roster:native-export',async(event,request)=>{
 const owner=rosterOwner(event);
 if(!request||!['list','export'].includes(request.action))throw Error('Unsupported native export action. Imports are not enabled.');
 if(nativeRosterExportBusy)throw Error('A saved-roster export is already running.');
 nativeRosterExportBusy=true;
 try{
  nativeRosterExportService ||= new (require('./core/native-roster-export').NativeRosterExport)({verify:require('./core/native-roster-connection').verify});
  if(request.action==='list')return await nativeRosterExportService.list();
  const result=await nativeRosterExportService.export(request.slot);
  const filename=result.name.replace(/[<>:"/\\|?*\x00-\x1f]/g,'_').slice(0,63)||'Roster';
  const choice=await ownedDialog(owner,()=>dialog.showSaveDialog(owner,{title:'Export one saved NBA roster',defaultPath:filename+'.2kroster',filters:[{name:'Native NBA roster',extensions:['2kroster']}]}));
  if(choice.canceled||!choice.filePath)return null;
  // Never overwrite an existing file. Publish a complete fsynced export only.
  const temporary=path.join(path.dirname(choice.filePath),'.roster-export-'+crypto.randomUUID()+'.tmp');
  let handle,owned=false;
  try{
   handle=await fsp.open(temporary,'wx');owned=true;await handle.writeFile(result.bytes);await handle.sync();await handle.close();handle=null;
   await fsp.link(temporary,choice.filePath);
  }catch(error){if(error.code==='EEXIST')throw Error('That file already exists. Export again with a new filename.');throw error;}
  finally{if(handle)await handle.close();if(owned)await fsp.unlink(temporary);}
  return {saved:true,name:result.name,bytes:result.bytes.length,importEnabled:false};
 }finally{nativeRosterExportBusy=false;}
});
registerRoster('roster:transfer',async(event,request)=>{
 const owner=rosterOwner(event),transfer=require('./core/roster-transfer');
 rosterTransferSession ||= new (require('./core/roster-transfer-session').RosterTransferSession)({manager:playerResearchManager,backupRoot:launcherStorage.mode==='organized'?storagePath('recovery','roster-backups'):storagePath('data','roster-backups'),scan:async()=>{const settings=await readSettings();return playerResearchManager.scan({gameRoot:settings.gameRoot,modsRoot:settings.modsRoot});}});
 const session=rosterTransferSession;
 if(request?.action==='status')return session.status();
 if(request?.action==='cancel')return session.cancel();
 if(session.busy)throw Error('A roster operation is already running.');
 if(request?.action==='apply'){
  throw Error('Whole-roster writes are disabled pending roster identity, ownership and recovery validation. Export and preview are read-only.');
 }
 if(request?.action==='export'){
  session.busy=true;
  try{
   const data=transfer.snapshot(await session.scan());
   const choice=await ownedDialog(owner,()=>dialog.showSaveDialog(owner,{title:'Export mapped roster data',defaultPath:'Roster.2kroster',filters:[{name:'2k roster data',extensions:['2kroster']}]}));
   if(choice.canceled||!choice.filePath)return null;
   await fsp.writeFile(choice.filePath,JSON.stringify(data),'utf8');return {saved:true,players:data.players.length,teams:data.teams.length};
  }finally{session.busy=false;}
 }
 if(request?.action==='preview'){
  const choice=await ownedDialog(owner,()=>dialog.showOpenDialog(owner,{title:'Preview roster import',properties:['openFile'],filters:[{name:'2k roster data',extensions:['2kroster']}]}));
  if(choice.canceled||!choice.filePaths[0])return null;
  if((await fsp.stat(choice.filePaths[0])).size>transfer.MAX_BYTES)throw Error('Roster files must be smaller than 64 MB.');
  const data=transfer.parse(await fsp.readFile(choice.filePaths[0],'utf8'));
  return session.prepare(data,request.sections,request.includeTeams===true);
 }
 throw Error('Unsupported roster transfer action.');
});
registerRoster('player-dna:transfer', async (event, request) => {
  const owner=rosterOwner(event);
  const dna=require('./core/player-dna');
  if(request?.action==='2kdb')return require('./core/twokdb-browser').read(BrowserWindow,owner,request.url);
  if(request?.action==='players')return (playerResearchManager.view().players||[]).filter(p=>p.source==='live-roster').map(p=>({recordIndex:p.recordIndex,name:[p.firstName,p.lastName].filter(Boolean).join(' '),teamName:p.teamName||'',key:JSON.stringify([p.firstName,p.lastName,p.cyberfaceId])}));
  if(request?.action==='copy'){
    const p=playerResearchManager.view().players?.find(p=>p.source==='live-roster'&&p.recordIndex===request.recordIndex&&JSON.stringify([p.firstName,p.lastName,p.cyberfaceId])===request.key);
    const result=dna.fromPlayer(p);
    if(request.sections?.includes('seasons')){
      const reader=new (require('./core/player-season-service').PlayerSeasonService)(playerResearchManager.helperPath);
      result.seasonHistory=require('./renderer/player-season-transfer').validate(await reader.read(p));result.version=2;
    }
    return dna.subset(result,request.sections);
  }
  if(!['import','export'].includes(request?.action))throw new Error('Invalid player DNA action.');
  if(playerDnaBusy)throw new Error('A player DNA file dialog is already open.');playerDnaBusy=true;
  try{
    if(request.action==='import'){
      const selection=await ownedDialog(owner,()=>dialog.showOpenDialog(owner,{title:'Import player DNA',properties:['openFile'],filters:[{name:'Player DNA',extensions:['2kplayer','json']}]}));
      if(selection.canceled||!selection.filePaths[0])return null;
      if((await fsp.stat(selection.filePaths[0])).size>256*1024)throw new Error('Player DNA must be smaller than 256 KB.');
      return dna.subset(dna.parse(await fsp.readFile(selection.filePaths[0],'utf8')),request.sections);
    }
    const payload=dna.subset(dna.validate(request.data),request.sections),name=payload.name.replace(/[^\p{L}\p{N} _-]/gu,'').trim().slice(0,60)||'Player';
    const selection=await ownedDialog(owner,()=>dialog.showSaveDialog(owner,{title:'Export player DNA',defaultPath:name+'.2kplayer',filters:[{name:'Player DNA',extensions:['2kplayer']}]}));
    if(selection.canceled||!selection.filePath)return null;
    await fsp.writeFile(selection.filePath,JSON.stringify(payload,null,2)+'\n','utf8');return {saved:true};
  }finally{playerDnaBusy=false;}
});
let player23Catalog,player23CatalogBusy=false;
let basketballReferenceStats;
registerRoster('player:reference-stats',async(event,url)=>{
 rosterOwner(event);
 basketballReferenceStats ||= new (require('./core/basketball-reference-stats').BasketballReferenceStats)();
 return basketballReferenceStats.load(url);
});
registerRoster('player23:catalog',async(event,request)=>{
 const owner=rosterOwner(event);
 player23Catalog ||= new (require('./core/player23-catalog').Player23Catalog)(launcherStorage.mode==='organized'?launcherStorage.root:app.getPath('userData'));
 if(request?.action==='status')return player23Catalog.load();
 if(request?.action==='search')return player23Catalog.search(request);
 if(request?.action==='player')return player23Catalog.player(request);
 if(request?.action==='selection')return player23Catalog.selection(request);
 if(request?.action!=='import')throw Error('Invalid 2K23 catalog action.');
 if(player23CatalogBusy)throw Error('A 2K23 catalog import is already open.');player23CatalogBusy=true;
 try{
  const selection=await ownedDialog(owner,()=>dialog.showOpenDialog(owner,{title:'Import 2K23 player catalog',properties:['openFile'],filters:[{name:'2K23 player / season catalog',extensions:['2k23catalog','2k23stats']}]}));
  if(selection.canceled||!selection.filePaths[0])return null;
  return await player23Catalog.import(selection.filePaths[0]);
 }finally{player23CatalogBusy=false;}
});
let playerSeasonService;
let playerSeasonImportService;
registerRoster('player:seasons',async(event,request)=>{
  const owner=rosterOwner(event);
  const player=playerResearchManager.view().players?.find(p=>p.source==='live-roster'&&p.recordIndex===request?.recordIndex&&p.firstName===request.firstName&&p.lastName===request.lastName&&p.cyberfaceId===request.cyberfaceId);
  if(!player)throw new Error('Refresh the live roster before opening season stats.');
  if(['teams','review-import','apply-import','cancel-import'].includes(request.action)){
    playerSeasonImportService ||= new (require('./core/player-season-import-service').PlayerSeasonImportService)(playerResearchManager.helperPath,{backupRoot:launcherStorage.mode==='organized'?storagePath('recovery','season-imports'):path.join(app.getPath('userData'),'season-imports')});
    if(request.action==='teams')return playerSeasonImportService.teams(player);
    if(request.action==='cancel-import'){if(playerSeasonImportService.busy)throw Error('Wait for the import to finish.');playerSeasonImportService.clear();return {canceled:true};}
    if(request.action==='review-import')return playerSeasonImportService.review(player,request);
    const result=await playerSeasonImportService.apply(player,request,async message=>{
      if(event===rosterWebEvent)return request.confirmed===true;
      const answer=await ownedDialog(owner,()=>dialog.showMessageBox(owner,{type:'warning',title:'Import selected seasons?',message,buttons:['Cancel','Apply selected seasons'],defaultId:0,cancelId:0,noLink:true}));return answer.response===1;
    });
    if(result.saved){playerSeasonService?.cache.clear();if(playerSeasonService)playerSeasonService.revision++;}
    return result;
  }
  playerSeasonService ||= new (require('./core/player-season-service').PlayerSeasonService)(playerResearchManager.helperPath,{backupRoot:launcherStorage.mode==='organized'?storagePath('recovery','season-edit-backups'):path.join(app.getPath('userData'),'season-edit-backups')});
  if(request.action==='read')return playerSeasonService.read(player);
  if(['add-older','remove-oldest'].includes(request.action))return playerSeasonService.structure(player,request,async message=>{
    if(event===rosterWebEvent)return request.confirmed===true;
    const result=await ownedDialog(owner,()=>dialog.showMessageBox(owner,{type:'warning',title:'Change season history?',message,detail:'Only this player is affected. Save the roster in-game to retain the change. Recovery snapshots are kept in the launcher folder.',buttons:['Cancel',request.action==='add-older'?'Add older season':'Remove oldest season'],defaultId:0,cancelId:0,noLink:true}));return result.response===1;
  });
  if(request.action!=='save')throw new Error('Invalid season action.');
  return playerSeasonService.save(player,request,async message=>{
    const result=await ownedDialog(owner,()=>dialog.showMessageBox(owner,{type:'warning',title:'Save season history?',message,detail:'Save the roster in-game to retain these edits.',buttons:['Cancel','Save season'],defaultId:0,cancelId:0,noLink:true}));return result.response===1;
  });
});
registerRoster('player:save', async (_event, edit) => {
  rosterOwner(_event);
  const settings = await readSettings();
  const report = await playerResearchManager.editPlayerWithRefresh(edit, { gameRoot: settings.gameRoot, modsRoot: settings.modsRoot });
  const playerResearch = await playerResearchManager.scan({ gameRoot: settings.gameRoot, modsRoot: settings.modsRoot });
  return { report, playerResearch };
});
registerRoster('team:save', async (_event, edit) => {
  rosterOwner(_event);
  const report = await playerResearchManager.editTeam(edit,{backupRoot:storagePath('recovery','team-edit-backups')});
  const settings = await readSettings();
  const playerResearch = await playerResearchManager.scan({ gameRoot: settings.gameRoot, modsRoot: settings.modsRoot });
  return { report, playerResearch };
});
let teamStaffService;
let teamRotationService;
registerRoster('team:rotations',async(event,request)=>{
 rosterOwner(event);
 teamRotationService ||= new (require('./core/team-rotations').TeamRotationService)(playerResearchManager,{backupRoot:storagePath('recovery','rotation-edit-backups')});
 if(request?.action==='read')return teamRotationService.read(request);
 if(request?.action==='save')return teamRotationService.save(request);
 throw Error('Invalid team rotations action.');
});
let teamDnaBusy=false;
let editorPresets;
registerRoster('editor:presets',async(event,request)=>{
 rosterOwner(event);
 editorPresets ||= new (require('./core/editor-presets').EditorPresets)(path.join(launcherStorage.mode==='organized'?launcherStorage.root:app.getPath('userData'),'Library','editor-presets'));
 return editorPresets.request(request);
});
registerRoster('team-dna:transfer',async(event,request)=>{
 const owner=rosterOwner(event),dna=require('./core/team-dna');
 if(!['import','export'].includes(request?.action))throw Error('Invalid Team DNA action.');
 if(teamDnaBusy)throw Error('A Team DNA file dialog is already open.');teamDnaBusy=true;
 try{
  if(request.action==='import'){
   const choice=await ownedDialog(owner,()=>dialog.showOpenDialog(owner,{title:'Import Team DNA',properties:['openFile'],filters:[{name:'Team DNA',extensions:['2kteam']}]}));
   if(choice.canceled||!choice.filePaths[0])return null;
   if((await fsp.stat(choice.filePaths[0])).size>512*1024)throw Error('Team DNA must be smaller than 512 KB.');
   return dna.subset(dna.parse(await fsp.readFile(choice.filePaths[0],'utf8')),request.sections);
  }
  const payload=dna.subset(request.data,request.sections),name=payload.name.replace(/[^\p{L}\p{N} _-]/gu,'').trim().slice(0,60)||'Team';
  const choice=await ownedDialog(owner,()=>dialog.showSaveDialog(owner,{title:'Export Team DNA',defaultPath:name+'.2kteam',filters:[{name:'Team DNA',extensions:['2kteam']}]}));
  if(choice.canceled||!choice.filePath)return null;
  await fsp.writeFile(choice.filePath,JSON.stringify(payload,null,2)+'\n','utf8');return {saved:true};
 }finally{teamDnaBusy=false;}
});
registerRoster('team:staff', async (event, team) => {
  rosterOwner(event);
  teamStaffService ||= new (require('./core/team-staff').TeamStaffService)(playerResearchManager,{backupRoot:storagePath('recovery','staff-edit-backups')});
  return teamStaffService.read(team);
});
registerRoster('team:staff-save', async(event,request)=>{
  rosterOwner(event);
  if(!teamStaffService)throw Error('Refresh staff before saving.');
  return teamStaffService.save(request);
});
registerRoster('team-uniform:save', async (_event, edit) => {
  const report = await playerResearchManager.editTeamUniform(edit);
  const settings = await readSettings();
  const playerResearch = await playerResearchManager.scan({ gameRoot: settings.gameRoot, modsRoot: settings.modsRoot });
  return { report, playerResearch };
});
registerRoster('team-uniform:link', async (_event, edit) => {
  const settings = await readSettings();
  const report = await playerResearchManager.linkTeamUniform(edit, { gameRoot: settings.gameRoot });
  const playerResearch = await playerResearchManager.scan({ gameRoot: settings.gameRoot, modsRoot: settings.modsRoot });
  return { report, playerResearch };
});
ipcMain.handle('arena:clone', async (_event, edit) => {
  const settings = await readSettings();
  const report = await playerResearchManager.cloneArena(edit, { modsRoot: settings.modsRoot });
  const playerResearch = await playerResearchManager.scan({ gameRoot: settings.gameRoot, modsRoot: settings.modsRoot });
  return { report, playerResearch };
});

ipcMain.handle('jukebox:status', async () => {
  const settings = await readSettings();
  return jukeboxManager.view(settings.modsRoot);
});
ipcMain.handle('jukebox:repair-missing-tracks', async () => {
  requireJukeboxEditable();
  const settings = await readSettings();
  return jukeboxManager.repairMissingCustomTracks(settings.modsRoot);
});
ipcMain.handle('jukebox:import-wav', async () => {
  requireJukeboxEditable();
  const selection = await dialog.showOpenDialog(mainWindow, {
    title: 'Choose a custom jukebox WAV',
    properties: ['openFile'],
    filters: [{ name: 'PCM WAV audio', extensions: ['wav'] }],
  });
  if (selection.canceled || !selection.filePaths[0]) return null;
  const settings = await readSettings();
  return jukeboxManager.importWav(settings.modsRoot, selection.filePaths[0]);
});
ipcMain.handle('jukebox:choose-audio', async () => {
  const selection = await dialog.showOpenDialog(mainWindow, {
    title: 'Choose music to add to the jukebox',
    properties: ['openFile'],
    filters: [
      { name: 'Audio files', extensions: ['mp3', 'wav', 'm4a', 'aac', 'flac', 'ogg', 'opus'] },
      { name: 'All files', extensions: ['*'] },
    ],
  });
  if (selection.canceled || !selection.filePaths[0]) return null;
  const sourcePath = selection.filePaths[0];
  const stat = await fsp.stat(sourcePath);
  if (!stat.isFile() || stat.size < 1 || stat.size > MAX_JUKEBOX_IMPORT_BYTES) {
    throw new Error('Choose an audio file smaller than 128 MB.');
  }
  const bytes = await fsp.readFile(sourcePath);
  return {
    name: path.basename(sourcePath),
    ...inferAudioMetadata(path.basename(sourcePath), bytes.subarray(0, 1024 * 1024), bytes.subarray(-128)),
    bytes: new Uint8Array(bytes),
  };
});
ipcMain.handle('jukebox:choose-audio-batch', async () => {
  const selection = await dialog.showOpenDialog(mainWindow, {
    title: 'Choose songs to add to the selected playlist',
    properties: ['openFile', 'multiSelections'],
    filters: [
      { name: 'Audio files', extensions: [...JUKEBOX_AUDIO_EXTENSIONS].map((value) => value.slice(1)) },
      { name: 'All files', extensions: ['*'] },
    ],
  });
  if (selection.canceled || !selection.filePaths.length) return null;
  if (selection.filePaths.length > MAX_JUKEBOX_BATCH_FILES) throw new Error(`Choose no more than ${MAX_JUKEBOX_BATCH_FILES} songs at once.`);
  jukeboxAudioSelections.clear();
  const results = [];
  for (const sourcePath of selection.filePaths) {
    const inspected = await inspectJukeboxAudio(sourcePath);
    const token = crypto.randomUUID();
    jukeboxAudioSelections.set(token, {
      path: inspected.resolved, size: inspected.stat.size, modified: inspected.stat.mtimeMs,
    });
    results.push({ token, name: path.basename(inspected.resolved), size: inspected.stat.size, ...inspected.metadata });
  }
  return results;
});
ipcMain.handle('jukebox:read-audio-selection', async (_event, token) => {
  const key = String(token || '');
  const selected = jukeboxAudioSelections.get(key);
  jukeboxAudioSelections.delete(key);
  if (!selected) throw new Error('This batch audio selection expired. Choose the songs again.');
  const stat = await fsp.stat(selected.path);
  if (!stat.isFile() || stat.size !== selected.size || stat.mtimeMs !== selected.modified) {
    throw new Error(`${path.basename(selected.path)} changed after it was selected.`);
  }
  return { name: path.basename(selected.path), bytes: new Uint8Array(await fsp.readFile(selected.path)) };
});
ipcMain.handle('jukebox:choose-artwork', async () => {
  const selection = await dialog.showOpenDialog(mainWindow, {
    title: 'Choose square album artwork',
    properties: ['openFile'],
    filters: [
      { name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'webp', 'bmp'] },
      { name: 'All files', extensions: ['*'] },
    ],
  });
  if (selection.canceled || !selection.filePaths[0]) return null;
  const sourcePath = selection.filePaths[0];
  const stat = await fsp.stat(sourcePath);
  if (!stat.isFile() || stat.size < 1 || stat.size > 20 * 1024 * 1024) {
    throw new Error('Choose an artwork image smaller than 20 MB.');
  }
  return { name: path.basename(sourcePath), bytes: new Uint8Array(await fsp.readFile(sourcePath)) };
});
ipcMain.handle('jukebox:lookup-artwork', async (_event, query) => {
  const result = await artworkLookup.find({ artist: query?.artist, title: query?.title });
  return { ...result, bytes: new Uint8Array(result.bytes) };
});
ipcMain.handle('jukebox:import-audio-data', async (_event, payload) => {
  requireJukeboxEditable();
  const bytes = Buffer.from(payload?.bytes || []);
  if (!bytes.length || bytes.length > 256 * 1024 * 1024) throw new Error('The converted WAV is outside the safe size limit.');
  const artworkBytes = payload?.artworkBytes?.length ? Buffer.from(payload.artworkBytes) : null;
  const settings = await readSettings();
  return jukeboxManager.importWaveBytes(settings.modsRoot, bytes, payload?.name, {
    title: payload?.title,
    artist: payload?.artist,
    playlistId: payload?.playlistId || null,
    deferBundle: Boolean(payload?.deferBundle),
    artworkBytes,
    gameRoot: settings.gameRoot,
  });
});
ipcMain.handle('jukebox:refresh-playlist-bundle', async (_event, id) => {
  requireJukeboxEditable();
  const settings = await readSettings();
  return jukeboxManager.refreshPlaylistBundle(settings.modsRoot, id);
});
ipcMain.handle('jukebox:preview-track', async (_event, key) => {
  const settings = await readSettings();
  const { path: audioPath, ...details } = await jukeboxManager.previewTrack(settings.modsRoot, key);
  return { ...details, url: pathToFileURL(audioPath).href };
});
ipcMain.handle('jukebox:preview-artwork', async (_event, key) => {
  const settings = await readSettings();
  const result = await jukeboxManager.previewArtwork(settings.modsRoot, key);
  return { ...result, bytes: new Uint8Array(result.bytes) };
});
ipcMain.handle('jukebox:restore-default', async () => {
  requireJukeboxEditable();
  const settings = await readSettings();
  return jukeboxManager.restoreDefault(settings.modsRoot, settings.gameRoot);
});
ipcMain.handle('jukebox:save-track', async (_event, edit) => {
  requireJukeboxEditable();
  const settings = await readSettings();
  const artworkBytes = edit?.artworkBytes?.length ? Buffer.from(edit.artworkBytes) : null;
  return jukeboxManager.saveTrack(settings.modsRoot, edit, {
    artworkBytes,
    gameRoot: settings.gameRoot,
    deferBundle: Boolean(edit?.deferBundle),
  });
});
ipcMain.handle('jukebox:save-playlist', async (_event, playlist) => {
  requireJukeboxEditable();
  const settings = await readSettings();
  return jukeboxManager.savePlaylist(settings.modsRoot, playlist);
});
ipcMain.handle('jukebox:activate-playlist', async (_event, id) => {
  requireJukeboxEditable();
  const settings = await readSettings();
  return jukeboxManager.activatePlaylist(settings.modsRoot, id);
});
ipcMain.handle('jukebox:delete-playlist', async (_event, id) => {
  requireJukeboxEditable();
  const settings = await readSettings();
  return jukeboxManager.deletePlaylist(settings.modsRoot, id);
});
ipcMain.handle('jukebox:import-playlist', async () => {
  requireJukeboxEditable();
  const selection = await dialog.showOpenDialog(mainWindow, {
    title: 'Import a shared NBA 2K27 playlist',
    properties: ['openFile'],
    filters: [{ name: 'NBA 2K27 playlists', extensions: ['2kplaylist'] }],
  });
  if (selection.canceled || !selection.filePaths[0]) return null;
  const settings = await readSettings();
  return jukeboxManager.importPlaylistBundle(settings.modsRoot, settings.gameRoot, selection.filePaths[0]);
});
ipcMain.handle('jukebox:open-playlist-folder', async () => {
  const folder = launcherStorage.mode==='organized'?musicPath('playlists'):await jukeboxManager.ensurePlaylistFolder();
  await fsp.mkdir(folder,{recursive:true});
  const error = await shell.openPath(folder);
  if (error) throw new Error(error);
  return folder;
});

ipcMain.handle('session:start', async () => session.start(await readSettings()));
ipcMain.handle('session:stop', async () => {
  await versionProxySession?.stop();
  return session.stop();
});
ipcMain.handle('game:launch-without-eac', async () => {
  requireNativeHost();
  if(presentationManager.busy)throw Error('Wait for the startup video import to finish or cancel it before Play.');
  if(launchInProgress)throw new Error('Game launch is already in progress.');
  launchInProgress=true;
  clearTimeout(animationWarmTimer);animationPreview?.setWarm(startupAnimationOwner,false);
  clothingCacheJob?.cancel();
  rosterWeb?.close().catch(()=>{});
  let settings;
  const startupStartedAt = Date.now();
  const timings = {};
  const launchStage = (phase, message) => { requireNativeHost(); return send('loader:status', {
    phase,
    message,
    startupStartedAt,
    timings: { ...timings },
  }); };
  try {
    if(await processIsRunning('NBA2K27.exe'))throw new Error('Close NBA 2K27 before starting a new session.');
    settings=await readSettings();
    const overlayVideoEnabled=settings.startupVideoEnabled===true;
    // A disabled video must not inspect or depend on its library during launch.
    // Optional media failures must not prevent the game from launching.
    const presentation=overlayVideoEnabled?await presentationManager.view().catch(error=>({movie:{error:error.message}})):null;
    if(overlayVideoEnabled&&!presentation.movie.info)console.warn('Startup video skipped:',presentation.movie.error||'No MP4 selected.');
    await videoOverlay?.finish('launch-failed');
    await sharedPresentationSession.restore();
    await playerFolders().prepare(settings.modsRoot);
    await jukeboxSession.restore();
    await cameraManager.namesSession.restore();
    await scoreboardLayouts.restore();
    await highschoolDigitProbe.restore();
    await ballScaleRecovery.restore();
    launchStage('preparing-launch', 'Checking the current NBA 2K27 build…');
    const layoutStartedAt = Date.now();
    const layoutPromise = refreshProviderLayout(settings).then((value) => {
      timings.buildCheckMs = Date.now() - layoutStartedAt;
      return value;
    });

    launchStage('preparing-selectors', 'Scanning the selected profile and preparing player selectors…');
    const selectorStartedAt = Date.now();
    const selectorPromise = multiConfigManager.prepare({
      gameRoot: settings.gameRoot,
      modsRoot: settings.modsRoot,
      enabled: true,
    }).then((value) => {
      timings.playerSelectorsMs = Date.now() - selectorStartedAt;
      return value;
    });

    const [layout, selectors] = await Promise.all([layoutPromise, selectorPromise]);
    if (layout.status !== 'ready') {
      throw new Error(`The current NBA 2K27 build is not safely resolved: ${layout.message}`);
    }

    launchStage('preparing-jukebox', 'Preparing the selected playlist for in-game playback…');
    const jukeboxStartedAt=Date.now();
    const playlistSource=await jukeboxManager.launchSource(settings.modsRoot,{cachePreparedPlaylists:settings.cachePreparedPlaylists!==false});
    timings.launchCache=playlistSource.launchCacheTimings||null;
    timings.playlistExtractionMs=Date.now()-jukeboxStartedAt;
    const audio=await jukeboxSession.prepare({modsRoot:settings.modsRoot,gameRoot:settings.gameRoot,...playlistSource});
    audio.freestylePlaylist=useFreestylePlaylist(settings,audio);
    timings.jukeboxPreparationMs=Date.now()-jukeboxStartedAt;
    timings.playlistPreparation=audio.preparationTimings||null;
    const readinessHelper=app.isPackaged?runtimePath('session-loader','NBA2K27AudioReadiness.exe')
      :path.join(__dirname,'../../nba2k27-version-proxy/dist/NBA2K27AudioReadiness.exe');
    versionProxySession.startupReadinessCheck=audio.audioPolicy ? ()=>waitAudioReadiness(
      readinessHelper,settings.gameRoot,versionProxySession.paths.armed) : null;
    versionProxySession.startupEarlyReadinessCheck=null;

    launchStage('preparing-profile', 'Activating the selected mod profile…');
    const profileStartedAt = Date.now();
    await modProfileManager.activate({ gameRoot: settings.gameRoot, modsRoot: settings.modsRoot });
    timings.profileActivationMs = Date.now() - profileStartedAt;

    launchStage('preparing-session', 'Starting the local game and camera monitor…');
    const sessionStartedAt = Date.now();
    if (!session.timer) await session.start(settings);
    timings.sessionMonitorMs = Date.now() - sessionStartedAt;

    launchStage('preparing-names', 'Checking the optional in-game camera names…');
    const namesStartedAt = Date.now();
    const cameraSetup=await cameraManager.readLibrary();
    const cameraNames=await cameraManager.namesSession.prepare({gameRoot:settings.gameRoot,modsRoot:settings.modsRoot,
      names:Object.fromEntries(cameraSetup.cameras.map(camera=>[camera.id,camera.name]))});
    timings.cameraNamesMs = Date.now() - namesStartedAt;

    launchStage('preparing-scoreboards','Preparing scoreboard data bindings…');
    await scoreboardLayouts.prepare({gameRoot:settings.gameRoot,modsRoot:settings.modsRoot});

    launchStage('preparing-ball','Preparing the selected Blacktop ball…');
    await balls().prepare({gameRoot:settings.gameRoot,modsRoot:settings.modsRoot});

    await retiredFrontMenuRecovery.restore();
    const forcedOverrides=[...(cameraNames.installed?['loc/english.iff']:[])];
    const providerAliases={};
    const result = await versionProxySession.start({
      gameRoot: settings.gameRoot,
      modsRoot: settings.modsRoot,
      proxySource: sessionProxyPath(),
      providerHost: providerHostExecutable(),
      forcedOverrides,
      providerAliases,
      mutableProviderAliases:[],
      selectorOverride: selectors.selectorOverride === true,
      scoreboardTextTest:true,
      ...audio,
      launchGame: async () => {
        try {
          requireNativeHost();
          scalerManager.arm(settings.gameRoot);
          freestyleUnlocks.arm(settings.gameRoot,settings.unlockWnbaFreestyle===true);
          await blacktopUnlocks.arm(settings.gameRoot,settings.unlockWnbaBlacktop===true);
          await debugMenus.arm(settings.gameRoot);
          await blacktopPresentation.arm(settings.gameRoot);
          await blacktopCrowd.arm(settings.gameRoot);
          await balls().arm(settings.gameRoot);
          // Begin watching before Steam starts the process, so even early exits
          // have an exit code. Diagnostic failures never block gameplay launch.
          await gameCapture.start(settings.gameRoot).catch(error=>console.warn('Game capture unavailable:',error.message));
          requireNativeHost();
          const launched=await launchWithoutEac(settings.gameRoot);
          // Do not block provider readiness while waiting for a visible game window.
          if(presentation?.movie.info)void videoOverlay.showWhenGameReady({videoPath:presentation.movie.path,gamePath:path.join(settings.gameRoot,'NBA2K27.exe'),owner:mainWindow}).catch(error=>console.warn('Startup video unavailable:',error.message));
          if(!globalShortcut.isRegistered('CommandOrControl+Shift+F12'))globalShortcut.register('CommandOrControl+Shift+F12',()=>{void gameCapture.capture().catch(error=>console.warn('Game snapshot:',error.message));});
          return launched;
        } catch(error) {
          await gameCapture.stop();
          scalerManager.stop();freestyleUnlocks.stop();await blacktopUnlocks.stop();
          await debugMenus.stop();
          await blacktopPresentation.stop();
          await blacktopCrowd.stop();
          await ballSelectionManager?.cleanup().catch(()=>{});
          throw error;
        }
      },
    });
    timings.untilSteamLaunchMs = Date.now() - startupStartedAt;
    await fsp.writeFile(path.join(app.getPath('userData'),'startup-timings.json'),JSON.stringify({recordedAt:new Date().toISOString(),timings},null,2)).catch(()=>{});
    return { ...result, startupTimings: timings };
  } catch (error) {
    await videoOverlay?.finish('launch-failed');
    freestyleUnlocks?.stop();
    blacktopUnlocks?.stop();
    await debugMenus?.stop();
    await blacktopPresentation?.stop();
    await blacktopCrowd?.stop();
    multiConfigManager.emit({ phase: 'error', message: error.message });
    send('loader:status', { phase: 'loader-failed', message: error.message, error: error.message, startupStartedAt, timings });
    await jukeboxSession.restore().catch(()=>{});
    await sharedPresentationSession?.restore().catch(()=>{});
    await cameraManager.namesSession.restore().catch(()=>{});
    await scoreboardLayouts.restore().catch(()=>{});
    await ballSelectionManager?.cleanup().catch(()=>{});
    await retiredFrontMenuRecovery?.restore().catch(()=>{});
    await highschoolDigitProbe.restore().catch(()=>{});
    await ballScaleRecovery.restore().catch(()=>{});
    await jukeboxManager.releaseCache().catch(()=>{});
    await modProfileManager.restoreIfSafe().catch(() => {});
    await playerFolders().restore().catch(()=>{});
    throw error;
  } finally {
    launchInProgress=false;
  }
});
ipcMain.handle('shell:open-mods', async () => {
  const settings = await readSettings();
  const error = await shell.openPath(settings.modsRoot);
  if (error) throw new Error(error);
  return true;
});
ipcMain.handle('shell:storage-info',event=>{requireLauncherSender(event);return {path:launcherStorage.mode==='organized'?launcherStorage.root:app.getPath('userData'),organized:launcherStorage.mode==='organized'};});
ipcMain.handle('shell:open-storage',async event=>{requireLauncherSender(event);const error=await shell.openPath(launcherStorage.mode==='organized'?launcherStorage.root:app.getPath('userData'));if(error)throw Error(error);return true;});

app.whenReady().then(async () => {
  if(!instanceLock)return;
  if(launcherStorage.mode==='blocked')throw launcherStorage.error;
  if(commandSelfTestIndex<0&&launcherStorage.needsPortableMigration){
    let choice=0;
    if(!launcherStorage.portableImportPending){
      const answer=await dialog.showMessageBox({
        type:'question',message:'How should this launcher folder start?',
        detail:'A previous launcher library was found. Import it to keep your settings and media, or start with empty settings and add content again. The old library is preserved either way.',
        buttons:['Import previous library','Start fresh','Cancel'],defaultId:0,cancelId:2,noLink:true,
      });
      choice=answer.response;
    }
    if(choice===2){app.exit(0);return;}
    if(choice===1){
      await writeSettings({});
    }else{
      if(await processIsRunning('NBA2K27.exe'))throw Error('Close NBA 2K27 before importing the previous launcher library. The original data remains untouched.');
      showStartupWindow();
      const {migrateToPortable}=require('./core/launcher-portable-migration');
      const result=await migrateToPortable({targetRoot:launcherStorage.root,sourceStorage:launcherStorage.migrationSource,isGameRunning:()=>processIsRunning('NBA2K27.exe')});
      closeStartupWindow();
      await dialog.showMessageBox({type:'info',message:'Launcher library copied into this portable folder.',detail:`Settings, outfits, media, previews, and logs are now in ${result.root}. The old library was preserved as a backup.`,buttons:['OK']});
    }
  }else if(commandSelfTestIndex<0&&launcherStorage.placement==='portable'&&!launcherStorage.needsPortableMigration){
    // A new folder with no old library must record that it is intentionally
    // fresh. Otherwise a later legacy folder could unexpectedly auto-import.
    try{await fsp.access(settingsPath());}catch(error){if(error.code!=='ENOENT')throw error;await writeSettings({});}
  }
  if(commandSelfTestIndex<0&&launcherStorage.placement!=='portable'&&launcherStorage.needsAppDataMigration&&!await processIsRunning('NBA2K27.exe')){
    // Migration can copy gigabytes before the main window exists. Normal
    // launches use only the main page's loading overlay.
    showStartupWindow();
    const {migrateToAppData}=require('./core/launcher-appdata-migration');
    const result=await migrateToAppData({appData:app.getPath('appData'),sourceStorage:launcherStorage});
    ensureNativeLocationHint(app.getPath('appData'),{mode:'organized',root:appDataRoot(app.getPath('appData'))});
    closeStartupWindow();
    await dialog.showMessageBox({type:'info',message:'Launcher library moved to AppData.',detail:`Your music, videos, previews, settings, and outfits are now in ${result.root}. The old folder was preserved as a backup. Reopen this same launcher EXE to continue.`,buttons:['OK']});
    app.exit(0);
    return;
  }
  const previewZip='blacktop-preview-images.zip';
  const managedPreviews=launcherStorage.mode==='organized'?launcherStorage.previews:null;
  let importedPack=await findImportedPreviewPack(app.getPath('userData'),managedPreviews);
  const previewResolver=createPreviewImageResolver({
    candidates:[importedPack,path.join(process.env.PORTABLE_EXECUTABLE_DIR||path.dirname(process.execPath),previewZip),path.join(app.getPath('userData'),previewZip)].filter(Boolean),
    bundledRoot:path.join(__dirname,'assets'),
  });
  let importingPreviews=false;
  ipcMain.handle('blacktop:preview-pack-status',event=>{requireLauncherSender(event);return {installed:Boolean(importedPack),count:previewResolver.status?.count||0};});
  ipcMain.handle('blacktop:preview-pack-import',async event=>{
    requireLauncherSender(event);if(importingPreviews)throw Error('A preview ZIP is already importing.');
    const choice=await ownedDialog(mainWindow,()=>dialog.showOpenDialog(mainWindow,{title:'Import Blacktop preview images',properties:['openFile'],filters:[{name:'Blacktop preview ZIP',extensions:['zip']}]}));
    if(choice.canceled||!choice.filePaths?.[0])return {cancelled:true};
    importingPreviews=true;
    try{
      const result=await importPreviewImagePack({source:choice.filePaths[0],userData:app.getPath('userData'),managedRoot:managedPreviews,onProgress:value=>send('blacktop:preview-pack-progress',value)});
      await previewResolver.activate(result.file);importedPack=result.file;
      send('blacktop:preview-pack-progress',{phase:'complete',count:result.count});
      return {installed:true,count:result.count,sizeBytes:result.sizeBytes};
    }catch(error){send('blacktop:preview-pack-progress',{phase:'failed',error:error.message});throw error;}
    finally{importingPreviews=false;}
  });
  protocol.handle('preview-image',async request=>{
    try{
      const url=new URL(request.url),name=decodeURIComponent(url.pathname.slice(1));
      if(url.hostname!=='images')return new Response(null,{status:404});
      if (name.startsWith('__animation/')) {
        const bytes = await animationPreview?.readToken(name.slice('__animation/'.length));
        return bytes ? new Response(bytes, { headers: { 'content-type': 'application/json', 'cache-control': 'no-store' } }) : new Response(null, { status: 410 });
      }
      const bytes=await previewResolver.read(name);
      return bytes?new Response(bytes,{headers:{'content-type':'image/png','cache-control':'no-store'}}):new Response(null,{status:404});
    }catch(error){console.warn('Preview image unavailable:',error.message);return new Response(null,{status:404});}
  });
  app.once('will-quit',()=>previewResolver.close()?.catch(()=>{}));
  // Show the main page's loading overlay while native helpers are verified;
  // there is no separate startup window on an ordinary launch.
  if(commandSelfTestIndex<0)createWindow({preparePanel:false});
  retiredFrontMenuRecovery=new RetiredFrontMenuRecovery({directory:path.join(app.getPath('userData'),'front-menu'),isGameRunning:()=>processIsRunning('NBA2K27.exe')});
  await retiredFrontMenuRecovery.restore().catch(error=>console.warn('Retired front-menu cleanup pending:',error.message));
  const selfTestIndex = process.argv.indexOf('--self-test');
  if(app.isPackaged)runtimeResourceRoot=await ensureRuntimeResources({sourceRoot:path.join(__dirname,'..','runtime-resources'),targetRoot:launcherStorage.mode==='organized'?storagePath('runtime',app.getVersion()):storagePath('data','runtime-resources',app.getVersion())});
  if(selfTestIndex<0)prepareBlacktopPanel();
  videoOverlay=new VideoOverlay({BrowserWindow,screen,ipcMain:electronIpcMain,
    waitForWindow:options=>waitForGameWindow({helper:app.isPackaged?runtimePath('video','GameWindow.exe'):path.join(__dirname,'../video-helper/GameWindow.exe'),...options}),
    volumeMonitor:new GameVolume({helper:app.isPackaged?runtimePath('video','GameVolume.exe'):path.join(__dirname,'../video-helper/GameVolume.exe'),cacheFile:storagePath('data','startup-video-volume.json'),log:row=>{void fsp.appendFile(logsPath('video-overlay.jsonl'),JSON.stringify({at:new Date().toISOString(),...row})+'\n').catch(()=>{});}}),
    helperPath:app.isPackaged?runtimePath('video','ControllerSkip.exe'):path.join(__dirname,'../video-helper/ControllerSkip.exe'),
    onReturn:async()=>{const settings=await readSettings();if(await processIsRunning('NBA2K27.exe'))await blacktopTools().focusGame(settings.gameRoot);else if(mainWindow&&!mainWindow.isDestroyed())mainWindow.focus();},
    log:row=>{void fsp.appendFile(logsPath('video-overlay.jsonl'),JSON.stringify({at:new Date().toISOString(),...row})+'\n').catch(()=>{});}});
  // Renderer bootstrap waits on initializationBarrier; early shell loading
  // cannot expose a partially recovered game session.
  modTagStore = new ModTagStore(path.join(app.getPath('userData'), 'mod-library-tags.json'));
  jukeboxSession=new JukeboxSession({root:musicPath('game-session'),libraryPath:musicPath('library.json'),
    onProgress:message=>send('loader:status',{phase:'preparing-jukebox',message})});
  await jukeboxSession.restore().catch(error=>{console.warn('Playlist recovery pending:',error.message);});
  // Keep the established app library independent of future mod selections.
  const jukeboxSettings = await readSettings();
  if(!await processIsRunning('NBA2K27.exe')){
    if(!jukeboxSession.active)await jukeboxSession.clearConversionCache(jukeboxSettings.cachePreparedPlaylists===false?0:undefined).catch(error=>console.warn('Audio cache cleanup pending:',error.message));
    await require('./core/diagnostic-cleanup').cleanupDiagnostics({directory:logsPath(),gameRoot:jukeboxSettings.gameRoot}).catch(error=>console.warn('Diagnostic cleanup pending:',error.message));
    await require('./core/temporary-cleanup').cleanupTemporaryFiles(app.getPath('userData')).catch(error=>console.warn('Temporary cleanup pending:',error.message));
  }
  let libraryRoot = jukeboxSettings.jukeboxLibraryRoot;
  if (!libraryRoot) {
    libraryRoot = jukeboxSettings.modsRoot;
    let bestCount = -1;
    for (const root of new Set([jukeboxSettings.modsRoot, ...(jukeboxSettings.modsProfiles || []).map(p => p.path)])) {
      const key = crypto.createHash('sha256').update(path.resolve(root).toLowerCase()).digest('hex').slice(0,16);
      const file = musicPath('archive-library',key,'library.json');
      try {
        const saved = JSON.parse(await fsp.readFile(file,'utf8'));
        const count = (saved.playlists || []).filter(p => !p.recoveryPlaylist).length;
        if (count > bestCount) { bestCount = count; libraryRoot = root; }
      } catch (error) { if (error.code !== 'ENOENT') console.warn('Previous jukebox library retained:', error.message); }
    }
    await writeSettings({jukeboxLibraryRoot: libraryRoot});
  }
  jukeboxManager = new JukeboxManager({
    opusEncoder:require('./core/opus-encoder').encodeNativeOpus,
    libraryRoot,
    flacPath: app.isPackaged ? runtimePath('flac','flac.exe') : path.join(__dirname,'../vendor/flac-1.5.0/unpacked/flac-1.5.0-win/Win64/flac.exe'),
    getGameRoot: async () => (await readSettings()).gameRoot,
    statePath: musicPath('library.json'),
    decoderPath: vczDecoderPath(),
    texconvPath: texconvExecutable(),
    onProgress:message=>send('loader:status',{phase:'preparing-jukebox',message}),
    isGameRunning:()=>processIsRunning('NBA2K27.exe'),
  });
  if(jukeboxSettings.cachePreparedPlaylists===false&&!await processIsRunning('NBA2K27.exe'))await jukeboxManager.clearLaunchCache(jukeboxSettings.modsRoot).catch(error=>console.warn('Playlist cache cleanup pending:',error.message));
  playerResearchManager = new PlayerResearchManager({
    helperPath: rosterResearchExecutable(),
    referenceCatalogPath: playerFaceIdCatalogPath(),
    seedCatalogPath: path.join(__dirname, 'data', 'player-catalog-seed.json'),
    teamSeedCatalogPath: path.join(__dirname, 'data', 'team-catalog-seed.json'),
    learnedCatalogPath: path.join(app.getPath('userData'), 'player-research', 'learned-player-catalog.json'),
    arenaProfilePath: path.join(app.getPath('userData'), 'player-research', 'custom-arena-profiles.json'),
  });
  multiConfigManager = new MultiConfigManager({
    managedRoot: path.join(app.getPath('userData'), 'multi-config'),
    decoderPath: vczDecoderPath(),
  });
  cameraManager = new CameraManager({
    managedRoot: path.join(app.getPath('userData'), 'camera-tools'),
    templateRoot: cameraTemplateDirectory(),
    onStatus: (state) => send('camera:status', state),
    managedNames:true,decoderPath:vczDecoderPath(),isGameRunning:()=>processIsRunning('NBA2K27.exe'),
    diagnosticsPath:logsPath('camera-service-history.json'),
  });
  await cameraManager.namesSession.restore().catch(error=>console.warn('Camera name recovery pending:',error.message));
  scoreboardLayouts=new ScoreboardLayoutSession({root:path.join(app.getPath('userData'),'blacktop','scoreboard-layouts'),decoderPath:vczDecoderPath(),isGameRunning:()=>processIsRunning('NBA2K27.exe')});
  await scoreboardLayouts.restore().catch(error=>console.warn('Scoreboard recovery pending:',error.message));
  highschoolDigitProbe=createHighschoolDigitProbe({root:path.join(app.getPath('userData'),'blacktop','highschool-digit-probe'),decoderPath:vczDecoderPath(),isGameRunning:()=>processIsRunning('NBA2K27.exe')});
  await highschoolDigitProbe.restore().catch(error=>console.warn('High school probe recovery pending:',error.message));
  ballScaleRecovery=new BallScaleRecovery({root:path.join(app.getPath('userData'),'scaler','ball-session'),isGameRunning:()=>processIsRunning('NBA2K27.exe')});
  await ballScaleRecovery.restore().catch(error=>console.warn('Ball mod recovery pending:',error.message));
  scalerManager=new ScalerManager({directory:path.join(app.getPath('userData'),'scaler'),
    helperPath:app.isPackaged?runtimePath('scaler','NBA2K27PlayerScaler.exe'):path.join(__dirname,'..','scaler-helper','NBA2K27PlayerScaler.exe'),
    publish:state=>send('scaler:status',state)});
  await scalerManager.load();
  await scalerManager.recover((await readSettings()).gameRoot);
  freestyleUnlocks=new FreestyleUnlocks({directory:path.join(app.getPath('userData'),'unlocks'),
    helperPath:app.isPackaged?runtimePath('unlocks','NBA2K27FreestyleTeams.exe'):path.join(__dirname,'..','unlocks-helper','NBA2K27FreestyleTeams.exe'),
    publish:state=>send('unlocks:freestyle-status',state)});
  let blacktopStatusWrite=Promise.resolve();
  blacktopUnlocks=new BlacktopUnlocks({helperPath:freestyleUnlocks.helperPath,publish:state=>{
    send('unlocks:blacktop-status',state);
    selectionStatusLog.record('wnba',state).catch(error=>console.warn('WNBA status diagnostic:',error.message));
    const snapshot=JSON.stringify({time:new Date().toISOString(),version:app.getVersion(),...state});
    blacktopStatusWrite=blacktopStatusWrite.then(async()=>{const dir=logsPath();await fsp.mkdir(dir,{recursive:true});await fsp.writeFile(path.join(dir,'blacktop-wnba-status.json'),snapshot,'utf8');}).catch(()=>{});
  }});
  debugMenus=new DebugMenus({directory:path.join(app.getPath('userData'),'unlocks'),helperPath:freestyleUnlocks.helperPath,publish:state=>send('unlocks:debug-status',state)});
  blacktopPresentation=new BlacktopPresentation({directory:path.join(app.getPath('userData'),'unlocks'),helperPath:freestyleUnlocks.helperPath,
    initialOptions:(await readSettings()).blacktopPresentation,persistOptions:selection=>writeSettings({blacktopPresentation:selection}),
    publish:state=>send('blacktop:presentation-status',state)});
  // The window can open before recovery finishes. Publish restored choices so
  // early renderer requests do not leave the switches showing default values.
  blacktopPresentation.emit(blacktopPresentation.state);
  blacktopCrowd=new BlacktopCrowd({directory:path.join(app.getPath('userData'),'unlocks'),helperPath:freestyleUnlocks.helperPath,
    initialEnabled:(await readSettings()).blacktopCrowdEnabled,persistEnabled:enabled=>writeSettings({blacktopCrowdEnabled:enabled}),
    publish:state=>{selectionStatusLog.record('crowd',{selected:state.enabled?'original':'remove',...state}).catch(error=>console.warn('Crowd status diagnostic:',error.message));send('blacktop:crowd-status',state);}});
  blacktopCrowd.emit(blacktopCrowd.state);
  await cameraManager.ensureInstalled();
  modProfileManager = new ModProfileManager({
    markerPath: path.join(app.getPath('userData'), 'active-mod-profile.json'),
  });
  // Retain recovery only for temporary MOV aliases from older releases.
  sharedPresentationSession=new SharedPresentationSession({directory:path.join(app.getPath('userData'),'presentation'),isGameRunning:()=>processIsRunning('NBA2K27.exe')});
  await sharedPresentationSession.restore().catch(error=>console.warn('Shared startup video recovery pending:',error.message));
  blacktopSession=new BlacktopSession({
    script:app.isPackaged?runtimePath('session-loader','session_saved_outfits.ps1'):path.join(__dirname,'../../nba2k27-version-proxy/session_saved_outfits.ps1'),
    choices:path.join(app.getPath('userData'),'blacktop','original-outfits.json.match-choices.json'),
    diagnosticsDir:logsPath('game-session'),
    onStatus:state=>{selectionStatusLog.record('outfit',{selected:'match-carryover',...state}).catch(()=>{});send('blacktop:carry-status',state);if(blacktopPanel?.window&&!blacktopPanel.window.isDestroyed())blacktopPanel.window.webContents.send('blacktop:carry-status',state);},
  });
  balls();
  await ballSelectionManager.cleanup().catch(error=>console.warn('Blacktop ball recovery pending:',error.message));
  await ballSelectionManager.liveSelector.recover((await readSettings()).gameRoot).catch(error=>console.warn('Live ball recovery pending:',error.message));
  versionProxySession = new VersionProxySession({
    gamePresence:()=>gameCapture.gamePresence(),
    audioDiagnosticsPath: logsPath('jukebox-audio-route.json'),
    nativeLocationHintPath:launcherStorage.mode==='organized'&&launcherStorage.placement!=='portable'?nativePointerFile(app.getPath('appData')):null,
    nativeLocationRoot:launcherStorage.mode==='organized'&&launcherStorage.placement!=='portable'?launcherStorage.root:null,
    portableLogsPath:launcherStorage.placement==='portable'?logsPath('game-session'):null,
    onStatus: (state) => {
      send('loader:status',state);
      if(state.phase==='loader-active')readSettings().then(settings=>blacktopSession?.start(settings.gameRoot)).catch(error=>selectionStatusLog.record('outfit',{selected:'match-carryover',phase:'start-error',message:error.message}).catch(()=>{}));
      if(['complete','stopped'].includes(state.phase))blacktopSession?.stop();
    },
    onCleanup: async () => {
      await require('./core/session-restoration').runRestorationSteps([
        ['Startup video window',()=>videoOverlay?.finish('game-ended')],
        ['Outfit helper',()=>blacktopSession?.stop()],
        ['Blacktop ball',()=>ballSelectionManager.cleanup()],
        ['Jukebox',()=>jukeboxSession.restore()],
        ['Startup video',()=>sharedPresentationSession.restore()],
        ['Camera names',()=>cameraManager.namesSession.restore()],
        ['Scoreboards',()=>scoreboardLayouts.restore()],
        ['Front menu',()=>retiredFrontMenuRecovery.restore()],
        ['Scoreboard digits',()=>highschoolDigitProbe.restore()],
        ['Ball scale',()=>ballScaleRecovery.restore()],
        ['Jukebox cache',()=>jukeboxManager.releaseCache()],
        ['Playlist cache',async()=>{
          const settings=await readSettings();
          if(settings.cachePreparedPlaylists===false)await jukeboxManager.clearLaunchCache(settings.modsRoot);
        }],
        ['Audio conversion cache',async()=>{
          if((await readSettings()).cachePreparedPlaylists===false)await jukeboxSession.clearConversionCache(0);
        }],
        ['Mod profile',()=>modProfileManager.restoreIfSafe()],
        ['Player folders',()=>playerFolders().restore()],
      ],{beforeEach:async()=>{
        if(gameCapture.gamePresence()===true||await processIsRunning('NBA2K27.exe',{fresh:true})||gameCapture.gamePresence()===true){
          const error=new Error('NBA 2K27 is still running. Session restoration will wait for confirmed exit.');
          error.code='GAME_SESSION_ACTIVE';throw error;
        }
      }});
    },
  });
  if (selfTestIndex >= 0) {
    const output = process.argv[selfTestIndex + 1];
    if (!output) throw new Error('--self-test requires an output JSON path.');
    const settings = await readSettings();
    const offlineDelegation = await runOfflineDelegationSelfTest();
    const nativeAbiHarness = await runNativeAbiHarness(nativeAbiHarnessPath());
    const crossModuleHarness = await runCrossModuleHarness(crossModuleHarnessDirectory());
    const observeOnlyAddon = await inspectObserveOnlyAddon(observeOnlyAddonPath());
    const binaryProvider = await runBinaryProviderSelfTest(providerHostExecutable());
    const cameraController = await fileKind(cameraManager.controllerPath);
    const providerResolver = await fileKind(providerResolverExecutable());
    const sessionProxy = await fileKind(sessionProxyPath());
    const audioReadiness = await fileKind(app.isPackaged?runtimePath('session-loader','NBA2K27AudioReadiness.exe'):path.join(__dirname,'../../nba2k27-version-proxy/dist/NBA2K27AudioReadiness.exe'));
    const vczDecoder = await fileKind(vczDecoderPath());
    const fastFitDecoder = await fileKind(fastFitDecoderPath());
    const texconv = await fileKind(texconvExecutable());
    const rosterResearch = await fileKind(rosterResearchExecutable());
    const playerCatalogSeed = await fileKind(path.join(__dirname, 'data', 'player-catalog-seed.json'));
    const teamCatalogSeed = await fileKind(path.join(__dirname, 'data', 'team-catalog-seed.json'));
    const allPassed = offlineDelegation.passed && nativeAbiHarness.passed
      && crossModuleHarness.passed && observeOnlyAddon.verified
      && binaryProvider.passed && cameraController === 'file'
      && providerResolver === 'file' && sessionProxy === 'file' && audioReadiness === 'file'
      && vczDecoder === 'file' && fastFitDecoder === 'file' && texconv === 'file' && rosterResearch === 'file'
      && playerCatalogSeed === 'file' && teamCatalogSeed === 'file';
    const report = {
      ok: allPassed,
      version: app.getVersion(),
      packaged: app.isPackaged,
      cameraController,
      providerResolver,
      sessionProxy,
      audioReadiness,
      sessionPlaylistAudio: true,
      playlistOnlySoundtrackRouting: true,
      vczDecoder,
      fastFitDecoder,
      texconv,
      rosterResearch,
      playerCatalogSeed,
      teamCatalogSeed,
      automaticMultiConfigUnlock: vczDecoder === 'file',
      automaticJukeboxArtworkConversion: texconv === 'file',
      onlineJukeboxArtworkLookup: true,
      automaticProviderLayoutResolution: providerResolver === 'file',
      integratedCameraTab: cameraController === 'file',
      integratedReadOnlyPlayerResearchTab: rosterResearch === 'file',
      integratedGuardedPlayerEditor: rosterResearch === 'file',
      integratedGuardedTeamEditor: rosterResearch === 'file',
      integratedCustomArenaCloner: rosterResearch === 'file',
      automaticRosterIndependentArenas: rosterResearch === 'file',
      automaticArenaRosterReloadMonitoring: rosterResearch === 'file',
      sessionModProfiles: true,
      searchableModTags: true,
      playerDependencyTrees: true,
      providers: Object.keys(PROVIDERS),
      defaultProvider: settings.provider,
      productionProviderHost: await fileKind(providerHostExecutable()),
      bridgeAudit: 'offline-read-only',
      resourceDeviceLifecycle: 'offline-mapped',
      stockDeviceOwnership: 'offline-mapped',
      crossModuleLifetime: 'offline-passed',
      developmentManifestRoute: 'verified-fallback-only',
      offlineDelegation,
      nativeAbiHarness,
      crossModuleHarness,
      observeOnlyAddon,
      binaryProvider,
      observeOnlyProbe: {
        protocolVersion: 16,
        lifecycleEvents: ['initialize', 'heartbeat', 'registration-result', 'live-route-result', 'path-diagnostic', 'shutdown'],
        hooksInstalled: false,
        deviceRegistrationEnabled: false,
        reversibleRegistrationProbeAvailable: true,
        customGetIdCallbackProbeAvailable: true,
        customExistsCallbackProbeAvailable: true,
        customFileInfoCallbackProbeAvailable: true,
        managerOwnedStreamProbeAvailable: true,
        realAssetStreamProbeAvailable: true,
        temporaryDefaultDelegationProbeAvailable: true,
        sourceBoundStockStreamProbeAvailable: true,
        unprefixedManagerOpenProbeAvailable: true,
        multiHandleRegistryProbeAvailable: true,
        pathAwareBindingsProbeAvailable: true,
        oneShotLiveRouteProbeAvailable: false,
        boundedPlayerPackageRouteAvailable: false,
        stockFirstModsFallbackAvailable: true,
        automaticStockFirstFallbackEnabled: true,
        configPathTraceAvailable: true,
        resourceReplacementEnabled: false,
      },
      safety: {
        launchedGame: false,
        launchedModium: false,
        wroteGameFolder: false,
      },
    };
    await fsp.writeFile(path.resolve(output), `${JSON.stringify(report, null, 2)}\n`, 'utf8');
    process.exitCode = allPassed ? 0 : 1;
    app.quit();
    return;
  }
  await modProfileManager.restoreIfSafe().catch((error) => {
    session.emit({ phase: 'idle', message: `Mod-profile recovery needs attention: ${error.message}` });
  });
  await playerFolders().restore().catch(error=>console.warn('Player-folder recovery needs attention:',error.message));
  initializationFinished=true;releaseInitialization();
  if(nativeHostLost)return;
  scheduleAnimationWarmup();
  try {
    await observeOnlyProbe.start();
  } catch (error) {
    autoFallbackError = `Automatic handshake could not start: ${error.message}`;
    publishProbeStatus(observeOnlyProbe.status);
  }
  try {
    if(nativeHostLost)return;
    await session.start(await readSettings());
  } catch (error) {
    session.emit({ phase: 'idle', message: error.message, cameraState: 'waiting' });
  }
}).catch(async error=>{
  initializationFailure=error instanceof Error?error:Error(String(error));
  initializationFinished=true;releaseInitialization();
  await shutdownWarning('application startup',initializationFailure);
  // If failure happened before the early shell could be created, make one last
  // bounded attempt so bootstrap can display the actionable startup error.
  if(instanceLock&&(!mainWindow||mainWindow.isDestroyed())){
    try{createWindow();}catch(windowError){await shutdownWarning('startup error window',windowError);app.exit(1);}
  }
}).finally(()=>{nativeStartupSettled=true;});
async function shutdownWarning(label,error){
  const line=`${new Date().toISOString()} | ${label} | ${error?.message||error}\n`;
  await fsp.mkdir(logsPath(),{recursive:true}).catch(()=>{});
  await fsp.appendFile(logsPath('launcher-shutdown.log'),line,'utf8').catch(()=>{});
}
const cleanupNativeSession = require('./core/shutdown-safety').shareCleanup(async()=>{
    // Normal close and host-loss recovery must share one restoration batch.
    await versionProxySession?.stop();
    const settings=await readSettings();
    const assertStopped=async()=>{
      if(!await require('./core/shutdown-safety').gameStopped({presence:()=>gameCapture.gamePresence(),running:processIsRunning}))throw Error('A game is still running; restoration will wait.');
    };
    await require('./core/session-restoration').runRestorationSteps([
      ['Startup video',()=>videoOverlay?.finish('launcher-exit')],
      ['Jukebox',()=>jukeboxSession?.restore()],
      ['Camera names',()=>cameraManager?.namesSession.restore()],
      ['Scoreboards',()=>scoreboardLayouts?.restore()],
      ['Front menu',()=>retiredFrontMenuRecovery?.restore()],
      ['Scoreboard digits',()=>highschoolDigitProbe?.restore()],
      ['Ball scale',()=>ballScaleRecovery?.restore()],
      ['Mod profile',()=>modProfileManager?.restoreIfSafe()],
      ['Player folders',()=>playerFolders().restore()],
    ],{beforeEach:assertStopped});
    for(const [name,stop] of [
      ['arena',()=>arenaManager?.stop()],['WNBA',()=>blacktopUnlocks?.stop()],
      ['ball',()=>ballSelectionManager?.cleanup()],['debug menus',()=>debugMenus?.stop()],
      ['presentation',()=>blacktopPresentation?.stop()],['crowd',()=>blacktopCrowd?.stop()],
      ['outfit observer',()=>blacktopSession?.stop()],['F8 listener',()=>blacktopPanel?.waitForShortcutStop()],
      ['camera controller',()=>cameraManager?.stop(settings.modsRoot)],['game monitor',()=>gameCapture.stop()],
      ['audio conversion',()=>require('./core/flac-audio').stopFlacChildren()],
      ['Jukebox cache',()=>jukeboxManager?.releaseCache()],['roster server',()=>rosterWeb?.close()],
      ['probe',()=>observeOnlyProbe.stop()],['provider',()=>stopProbeProvider()],
    ]) {
      await assertStopped();
      if(await stop()===false)throw Error(`${name} cleanup is still pending.`);
    }
    await assertStopped();
    await sharedPresentationSession?.restore();
});
const recoverNativeHost = require('./core/host-loss-cleanup').createHostLossCleanup({
  ready: async()=>{
    if(!nativeStartupSettled||!initializationFinished||launchInProgress||versionProxySession?.background)return false;
    if(gameCapture.gamePresence()===true)return false;
    return !await processIsRunning('NBA2K27.exe',{fresh:true})&&gameCapture.gamePresence()!==true;
  },
  cleanup: cleanupNativeSession,
  complete:()=>app.finishNativeHostCleanup?.(),
  report:value=>{void fsp.appendFile(logsPath('native-host-recovery.jsonl'),JSON.stringify({at:new Date().toISOString(),...value})+'\n').catch(()=>{});},
});
app.on('native-host-lost',()=>{
  if(!process.env.WPF_NATIVE_HOST)return;
  nativeHostLost=true;presentationManager.cancel();blacktopAutoOpen?.stop();clothingCacheJob?.cancel();
  clearTimeout(animationWarmTimer);
  void animationPreview?.stop().catch(error => shutdownWarning('animation preview cleanup', error));
  blacktopPanel?.dispose();scalerManager?.stop();freestyleUnlocks?.stop();cameraManager?.stopSync();session.stop('stopped','Native host lost; headless cleanup is active.');
  void recoverNativeHost();
});
async function boundedShutdown(label,operation,milliseconds){
  let timer;try{return await Promise.race([Promise.resolve().then(operation),new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error(`${label} exceeded ${milliseconds} ms`)),milliseconds);})]);}
  catch(error){await shutdownWarning(label,error);return null;}finally{clearTimeout(timer);}
}
app.on('before-quit', require('./core/exit-handler').createExitHandler({
  quit: () => {workspaceCloseAllowed=true;app.quit();},
  forceQuit: () => app.exit(0),
  timeoutMs: 15000,
  forceOnTimeout: !process.env.WPF_NATIVE_HOST,
  onTimeout: error => shutdownWarning(process.env.WPF_NATIVE_HOST?'shutdown still pending':'hard shutdown watchdog',error),
  onError: async error => {
    nativeClosing=false;
    closingFeedback(false);
    console.error('Launcher exit needs attention:',error);
    if(nativeHostLost){await shutdownWarning('headless shutdown handoff',error);return;}
    if(!mainWindow||mainWindow.isDestroyed())createWindow();
    await dialog.showMessageBox(mainWindow,{type:'warning',message:'Launcher cleanup could not finish.',detail:'Please try closing again. '+error.message});
  },
  canQuit: async () => {
    if(process.env.WPF_NATIVE_HOST&&(!nativeStartupSettled||!initializationFinished||launchInProgress||versionProxySession?.background))throw Error('Wait for launcher initialization or game launch to finish before closing.');
    // Explicit app/OS quit must respect the same guard as the window X.
    if(commandSelfTestIndex<0&&!await require('./core/shutdown-safety').gameStopped({presence:()=>gameCapture.gamePresence(),running:processIsRunning})) {
      closingFeedback(false);
      if(!mainWindow||mainWindow.isDestroyed())createWindow();
      mainWindow.show();mainWindow.focus();
      await dialog.showMessageBox(mainWindow,{type:'info',message:'Close NBA 2K27 first.',detail:'The launcher must remain open to restore your game session.',buttons:['OK']});
      return false;
    }
    return true;
  },
  prepare: async () => {
    app.shutdownStartedAt=Date.now();
    clearTimeout(animationWarmTimer);
    closingFeedback(true,'Closing launcher… Restoring session files and stopping helpers.');
    if (await animationPreview?.stop() === false) throw Error('Animation preview cleanup is still pending.');
    if(process.env.WPF_NATIVE_HOST)nativeClosing=true;
    globalShortcut.unregister('CommandOrControl+Shift+F12');
    presentationManager.cancel();
    blacktopAutoOpen?.stop();
    clothingCacheJob?.cancel();
    blacktopPanel?.dispose(); // Destroys the hidden panel and unregisters F8.
    scalerManager?.stop();
    freestyleUnlocks?.stop();
    cameraManager?.stopSync();
    session.stop('stopped', 'Launcher closed.');
    if(process.env.WPF_NATIVE_HOST){await cleanupNativeSession();return;}
    await boundedShutdown('session loader cleanup',()=>versionProxySession?.stop(),8000);
    const closingSettings=await readSettings().catch(()=>({modsRoot:''}));
    await Promise.all([
      boundedShutdown('game diagnostic monitor',()=>gameCapture.stop(),18000),
      boundedShutdown('arena helper',()=>arenaManager?.stop(),2500),
      boundedShutdown('Blacktop ball observer',()=>ballSelectionManager?.liveSelector.stop(),2000),
      boundedShutdown('blacktop unlock helper',()=>blacktopUnlocks?.stop(),2500),
      boundedShutdown('debug menu helper',()=>debugMenus?.stop(),9000),
      boundedShutdown('Blacktop presentation helper',()=>blacktopPresentation?.stop(),9000),
      boundedShutdown('crowd restoration helper',()=>blacktopCrowd?.stop(),9000),
      boundedShutdown('Startup video window',()=>videoOverlay?.finish('launcher-exit'),2500),
      boundedShutdown('Blacktop outfit helper',()=>blacktopSession?.stop(),2500),
      boundedShutdown('F8 listener',()=>blacktopPanel?.waitForShortcutStop(),2500),
      boundedShutdown('camera controller',()=>cameraManager?.stop(closingSettings.modsRoot),3500),
      boundedShutdown('audio conversion workers',()=>require('./core/flac-audio').stopFlacChildren(),2000),
      boundedShutdown('jukebox cache release',()=>jukeboxManager?.releaseCache(),3000),
      boundedShutdown('roster web server',()=>rosterWeb?.close(),2000),
      boundedShutdown('observe-only probe',()=>observeOnlyProbe.stop(),2000),
      boundedShutdown('provider process',()=>stopProbeProvider(),6000),
    ]);
  },
}));
app.on('window-all-closed', () => { if (process.platform !== 'darwin') app.quit(); });
app.on('activate', () => { if (BrowserWindow.getAllWindows().length === 0) createWindow(); });
