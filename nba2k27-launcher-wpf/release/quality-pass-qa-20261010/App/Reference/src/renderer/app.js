const $ = (id) => document.getElementById(id);
const rosterWorkspace = new URLSearchParams(location.search).get('workspace') === 'roster';
const state = {
  settings: null,
  homeBusy: false,
  homeLoading: false,
  homeModsRoot: null,
  validation: null,
  providers: [],
  session: null,
  loader: null,
  probe: null,
  cameras: null,
  modCatalog: null,
  expandedModGroups: new Set(),
  editingModGroup: null,
  playerResearch: null,
  editingPlayer: null,
  editingTeam: null,
  jukebox: null,
  presentation: null,
  jukeboxDraftKeys: new Set(),
  editingJukeboxTrack: null,
  jukeboxAudio: null,
  playingJukeboxKey: null,
  jukeboxImportAudio: null,
  jukeboxImportArtwork: null,
  jukeboxArtworkUrl: null,
  jukeboxEditArtwork: null,
  jukeboxEditArtworkUrl: null,
  jukeboxArtworkPreviewUrls: new Map(),
  jukeboxDirtyPlaylistIds: new Set(),
  providerLayout: null,
  routePreparing: false,
  launchStartedAt: 0,
  launchTimer: null,
  toastTimer: null,
};

const LAUNCH_PHASES = Object.freeze({
  'preparing-launch': { progress: 8, step: 'Checking game build' },
  'preparing-selectors': { progress: 22, step: 'Scanning selected profile' },
  'preparing-profile': { progress: 38, step: 'Activating mod profile' },
  'preparing-jukebox': { progress: 32, step: 'Preparing playlist audio' },
  'preparing-session': { progress: 50, step: 'Starting session monitor' },
  'preparing-names': { progress: 58, step: 'Preparing camera names' },
  'preparing-loader': { progress: 66, step: 'Starting local asset loader' },
  'launching-game': { progress: 78, step: 'Opening NBA 2K27' },
  'waiting-for-game': { progress: 86, step: 'Waiting for the game' },
  'loader-armed': { progress: 91, step: 'Game connected' },
  'activating-loader': { progress: 96, step: 'Finishing unique assets' },
  'loader-active': { progress: 100, step: 'Ready' },
  'game-running': { progress: 100, step: 'Ready' },
});

function updateLaunchElapsed() {
  if (!state.launchStartedAt) return;
  const seconds = Math.max(0, Math.floor((Date.now() - state.launchStartedAt) / 1000));
  $('launchElapsed').textContent = `${seconds} second${seconds === 1 ? '' : 's'}`;
}

function showLaunchOverlay(message = 'Preparing your selected mod profile.') {
  state.launchStartedAt = Date.now();
  $('launchTitle').textContent = 'Starting NBA 2K27…';
  $('launchStage').textContent = message;
  $('launchStep').textContent = 'Preparing';
  $('launchProgressBar').style.width = '8%';
  $('launchOverlay').classList.remove('hidden');
  updateLaunchElapsed();
  clearInterval(state.launchTimer);
  state.launchTimer = setInterval(updateLaunchElapsed, 250);
}

function hideLaunchOverlay() {
  clearInterval(state.launchTimer);
  state.launchTimer = null;
  state.launchStartedAt = 0;
  $('launchOverlay').classList.add('hidden');
}

function updateLaunchOverlay(phase, message) {
  if ($('launchOverlay').classList.contains('hidden')) return;
  if (phase === 'loader-failed') {
    $('launchTitle').textContent = 'Startup needs attention';
    $('launchStage').textContent = message || 'The game could not be started.';
    $('launchStep').textContent = 'Stopped safely';
    $('launchProgressBar').style.width = '100%';
    setTimeout(hideLaunchOverlay, 2600);
    return;
  }
  const stage = LAUNCH_PHASES[phase] || { progress: 12, step: String(phase || 'Preparing').replaceAll('-', ' ') };
  $('launchStage').textContent = message || 'Preparing NBA 2K27…';
  $('launchStep').textContent = stage.step;
  $('launchProgressBar').style.width = `${stage.progress}%`;
  if (stage.progress >= 100) {
    $('launchTitle').textContent = 'NBA 2K27 is ready';
    setTimeout(hideLaunchOverlay, 900);
  }
}

function toast(message, error = false) {
  const node = $('toast');
  node.textContent = message;
  node.classList.toggle('error', error);
  node.classList.remove('hidden');
  clearTimeout(state.toastTimer);
  state.toastTimer = setTimeout(() => node.classList.add('hidden'), 5000);
}

function formatBytes(value) {
  const units = ['B', 'KB', 'MB', 'GB'];
  let size = Number(value || 0);
  let unit = 0;
  while (size >= 1024 && unit < units.length - 1) { size /= 1024; unit += 1; }
  return `${size >= 10 || unit === 0 ? size.toFixed(0) : size.toFixed(1)} ${units[unit]}`;
}

function renderPresentation(value) {
  state.presentation=value||{movie:{present:false}};
  const movie=state.presentation.movie||{};
  const info=movie.info;
  const library=state.presentation.library||{items:[],selected:null};
  const picker=$('presentationVideoSelect');picker.replaceChildren();
  if(!library.items.length)picker.add(new Option(movie.present?'Current startup video':'No movie selected',''));
  for(const item of library.items)picker.add(new Option(item.name,item.id));
  picker.value=library.selected||'';picker.disabled=!library.items.length||state.presentationImportBusy===true;
  const list=$('presentationLibrary');list.replaceChildren();
  if(!library.items.length){const empty=document.createElement('p');empty.textContent='Add an MP4 to start your video library. Choose it on Play to use it at launch.';list.append(empty);}
  for(const item of library.items){
    const row=document.createElement('div');row.className='video-library-row';
    row.classList.toggle('selected',item.id===library.selected);
    const identity=document.createElement('div');identity.className='video-library-identity';
    const poster=document.createElement('span');poster.className='video-library-poster';poster.setAttribute('aria-hidden','true');
    const year=String(item.name||'').match(/\b(?:NBA\s*)?2K(\d{2})\b/i);
    poster.textContent=year?`2K${year[1]}`:'VIDEO';
    const name=document.createElement('span');name.className='video-library-name';name.textContent=item.name;name.title=item.name;
    identity.append(poster,name);
    const meta=document.createElement('small');meta.textContent=`${Math.round(item.durationSeconds)} sec · ${formatBytes(item.bytes)}`;
    const actions=document.createElement('div');actions.className='video-library-actions';row.append(identity,meta,actions);
    for(const [action,label] of [['preview','Preview'],['rename','Edit'],['remove','Remove']]){
      const button=document.createElement('button');button.className='secondary small';button.textContent=label;
      button.disabled=state.presentationImportBusy===true;
      button.addEventListener('click',async()=>{
        if(action==='preview'){try{await window.modLauncher.previewPresentation(item.id);}catch(error){toast(error.message,true);}return;}
        if(action==='rename'){
          const dialog=$('presentationRenameDialog');dialog.dataset.videoId=item.id;
          $('presentationRenameInput').value=item.name;dialog.showModal();$('presentationRenameInput').focus();$('presentationRenameInput').select();return;
        }
        if(action==='remove'&&!confirm(`Remove "${item.name}" from the video library? The original imported file is not deleted.`))return;
        state.presentationImportBusy=true;renderPresentation(state.presentation);
        try{await applyPresentationChoice(()=>window.modLauncher.editPresentationLibrary(action,item.id),'Video library updated.');}
        finally{state.presentationImportBusy=false;renderPresentation(state.presentation);}
      });actions.append(button);
    }list.append(row);
  }
  $('choosePresentationMovie').disabled=state.presentationImportBusy===true;
  $('presentationMovieDetail').textContent=movie.error||(state.presentation.legacyPresent?'Older game-format MOV videos are no longer used. Add the original MP4 files to this library.':'Videos stay in Library / Videos. Playback does not replace or patch the game’s startup movies.');
  $('startupVideoEnabled').checked=state.settings?.startupVideoEnabled===true;
  $('startupVideoEnabled').disabled=!info||state.presentationImportBusy===true;
}

async function refreshPresentation() {
  try {renderPresentation(await window.modLauncher.presentationView());}
  catch(error){toast(error.message,true);}
}

$('presentationVideoSelect').addEventListener('change',async event=>{
  const id=event.target.value;if(!id||state.presentationImportBusy)return;
  state.presentationImportBusy=true;renderPresentation(state.presentation);
  try{await applyPresentationChoice(()=>window.modLauncher.editPresentationLibrary('select',id),'Startup video selected.');}
  finally{state.presentationImportBusy=false;renderPresentation(state.presentation);}
});
$('cancelPresentationRename').addEventListener('click',()=>$('presentationRenameDialog').close());
$('presentationRenameForm').addEventListener('submit',async event=>{
  event.preventDefault();const dialog=$('presentationRenameDialog'),name=$('presentationRenameInput').value.trim(),id=dialog.dataset.videoId;
  if(!name||state.presentationImportBusy)return;
  state.presentationImportBusy=true;dialog.close();renderPresentation(state.presentation);
  try{await applyPresentationChoice(()=>window.modLauncher.editPresentationLibrary('rename',id,name),'Video renamed.');}
  finally{state.presentationImportBusy=false;renderPresentation(state.presentation);}
});

function renderProvider() {
  const provider = state.providers.find((item) => item.id === state.settings.provider);
  $('providerLabel').textContent = provider?.name || 'Not configured';
  $('providerPill').textContent = (provider?.name || 'No provider').toUpperCase();
  $('providerDot').classList.toggle('ready', Boolean(provider?.ready));
}


function homeSetupProblem() {
  const v=state.validation;
  if(!v&&state.homeSetupError)return {kind:'blocked',message:'Setup check failed: '+state.homeSetupError,key:null};
  if(!v)return {kind:'checking',message:'Checking your game and profile folders.',key:null};
  if(v.gameRoot!=='directory')return {kind:'blocked',message:'Game folder not found. Choose your NBA 2K27 folder.',key:'gameRoot',path:state.settings?.gameRoot};
  if(!v.ready&&v.modsRoot==='directory')return {kind:'blocked',message:'NBA2K27.exe was not found in the selected game folder.',key:'gameRoot',path:v.gameExe};
  if(v.modsRoot!=='directory')return {kind:'blocked',message:'The selected mods folder does not exist.',key:'modsRoot',path:v.normalizedModsRoot};
  if(!v.ready)return {kind:'blocked',message:'Setup validation did not pass. Retry to check your folders.',key:null};
  const failure=[state.loader,state.session].find(s=>/error|failed/.test(s?.phase||''));
  if(failure)return {kind:'warning',message:failure.message||'Your session needs attention. Retry to check setup.',key:null};
  if(state.homeSetupError)return {kind:'warning',message:state.homeSetupError,key:null};
  return {kind:'ready',message:'Game and profile folders checked.',key:null};
}
function renderHomeSetup() {
  const problem=homeSetupProblem(),box=$('homeSetup');
  const active=profilesLocked();
  if(active){problem.kind='active';problem.message=state.session?.phase==='game-running'?'Session in progress. Keep the launcher open while you play.':(state.loader?.message||state.session?.message||'Preparing your session.');}
  box.dataset.state=problem.kind;
  $('homeSetupIssue').textContent=problem.message;
  $('sessionMessage').hidden=problem.kind==='ready'||problem.kind==='checking';
  $('homeSetupPath').textContent=problem.path||'';
  $('homeSetupPath').hidden=!problem.path;
  $('homeChooseFolder').hidden=!problem.key;
  $('homeChooseFolder').textContent=problem.key==='gameRoot'?'Choose game folder':'Choose mods folder';
  $('homeRetrySetup').hidden=problem.kind==='ready';
  $('homeChooseFolder').disabled=active||state.homeSetupBusy===true;
  $('homeRetrySetup').disabled=active||state.homeSetupBusy===true;
  $('startSession').disabled=problem.kind==='blocked'||problem.kind==='checking'||state.homeSetupBusy===true||state.homeBusy||state.homeLoading||active||Boolean(state.launchStartedAt);
  if(problem.kind==='checking')$('launchStatus').dataset.state='busy';
  if(!active&&!state.launchStartedAt) {
    $('launchStatus').textContent=problem.kind==='ready'?'Ready':problem.kind==='checking'?'Checking':problem.kind==='warning'?'Check scan':'Check setup';
    $('launchStatus').dataset.state=problem.kind==='ready'?'ready':problem.kind==='checking'?'busy':'error';
  }
}
async function refreshHomeSetup() {
  const data=await window.modLauncher.bootstrap();
  state.settings=data.settings;state.validation=data.validation;state.providers=data.providers;
  state.homeSetupError=null;fillForm();renderProvider();renderValidation();renderSessionUi();
  if(state.validation.ready)await scanModCatalog();
}
$('homeRetrySetup').addEventListener('click',async()=>{
  if(state.homeSetupBusy||profilesLocked())return;
  state.homeSetupBusy=true;renderHomeSetup();
  try{await refreshHomeSetup();}catch(error){state.homeSetupError=error.message;}
  finally{state.homeSetupBusy=false;renderHomeSetup();}
});
$('homeChooseFolder').addEventListener('click',async()=>{
  const key=homeSetupProblem().key;if(!key||state.homeSetupBusy||profilesLocked())return;
  state.homeSetupBusy=true;renderHomeSetup();
  try {
    const selected=await window.modLauncher.choosePath(key);
    if(selected){const result=await window.modLauncher.saveSettings({[key]:selected});state.settings=result.settings;state.validation=result.validation;state.homeSetupError=null;fillForm();renderValidation();if(state.validation.ready)await scanModCatalog();}
  }catch(error){state.homeSetupError=error.message;}
  finally{state.homeSetupBusy=false;renderHomeSetup();}
});

function renderValidation() {
  const ready = Boolean(state.validation?.ready);
  $('validationLabel').textContent = ready ? 'GAME READY' : 'CHECK PATHS';
  $('validationLabel').style.color = ready ? 'var(--green)' : 'var(--amber)';
  renderHomeSetup();
}

function renderProviderLayout(value) {
  state.providerLayout = value;
  const status = value?.status || 'pending';
  const report = value?.report;
  $('resolverState').textContent = status.toUpperCase();
  $('resolverSummary').textContent = value?.message || 'No provider-layout result is available.';
  $('resolverDot').className = `status-dot ${status === 'ready' ? 'ready' : status === 'error' || status === 'blocked' ? 'error' : ''}`;
  $('resolverOutput').textContent = report
    ? JSON.stringify(report, null, 2)
    : (value?.message || 'No resolver report yet.');
}

function renderSessionUi() {
  const session = state.session || {};
  const loader = state.loader || {};
  const sessionPhase = session.phase || 'idle';
  const loaderPhase = loader.phase || 'idle';
  const loaderBusy = ['preparing-loader', 'launching-game', 'waiting-for-game', 'loader-armed', 'activating-loader', 'loader-active', 'stopping-loader'].includes(loaderPhase);
  const displayLoader = !['idle', 'stopped'].includes(loaderPhase);
  const phase = displayLoader ? loaderPhase : sessionPhase;
  $('sessionPhase').textContent = phase.replaceAll('-', ' ').toUpperCase();
  const sessionDetail = displayLoader ? loader.message : (session.message || 'No active session.');
  const launchError=/error|failed/.test(phase);
  $('launchStatus').textContent=launchError?'Needs attention':sessionPhase==='game-running'?'Running':loaderBusy?'Session active':state.validation?.ready?'Ready':'Check setup';
  $('launchStatus').dataset.state=launchError?'error':sessionPhase==='game-running'?'running':loaderBusy?'busy':state.validation?.ready?'ready':'idle';
  $('homeSessionActive').classList.toggle('hidden',!(loaderBusy||sessionPhase==='game-running'));
  $('homeSessionActiveTitle').textContent=sessionPhase==='game-running'?'Session in progress':'Preparing your session';
  $('sessionMessage').textContent=launchError?sessionDetail:loaderBusy||sessionPhase==='game-running'?'Your session is active.':'Choose your profile and playlist below.';
  $('sessionMessage').title=sessionDetail||'';
  $('startSession').classList.toggle('hidden', loaderBusy || sessionPhase === 'game-running');
  $('stopSession').classList.add('hidden');
  ['stepProvider', 'stepGame', 'stepCamera'].forEach((id) => $(id).classList.remove('active', 'done'));
  if (loaderBusy && loaderPhase !== 'loader-active') $('stepProvider').classList.add('active');
  if (loaderPhase === 'loader-active' || loaderPhase === 'complete') $('stepProvider').classList.add('done');
  if (['launching-game', 'waiting-for-game', 'loader-armed', 'activating-loader'].includes(loaderPhase)) $('stepGame').classList.add('active');
  if (sessionPhase === 'game-running' || loaderPhase === 'loader-active' || loaderPhase === 'complete') $('stepGame').classList.add('done');
  if (sessionPhase === 'game-running') $('stepCamera').classList.add(session.cameraState === 'started' ? 'done' : 'active');
  if (loaderPhase === 'complete' && session.cameraState === 'started') $('stepCamera').classList.add('done');
  const profileLocked = loaderBusy || sessionPhase === 'game-running';
  $('profilesSelectionStatus').textContent=profileLocked?'Profiles are locked while your game session is active.':'Choose a profile to make it active. Your selection is saved for the next Play session.';
  document.querySelectorAll('[name="activeModsProfile"]').forEach((input) => { input.disabled = profileLocked; });
  $('addModsProfile').disabled = profileLocked || !!state.profilePicking;
  document.querySelectorAll('[data-remove-profile], [data-edit-profile]').forEach(button=>{button.disabled=profileLocked;});
  const profileDialogBusy = $('profileDialog').dataset.saving === 'true' || $('profileDialog').dataset.picking === 'true';
  $('saveProfileDialog').disabled = profileLocked || profileDialogBusy;
  $('chooseProfileDialogFolder').disabled = profileLocked || profileDialogBusy;
  syncHomePicks();
}

function syncHomePicks() {
  const profile=$('homeModProfile'), playlist=$('homeJukeboxPlaylist');
  const profiles=state.settings?.modsProfiles || [];
  profile.replaceChildren(...profiles.map(p=>new Option(p.name,p.path)));
  const active=profiles.find(p=>p.path.toLowerCase()===String(state.settings?.modsRoot||'').toLowerCase());
  if(active) profile.value=active.path;
  const value=state.jukebox;
  playlist.replaceChildren(new Option(state.homeLoading?'Loading playlists…':value?.available?'Default Playlist (Locked)':'Jukebox unavailable',''),
    ...(value?.playlists||[]).map(p=>new Option(`${p.name} (${p.trackCount})`,p.id)));
  playlist.value=value?.activePlaylistId||'';
  $('homeJukeboxEnabled').checked=Boolean(value?.activePlaylistId);
  const locked=state.session?.phase==='game-running'||['preparing-loader','launching-game','waiting-for-game','loader-armed','activating-loader','loader-active','stopping-loader'].includes(state.loader?.phase);
  profile.disabled=locked||state.homeBusy||state.homeLoading||!profiles.length;
  playlist.disabled=locked||state.homeBusy||state.homeLoading||!value?.available||!value?.activePlaylistId;
  $('homeJukeboxEnabled').disabled=locked||state.homeBusy||state.homeLoading||!value?.available||!value?.playlists?.length;
  $('homeJukeboxEnabled').title=value?.playlists?.length?'Turn off to use the default jukebox.':'Add a playlist in Jukebox to enable custom music.';
  if(state.homeBusy||state.homeLoading) $('startSession').disabled=true;
  renderHomeSetup();
  $('homePicksStatus').textContent=state.homeBusy?'Saving selection…':state.homeLoading?'Reading your jukebox…':locked?'Selections are locked while the game session is active.':'';
}

function renderSession(session) {
  state.session = session;
  updateLaunchOverlay(session?.phase, session?.message);
  renderSessionUi();
}

function renderLoader(loader) {
  state.loader = loader;
  updateLaunchOverlay(loader?.phase, loader?.message);
  renderSessionUi();
}

function cameraSetupFromUi() {
  return {
    lockValues: $('lockCameraValues').checked,
    cameras: (state.cameras?.cameras || []).map((camera, index) => {
      const row = document.querySelector(`.camera-row[data-index="${index}"]`);
      return {
        id: camera.id,
        name: row?.querySelector('[data-field="name"]')?.value ?? camera.name,
        selected: Boolean(row?.querySelector('[data-field="selected"]')?.checked),
        height: Number.parseInt(row?.querySelector('[data-field="height"]')?.value, 10),
        zoom: Number.parseInt(row?.querySelector('[data-field="zoom"]')?.value, 10),
      };
    }),
  };
}

function renderCameraRows() {
  const cameras = state.cameras?.cameras || [];
  const rows = $('cameraRows');
  rows.replaceChildren(...cameras.map((camera, index) => {
    const row = document.createElement('div');
    row.className = 'camera-row';
    row.dataset.index = String(index);
    row.innerHTML = `<input type="checkbox" data-field="selected" ${camera.selected ? 'checked' : ''}>
      <div class="camera-name"><div class="camera-name-editor"><input type="text" data-field="name" maxlength="32" spellcheck="false"><button type="button" data-reset-name>↺</button></div></div>
      <input type="number" min="-100" max="100" step="1" data-field="height" value="${camera.height}">
      <input type="number" min="-100" max="100" step="1" data-field="zoom" value="${camera.zoom}">
      <div class="camera-order-buttons"><button data-move="up" ${index === 0 ? 'disabled' : ''}>↑</button><button data-move="down" ${index === cameras.length - 1 ? 'disabled' : ''}>↓</button></div>`;
    row.querySelector('[data-field="name"]').value = camera.name;
    row.querySelector('[data-field="name"]').maxLength = camera.nameMaxLength || 18;
    row.querySelector('[data-field="name"]').disabled = Boolean(camera.nameLocked);
    row.querySelector('[data-reset-name]').disabled = Boolean(camera.nameLocked);
    const nameHelp = camera.nameLocked ? 'Stock camera name is locked' : `${camera.nameMaxLength} characters maximum`;
    row.querySelector('.camera-name').title = nameHelp;
    row.querySelector('[data-field="name"]').title = nameHelp;
    row.querySelector('[data-reset-name]').title = camera.nameLocked ? 'Stock camera name is locked' : `Reset to ${camera.defaultName || camera.name}`;
    row.querySelector('[data-reset-name]').setAttribute('aria-label', row.querySelector('[data-reset-name]').title);
    if (camera.nameLocked) row.querySelector('[data-reset-name]').textContent = '—';
    row.querySelector('[data-field="name"]').placeholder = camera.defaultName || camera.name;
    for (const field of ['selected', 'name', 'height', 'zoom']) {
      row.querySelector(`[data-field="${field}"]`).setAttribute('aria-label', `${camera.name} ${field}`);
    }
    row.querySelector('[data-field="name"]').setAttribute('aria-description', nameHelp);
    for (const direction of ['up', 'down']) row.querySelector(`[data-move="${direction}"]`).setAttribute('aria-label', `Move ${camera.name} ${direction}`);
    return row;
  }));
  $('lockCameraValues').checked = state.cameras?.lockValues === true;
  renderCameraStatus(state.cameras);
}

function renderCameraStatus(value) {
  if (!value) return;
  state.cameras = { ...(state.cameras || {}), ...value, cameras: value.cameras || state.cameras?.cameras || [] };
  const service = value.service || state.cameras.service || {};
  const phase = service.state || 'STOPPED';
  const active = phase === 'WAITING' || phase === 'STARTING' || phase === 'LIVE';
  $('cameraState').textContent = phase.replaceAll('_', ' ');
  $('cameraMessage').textContent = service.message || 'The camera controller is stopped.';
  $('cameraDot').className = `status-dot ${phase === 'LIVE' ? 'live' : (phase === 'WAITING' || phase === 'STARTING') ? 'waiting' : phase === 'ERROR' ? 'error' : ''}`;
  $('stopCameras').classList.toggle('hidden', !active);
  $('retryCameras').classList.toggle('hidden', phase !== 'ERROR');
  const selectedCount = (state.cameras.cameras || []).filter((camera) => camera.selected).length;
  $('cameraCount').textContent = `${selectedCount} SELECTED`;
  const names = value.namesMod || state.cameras.namesMod || {};
  $('cameraNamesStatus').textContent = names.appManaged
    ? (names.conflict ? 'Another English text mod is present; it will not be overwritten.' : names.installed ? 'Camera names active for this game session; cleaned up after exit.' : 'App-managed camera names apply automatically on Play. Restart the game after updating.')
    : names.available === false
    ? 'Optional name file unavailable in this build; cameras still load normally.'
    : names.installed
    ? 'Installed and matches the tested camera-name mod.'
    : (names.conflict ? 'A different English text mod is installed; it will not be overwritten.' : 'Not installed in the active mods folder.');
  $('installCameraNames').classList.toggle('hidden', Boolean(names.appManaged) || Boolean(names.installed) || names.available === false);
  $('installCameraNames').disabled = Boolean(names.conflict) || names.available === false;
}

function renderCameras(value) {
  state.cameras = value;
  renderCameraRows();
}

function renderProbe(probe) {
  state.probe = probe;
  const armed = Boolean(probe?.armed);
  const handshake = probe?.lastHandshake;
  const lifecycle = probe?.lifecycle;
  const live = Boolean(armed && lifecycle?.active);
  const stopped = Boolean(lifecycle?.orderlyShutdown);
  const processExited = Boolean(lifecycle?.processExitConfirmed);
  const registration = lifecycle?.registration;
  const registrationFailure = lifecycle?.registrationFailure;
  const liveRoute = lifecycle?.liveRoute;
  const liveRouteFailure = lifecycle?.liveRouteFailure;
  const liveRouteState = lifecycle?.liveRouteState;
  const configTraces = Array.isArray(lifecycle?.pathDiagnostics)
    ? lifecycle.pathDiagnostics
    : [];
  const autoFallback = probe?.autoFallback;
  const autoFallbackError = autoFallback?.error;
  const providerAsset = probe?.providerAsset;
  const providerAssets = probe?.providerAssets || (providerAsset ? [providerAsset] : []);
  const assetLabel = providerAsset?.logicalPath || 'local asset';
  const secondAssetLabel = providerAssets[1]?.logicalPath || 'second local asset';
  const failed = Boolean(registrationFailure || liveRouteFailure || autoFallbackError);
  const routePending = Boolean(probe?.liveRouteArmed && !liveRoute);
  $('probeState').textContent = failed
    ? 'ROUTE FAILED'
    : (liveRoute?.passed
      ? 'ROUTE PASSED'
      : (routePending
        ? (liveRouteState?.targetSeen ? 'SERVING MODS' : 'FALLBACK ARMED')
        : (live
          ? 'LIVE'
          : (stopped ? 'STOPPED CLEANLY' : (processExited ? 'GAME CLOSED' : (armed ? 'ARMED' : 'DISARMED'))))));
  $('probeState').style.color = failed
    ? 'var(--red)'
    : (liveRoute?.passed || live || stopped || processExited
      ? 'var(--green)'
      : (armed ? 'var(--amber)' : 'var(--muted)'));
  $('armProbe').classList.toggle('hidden', armed);
  $('disarmProbe').classList.toggle('hidden', !armed);
  $('registrationProbe').disabled = armed;
  $('activateLiveRoute').classList.toggle('hidden', !autoFallbackError);
  $('activateLiveRoute').disabled = state.routePreparing;
  $('activateLiveRoute').textContent = state.routePreparing
    ? 'Retrying fallback…'
    : 'Retry local fallback';
  const output = $('probeDiagnostic');
  output.className = '';
  if (!handshake) {
    if (autoFallbackError) output.className = 'failure';
    output.textContent = autoFallbackError
      ? `AUTOMATIC FALLBACK ERROR · ${autoFallbackError}`
      : (armed
      ? `${probe.registrationProbeArmed ? 'STREAM TEST ARMED' : 'AUTOMATIC FALLBACK ARMED'} · launch NBA 2K27 and the packaged add-on will connect automatically · hooks OFF.`
      : 'Disarmed. Connect to the game before activating the local fallback.');
    return;
  }
  const hex = (value) => `0x${Number(value || 0).toString(16).toUpperCase().padStart(8, '0')}`;
  if (liveRouteFailure) {
    output.className = 'failure';
    output.textContent = `ENDED · local fallback did not close cleanly · reason ${liveRouteFailure.reason} · local info hits ${liveRouteFailure.targetFileInfoHits}, opens ${liveRouteFailure.targetOpenSuccesses}, reads ${liveRouteFailure.targetTransferCalls}, closes ${liveRouteFailure.targetCloseSuccesses} · provider ${liveRouteFailure.providerReadCalls} reads / ${formatBytes(Number(liveRouteFailure.providerBytes || 0))} · bindings ${liveRouteFailure.bindingReleases}/${liveRouteFailure.bindingAllocations} released, active ${liveRouteFailure.bindingActiveAfter} · default ${hex(liveRouteFailure.defaultDuring)} → ${hex(liveRouteFailure.defaultAfter)} · hooks OFF · manifest-backed stock assets unchanged.`;
    return;
  }
  if (registrationFailure) {
    output.className = 'failure';
    output.textContent = `FAILED · multi-handle report rejected · reason ${registrationFailure.reason} · checks ${hex(registrationFailure.checks)} · bindings allocated ${registrationFailure.bindingAllocations}, released ${registrationFailure.bindingReleases}, peak ${registrationFailure.bindingPeak}/${registrationFailure.bindingCapacity}, active ${registrationFailure.bindingActiveAfter}, read rounds ${registrationFailure.interleavedReadRounds}/2 · unprefixed opens ${registrationFailure.unprefixedOpenSuccesses}/${registrationFailure.unprefixedOpenAttempts}, local ${registrationFailure.unprefixedLocalOpenSuccesses}, stock ${registrationFailure.unprefixedStockOpenSuccesses}, restores ${registrationFailure.unprefixedOpenRestores}; default ${hex(registrationFailure.openDefaultDuring)} → ${hex(registrationFailure.openDefaultAfter)} · local SHA matches ${registrationFailure.payloadMatches}/2, provider ${registrationFailure.providerReadCalls} reads / ${formatBytes(Number(registrationFailure.providerBytes || 0))} · stock transfers ${registrationFailure.stockTransferCalls}, closes ${registrationFailure.stockCloseCalls}, byte matches ${registrationFailure.stockPayloadMatches}/2 · all closes ${registrationFailure.closeCalls}/4 · current handles ${lifecycle.resourceHandles} · hooks OFF · stock replacement OFF.`;
    return;
  }
  if (!handshake.accepted) {
    output.className = 'failure';
    output.textContent = `REJECTED · ${handshake.eventName} · reason ${handshake.reason} · unsupported or invalid lifecycle event · hooks OFF · registration OFF · stock replacement OFF.`;
    return;
  }
  output.className = 'success';
  if (autoFallbackError) {
    output.className = 'failure';
    output.textContent = `AUTOMATIC FALLBACK ERROR · ${autoFallbackError} · correct the mods-folder setting, then use Retry local fallback.`;
  } else if (lifecycle?.orderlyShutdown) {
    output.textContent = `PASS · orderly shutdown · PID ${lifecycle.pid} · callbacks 0 · resource handles 0 · hooks OFF · active registration OFF · stock replacement OFF.`;
  } else if (lifecycle?.processExitConfirmed) {
    output.textContent = `PASS · game process exit confirmed · PID ${lifecycle.pid} is no longer running · the add-on and its heartbeat thread cannot remain loaded · hooks OFF · active registration OFF · stock replacement OFF.`;
  } else if (liveRoute?.passed) {
    output.textContent = `PASS · local fallback stopped cleanly · local info hits ${liveRoute.targetFileInfoHits}, opens ${liveRoute.targetOpenSuccesses}, game reads ${liveRoute.targetTransferCalls}, closes ${liveRoute.targetCloseSuccesses} · ${formatBytes(Number(liveRoute.providerBytes))} served in ${liveRoute.providerReadCalls} provider reads · stock paths preserved ${liveRoute.stockFileInfoSuccesses}/${liveRoute.stockFileInfoCalls} · bindings ${liveRoute.bindingReleases}/${liveRoute.bindingAllocations} released, peak ${liveRoute.bindingPeak}/${liveRoute.bindingCapacity}, active 0 · default ${hex(liveRoute.temporaryDefaultDeviceId)} → stock ${hex(liveRoute.restoredDefaultDeviceId)} · registration OFF · hooks OFF.`;
  } else if (lifecycle?.active && probe.liveRouteArmed) {
    if (liveRouteState?.draining) {
      output.textContent = `LOCAL FALLBACK STOPPING · stock default restored · draining ${liveRouteState.resourceHandles} already-open handle${liveRouteState.resourceHandles === 1 ? '' : 's'} before unregister · hooks OFF.`;
    } else if (liveRouteState?.targetSeen) {
      const trace = configTraces.length
        ? ` · config trace ${configTraces.length}: ${configTraces.map((item) => item.requestPath).join(' | ')}`
        : '';
      output.textContent = `LOCAL FALLBACK SERVING · at least one stock-missing game path matched an exact file under mods${trace} · fallback remains active for this game session · hooks OFF.`;
    } else if (liveRouteState?.registrationActive && liveRouteState?.defaultActive) {
      const trace = configTraces.length
        ? ` · config trace ${configTraces.length}: ${configTraces.map((item) => item.requestPath).join(' | ')}`
        : '';
      output.textContent = `LOCAL FALLBACK ACTIVE · stock paths stay stock; unresolved paths are checked against exact files under mods${trace} · hooks OFF · no filename guessing.`;
    } else {
      output.textContent = 'LOCAL FALLBACK PENDING · the current-user-only provider is indexed and waiting for the next game heartbeat · hooks OFF.';
    }
  } else if (lifecycle?.active && autoFallback?.starting) {
    output.textContent = 'STARTING LOCAL FALLBACK · indexing exact paths under mods and opening the current-user-only provider automatically…';
  } else if (lifecycle?.active) {
    const registrationState = registration?.passed
      ? `path-aware bindings PASS · distinct-path check YES, bound provider handles ${registration.pathBoundProviderHandles} · ${assetLabel} ${formatBytes(Number(registration.assetSize))} SHA-256 MATCH · ${secondAssetLabel} ${formatBytes(Number(registration.secondAssetSize))} SHA-256 MATCH · bindings ${registration.bindingAllocations}/${registration.bindingCapacity}, released ${registration.bindingReleases}, peak ${registration.bindingPeak}, active 0 · interleaved read rounds ${registration.interleavedReadRounds}/2 · device ID 0 opens ${registration.unprefixedOpenSuccesses}/${registration.unprefixedOpenAttempts}, local ${registration.unprefixedLocalOpenSuccesses}, stock ${registration.unprefixedStockOpenSuccesses}, immediate restores ${registration.unprefixedOpenRestores} · default ${hex(registration.openDefaultDuring)} → stock ${hex(registration.openDefaultAfter)} before reads · two stock streams direct-baseline MATCH · active registration OFF · `
      : (lifecycle.registrationProbeArmed ? `controlled path-aware test pending for ${assetLabel} · ` : 'automatic local fallback waiting for activation · ');
    output.textContent = `LIVE · NBA 2K27 PID ${lifecycle.pid} · exact build · ${registrationState}${lifecycle.heartbeatCount} heartbeats · callbacks ${lifecycle.callbacksInFlight}, resource handles ${lifecycle.resourceHandles} · hooks OFF · stock replacement OFF.`;
  } else {
    output.textContent = `PASS · ${handshake.eventName} · NBA 2K27 PID ${handshake.pid} · hooks OFF · registration OFF · stock replacement OFF.`;
  }
}

function readForm() {
  const modsRoot = String(state.settings?.modsRoot || '').trim();
  const profiles = Array.isArray(state.settings?.modsProfiles) ? state.settings.modsProfiles : [];
  const modsProfiles = [...profiles];
  if (modsRoot && !modsProfiles.some((profile) => profile.path.toLowerCase() === modsRoot.toLowerCase())) {
    const parts = modsRoot.replace(/[\\/]+$/, '').split(/[\\/]/);
    modsProfiles.push({ name: parts.at(-1) || 'Mods', path: modsRoot });
  }
  return {
    gameRoot: $('gameRoot').value.trim(),
    modsRoot,
    modsProfiles,
  };
}

function fillForm() {
  window.dispatchEvent(new Event('launcher:settings-refreshed'));
  $('unlockWnbaFreestyle').checked = state.settings.unlockWnbaFreestyle === true;
  $('unlockWnbaBlacktop').checked = state.settings.unlockWnbaBlacktop === true;
  $('autoOpenBlacktop').checked = state.settings.autoOpenBlacktop === true;
  $('usePlaylistInFreestyle').checked = state.settings.usePlaylistInFreestyle !== false;
  $('cachePreparedPlaylists').checked = state.settings.cachePreparedPlaylists !== false;
  $('startupVideoEnabled').checked = state.settings.startupVideoEnabled === true;
  $('gameRoot').value = state.settings.gameRoot || '';
  const profiles = state.settings.modsProfiles || [];
  $('profileRows').replaceChildren(...profiles.map((profile) => {
    const row = document.createElement('div');
    row.className = 'profile-row';
    const choice = document.createElement('label');
    choice.className = 'profile-choice';
    const input = document.createElement('input');
    input.type = 'radio';
    input.name = 'activeModsProfile';
    input.value = profile.path;
    input.checked = profile.path.toLowerCase() === String(state.settings.modsRoot || '').toLowerCase();
    const detail = document.createElement('span');
    const name = document.createElement('strong');
    name.textContent = profile.name;
    name.title = profile.name;
    const profilePath = document.createElement('small');
    profilePath.textContent = profile.path;
    profilePath.title = profile.path;
    detail.append(name, profilePath);
    choice.append(input, detail);
    row.append(choice);
    const actions=document.createElement('span');actions.className='profile-row-actions';
    const buttons=document.createElement('span');buttons.className='profile-row-buttons';
    const defaultPath = `${String(state.settings.gameRoot || '').replace(/[\\/]+$/, '')}\\mods`;
    if (profile.path.toLowerCase() !== defaultPath.toLowerCase()) {
      const edit=document.createElement('button');edit.type='button';edit.className='secondary small profile-edit';
      edit.textContent='Edit';edit.dataset.editProfile=profile.path;edit.setAttribute('aria-label',`Edit ${profile.name} profile`);
      edit.addEventListener('click',()=>openProfileDialog('edit',profile.path));
      buttons.append(edit);
    }
    if (!input.checked && profile.path.toLowerCase() !== defaultPath.toLowerCase()) {
      const remove=document.createElement('button');remove.type='button';remove.className='secondary small profile-remove';
      remove.textContent='Remove';remove.dataset.removeProfile=profile.path;remove.setAttribute('aria-label',`Remove ${profile.name} profile`);
      remove.addEventListener('click',event=>{event.preventDefault();event.stopPropagation();removeProfileShortcut(profile.path);});
      buttons.append(remove);
    }
    actions.append(buttons);
    row.append(actions);
    return row;
  }));
  const sessionPhase = state.session?.phase || 'idle';
  const loaderPhase = state.loader?.phase || 'idle';
  const profileLocked = sessionPhase === 'game-running'
    || ['preparing-loader', 'launching-game', 'waiting-for-game', 'loader-armed', 'activating-loader', 'loader-active', 'stopping-loader'].includes(loaderPhase);
  document.querySelectorAll('[name="activeModsProfile"], [data-remove-profile], [data-edit-profile]').forEach(control=>{control.disabled=profileLocked;});
  $('profileCount').textContent = `${profiles.length} PROFILE${profiles.length === 1 ? '' : 'S'}`;
  const active = profiles.find((profile) => profile.path.toLowerCase() === String(state.settings.modsRoot || '').toLowerCase());
  $('activeProfileName').textContent = active?.name || 'Active profile';
  $('activeProfilePath').textContent = state.settings.modsRoot || 'No profile selected.';
  $('modsRootLabel').textContent = state.settings.modsRoot;
  const changed=state.homeModsRoot!==null && state.homeModsRoot!==state.settings.modsRoot;
  state.homeModsRoot=state.settings.modsRoot;
  if(changed) {
    state.expandedModGroups.clear();
    renderCatalog({root:state.settings.modsRoot,files:[],count:0,totalBytes:0,categories:{}});
    state.homeCatalogPromise=scanModCatalog({startup:true});
  }
  syncHomePicks();
}

async function saveSettings() {
  try {
    const result = await window.modLauncher.saveSettings(readForm());
    state.settings = result.settings;
    state.validation = result.validation;
    state.providers = (await window.modLauncher.bootstrap()).providers;
    fillForm();
    renderProvider();
    renderValidation();
    toast('Settings saved.');
  } catch (error) { toast(error.message, true); }
}

function renderCatalog(catalog) {
  state.modCatalog = catalog;
  const stats = $('modStats').children;
  stats[0].querySelector('strong').textContent = catalog.count.toLocaleString();
  stats[1].querySelector('strong').textContent = formatBytes(catalog.totalBytes);
  stats[2].querySelector('strong').textContent = (catalog.categories.Player || 0).toLocaleString();
  stats[3].querySelector('strong').textContent = (catalog.categories.Arena || 0).toLocaleString();
  $('modsRootLabel').textContent = catalog.root;
  $('modsRootLabel').title = catalog.root;
  renderModRows();
}

let modRenderRevision = 0;
function createModFileActions(file) {
  const actions=document.createElement('div');actions.className='mod-file-actions';
  for(const [action,label,title] of [['file','Open','Open this mod file'],['folder','Folder','Show this mod in its folder']]){
    const button=document.createElement('button');button.type='button';button.className='secondary small';button.textContent=label;button.title=title;button.setAttribute('aria-label',`${title}: ${file.relativePath}`);
    button.prepend(createLauncherIcon(action==='folder'?'profiles':'file'));
    button.addEventListener('click',event=>{event.stopPropagation();window.modLauncher.openModItem(file.id,action).catch(error=>toast(error.message,true));});actions.append(button);
  }
  return actions;
}
async function renderModRows() {
  const revision = ++modRenderRevision;
  if (!$('page-mods').classList.contains('active')) return;
  const catalog = state.modCatalog;
  const body = $('modRows');
  const staging = document.createElement('div');
  if (!catalog) {
    body.innerHTML = '<div class="empty">Scan the folder to build the local mod catalog.</div>';
    $('modResultSummary').textContent = 'Scan the folder to search its files.';
    $('modShownSummary').textContent = 'No catalog loaded';
    return;
  }
  const groupsById = new Map();
  let grouped = 0;
  for (const file of catalog.files) {
    if (++grouped % 250 === 0) {
      await new Promise(resolve => setTimeout(resolve, 0));
      if (revision !== modRenderRevision) return;
    }
    const parts = file.relativePath.replaceAll('\\', '/').split('/');
    const leaf = parts.at(-1);
    const folder = parts.slice(0, -1).join('/');
    const stemMatch = /^((?:png)\d{4,6})(?=[_.]|$)/i.exec(leaf);
    const groupId = stemMatch ? `player:${folder.toLowerCase()}:${stemMatch[1].toLowerCase()}` : `file:${file.id}`;
    if (!groupsById.has(groupId)) groupsById.set(groupId, { id: groupId, stem: stemMatch?.[1] || '', folder, files: [] });
    groupsById.get(groupId).files.push(file);
  }
  const groups = [...groupsById.values()].map((group) => {
    const base = group.stem
      ? group.files.find((file) => file.name.toLowerCase() === `${group.stem.toLowerCase()}.iff`) || group.files[0]
      : group.files[0];
    const tags = [...new Set(group.files.flatMap((file) => file.tags || []))];
    return {
      ...group,
      title: group.stem ? [group.folder, group.stem].filter(Boolean).join('/') : base.relativePath,
      tagFileId: base.id,
      category: base.category,
      baseFile: base,
      tags,
      size: group.files.reduce((sum, file) => sum + file.size, 0),
    };
  });
  const sort=state.modSort || {key:'title',direction:1};
  groups.sort((left,right)=>{
    const compared=sort.key==='size'?left.size-right.size:String(sort.key==='tags'?left.tags.join(', '):left[sort.key]).localeCompare(String(sort.key==='tags'?right.tags.join(', '):right[sort.key]),undefined,{numeric:true});
    return (compared || left.title.localeCompare(right.title))*sort.direction;
  });
  const query = $('modSearch').value.trim().toLowerCase();
  const matching = query
    ? groups.filter((group) => `${group.title} ${group.category} ${group.tags.join(' ')} ${group.files.map((file) => file.relativePath).join(' ')}`.toLowerCase().includes(query))
    : groups;
  const shown = matching.slice(0, 5000);
  let rendered = 0;
  for (const group of shown) {
    if (++rendered % 40 === 0) {
      await new Promise(resolve => setTimeout(resolve, 0));
      if (revision !== modRenderRevision) return;
    }
    const row = document.createElement('div');
    row.className = 'mod-row';
    row.setAttribute('data-view-key', group.id);
    const asset = document.createElement('div');
    asset.className = 'mod-asset-cell';
    const toggle = document.createElement('button');
    toggle.className = 'mod-tree-toggle';
    toggle.textContent = group.files.length > 1 ? (state.expandedModGroups.has(group.id) ? '▼' : '▶') : '•';
    toggle.disabled = group.files.length === 1;
    toggle.type = 'button';
    toggle.setAttribute('aria-label',`${state.expandedModGroups.has(group.id)?'Collapse':'Expand'} ${group.title}`);
    if(group.files.length>1)toggle.setAttribute('aria-expanded',String(state.expandedModGroups.has(group.id)));
    const icon=createLauncherIcon(group.category==='Player'?'players':group.category==='Arena'?'blacktop':group.category==='Uniform'?'uniform':/^balls[\\/]/i.test(group.title)?'ball':'file');
    icon.classList.add('mod-asset-icon');
    const assetText = document.createElement('div');
    const assetName = document.createElement('div');
    assetName.className = 'mod-asset-name';
    assetName.textContent = group.title;
    assetName.title = group.title;
    const dependencyCount = document.createElement('small');
    dependencyCount.textContent = group.files.length > 1 ? `${group.files.length} bundled files` : 'Single file';
    assetText.append(assetName, dependencyCount);
    asset.append(toggle, icon, assetText);
    const category = document.createElement('span');
    category.className = 'category';
    category.textContent = group.category;
    const tags = document.createElement('button');
    tags.className = `mod-tag-button${group.tags.length ? '' : ' untagged'}`;
    tags.textContent = group.tags.length ? group.tags.join(', ') : 'Add tags';
    tags.title = group.tags.length ? group.tags.join(', ') : 'Add searchable tags';
    tags.setAttribute('aria-label',`Edit tags for ${group.title}`);
    const size = document.createElement('span');
    size.className = 'size';
    size.textContent = formatBytes(group.size);
    row.append(asset, category, tags, size, createModFileActions(group.baseFile));
    staging.append(row);
    let dependencyRows = [];
    const updateGroupRows = () => {
      const expanded = state.expandedModGroups.has(group.id);
      toggle.textContent = expanded ? '▼' : '▶';
      toggle.setAttribute('aria-expanded', String(expanded));
      toggle.setAttribute('aria-label', `${expanded ? 'Collapse' : 'Expand'} ${group.title}`);
      for (const dependency of dependencyRows) dependency.remove();
      dependencyRows = [];
      if (!expanded) return;
      const nextRow = row.nextSibling;
      for (const file of group.files) {
        const dependency = document.createElement('div');
        dependency.className = 'mod-dependency-row';
        dependency.setAttribute('data-view-key', `dependency:${file.id}`);
        const dependencyPath = document.createElement('span');
        dependencyPath.className = 'dependency-path mono';
        dependencyPath.textContent = `└ ${file.relativePath}`;
        dependencyPath.title = file.relativePath;
        const dependencyType = document.createElement('span');
        dependencyType.textContent = file.id === group.tagFileId ? 'Base asset' : 'Dependency';
        const dependencyTags = document.createElement('span');
        dependencyTags.textContent = '';
        const dependencySize = document.createElement('span');
        dependencySize.textContent = formatBytes(file.size);
        dependency.append(dependencyPath, dependencyType, dependencyTags, dependencySize, createModFileActions(file));
        row.parentNode.insertBefore(dependency, nextRow);
        dependencyRows.push(dependency);
      }
    };
    const toggleGroup = () => {
      if (group.files.length === 1) { openModTagEditor(group); return; }
      // Never clear the scroll container just to expand one bundle. Keeping the
      // existing rows also preserves keyboard focus and avoids an async rebuild.
      const scrollTop = body.scrollTop, scrollLeft = body.scrollLeft;
      if (state.expandedModGroups.has(group.id)) state.expandedModGroups.delete(group.id);
      else state.expandedModGroups.add(group.id);
      updateGroupRows();
      body.scrollTop = scrollTop;
      body.scrollLeft = scrollLeft;
    };
    row.addEventListener('click', toggleGroup);
    toggle.addEventListener('click', (event) => { event.stopPropagation(); toggleGroup(); });
    tags.addEventListener('click', (event) => { event.stopPropagation(); openModTagEditor(group); });
    if (group.files.length > 1 && state.expandedModGroups.has(group.id)) updateGroupRows();
  }
  const restoreView = globalThis.LauncherListView?.capture(body,JSON.stringify([catalog.root,query,sort]));
  body.replaceChildren(...staging.children);
  if (!matching.length) body.innerHTML = `<div class="empty">${catalog.files.length ? 'No mod files match that search.' : 'The selected folder is empty.'}</div>`;
  restoreView?.();
  $('modResultSummary').textContent = matching.length > shown.length
    ? `${matching.length.toLocaleString()} matches · showing the first ${shown.length.toLocaleString()}`
    : `${matching.length.toLocaleString()} matching mod entries · ${catalog.files.length.toLocaleString()} files total`;
  $('modShownSummary').textContent = `${shown.length.toLocaleString()} of ${matching.length.toLocaleString()} mod entries listed`;
}

function closeModTagEditor() {
  state.editingModGroup = null;
  $('modTagModal').classList.add('hidden');
}

function openModTagEditor(group) {
  state.editingModGroup = group;
  $('modTagTitle').textContent = group.stem || group.files[0].name;
  $('modTagIdentity').textContent = group.title;
  $('modTagInput').value = group.tags.join(', ');
  $('modTagBundleInfo').textContent = group.files.length > 1
    ? `${group.files.length} files are bundled under this mod entry. The tags apply to the whole bundle.`
    : 'This mod entry contains one file.';
  $('modTagModal').classList.remove('hidden');
  $('modTagInput').focus();
  $('modTagInput').select();
}

async function saveModTagEdit() {
  const group = state.editingModGroup;
  if (!group) return;
  const button = $('saveModTagEdit');
  button.disabled = true;
  button.textContent = 'Saving…';
  try {
    renderCatalog(await window.modLauncher.saveModTags({ fileId: group.tagFileId, tags: $('modTagInput').value }));
    closeModTagEditor();
    toast('Mod tags saved.');
  } catch (error) { toast(error.message, true); }
  finally { button.disabled = false; button.textContent = 'Save tags'; }
}

function renderPlayerRows() {
  const allPlayers = state.playerResearch?.players || [];
  const sourcePlayers = allPlayers.filter((player) => player.source === 'live-roster');
  const teamFilter = $('playerTeamFilter').value;
  const players = teamFilter === '__unassigned'
    ? sourcePlayers.filter((player) => !player.teamName)
    : teamFilter ? sourcePlayers.filter((player) => player.teamName === teamFilter) : sourcePlayers;
  const query = $('playerSearch').value.trim().toLowerCase();
  const matching = query ? players.filter((player) => {
    const cyberfaceId = Number.isInteger(player.cyberfaceId) ? `${player.cyberfaceId} ${String(player.cyberfaceId).padStart(5, '0')}` : '';
    return `${player.name} ${player.firstName || ''} ${player.lastName || ''} ${player.recordIndex} ${cyberfaceId} ${player.teamName || ''}`.toLowerCase().includes(query);
  }) : players;
  const shown = matching.slice(0, 750);
  const body = $('playerRows');
  const restoreView = globalThis.LauncherListView?.capture(body,JSON.stringify([query,teamFilter]));
  body.replaceChildren();
  for (const player of shown) {
    const row = document.createElement('div');
    row.className = 'player-row';
    row.setAttribute('data-view-key', `${player.source}:${player.recordIndex}`);
    row.dataset.playerIndex = String(allPlayers.indexOf(player));
    row.classList.toggle('editable', player.source === 'live-roster');
    const record = document.createElement('span');
    record.className = 'mono';
    record.textContent = Number.isInteger(player.recordIndex)
      ? `#${player.recordIndex.toLocaleString()}`
      : player.source === 'mods-only' ? 'MODS' : 'MANIFEST';
    if (player.source === 'cataloged-roster') record.textContent = 'CATALOG';
    record.title = player.source || '';
    const name = document.createElement('span');
    name.className = 'player-name-cell';
    const primaryName = document.createElement('strong');
    primaryName.textContent = player.name;
    const vitals = document.createElement('small');
    if (['live-roster', 'cataloged-roster'].includes(player.source)) {
      const position = [player.position, player.secondaryPosition].filter((value, index, values) => value && value !== 'N/A' && values.indexOf(value) === index).join('/');
      const feet = Number.isFinite(player.heightCm) ? Math.floor(player.heightCm / 30.48) : null;
      const inches = Number.isFinite(player.heightCm) ? Math.round((player.heightCm / 2.54) - feet * 12) : null;
      const height = feet !== null ? `${feet}′${inches === 12 ? 0 : inches}″` : '';
      const details = [position, player.jerseyNumber ? `#${player.jerseyNumber}` : '', height,
        Number.isFinite(player.weightLbs) ? `${Math.round(player.weightLbs)} lb` : '',
        Number.isInteger(player.overall) ? `OVR ${player.overall}` : '',
        Number.isInteger(player.potential) ? `POT ${player.potential}` : ''].filter(Boolean);
      vitals.textContent = [player.source === 'cataloged-roster' && player.rosterGroup ? `${player.rosterGroup.toUpperCase()} catalog` : '', ...details].filter(Boolean).join(' · ');
    } else {
      vitals.textContent = player.nameStatus === 'reference-name' ? 'Reference name · asset not in live roster' : 'Asset not in live roster';
    }
    name.append(primaryName, vitals);
    name.title = player.nameStatus === 'live-exact'
      ? 'Exact name read from the live roster'
      : player.nameStatus === 'cataloged-exact' ? 'Exact name retained from an earlier read-only NBA/WNBA roster scan'
      : player.nameStatus === 'reference-name' ? 'Name identified from the offline reference list' : 'No verified name source is available';
    const ids = document.createElement('span');
    ids.className = 'mono live-id';
    ids.textContent = Number.isInteger(player.cyberfaceId) ? String(player.cyberfaceId).padStart(5, '0') : '—';
    ids.title = Number.isInteger(player.cyberfaceId) ? 'Read directly from live record offset +0x110' : 'unavailable';
    const team = document.createElement('span');
    team.textContent = player.teamName || '—';
    const address = document.createElement('span');
    address.className = 'mono address';
    address.textContent = player.recordAddress || (player.source === 'cataloged-roster' ? 'Retained scan' : player.modPresent ? 'Mods asset' : 'Stock asset');
    row.append(record, name, ids, team, address);
    body.append(row);
  }
  if (!matching.length) body.innerHTML = '<div class="empty">No players match that search.</div>';
  restoreView?.();
  const shownSummary = matching.length > shown.length
    ? `${matching.length.toLocaleString()} matches · showing the first ${shown.length.toLocaleString()}`
    : `${matching.length.toLocaleString()} matching live players`;
  $('playerResultSummary').textContent = shownSummary;
}

function renderPlayerTeamFilter() {
  const select = $('playerTeamFilter');
  const selected = select.value;
  const livePlayers = (state.playerResearch?.players || []).filter((player) => player.source === 'live-roster');
  const names = [...new Set(livePlayers.map((player) => player.teamName).filter(Boolean))]
    .sort((left, right) => left.localeCompare(right));
  const options = [{ value: '', label: 'All teams' }, ...names.map((name) => ({ value: name, label: name }))];
  if (livePlayers.some((player) => !player.teamName)) options.push({ value: '__unassigned', label: 'Unassigned / no verified team' });
  select.replaceChildren(...options.map(({ value, label }) => {
    const option = document.createElement('option');
    option.value = value;
    option.textContent = label;
    return option;
  }));
  select.value = options.some((option) => option.value === selected) ? selected : '';
}

const PLAYER_POSITIONS = ['PG', 'SG', 'SF', 'PF', 'C', 'N/A'];

function fillPositionSelect(select, selected) {
  select.replaceChildren(...PLAYER_POSITIONS.map((name, id) => {
    const option = document.createElement('option');
    option.value = String(id);
    option.textContent = name;
    option.selected = id === selected;
    return option;
  }));
}

function closePlayerEditor(force = false) {
  if(force!==true&&window.canLeavePlayerEditor&&!window.canLeavePlayerEditor())return;
  state.editingPlayer = null;
  $('playerEditorModal').classList.add('hidden');
  window.dispatchEvent(new Event('player-editor:closed'));
}

function openPlayerEditor(player) {
  if(state.editingPlayer&&window.canLeavePlayerEditor&&!window.canLeavePlayerEditor())return;
  state.editingPlayer = player;
  const editable = player?.source === 'live-roster';
  $('playerEditTitle').textContent = player?.name || 'Player';
  $('playerEditIdentity').textContent = Number.isInteger(player?.recordIndex)
    ? `Record #${player.recordIndex} · ${player.recordAddress || 'live address unavailable'}`
    : `${String(player?.source || 'catalog').toUpperCase()} · no live record`;
  $('playerEditFirstName').value = player?.firstName || '';
  $('playerEditLastName').value = player?.lastName || '';
  $('playerEditCyberface').value = Number.isInteger(player?.cyberfaceId) ? String(player.cyberfaceId) : '';
  $('playerEditJersey').value = player?.jerseyNumber ?? '';
  fillPositionSelect($('playerEditPosition'), Number.isInteger(player?.positionId) ? player.positionId : 5);
  fillPositionSelect($('playerEditSecondary'), Number.isInteger(player?.secondaryPositionId) ? player.secondaryPositionId : 5);
  const fromSelect=$('playerEditFrom');fromSelect.replaceChildren();
  const keepFrom=document.createElement('option');keepFrom.value='';keepFrom.textContent=player?.collegeName||'Unassigned / unavailable';fromSelect.append(keepFrom);
  for(const origin of [...(state.playerResearch?.origins||[])].sort((a,b)=>a.name.localeCompare(b.name))){const option=document.createElement('option');option.value=String(origin.index);option.textContent=origin.name;fromSelect.append(option);}
  fromSelect.value=Number.isInteger(player?.fromIndex)&&player.fromIndex>=0?String(player.fromIndex):'';
  if(fromSelect.selectedIndex<0)fromSelect.value='';
  window.playerMeasurements.writeHeight($('playerEditHeight'), $('playerEditHeightInches'), player?.heightCm);
  window.playerMeasurements.writeWingspan($('playerEditWingspan'), player?.wingspanCm);
  $('playerEditWeight').value = Number.isFinite(player?.weightLbs) ? String(player.weightLbs) : '';
  $('playerEditPotential').value = Number.isInteger(player?.potential) ? String(player.potential) : '';
  $('playerEditBirthDate').value=player?.bioLayoutVerified===true&&player.birthDate?player.birthDate:'';
  $('playerEditYearsPro').value=player?.bioLayoutVerified===true&&Number.isInteger(player.yearsPro)?String(player.yearsPro):'';
  $('playerEditOverall').value = Number.isInteger(player?.overall) ? String(player.overall) : 'Not mapped';
  $('playerEditTeam').value = player?.teamName || 'Unassigned / unavailable';
  const heroTeam=(state.playerResearch?.teams||[]).find(t=>t.recordIndex===player?.teamIndex);
  const audio = editable ? player?.audio : null;
  const audioId = (value) => !Number.isInteger(value) ? 'Unavailable'
    : value === -1 ? 'Not assigned' : String(value);
  $('playerAudioFullName').value = Number.isInteger(audio?.fullNameId) ? String(audio.fullNameId) : '';
  $('playerAudioFirstCommentary').value = audioId(audio?.firstName?.commentaryId);
  $('playerAudioFirstPa').value = audioId(audio?.firstName?.paId);
  $('playerAudioLastCommentary').value = audioId(audio?.lastName?.commentaryId);
  $('playerAudioLastPa').value = audioId(audio?.lastName?.paId);
  $('playerAudioOverride').value = !audio ? 'Unavailable'
    : audio.nameOverrideActive ? 'Active — surname mapping unresolved' : 'None';
  $('playerEditSource').value = player?.source === 'live-roster' ? 'Current live roster'
    : player?.source === 'cataloged-roster' ? 'Retained roster catalog'
    : player?.source === 'mods-only' ? 'Mods-only face asset' : 'Stock manifest face asset';
  $('playerEditRecord').value = Number.isInteger(player?.recordIndex)
    ? `#${player.recordIndex} · ${player.recordAddress || 'not live'}` : 'No roster record';
  $('playerEditTeamRecord').value = Number.isInteger(player?.teamIndex)
    ? `#${player.teamIndex} · ${player.teamAddress || 'address unavailable'}` : 'Unlinked / unavailable';
  $('playerEditAssets').value = [player?.manifestListed ? 'Stock manifest' : '', player?.modPresent ? 'Mods file present' : ''].filter(Boolean).join(' + ') || 'No local asset match';
  $('playerEditBodyRaw').value = Number.isInteger(player?.heightRaw) && Number.isInteger(player?.wingspanRaw)
    ? `${player.heightRaw} / ${player.wingspanRaw}` : 'Unavailable';
  $('playerEditPotentialRaw').value = Number.isInteger(player?.potentialRaw) ? String(player.potentialRaw) : 'Unavailable';
  $('playerEditReferences').value = Array.isArray(player?.referenceFaceIds) && player.referenceFaceIds.length
    ? player.referenceFaceIds.map((id) => String(id).padStart(5, '0')).join(', ') : 'No offline reference match';
  document.querySelectorAll('#playerEditorModal input:not([readonly]), #playerEditorModal select').forEach((control) => { control.disabled = !editable; });
  $('playerAudioFullName').disabled = !editable || !Number.isInteger(audio?.fullNameId);
  fromSelect.disabled=!editable||!Number.isInteger(player?.fromIndex)||!state.playerResearch?.origins?.length;
  for(const id of ['playerEditBirthDate','playerEditYearsPro'])$(id).disabled=!editable||player?.bioLayoutVerified!==true;
  $('savePlayerEdit').disabled = !editable;
  $('playerEditBoundary').textContent = editable
    ? 'Edits are read back and verified, with rollback on failure. Only the player-specific full-name audio ID is editable; shared name mappings remain read-only. Save the roster in-game to retain changes.'
    : 'This is not a live roster record. Its known data is shown for reference, but editing is disabled until that exact roster is loaded and refreshed.';
  window.dispatchEvent(new CustomEvent('player-editor:open', { detail: {...player,teamCity:heroTeam?.cityName} }));
  $('playerEditorModal').classList.remove('hidden');
}

function playerEditPayload() {
  const player = state.editingPlayer;
  const bio={};
  const birthInput=$('playerEditBirthDate'),yearsInput=$('playerEditYearsPro');
  if(!birthInput.disabled){if(!birthInput.value&&player.birthDate)throw Error('Birthday cannot be cleared. Enter a complete date or reset the field.');if(birthInput.value&&birthInput.value!==player.birthDate){window.playerBio.dateParts(birthInput.value);Object.assign(bio,{birthDate:birthInput.value,expectedBirthYear:player.birthYear,expectedBirthMonth:player.birthMonth,expectedBirthDay:player.birthDay});}}
  if(!yearsInput.disabled){if(!yearsInput.value.trim())throw Error('Enter years pro, including 0 for a rookie.');const value=Number(yearsInput.value);window.playerBio.years(value);if(value!==player.yearsPro)Object.assign(bio,{yearsPro:value,expectedYearsPro:player.yearsPro});}
  const jerseyText = $('playerEditJersey').value.trim();
  const jerseyNumberRaw = jerseyText === '00' ? 100 : Number.parseInt(jerseyText, 10);
  return {
    recordIndex: player.recordIndex,
    ...bio,
    expectedFirstName: player.firstName,
    expectedLastName: player.lastName,
    expectedCyberfaceId: player.cyberfaceId,
    ratingEdits: window.playerRatingChanges?.() || [],
    ...(!$('playerAudioFullName').disabled ? { fullNameId: $('playerAudioFullName').value } : {}),
    ...(!$('playerEditFrom').disabled&&$('playerEditFrom').value!==''?{fromIndex:Number($('playerEditFrom').value)}:{}),
    firstName: $('playerEditFirstName').value,
    lastName: $('playerEditLastName').value,
    cyberfaceId: Number.parseInt($('playerEditCyberface').value, 10),
    positionId: Number.parseInt($('playerEditPosition').value, 10),
    secondaryPositionId: Number.parseInt($('playerEditSecondary').value, 10),
    jerseyNumberRaw,
    weightLbs: Number($('playerEditWeight').value),
    heightCm: window.playerMeasurements.readHeight($('playerEditHeight'), $('playerEditHeightInches')),
    wingspanCm: window.playerMeasurements.readWingspan($('playerEditWingspan')),
    potential: Number.parseInt($('playerEditPotential').value, 10),
  };
}

window.applyTwokdbRatings=async(data)=>{
 const player=state.editingPlayer;
 if(!player||$('playerEditorModal').classList.contains('hidden'))throw Error('Open a live player before importing.');
 if(player.editorNeedsRefresh)throw Error('Refresh live players and reopen this player before importing again.');
 const prepared=window.twokdbApply.prepare(player,data,window.playerRatingSchema,state.playerResearch?.origins||[]);
 if(window.ImportReview&&!await window.ImportReview.confirm({title:'Import ratings',rows:window.ImportReview.playerRows({info:prepared.importedInfo,fields:prepared.fields},player,window.playerRatingSchema),action:'Apply & verify',note:'These selected fields will be written and read back for verification. Other fields stay unchanged. '+prepared.warnings.join(' '),valid:()=>state.editingPlayer===player&&!$('playerEditorModal').classList.contains('hidden')}))return {canceled:true};
 if(state.editingPlayer!==player||$('playerEditorModal').classList.contains('hidden'))throw Error('The destination player changed. Import again.');
 const result=await window.modLauncher.savePlayer(prepared.payload);
 renderPlayerResearch(result.playerResearch);
 if(state.editingPlayer===player&&!$('playerEditorModal').classList.contains('hidden')){
  const fresh=result.playerResearch?.players?.find(p=>p.source==='live-roster'&&p.recordIndex===player.recordIndex&&p.firstName===prepared.payload.firstName&&p.lastName===prepared.payload.lastName&&p.cyberfaceId===player.cyberfaceId);
  if(!fresh){$('savePlayerEdit').disabled=true;player.editorNeedsRefresh=true;throw Error('The import was saved, but the updated player could not be reloaded. Refresh live players and reopen the player before another edit.');}
  Object.assign(player,fresh);delete player.editorNeedsRefresh;
  window.acceptImportedPlayerRatings(prepared.fields);
  for(const [key,id] of Object.entries({firstName:'playerEditFirstName',lastName:'playerEditLastName',positionId:'playerEditPosition',secondaryPositionId:'playerEditSecondary'}))if(Object.hasOwn(prepared.importedInfo,key))$(id).value=String(prepared.importedInfo[key]);
  if(Object.hasOwn(prepared.importedInfo,'heightCm'))window.playerMeasurements.writeHeight($('playerEditHeight'),$('playerEditHeightInches'),prepared.importedInfo.heightCm);
  if(Object.hasOwn(prepared.importedInfo,'wingspanCm'))window.playerMeasurements.writeWingspan($('playerEditWingspan'),prepared.importedInfo.wingspanCm);
  if(Object.hasOwn(prepared.importedInfo,'weightLbs'))$('playerEditWeight').value=String(prepared.importedInfo.weightLbs);
  if(Object.hasOwn(prepared.importedInfo,'birthDate'))$('playerEditBirthDate').value=prepared.importedInfo.birthDate;
  if(Object.hasOwn(prepared.importedInfo,'yearsPro'))$('playerEditYearsPro').value=String(prepared.importedInfo.yearsPro);
  if(Object.hasOwn(prepared.payload,'fromIndex'))$('playerEditFrom').value=String(prepared.payload.fromIndex);
  if(fresh&&$('playerHeroFrom'))$('playerHeroFrom').textContent=fresh.collegeName||'—';
  if(data.info?.potential!==undefined)$('playerEditPotential').value=String(prepared.payload.potential);
  window.dispatchEvent(new CustomEvent('player-editor:committed',{detail:{player:{...fresh},infoKeys:Object.keys(prepared.importedInfo),ratingKeys:Object.keys(prepared.fields)}}));
  window.dispatchEvent(new Event('player-editor:staged'));
 }
 window.dispatchEvent(new Event('player-editor:import-verified'));
 return {count:prepared.count,warnings:prepared.warnings};
};

async function savePlayerEdit() {
  if (state.editingPlayer?.source !== 'live-roster') return;
  const status=$('playerEditBoundary');
  const feedback=(message,error=false)=>{status.textContent=message;status.setAttribute('role',error?'alert':'status');status.classList.toggle('save-error',error);toast(message,error);};
  if(state.editingPlayer.editorNeedsRefresh){feedback('Refresh live players and reopen this player before saving again.',true);return;}
  if(window.isPlayerDnaBusy?.()){feedback('Wait for the Player DNA operation to finish.',true);return;}
  if(window.hasPendingSeasonImport?.()){feedback('Apply selected seasons or discard the season import before saving player values.',true);return;}
  if(window.hasUnsavedSeasonChanges?.()){feedback('Save or reset your edited season rows first using Save season.',true);return;}
  const button = $('savePlayerEdit');
  button.disabled = true;
  button.textContent = 'Saving…';
  window.dispatchEvent(new Event('player-editor:preview-invalidated'));
  status.textContent='Saving player changes and verifying them…';status.classList.remove('save-error');
  try {
    const result = await window.modLauncher.savePlayer(playerEditPayload());
    renderPlayerResearch(result.playerResearch);
    closePlayerEditor(true);
    toast('Player saved and verified. Save the roster in-game to keep the changes.');
  } catch (error) { feedback(error?.message||'Could not save this player. Your edits are still here.',true); }
  finally { button.disabled = false; button.textContent = 'Save & verify'; }
}

function renderTeamRows() {
  const teams = state.playerResearch?.teams || [];
  const query = $('teamSearch').value.trim().toLowerCase();
  const matching = query ? teams.filter((team) => `${team.recordIndex} ${team.name} ${team.cityName || ''} ${team.cityShort || ''} ${team.teamId} ${team.rosterType || ''} ${team.historicYear || ''} ${team.arenaId ?? ''} ${team.arenaName || ''} ${team.arenaFile || ''} ${team.mainLogo || ''}`.toLowerCase().includes(query)) : teams;
  const shown = matching.slice(0, 500);
  const body = $('teamRows');
  const restoreView = globalThis.LauncherListView?.capture(body,query);
  body.replaceChildren();
  for (const team of shown) {
    const row = document.createElement('div');
    row.className = `team-row${team.source === 'live-team' ? ' editable' : ''}`;
    row.dataset.recordIndex = String(team.recordIndex);
    row.setAttribute('data-view-key', String(team.recordIndex));
    row.dataset.teamId = String(team.teamId);
    row.title = team.source === 'live-team' ? 'Click to view and edit this live team' : 'Load this team’s roster and refresh to edit it';
    const record = document.createElement('span');
    record.className = 'mono';
    record.textContent = `#${team.recordIndex}`;
    const identity = document.createElement('span');
    const name = document.createElement('strong');
    name.textContent = team.name;
    const city = document.createElement('small');
    city.textContent = [team.cityName, team.cityShort].filter(Boolean).join(' · ');
    identity.append(name, city);
    const id = document.createElement('span');
    id.className = 'mono live-id';
    id.textContent = String(team.teamId);
    const type = document.createElement('span');
    const typeName = document.createElement('strong');
    typeName.textContent = team.rosterType || 'Unknown';
    const season = document.createElement('small');
    season.textContent = team.historicYear || '—';
    type.append(typeName, season);
    const arena = document.createElement('span');
    const arenaName = document.createElement('strong');
    arenaName.textContent = team.arenaName || '—';
    const arenaFile = document.createElement('small');
    arenaFile.textContent = Number.isInteger(team.arenaId) ? `ID ${team.arenaId} · ${team.arenaFile || 'file unknown'}` : 'No validated link';
    arena.append(arenaName, arenaFile);
    const logo = document.createElement('span');
    logo.className = 'mono address';
    logo.textContent = team.mainLogo || '—';
    row.append(record, identity, id, type, arena, logo);
    body.append(row);
  }
  if (!matching.length) body.innerHTML = '<div class="empty">No teams match that search.</div>';
  restoreView?.();
  $('teamResultSummary').textContent = matching.length > shown.length
    ? `${matching.length.toLocaleString()} matches · showing the first ${shown.length.toLocaleString()}`
    : `${matching.length.toLocaleString()} matching team records`;
}

const rosterTypes = [
  'Normal', 'Historic', 'User', 'Online', 'All-Time', 'Classic', 'WNBA', 'G League',
  'International', 'College', 'High School', 'Special', 'MyTEAM', 'Blacktop', 'Practice', 'Unused',
];

function closeTeamEditor() {
  state.editingTeam = null;
  teamStaffView?.reset(null);
  teamRotationView?.reset(null);
  $('teamEditorModal').classList.add('hidden');
}

let teamStaffView,teamRotationView,teamUniformView,teamUniformBusy=false;
let teamReviewBaseline=new Map();
function reviewTeamChanges(){
 const team=state.editingTeam;if(!team)return;
 window.EditorReview?.open({title:'Review team changes',rows:[...teamReviewBaseline].filter(([id,v])=>$(id).value!==v).map(([id,v])=>({label:$(id).closest('label')?.firstChild?.textContent||id,before:v,after:$(id).value,remove:()=>{if(state.editingTeam!==team||$('saveTeamEdit').disabled)return false;$(id).value=v;$(id).dispatchEvent(new Event('change'));return true;}}))});
}
function selectTeamEditorTab(name = 'info') {
  const target = ['info', 'uniforms', 'arena', 'staff', 'rotations', 'dna'].includes(name) ? name : 'info';
  document.querySelectorAll('[data-team-tab]').forEach((tab) => {
    const active = tab.dataset.teamTab === target;
    tab.setAttribute('aria-selected', String(active));
    tab.tabIndex = active ? 0 : -1;
    const panel = $(tab.getAttribute('aria-controls'));
    if (panel) panel.hidden = !active;
  });
  $('saveTeamEdit').hidden = ['uniforms','staff','rotations','dna'].includes(target);
  $('teamEditBoundary').hidden = ['staff','rotations','dna'].includes(target);
  if (target === 'staff') void teamStaffView?.load();
  if (target === 'rotations') void teamRotationView?.load();
}

function availableUniformsForTeam(team) {
  return (state.playerResearch?.unreferencedPopulatedUniforms || []).filter((uniform) => uniform.teamId === team.teamId
    && uniform.stockResourceAvailable === true);
}

let teamDnaBusy=false;
const teamDnaInfoInputs={name:'teamEditName',cityName:'teamEditCity',cityShort:'teamEditCityShort',stateShort:'teamEditStateShort',mainLogo:'teamEditLogo',rosterTypeId:'teamEditRosterType',historicYearId:'teamEditHistoricYear'};
async function transferTeamDna(action){
 const team=state.editingTeam;if(!team||team.source!=='live-team'||teamDnaBusy||teamUniformBusy)return;
 const sections=[...$('teamDnaSections').querySelectorAll('input:checked')].map(i=>i.value),status=$('teamDnaStatus');
 if(!sections.length){status.textContent='Choose at least one Team DNA section.';return;}
 teamDnaBusy=true;$('teamDnaImport').disabled=$('teamDnaExport').disabled=true;status.textContent='';
 try{
  let data;
  if(action==='export'){
   if(sections.includes('staff'))await teamStaffView.load();if(state.editingTeam!==team)return;
   const info=Object.fromEntries(Object.entries(teamDnaInfoInputs).map(([key,id])=>[key,['rosterTypeId','historicYearId'].includes(key)?Number($(id).value):$(id).value]));
   const arena=(state.playerResearch?.arenas||[]).find(a=>a.arenaId===Number($('teamEditArenaId').value));
   const staff=sections.includes('staff')?teamStaffView.snapshot():[];
   const rotations=sections.includes('rotations')?await teamRotationView.portable():null;if(state.editingTeam!==team)return;
   const uniforms=sections.includes('uniforms')?(teamUniformView?.snapshot()||[]):[];
   data={format:'2kteam',version:1,game:'NBA2K27',name:info.name,info,arena:arena?{name:arena.name,arenaFile:arena.arenaFile,floorFile:$('teamEditFloor')?.value||arena.floorFile}:null,staff,rotations,uniforms};
  }
  const result=await window.modLauncher.teamDna({action,sections,data});if(!result||state.editingTeam!==team)return;
  if(action==='export'){status.textContent='Team DNA exported. Team ID, staff IDs and assignments are not included.';return;}
  if(window.ImportReview){
   if(result.staff.length)await teamStaffView.load();
   const bySlot=rows=>Object.fromEntries(rows.map(row=>[row.slot,row.values]));
   const byUniform=rows=>Object.fromEntries(rows.map(row=>[JSON.stringify(row.match),row.values]));
   const arena=(state.playerResearch?.arenas||[]).find(a=>a.arenaId===Number($('teamEditArenaId').value));
   const before={info:Object.fromEntries(Object.entries(teamDnaInfoInputs).map(([key,id])=>[key,['rosterTypeId','historicYearId'].includes(key)?Number($(id).value):$(id).value])),arena:arena?{name:arena.name,arenaFile:arena.arenaFile,floorFile:$('teamEditFloor').value}:null,staff:result.staff.length?bySlot(teamStaffView.snapshot()):{},uniforms:byUniform(teamUniformView?.snapshot()||[]),rotations:result.rotations?await teamRotationView.portable():null};
   if(state.editingTeam!==team)return;
   const after={info:result.info,...(result.arena?{arena:result.arena}:{}),staff:bySlot(result.staff),uniforms:byUniform(result.uniforms||[]),...(result.rotations?{rotations:result.rotations}:{})};
   if(!await window.ImportReview.confirm({title:'Import Team DNA · '+result.name,rows:window.ImportReview.diff(before,after),note:'Destination Team ID stays unchanged. This stages changes only; save each affected tab to apply. Unresolved arena, staff or uniform matches remain unchanged.',valid:()=>state.editingTeam===team})){status.textContent='Import canceled. Existing edits are unchanged.';return;}
  }
  const warnings=[];
  if(result.uniforms?.length)warnings.push(...(teamUniformView?.stage(result.uniforms)||['No editable uniform slots are available.']));
  if(result.rotations)try{await teamRotationView.stage(result.rotations);}catch(e){warnings.push(e.message);}if(state.editingTeam!==team)return;
  if(result.staff.length){const missing=await teamStaffView.stage(result.staff);if(state.editingTeam!==team)return;if(missing.length)warnings.push(`${missing.length} unassigned staff roles were left unchanged.`);}
  for(const [key,v] of Object.entries(result.info))if(teamDnaInfoInputs[key])$(teamDnaInfoInputs[key]).value=String(v);
  if(result.arena){const matches=(state.playerResearch?.arenas||[]).filter(a=>a.arenaFile===result.arena.arenaFile);const chosen=matches.find(a=>a.arenaId===team.arenaId)||matches.find(a=>a.name===result.arena.name)||matches[0];
   if(chosen&&[...$('teamEditFloor').options].some(o=>o.value===result.arena.floorFile)){$('teamEditArenaId').value=String(chosen.arenaId);$('teamEditArenaModel').value=String(chosen.arenaId);$('teamEditFloor').value=result.arena.floorFile;$('teamEditArenaModel').dispatchEvent(new Event('change'));$('teamEditFloor').dispatchEvent(new Event('change'));}
   else warnings.push('Arena/floor references could not be resolved; left unchanged.');
  }
  status.textContent=`Staged Team DNA from ${result.name}. Destination Team ID ${team.teamId} is unchanged. Review Team info and Arena, then Save & verify; save each imported staff member and uniform in its own tab. ${warnings.join(' ')}`;
 }catch(e){if(state.editingTeam===team)status.textContent=e.message;}
 finally{teamDnaBusy=false;$('teamDnaImport').disabled=$('teamDnaExport').disabled=false;}
}

function renderTeamUniformPanels(team) {
  teamUniformView=window.TeamUniforms?.render($('teamUniformList'), team, $('teamUniformSummary'), { onSave: saveTeamUniform });
  window.TeamUniforms?.renderAvailable(
    $('teamAvailableUniformList'), team, availableUniformsForTeam(team), $('teamAvailableUniformSummary'), { onLink: linkTeamUniform },
  );
}

function openTeamEditor(team) {
  window.EditorReview?.close();
  state.editingTeam = team;
  teamStaffView ||= window.TeamStaffView?.create($('teamStaffPanel'), request => window.modLauncher.teamStaff(request), request => window.modLauncher.saveTeamStaff(request));
  teamStaffView?.reset(team);
  teamRotationView ||= window.TeamRotationView?.create($('teamRotationsPanel'),request=>window.modLauncher.teamRotations(request));
  teamRotationView?.reset(team);
  $('teamDnaStatus').textContent='';
  $('teamEditTitle').textContent = team.name || `Team ${team.teamId}`;
  $('teamEditIdentity').textContent = `Team ID ${team.teamId} · record #${team.recordIndex} · ${team.source === 'live-team' ? 'LIVE' : 'CATALOG'}`;
  $('teamEditName').value = team.name || '';
  $('teamEditCity').value = team.cityName || '';
  $('teamEditCityShort').value = team.cityShort || '';
  $('teamEditStateShort').value = team.stateShort || '';
  $('teamEditLogo').value = team.mainLogo || '';
  $('teamEditRosterType').replaceChildren(...Array.from({ length: 32 }, (_unused, index) => {
    const option = document.createElement('option');
    option.value = String(index);
    option.textContent = `${index} — ${rosterTypes[index] || 'Unknown / reserved'}`;
    return option;
  }));
  $('teamEditRosterType').value = String(team.rosterTypeId ?? 0);
  $('teamEditHistoricYear').value = String(team.historicYearId ?? 0);
  $('teamEditArenaId').value = Number.isInteger(team.arenaId) ? String(team.arenaId) : '';
  $('teamEditArenaHelp').textContent = [team.arenaName, team.arenaFile].filter(Boolean).join(' · ') || 'No validated arena link';
  const arenas = state.playerResearch?.arenas || [];
  const linkedArena = arenas.find((arena) => arena.arenaId === team.arenaId);
  $('teamEditVerifiedId').value = String(team.teamId);
  $('teamEditVerifiedRecord').value = `#${team.recordIndex}`;
  $('teamEditSource').value = team.source === 'live-team' ? 'Current live roster' : 'Retained team catalog';
  $('teamEditAddress').value = team.recordAddress || 'Not live';
  $('teamEditArenaName').value = team.arenaName || linkedArena?.name || 'No validated arena link';
  $('teamEditArenaAssets').value = team.arenaFile || linkedArena?.arenaFile || 'Unavailable';
  $('teamEditFloorAsset').value = linkedArena?.floorFile || 'Unavailable';
  const geometry=arenas.filter(a=>!a.emptyCandidate&&a.arenaFile&&a.floorFile);
  $('teamEditArenaModel').replaceChildren(...geometry.map(a=>{const o=document.createElement('option');o.value=String(a.arenaId);o.textContent=`${a.name} · ID ${a.arenaId}`;return o;}));
  $('teamEditArenaModel').value=String(team.arenaId??'');
  $('teamEditFloor').replaceChildren(...[...new Map(geometry.map(a=>[a.floorFile,a])).values()].map(a=>{const o=document.createElement('option');o.value=a.floorFile;o.textContent=`${a.name} · ${a.floorFile}`;return o;}));
  $('teamEditFloor').value=linkedArena?.floorFile||'';
  $('teamEditArenaModel').disabled=$('teamEditFloor').disabled=team.source!=='live-team';
  renderTeamUniformPanels(team);
  const donors = arenas.filter((arena) => !arena.emptyCandidate && arena.name && arena.arenaFile)
    .sort((left, right) => left.arenaId - right.arenaId);
  $('customArenaDonor').replaceChildren(...donors.map((arena) => {
    const option = document.createElement('option');
    option.value = String(arena.arenaId);
    option.textContent = `${arena.arenaId} — ${arena.name}`;
    return option;
  }));
  if (donors.some((arena) => arena.arenaId === team.arenaId)) $('customArenaDonor').value = String(team.arenaId);
  const reserved = arenas.filter((arena) => arena.arenaId >= 996 && arena.arenaId <= 1045)
    .sort((left, right) => left.arenaId - right.arenaId);
  $('customArenaTarget').replaceChildren(...reserved.map((arena) => {
    const option = document.createElement('option');
    option.value = String(arena.arenaId);
    option.textContent = `${arena.arenaId} — ${arena.emptyCandidate ? 'Available' : arena.name}`;
    return option;
  }));
  const preferred = reserved.find((arena) => arena.arenaId === 999) || reserved.find((arena) => arena.emptyCandidate) || reserved[0];
  if (preferred) $('customArenaTarget').value = String(preferred.arenaId);
  loadCustomArenaFields();
  const live = team.source === 'live-team';
  $('saveTeamEdit').disabled = !live;
  $('createCustomArena').disabled = !live;
  $('teamEditBoundary').textContent = live
    ? 'Save rechecks this team ID and original name, verifies every field, and rolls back all applied fields if any check fails.'
    : 'This is a retained catalog row. Load the roster containing it and Refresh live teams before editing.';
  selectTeamEditorTab('info');
  teamReviewBaseline=new Map([...Object.values(teamDnaInfoInputs),'teamEditArenaModel','teamEditFloor'].map(id=>[id,$(id).value]));
  $('teamEditorModal').classList.remove('hidden');
}

function loadCustomArenaFields() {
  const targetId = Number($('customArenaTarget').value);
  const target = (state.playerResearch?.arenas || []).find((arena) => arena.arenaId === targetId);
  $('customArenaName').value = target && !target.emptyCandidate ? target.name : 'Arena Test';
  $('customArenaFile').value = target && !target.emptyCandidate ? target.arenaFile : `arena_${targetId}_int`;
  $('customArenaFloor').value = target && !target.emptyCandidate ? target.floorFile : `arena_${targetId}_int_floor`;
}

async function createCustomArena() {
  const team = state.editingTeam;
  if (!team) return;
  const targetId = Number($('customArenaTarget').value);
  const target = (state.playerResearch?.arenas || []).find((arena) => arena.arenaId === targetId);
  const button = $('createCustomArena');
  button.disabled = true;
  button.textContent = 'Cloning & verifying…';
  try {
    const result = await window.modLauncher.cloneArena({
      recordIndex: team.recordIndex,
      teamId: team.teamId,
      expectedTeamName: team.name,
      targetArenaId: targetId,
      donorArenaId: Number($('customArenaDonor').value),
      expectedTargetName: target?.name || '',
      arenaName: $('customArenaName').value,
      arenaFile: $('customArenaFile').value,
      floorFile: $('customArenaFloor').value,
    });
    renderPlayerResearch(result.playerResearch);
    closeTeamEditor();
    toast(`Custom arena ID ${result.report.targetArenaId} created, linked, and saved for automatic startup.`);
  } catch (error) {
    toast(error.message, true);
  } finally {
    button.disabled = false;
    button.textContent = 'Create, verify & link';
  }
}

async function saveTeamEdit() {
  const team = state.editingTeam;
  if (!team) return;
  const button = $('saveTeamEdit');
  button.disabled = true;
  button.textContent = 'Verifying & saving…';
  try {
    const result = await window.modLauncher.saveTeam({
      recordIndex: team.recordIndex,
      teamId: team.teamId,
      expectedName: team.name,
      name: $('teamEditName').value,
      cityName: $('teamEditCity').value,
      cityShort: $('teamEditCityShort').value,
      stateShort: $('teamEditStateShort').value,
      mainLogo: $('teamEditLogo').value,
      rosterTypeId: Number($('teamEditRosterType').value),
      historicYearId: Number($('teamEditHistoricYear').value),
      arenaId: Number($('teamEditArenaId').value),
      ...($('teamEditFloor').value?{floorFile:$('teamEditFloor').value}:{}),
    });
    renderPlayerResearch(result.playerResearch);
    closeTeamEditor();
    toast(`Saved and verified all ${result.report.writes} fields for team ID ${result.report.teamId}.`);
  } catch (error) {
    toast(error.message, true);
  } finally {
    button.disabled = false;
    button.textContent = 'Save & verify';
  }
}

async function saveTeamUniform(edit, button) {
  if (!edit || !button || teamUniformBusy) return;
  const owner=state.editingTeam,view=teamUniformView;teamUniformBusy=true;$('teamUniformList').inert=true;
  const originalText = button.textContent;
  button.disabled = true;
  button.textContent = 'Verifying & saving…';
  try {
    const result = await window.modLauncher.saveTeamUniform(edit);
    if(state.editingTeam!==owner)return;
    const drafts=view?.drafts(edit.slot)||[];
    renderPlayerResearch(result.playerResearch);
    const refreshed = (result.playerResearch?.teams || []).find((team) => team.source === 'live-team'
      && team.recordIndex === edit.recordIndex && team.teamId === edit.teamId);
    if (!refreshed) throw new Error('The uniform saved, but the refreshed team record was not returned. Refresh live teams.');
    state.editingTeam = refreshed;
    renderTeamUniformPanels(refreshed);
    teamUniformView?.restoreDrafts(drafts);
    toast(`Saved and verified uniform settings for ${edit.expectedBaseFilename} (slot ${edit.slot}). Save the roster in-game to retain it.`);
  } catch (error) {
    toast(error.message, true);
    button.disabled = false;
    button.textContent = originalText;
  } finally {teamUniformBusy=false;$('teamUniformList').inert=false;}
}

async function linkTeamUniform(edit, button) {
  if (!edit || !button) return;
  const originalText = button.textContent;
  button.disabled = true;
  button.textContent = 'Adding & verifying…';
  try {
    const result = await window.modLauncher.linkTeamUniform(edit);
    renderPlayerResearch(result.playerResearch);
    const refreshed = (result.playerResearch?.teams || []).find((team) => team.source === 'live-team'
      && team.recordIndex === edit.recordIndex && team.teamId === edit.teamId);
    if (!refreshed) throw new Error('The jersey was added, but the refreshed team record was not returned. Refresh live teams.');
    state.editingTeam = refreshed;
    renderTeamUniformPanels(refreshed);
    toast(`Added and verified ${edit.expectedBaseFilename} in slot ${edit.slot}. Save the roster in-game to retain it.`);
  } catch (error) {
    toast(error.message, true);
    button.disabled = false;
    button.textContent = originalText;
  }
}

function renderPlayerResearch(value) {
  state.playerResearch = value || { status: 'idle', players: [] };
  const status = state.playerResearch.status || 'idle';
  $('playerResearchState').textContent = status.toUpperCase();
  $('playerResearchMessage').textContent = state.playerResearch.message || 'No player research result is available.';
  $('playerResearchDot').className = `status-dot ${status === 'ready' ? 'ready' : status === 'error' ? 'error' : ''}`;
  $('playerCount').textContent = Number.isInteger(state.playerResearch.decodedPlayerCount)
    ? state.playerResearch.decodedPlayerCount.toLocaleString() : '—';
  $('playerCatalogCount').textContent = Number.isInteger(state.playerResearch.totalCatalogEntries)
    ? state.playerResearch.totalCatalogEntries.toLocaleString() : '—';
  $('playerManifestCount').textContent = Number.isInteger(state.playerResearch.manifestCyberfaceCount)
    ? state.playerResearch.manifestCyberfaceCount.toLocaleString() : '—';
  $('playerModsCount').textContent = Number.isInteger(state.playerResearch.modsCyberfaceCount)
    ? state.playerResearch.modsCyberfaceCount.toLocaleString() : '—';
  const teams = state.playerResearch.teams || [];
  $('teamNamedCount').textContent = teams.length ? teams.length.toLocaleString() : '—';
  $('teamNormalCount').textContent = teams.length ? teams.filter((team) => team.rosterTypeId === 0).length.toLocaleString() : '—';
  $('teamHistoricCount').textContent = teams.length ? teams.filter((team) => team.rosterTypeId === 1).length.toLocaleString() : '—';
  $('teamArenaCount').textContent = teams.length ? teams.filter((team) => Number.isInteger(team.arenaId)).length.toLocaleString() : '—';
  $('exportPlayers').disabled = status !== 'ready';
  renderPlayerTeamFilter();
  renderPlayerRows();
  renderTeamRows();
}

async function exportPlayerCatalog() {
  try {
    const result = await window.modLauncher.exportPlayerResearch();
    if (result) toast(`Exported ${result.count.toLocaleString()} player catalog entries.`);
  } catch (error) {
    toast(error.message, true);
  }
}

function liveRosterErrorMessage(error) {
  if (/NBA 2K27 is not running\.?/i.test(String(error?.message || error || ''))) {
    return 'Error: NBA 2K27 is not running.';
  }
  return 'Error: Could not refresh players and teams. Please try again.';
}

async function refreshPlayerResearch() {
  const buttons = [$('refreshPlayers'), $('refreshTeams')];
  buttons.forEach((button) => { button.disabled = true; button.textContent = 'Reading…'; });
  renderPlayerResearch({ ...(state.playerResearch || {}), status: 'scanning', message: 'Reading the live player and team directories…' });
  try {
    renderPlayerResearch(await window.modLauncher.scanPlayerResearch());
    toast('Live player directory refreshed without game-memory writes.');
  } catch (error) {
    const message = liveRosterErrorMessage(error);
    console.error('Live roster refresh failed:', error);
    renderPlayerResearch({ status: 'error', message, players: [] });
    toast(message, true);
  } finally {
    buttons.forEach((button) => { button.disabled = false; });
    $('refreshPlayers').textContent = 'Refresh live players';
    $('refreshTeams').textContent = 'Refresh live teams';
  }
}

async function loadJukeboxArtworkPreview(key) {
  let lastError = null;
  for (let attempt = 0; attempt < 3; attempt += 1) {
    try {
      const preview = await window.modLauncher.previewJukeboxArtwork(key);
      return URL.createObjectURL(new Blob([transferableBytes(preview.bytes)], { type: 'image/png' }));
    } catch (error) {
      lastError = error;
      if (attempt < 2) await new Promise((resolve) => setTimeout(resolve, 120 * (attempt + 1)));
    }
  }
  throw lastError;
}

function renderJukeboxRows() {
  if (!$('page-jukebox').classList.contains('active')) return;
  const tracks = state.jukebox?.tracks || [];
  const editablePlaylist = Boolean($('jukeboxPlaylistSelect').value);
  const playlistTracks = editablePlaylist
    ? tracks.filter((track) => state.jukeboxDraftKeys.has(track.key))
    : tracks.filter((track) => track.selected);
  const query = $('jukeboxSearch').value.trim().toLowerCase();
  const matching = query ? playlistTracks.filter((track) => (
    `${track.title} ${track.artist} ${track.coverName} ${track.collection} ${track.bankIndex} ${track.uniqueId}`
      .toLowerCase().includes(query)
  )) : playlistTracks;
  const rows = $('jukeboxRows');
  const restoreView = globalThis.LauncherListView?.capture(rows,JSON.stringify([query,$('jukeboxPlaylistSelect').value]));
  if (!matching.length) {
    const message = query && playlistTracks.length
      ? 'No songs in this playlist match the search.'
      : editablePlaylist ? 'This playlist is empty. Use + Add song to add its first track.'
        : (tracks.length ? 'No songs match this search.' : 'No jukebox tracks are available.');
    rows.innerHTML = `<div class="empty">${message}</div>`;
  } else {
    rows.replaceChildren(...matching.map((track) => {
      const row = document.createElement('div');
      row.className = 'jukebox-row';
      row.dataset.trackKey = track.key;
      row.setAttribute('data-view-key', track.key);
      row.innerHTML = `<input type="checkbox" data-jukebox-select aria-label="Include song">
        <div class="jukebox-art-placeholder" data-jukebox-art aria-label="No saved artwork preview">♫</div>
        <div><strong class="jukebox-title"></strong></div>
        <div><strong class="jukebox-artist"></strong></div>
        <div class="jukebox-row-actions"><button class="jukebox-play-button" data-jukebox-preview>Play</button><button class="jukebox-edit-button" data-jukebox-edit>Edit</button><button class="jukebox-remove-button" data-jukebox-remove>Remove</button></div>`;
      row.querySelector('[data-jukebox-select]').checked = state.jukeboxDraftKeys.has(track.key);
      row.querySelector('[data-jukebox-select]').disabled = !editablePlaylist;
      row.querySelector('.jukebox-title').textContent = track.title || '(untitled)';
      row.querySelector('.jukebox-artist').textContent = track.artist || '(no artist)';
      if (track.artworkPreviewAvailable) {
        const placeholder = row.querySelector('[data-jukebox-art]');
        const showArtwork = (url) => {
          if (!placeholder.parentNode) return;
          const artwork = document.createElement('img');
          artwork.className = 'jukebox-row-art';
          artwork.alt = `${track.title || 'Song'} artwork`;
          artwork.src = url;
          placeholder.replaceWith(artwork);
        };
        const cached = state.jukeboxArtworkPreviewUrls.get(track.key);
        if (typeof cached === 'string') showArtwork(cached);
        else {
          const pending = cached || loadJukeboxArtworkPreview(track.key).then((url) => {
            state.jukeboxArtworkPreviewUrls.set(track.key, url);
            return url;
          }).catch(() => {
            state.jukeboxArtworkPreviewUrls.delete(track.key);
            return null;
          });
          if (!cached) state.jukeboxArtworkPreviewUrls.set(track.key, pending);
          pending.then((url) => { if (url) showArtwork(url); });
        }
      }
      const previewButton = row.querySelector('[data-jukebox-preview]');
      const playing = state.playingJukeboxKey === track.key && !state.jukeboxAudio?.paused;
      previewButton.textContent = track.previewMissing ? 'Missing'
        : track.previewAvailable ? (playing ? 'Pause' : 'Play') : 'In game';
      previewButton.classList.toggle('playing', playing);
      previewButton.disabled = !track.previewAvailable;
      previewButton.title = track.previewMissing
        ? 'This custom song file is missing. Remove this row or add the song again.'
        : track.previewAvailable
        ? `${playing ? 'Pause' : 'Preview'} ${track.title || 'custom track'}`
        : 'Stock songs remain inside the game audio bank and can only be heard in NBA 2K27.';
      const removeButton = row.querySelector('[data-jukebox-remove]');
      removeButton.disabled = !editablePlaylist || !state.jukeboxDraftKeys.has(track.key);
      removeButton.title = editablePlaylist
        ? 'Remove this song from the selected playlist without deleting its audio file.'
        : 'The Default Playlist is locked.';
      return row;
    }));
  }
  restoreView?.();
  $('jukeboxResultSummary').textContent = editablePlaylist
    ? `${matching.length.toLocaleString()} of ${playlistTracks.length.toLocaleString()} playlist songs shown.`
    : `${matching.length.toLocaleString()} of ${playlistTracks.length.toLocaleString()} default songs shown · Default playlist locked.`;
}

function updateJukeboxSaveButton(playlist = (state.jukebox?.playlists || [])
  .find((item) => item.id === $('jukeboxPlaylistSelect').value)) {
  const dirty = Boolean(playlist && state.jukeboxDirtyPlaylistIds.has(playlist.id));
  const saveButton = $('saveJukeboxPlaylistBundle');
  saveButton.disabled = !playlist;
  saveButton.textContent = dirty ? 'Save •' : 'Save';
  saveButton.classList.toggle('primary', dirty);
  saveButton.classList.toggle('secondary', !dirty);
  saveButton.title = playlist
    ? (dirty ? 'Save changes and rebuild this playlist’s share files.' : 'Rebuild this playlist’s share files.')
    : 'Select an editable playlist first.';
}

function syncJukeboxPlaylistEditor() {
  const selectedId = $('jukeboxPlaylistSelect').value;
  const playlist = (state.jukebox?.playlists || []).find((item) => item.id === selectedId);
  if (playlist) {
    state.jukeboxDraftKeys = new Set(playlist.trackKeys || []);
  } else {
    state.jukeboxDraftKeys = new Set((state.jukebox?.tracks || []).filter((track) => track.selected).map((track) => track.key));
  }
  $('deleteJukeboxPlaylist').disabled = !playlist;
  $('editJukeboxPlaylist').disabled = !playlist;
  $('batchImportJukeboxWav').disabled = !playlist;
  $('findAllJukeboxArtwork').disabled = !playlist;
  updateJukeboxSaveButton(playlist);
  $('batchImportJukeboxWav').title = playlist
    ? `Add multiple songs to ${playlist.name}` : 'Create or select an editable playlist before batch importing songs.';
  $('findAllJukeboxArtwork').title = playlist
    ? `Find artwork only for songs missing it in ${playlist.name}` : 'Select an editable playlist first.';
  renderJukeboxRows();
}

function renderJukebox(value, { preserveSelection = false } = {}) {
  state.jukebox = value || { available: false, tracks: [], playlists: [] };
  if (state.playingJukeboxKey && !state.jukebox.tracks.some((track) => (
    track.key === state.playingJukeboxKey && track.previewAvailable
  ))) stopJukeboxPreview();
  const ready = Boolean(state.jukebox.available);
  $('jukeboxState').textContent = ready ? 'READY' : 'NO MUSIC FILE';
  $('jukeboxMessage').textContent = ready
    ? `Loaded ${state.jukebox.musicIff}`
    : (state.jukebox.message || 'No music.iff is installed in the active profile.');
  $('jukeboxDot').className = `status-dot ${ready ? 'ready' : 'error'}`;
  $('jukeboxActivePlaylist').textContent = String(state.jukebox.activePlaylistName || 'Default Playlist (Locked)').toUpperCase();
  if (!preserveSelection) state.jukeboxDraftKeys = new Set(state.jukebox.tracks.filter((track) => track.selected).map((track) => track.key));
  const select = $('jukeboxPlaylistSelect');
  const selectedId = state.jukebox.activePlaylistId || '';
  select.replaceChildren(new Option('Default Playlist (Locked)', ''), ...(state.jukebox.playlists || []).map((playlist) => (
    new Option(`${playlist.name} (${playlist.trackCount})`, playlist.id)
  )));
  select.value = selectedId;
  $('deleteJukeboxPlaylist').disabled = !ready || !select.value;
  $('newJukeboxPlaylist').disabled = !ready;
  syncJukeboxPlaylistEditor();
  syncHomePicks();
}

function markJukeboxPlaylistsDirty(playlistIds) {
  for (const id of playlistIds || []) if (id) state.jukeboxDirtyPlaylistIds.add(id);
  updateJukeboxSaveButton();
}

function stopJukeboxPreview({ rerender = true } = {}) {
  if (state.jukeboxAudio) {
    state.jukeboxAudio.pause();
    state.jukeboxAudio.removeAttribute('src');
    state.jukeboxAudio.load();
  }
  state.playingJukeboxKey = null;
  if (rerender && state.jukebox) renderJukeboxRows();
}

async function toggleJukeboxPreview(track) {
  if (!track?.previewAvailable) return;
  if (state.playingJukeboxKey === track.key && state.jukeboxAudio && !state.jukeboxAudio.paused) {
    state.jukeboxAudio.pause();
    state.playingJukeboxKey = null;
    renderJukeboxRows();
    return;
  }
  const preview = await window.modLauncher.previewJukeboxTrack(track.key);
  if (!state.jukeboxAudio) {
    state.jukeboxAudio = new Audio();
    state.jukeboxAudio.preload = 'metadata';
    state.jukeboxAudio.addEventListener('ended', () => {
      state.playingJukeboxKey = null;
      renderJukeboxRows();
    });
  } else {
    state.jukeboxAudio.pause();
  }
  state.jukeboxAudio.src = preview.url;
  state.playingJukeboxKey = track.key;
  try {
    await state.jukeboxAudio.play();
    renderJukeboxRows();
  } catch (error) {
    stopJukeboxPreview();
    throw new Error(`Could not play the custom WAV: ${error.message}`);
  }
}

function transferableBytes(value) {
  if (value instanceof Uint8Array) return value;
  if (Array.isArray(value?.data)) return Uint8Array.from(value.data);
  return new Uint8Array(value || []);
}

function closeJukeboxImport() {
  $('jukeboxImportModal').classList.add('hidden');
  state.jukeboxImportAudio = null;
  state.jukeboxImportArtwork = null;
  if (state.jukeboxArtworkUrl) URL.revokeObjectURL(state.jukeboxArtworkUrl);
  state.jukeboxArtworkUrl = null;
  $('jukeboxArtworkPreview').classList.add('hidden');
  $('jukeboxArtworkPreview').removeAttribute('src');
}

function openJukeboxImport() {
  state.jukeboxImportAudio = null;
  state.jukeboxImportArtwork = null;
  $('jukeboxImportName').value = '';
  $('jukeboxImportArtist').value = 'Custom';
  $('jukeboxImportAudioName').textContent = 'Choose MP3, WAV, M4A, AAC, FLAC, OGG, or Opus.';
  $('jukeboxImportArtworkName').textContent = 'Optional. PNG, JPG, WebP, or BMP; it will be center-cropped to 256 × 256.';
  const playlist = $('jukeboxImportPlaylist');
  playlist.replaceChildren(new Option('Default Playlist (Locked)', ''), ...(state.jukebox?.playlists || []).map((item) => (
    new Option(item.name, item.id)
  )));
  playlist.value = $('jukeboxPlaylistSelect').value || '';
  $('jukeboxImportModal').classList.remove('hidden');
}

function closeJukeboxNewPlaylist() {
  $('jukeboxNewPlaylistModal').classList.add('hidden');
  $('jukeboxNewPlaylistName').value = '';
  $('jukeboxNewPlaylistDefaults').checked = false;
}

function openJukeboxNewPlaylist() {
  $('jukeboxNewPlaylistModal').dataset.mode = 'new';
  $('jukeboxPlaylistModalEyebrow').textContent = 'NEW PLAYLIST';
  $('jukeboxNewPlaylistTitle').textContent = 'Name your playlist';
  $('jukeboxPlaylistModalHelp').textContent = 'It starts empty unless you choose to include the default soundtrack.';
  $('jukeboxNewPlaylistDefaults').closest('label').classList.remove('hidden');
  $('createJukeboxNewPlaylist').textContent = 'Create playlist';
  $('jukeboxNewPlaylistName').value = '';
  $('jukeboxNewPlaylistDefaults').checked = false;
  $('jukeboxNewPlaylistModal').classList.remove('hidden');
  setTimeout(() => $('jukeboxNewPlaylistName').focus(), 0);
}

function openJukeboxEditPlaylist() {
  const playlist = (state.jukebox?.playlists || []).find((item) => item.id === $('jukeboxPlaylistSelect').value);
  if (!playlist) return;
  $('jukeboxNewPlaylistModal').dataset.mode = 'edit';
  $('jukeboxPlaylistModalEyebrow').textContent = 'EDIT PLAYLIST';
  $('jukeboxNewPlaylistTitle').textContent = 'Rename playlist';
  $('jukeboxPlaylistModalHelp').textContent = 'Rename it now, then use Save playlist once when you finish editing.';
  $('jukeboxNewPlaylistDefaults').closest('label').classList.add('hidden');
  $('createJukeboxNewPlaylist').textContent = 'Save name';
  $('jukeboxNewPlaylistName').value = playlist.name;
  $('jukeboxNewPlaylistDefaults').checked = false;
  $('jukeboxNewPlaylistModal').classList.remove('hidden');
  setTimeout(() => $('jukeboxNewPlaylistName').select(), 0);
}

async function createJukeboxNewPlaylist() {
  const button = $('createJukeboxNewPlaylist');
  const editing = $('jukeboxNewPlaylistModal').dataset.mode === 'edit';
  const seeded = $('jukeboxNewPlaylistDefaults').checked;
  const selected = editing
    ? (state.jukebox?.playlists || []).find((item) => item.id === $('jukeboxPlaylistSelect').value)
    : null;
  button.disabled = true;
  try {
    const result = await window.modLauncher.saveJukeboxPlaylist({
      id: selected?.id,
      name: $('jukeboxNewPlaylistName').value,
      trackKeys: editing ? [...state.jukeboxDraftKeys] : seeded
        ? (state.jukebox?.tracks || []).map((track) => track.key) : [],
      deferBundle: true,
    });
    renderJukebox(result, { preserveSelection: true });
    $('jukeboxPlaylistSelect').value = result.savedPlaylistId;
    syncJukeboxPlaylistEditor();
    markJukeboxPlaylistsDirty([result.savedPlaylistId]);
    closeJukeboxNewPlaylist();
    toast(editing
      ? 'Playlist renamed. Save the playlist when you finish editing.'
      : `New ${seeded ? 'playlist with default songs' : 'empty playlist'} created. Save it when you finish editing.`);
  } catch (error) { toast(error.message, true); }
  finally { button.disabled = false; }
}

function encodePcmWave(audioBuffer) {
  const frames = audioBuffer.length;
  const output = new ArrayBuffer(44 + frames * 4);
  const view = new DataView(output);
  const writeText = (offset, text) => [...text].forEach((character, index) => view.setUint8(offset + index, character.charCodeAt(0)));
  writeText(0, 'RIFF');
  view.setUint32(4, output.byteLength - 8, true);
  writeText(8, 'WAVE');
  writeText(12, 'fmt ');
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, 2, true);
  view.setUint32(24, 48000, true);
  view.setUint32(28, 192000, true);
  view.setUint16(32, 4, true);
  view.setUint16(34, 16, true);
  writeText(36, 'data');
  view.setUint32(40, frames * 4, true);
  const left = audioBuffer.getChannelData(0);
  const right = audioBuffer.numberOfChannels > 1 ? audioBuffer.getChannelData(1) : left;
  let cursor = 44;
  for (let index = 0; index < frames; index += 1) {
    const leftSample = Math.max(-1, Math.min(1, left[index]));
    const rightSample = Math.max(-1, Math.min(1, right[index]));
    view.setInt16(cursor, leftSample < 0 ? leftSample * 32768 : leftSample * 32767, true);
    view.setInt16(cursor + 2, rightSample < 0 ? rightSample * 32768 : rightSample * 32767, true);
    cursor += 4;
  }
  return new Uint8Array(output);
}

async function convertAudioForJukebox(input) {
  const bytes = transferableBytes(input.bytes);
  const AudioContextClass = window.AudioContext || window.webkitAudioContext;
  const decodeContext = new AudioContextClass();
  try {
    const copy = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
    const decoded = await decodeContext.decodeAudioData(copy);
    if (!Number.isFinite(decoded.duration) || decoded.duration <= 0 || decoded.duration > 20 * 60) {
      throw new Error('Choose a song no longer than 20 minutes.');
    }
    const frameCount = Math.ceil(decoded.duration * 48000);
    if (44 + frameCount * 4 > 256 * 1024 * 1024) throw new Error('The converted song would exceed the 256 MB safety limit.');
    const offline = new OfflineAudioContext(2, frameCount, 48000);
    const source = offline.createBufferSource();
    source.buffer = decoded;
    source.connect(offline.destination);
    source.start();
    return encodePcmWave(await offline.startRendering());
  } finally {
    await decodeContext.close().catch(() => {});
  }
}

async function convertArtworkForJukebox(input) {
  const bytes = transferableBytes(input.bytes);
  const blob = new Blob([bytes]);
  const bitmap = await createImageBitmap(blob);
  try {
    const canvas = document.createElement('canvas');
    canvas.width = 256;
    canvas.height = 256;
    const context = canvas.getContext('2d', { alpha: false });
    context.imageSmoothingEnabled = true;
    context.imageSmoothingQuality = 'high';
    const scale = Math.max(256 / bitmap.width, 256 / bitmap.height);
    const width = bitmap.width * scale;
    const height = bitmap.height * scale;
    context.drawImage(bitmap, (256 - width) / 2, (256 - height) / 2, width, height);
    const png = await new Promise((resolve, reject) => canvas.toBlob(
      (value) => (value ? resolve(value) : reject(new Error('Could not prepare the cover image.'))), 'image/png',
    ));
    return new Uint8Array(await png.arrayBuffer());
  } finally { bitmap.close(); }
}

function showJukeboxArtworkSelection(selected, editing) {
  const stateKey = editing ? 'jukeboxEditArtwork' : 'jukeboxImportArtwork';
  const urlKey = editing ? 'jukeboxEditArtworkUrl' : 'jukeboxArtworkUrl';
  const nameId = editing ? 'jukeboxEditArtworkName' : 'jukeboxImportArtworkName';
  const imageId = editing ? 'jukeboxEditArtworkPreview' : 'jukeboxArtworkPreview';
  state[stateKey] = selected;
  const match = selected.match;
  $(nameId).textContent = match
    ? `${match.artist || 'Matched artist'} · ${match.album || match.title || 'online cover'} · ${match.provider}`
    : `${selected.name} · will be center-cropped to 256 × 256`;
  if (state[urlKey]) URL.revokeObjectURL(state[urlKey]);
  state[urlKey] = URL.createObjectURL(new Blob([transferableBytes(selected.bytes)]));
  $(imageId).src = state[urlKey];
  $(imageId).classList.remove('hidden');
}

async function findJukeboxArtworkOnline({ button, artist, title, editing }) {
  button.disabled = true;
  const original = button.textContent;
  button.textContent = 'Searching…';
  try {
    const selected = await window.modLauncher.lookupJukeboxArtwork({ artist, title });
    showJukeboxArtworkSelection(selected, editing);
    toast(`Found artwork for ${artist} - ${title}. Review the preview, then save.`);
  } catch (error) {
    const message = error?.message || 'Artwork lookup failed.';
    $(editing ? 'jukeboxEditArtworkName' : 'jukeboxImportArtworkName').textContent = `Lookup failed: ${message}`;
    throw error;
  } finally {
    button.disabled = false;
    button.textContent = original;
  }
}

async function refreshJukebox() {
  const root=state.settings?.jukeboxLibraryRoot;
  state.homeLoading=true;syncHomePicks();
  try { const result=await window.modLauncher.jukeboxStatus(); if(root===state.settings?.jukeboxLibraryRoot) renderJukebox(result); }
  catch (error) { if(root===state.settings?.jukeboxLibraryRoot) renderJukebox({ available: false, message: error.message, tracks: [], playlists: [] }); }
  finally { if(root===state.settings?.jukeboxLibraryRoot){state.homeLoading=false;syncHomePicks();if(!state.homeBusy&&!state.launchStartedAt)renderHomeSetup();} }
}

function openJukeboxEditor(track) {
  state.editingJukeboxTrack = track;
  state.jukeboxEditArtwork = null;
  if (state.jukeboxEditArtworkUrl) URL.revokeObjectURL(state.jukeboxEditArtworkUrl);
  state.jukeboxEditArtworkUrl = null;
  $('jukeboxEditTitle').textContent = track.title || 'Song';
  $('jukeboxEditName').value = track.title;
  $('jukeboxEditArtist').value = track.artist;
  $('jukeboxEditArtworkName').textContent = track.artworkManaged
    ? 'Custom artwork is active. Choose a replacement, or save to keep it.'
    : 'Using game artwork. Choose an image to replace it for this profile.';
  $('jukeboxEditArtworkPreview').classList.add('hidden');
  $('jukeboxEditArtworkPreview').removeAttribute('src');
  $('jukeboxEditorModal').classList.remove('hidden');
  if (track.artworkPreviewAvailable) {
    loadJukeboxArtworkPreview(track.key).then((url) => {
      if (state.editingJukeboxTrack?.key !== track.key || state.jukeboxEditArtwork) {
        URL.revokeObjectURL(url);
        return;
      }
      if (state.jukeboxEditArtworkUrl) URL.revokeObjectURL(state.jukeboxEditArtworkUrl);
      state.jukeboxEditArtworkUrl = url;
      $('jukeboxEditArtworkPreview').src = state.jukeboxEditArtworkUrl;
      $('jukeboxEditArtworkPreview').classList.remove('hidden');
      $('jukeboxEditArtworkName').textContent = 'Saved custom artwork for this song and mod profile.';
    }).catch(() => {});
  }
}

function closeJukeboxEditor() {
  state.editingJukeboxTrack = null;
  state.jukeboxEditArtwork = null;
  if (state.jukeboxEditArtworkUrl) URL.revokeObjectURL(state.jukeboxEditArtworkUrl);
  state.jukeboxEditArtworkUrl = null;
  $('jukeboxEditArtworkPreview').classList.add('hidden');
  $('jukeboxEditArtworkPreview').removeAttribute('src');
  $('jukeboxEditorModal').classList.add('hidden');
}

async function saveJukeboxEdit() {
  const track = state.editingJukeboxTrack;
  if (!track) return;
  const button = $('saveJukeboxEdit');
  const selectedPlaylistId = $('jukeboxPlaylistSelect').value;
  const affectedPlaylistIds = (state.jukebox?.playlists || [])
    .filter((playlist) => playlist.trackKeys?.includes(track.key)).map((playlist) => playlist.id);
  button.disabled = true;
  button.textContent = state.jukeboxEditArtwork ? 'Converting…' : 'Saving…';
  try {
    const artworkBytes = state.jukeboxEditArtwork
      ? await convertArtworkForJukebox(state.jukeboxEditArtwork) : null;
    const result = await window.modLauncher.saveJukeboxTrack({
      key: track.key,
      title: $('jukeboxEditName').value,
      artist: $('jukeboxEditArtist').value,
      coverName: track.coverName,
      artworkBytes,
      deferBundle: true,
    });
    const cachedArtwork = state.jukeboxArtworkPreviewUrls.get(track.key);
    if (typeof cachedArtwork === 'string') URL.revokeObjectURL(cachedArtwork);
    state.jukeboxArtworkPreviewUrls.delete(track.key);
    renderJukebox(result);
    if (!selectedPlaylistId || (result.playlists || []).some((playlist) => playlist.id === selectedPlaylistId)) {
      $('jukeboxPlaylistSelect').value = selectedPlaylistId;
      syncJukeboxPlaylistEditor();
    }
    closeJukeboxEditor();
    markJukeboxPlaylistsDirty(affectedPlaylistIds);
    toast(`Song saved${result.editReport?.artwork ? ' with updated artwork' : ''}. Restart the game to reload it.`);
  } catch (error) { toast(error.message, true); }
  finally { button.disabled = false; button.textContent = 'Save song'; }
}

document.querySelectorAll('.nav-item').forEach((button) => button.addEventListener('click', () => {
  if (!rosterWorkspace && ['players', 'teams'].includes(button.dataset.page)) { window.modLauncher.openRosterEditor(button.dataset.page).catch(error=>toast(error.message,true)); return; }
  document.querySelectorAll('.nav-item').forEach((item) => item.classList.toggle('active', item === button));
  document.querySelectorAll('.page').forEach((page) => page.classList.toggle('active', page.id === `page-${button.dataset.page}`));
  const titles = { play: '2K Launcher', mods: 'Mod library', jukebox: 'Jukebox', presentation: 'Presentation', profiles: 'Profiles', players: 'Player Editor', teams: 'Team Editor', unlocks: 'Unlocks', settings: 'Launcher settings.', developer: 'Developer diagnostics.' };
  $('pageTitle').textContent = button.dataset.page === 'scaler' ? 'Scaler' : button.dataset.page === 'blacktop' ? 'Blacktop' : titles[button.dataset.page];
  if(button.dataset.page==='play'){
    $('pageTitle').replaceChildren(globalThis.createLauncherIcon('play'),document.createTextNode('2K Launcher'));
  }
  if (['players', 'teams'].includes(button.dataset.page) && state.playerResearch?.status === 'idle') refreshPlayerResearch();
  if (button.dataset.page === 'jukebox' && (!state.jukebox || state.jukebox.available === false)) refreshJukebox();
  else if (button.dataset.page === 'jukebox') renderJukeboxRows();
  if (button.dataset.page === 'mods') renderModRows();
  if (button.dataset.page === 'presentation') refreshPresentation();
}));

$('importJukeboxWav').addEventListener('click', async () => {
  openJukeboxImport();
});
$('batchImportJukeboxWav').addEventListener('click', async () => {
  const playlistId = $('jukeboxPlaylistSelect').value;
  const playlist = (state.jukebox?.playlists || []).find((item) => item.id === playlistId);
  if (!playlist) { toast('Create or select an editable playlist before batch importing songs.', true); return; }
  const button = $('batchImportJukeboxWav');
  button.disabled = true;
  try {
    const repaired = await window.modLauncher.repairMissingJukeboxTracks();
    const repairedCount = Number(repaired?.repairReport?.removed || 0);
    if (repairedCount) renderJukebox(repaired, { preserveSelection: true });
    const selections = await window.modLauncher.chooseJukeboxAudioBatch();
    if (!selections?.length) return;
    $('batchImportEyebrow').textContent = 'BUILDING PLAYLIST';
    $('batchImportTitle').textContent = 'Adding your songs…';
    $('batchImportHelp').textContent = 'Audio is converted and installed one song at a time to keep the launcher responsive.';
    $('batchImportSong').textContent = 'Preparing the selected audio files.';
    $('batchImportStage').textContent = 'Converting';
    $('batchImportCount').textContent = `0 / ${selections.length}`;
    $('batchImportProgressBar').style.width = '2%';
    $('batchImportOverlay').classList.remove('hidden');
    let latest = null;
    const failures = [];
    for (let index = 0; index < selections.length; index += 1) {
      const selection = selections[index];
      button.textContent = `${index + 1} / ${selections.length}`;
      $('batchImportSong').textContent = `${selection.artist} - ${selection.title}`;
      $('batchImportStage').textContent = 'Converting audio';
      $('batchImportCount').textContent = `${index + 1} / ${selections.length}`;
      $('batchImportProgressBar').style.width = `${Math.max(3, Math.round((index / selections.length) * 100))}%`;
      try {
        const input = await window.modLauncher.readJukeboxAudioSelection(selection.token);
        const wavBytes = await convertAudioForJukebox(input);
        $('batchImportStage').textContent = 'Installing song';
        latest = await window.modLauncher.importJukeboxAudioData({
          name: input.name.replace(/\.[^.]+$/, '.wav'),
          title: selection.title,
          artist: selection.artist,
          playlistId,
          deferBundle: true,
          bytes: wavBytes,
          artworkBytes: null,
        });
        $('batchImportProgressBar').style.width = `${Math.round(((index + 1) / selections.length) * 100)}%`;
      } catch (error) { failures.push(`${selection.name}: ${error.message}`); }
    }
    if (latest) {
      renderJukebox(latest, { preserveSelection: true });
      $('jukeboxPlaylistSelect').value = playlistId;
      syncJukeboxPlaylistEditor();
      markJukeboxPlaylistsDirty([playlistId]);
    }
    const added = selections.length - failures.length;
    if (failures.length) toast(`Added ${added} of ${selections.length} songs. ${failures[0]}`, true);
    else toast(`Added ${added} songs to ${playlist.name}${repairedCount ? ` after clearing ${repairedCount} incomplete entries` : ''}. Click Save playlist when finished.`);
  } catch (error) { toast(error.message, true); }
  finally {
    $('batchImportOverlay').classList.add('hidden');
    button.textContent = 'Batch add';
    syncJukeboxPlaylistEditor();
  }
});
$('findAllJukeboxArtwork').addEventListener('click', async () => {
  const playlistId = $('jukeboxPlaylistSelect').value;
  const playlist = (state.jukebox?.playlists || []).find((item) => item.id === playlistId);
  if (!playlist) { toast('Select an editable playlist first.', true); return; }
  const playlistKeys = new Set(state.jukeboxDraftKeys);
  const missing = (state.jukebox?.tracks || []).filter((track) => (
    playlistKeys.has(track.key) && !track.artworkPreviewAvailable
  ));
  if (!missing.length) { toast(`${playlist.name} already has saved artwork for every song.`); return; }
  const button = $('findAllJukeboxArtwork');
  button.disabled = true;
  $('batchImportEyebrow').textContent = 'FINDING ARTWORK';
  $('batchImportTitle').textContent = 'Filling missing covers…';
  $('batchImportSong').textContent = 'Preparing the playlist.';
  $('batchImportStage').textContent = 'Searching online';
  $('batchImportCount').textContent = `0 / ${missing.length}`;
  $('batchImportProgressBar').style.width = '2%';
  $('batchImportHelp').textContent = 'Preparing up to 3 covers at a time. Only missing artwork is searched; existing covers are kept.';
  $('batchImportOverlay').classList.remove('hidden');
  let latest = null;
  let failures = [];
  let updatedKeys = new Set();
  try {
    await window.modLauncher.saveJukeboxPlaylist({
      id: playlist.id, name: playlist.name, trackKeys: [...state.jukeboxDraftKeys], deferBundle: true,
    });
    const result = await window.runArtworkBatch(missing, {
      prepare: async (track) => {
        const selected = await window.modLauncher.lookupJukeboxArtwork({ artist: track.artist, title: track.title });
        return convertArtworkForJukebox(selected);
      },
      save: async (track, artworkBytes) => {
        const saved = await window.modLauncher.saveJukeboxTrack({
          key: track.key,
          title: track.title,
          artist: track.artist,
          coverName: track.coverName,
          artworkBytes,
          deferBundle: true,
        });
        const cached = state.jukeboxArtworkPreviewUrls.get(track.key);
        if (typeof cached === 'string') URL.revokeObjectURL(cached);
        state.jukeboxArtworkPreviewUrls.delete(track.key);
        return saved;
      },
      onProgress: ({ item, completed, total, stage }) => {
        button.textContent = `${completed} / ${total}`;
        $('batchImportSong').textContent = `${item.artist} - ${item.title}`;
        $('batchImportStage').textContent = stage;
        $('batchImportCount').textContent = `${completed} / ${total}`;
        $('batchImportProgressBar').style.width = `${Math.max(2, Math.round(completed / total * 100))}%`;
      },
    });
    ({ latest, failures, updatedKeys } = result);
    if (latest) {
      renderJukebox(latest, { preserveSelection: true });
      $('jukeboxPlaylistSelect').value = playlistId;
      syncJukeboxPlaylistEditor();
      const affected = (latest.playlists || []).filter((item) => (
        item.trackKeys?.some((key) => updatedKeys.has(key))
      )).map((item) => item.id);
      markJukeboxPlaylistsDirty(affected);
    }
    if (failures.length) {
      toast(`Added artwork to ${updatedKeys.size} of ${missing.length} missing songs. ${failures[0]}`, true);
    } else {
      toast(`Added artwork to all ${updatedKeys.size} missing songs. Click Save playlist when finished.`);
    }
  } catch (error) { toast(error.message, true); }
  finally {
    $('batchImportOverlay').classList.add('hidden');
    button.textContent = 'Find missing artwork';
    syncJukeboxPlaylistEditor();
  }
});
$('importJukeboxPlaylist').addEventListener('click', async () => {
  const button = $('importJukeboxPlaylist');
  button.disabled = true;
  button.textContent = 'Importing…';
  try {
    const result = await window.modLauncher.importJukeboxPlaylist();
    if (!result) return;
    renderJukebox(result, { preserveSelection: true });
    $('jukeboxPlaylistSelect').value = result.savedPlaylistId;
    syncJukeboxPlaylistEditor();
    const warnings = result.importPlaylistReport.warnings.length;
    toast(`Imported ${result.importPlaylistReport.name} with ${result.importPlaylistReport.trackCount} songs${warnings ? ` (${warnings} unavailable stock references skipped)` : ''}.`);
  } catch (error) { toast(error.message, true); }
  finally { button.disabled = false; button.textContent = 'Import'; }
});
$('openJukeboxPlaylistFolder').addEventListener('click', async () => {
  try { await window.modLauncher.openJukeboxPlaylistFolder(); }
  catch (error) { toast(error.message, true); }
});
$('chooseJukeboxAudio').addEventListener('click', async () => {
  try {
    const selected = await window.modLauncher.chooseJukeboxAudio();
    if (!selected) return;
    state.jukeboxImportAudio = selected;
    $('jukeboxImportAudioName').textContent = selected.name;
    if (!$('jukeboxImportName').value.trim()) {
      $('jukeboxImportName').value = selected.title;
    }
    if (!$('jukeboxImportArtist').value.trim() || $('jukeboxImportArtist').value === 'Custom') {
      $('jukeboxImportArtist').value = selected.artist;
    }
  } catch (error) { toast(error.message, true); }
});
$('chooseJukeboxArtwork').addEventListener('click', async () => {
  try {
    const selected = await window.modLauncher.chooseJukeboxArtwork();
    if (!selected) return;
    showJukeboxArtworkSelection(selected, false);
  } catch (error) { toast(error.message, true); }
});
$('findJukeboxArtwork').addEventListener('click', () => {
  findJukeboxArtworkOnline({
    button: $('findJukeboxArtwork'),
    artist: $('jukeboxImportArtist').value,
    title: $('jukeboxImportName').value,
    editing: false,
  }).catch((error) => toast(error.message, true));
});
$('chooseJukeboxEditArtwork').addEventListener('click', async () => {
  try {
    const selected = await window.modLauncher.chooseJukeboxArtwork();
    if (!selected) return;
    showJukeboxArtworkSelection(selected, true);
  } catch (error) { toast(error.message, true); }
});
$('findJukeboxEditArtwork').addEventListener('click', () => {
  findJukeboxArtworkOnline({
    button: $('findJukeboxEditArtwork'),
    artist: $('jukeboxEditArtist').value,
    title: $('jukeboxEditName').value,
    editing: true,
  }).catch((error) => toast(error.message, true));
});
$('saveJukeboxImport').addEventListener('click', async () => {
  if (!state.jukeboxImportAudio) { toast('Choose a song file first.', true); return; }
  const button = $('saveJukeboxImport');
  button.disabled = true;
  button.textContent = 'Converting…';
  try {
    const targetPlaylistId = $('jukeboxImportPlaylist').value || '';
    const [wavBytes, artworkBytes] = await Promise.all([
      convertAudioForJukebox(state.jukeboxImportAudio),
      state.jukeboxImportArtwork ? convertArtworkForJukebox(state.jukeboxImportArtwork) : null,
    ]);
    button.textContent = 'Installing…';
    const result = await window.modLauncher.importJukeboxAudioData({
      name: state.jukeboxImportAudio.name.replace(/\.[^.]+$/, '.wav'),
      title: $('jukeboxImportName').value,
      artist: $('jukeboxImportArtist').value,
      playlistId: targetPlaylistId || null,
      deferBundle: Boolean(targetPlaylistId),
      bytes: wavBytes,
      artworkBytes,
    });
    renderJukebox(result);
    if (!targetPlaylistId || (result.playlists || []).some((playlist) => playlist.id === targetPlaylistId)) {
      $('jukeboxPlaylistSelect').value = targetPlaylistId;
      syncJukeboxPlaylistEditor();
    }
    if (targetPlaylistId) markJukeboxPlaylistsDirty([targetPlaylistId]);
    closeJukeboxImport();
    const art = result.importReport.artwork ? ' with custom artwork' : '';
    toast(`Added ${result.importReport.title}${art} as custom bank ${result.importReport.bankIndex}. Restart NBA 2K27 to load it.`);
  } catch (error) { toast(error.message, true); }
  finally { button.disabled = false; button.textContent = 'Convert & add song'; }
});
$('closeJukeboxImport').addEventListener('click', closeJukeboxImport);
$('cancelJukeboxImport').addEventListener('click', closeJukeboxImport);
$('jukeboxSearch').addEventListener('input', renderJukeboxRows);
$('jukeboxRows').addEventListener('change', (event) => {
  const checkbox = event.target.closest('[data-jukebox-select]');
  const row = event.target.closest('.jukebox-row');
  if (!checkbox || !row) return;
  if (checkbox.checked) state.jukeboxDraftKeys.add(row.dataset.trackKey);
  else state.jukeboxDraftKeys.delete(row.dataset.trackKey);
  markJukeboxPlaylistsDirty([$('jukeboxPlaylistSelect').value]);
  renderJukeboxRows();
});
$('jukeboxRows').addEventListener('click', (event) => {
  const row = event.target.closest('.jukebox-row');
  if (!row) return;
  const track = (state.jukebox?.tracks || []).find((item) => item.key === row.dataset.trackKey);
  if (!track) return;
  if (event.target.closest('[data-jukebox-preview]')) {
    toggleJukeboxPreview(track).catch((error) => toast(error.message, true));
  } else if (event.target.closest('[data-jukebox-edit]')) {
    openJukeboxEditor(track);
  } else if (event.target.closest('[data-jukebox-remove]')) {
    const playlistId = $('jukeboxPlaylistSelect').value;
    const playlist = (state.jukebox?.playlists || []).find((item) => item.id === playlistId);
    if (!playlist) return;
    const removeButton = event.target.closest('[data-jukebox-remove]');
    removeButton.disabled = true;
    state.jukeboxDraftKeys.delete(track.key);
    renderJukeboxRows();
    window.modLauncher.saveJukeboxPlaylist({
      id: playlist.id,
      name: playlist.name,
      trackKeys: [...state.jukeboxDraftKeys],
      deferBundle: true,
    }).then((result) => {
      renderJukebox(result, { preserveSelection: true });
      $('jukeboxPlaylistSelect').value = playlist.id;
      syncJukeboxPlaylistEditor();
      markJukeboxPlaylistsDirty([playlist.id]);
      toast(`${track.title || 'Song'} removed from ${playlist.name}. Save the playlist when finished.`);
    }).catch((error) => {
      state.jukeboxDraftKeys.add(track.key);
      renderJukeboxRows();
      toast(error.message, true);
    });
  }
});
$('newJukeboxPlaylist').addEventListener('click', openJukeboxNewPlaylist);
$('editJukeboxPlaylist').addEventListener('click', openJukeboxEditPlaylist);
$('closeJukeboxNewPlaylist').addEventListener('click', closeJukeboxNewPlaylist);
$('cancelJukeboxNewPlaylist').addEventListener('click', closeJukeboxNewPlaylist);
$('createJukeboxNewPlaylist').addEventListener('click', createJukeboxNewPlaylist);
$('jukeboxNewPlaylistName').addEventListener('keydown', (event) => {
  if (event.key === 'Enter') createJukeboxNewPlaylist();
});
$('jukeboxPlaylistSelect').addEventListener('change', () => {
  syncJukeboxPlaylistEditor();
  activateSelectedJukeboxPlaylist();
});
$('saveJukeboxPlaylistBundle').addEventListener('click', async () => {
  const playlistId = $('jukeboxPlaylistSelect').value;
  const playlist = (state.jukebox?.playlists || []).find((item) => item.id === playlistId);
  if (!playlist) return;
  const button = $('saveJukeboxPlaylistBundle');
  button.disabled = true;
  button.textContent = 'Saving…';
  $('batchImportEyebrow').textContent = 'SAVING PLAYLIST';
  $('batchImportTitle').textContent = 'Packaging your playlist…';
  $('batchImportSong').textContent = playlist.name;
  $('batchImportStage').textContent = 'Building share files';
  $('batchImportCount').textContent = `${state.jukeboxDraftKeys.size} songs`;
  $('batchImportProgressBar').style.width = '65%';
  $('batchImportHelp').textContent = 'Gameplay changes are already local. This step creates the compressed files used for backup and sharing.';
  $('batchImportOverlay').classList.remove('hidden');
  try {
    const result = await window.modLauncher.saveJukeboxPlaylist({
      id: playlist.id,
      name: playlist.name,
      trackKeys: [...state.jukeboxDraftKeys],
    });
    state.jukeboxDirtyPlaylistIds.delete(playlist.id);
    renderJukebox(result, { preserveSelection: true });
    $('jukeboxPlaylistSelect').value = playlist.id;
    syncJukeboxPlaylistEditor();
    const parts = Number(result.bundleReport?.parts || 1);
    toast(parts > 1
      ? `Playlist saved as ${parts} numbered .2kplaylist files. Keep them together when sharing.`
      : 'Playlist saved and its .2kplaylist share file is ready.');
  } catch (error) { toast(error.message, true); }
  finally {
    $('batchImportOverlay').classList.add('hidden');
    updateJukeboxSaveButton();
  }
});
async function activateSelectedJukeboxPlaylist() {
  const select=$('jukeboxPlaylistSelect');
  if(state.homeBusy||state.homeLoading)return;
  const selected=(state.jukebox?.playlists||[]).find(p=>p.id===select.value);
  if(selected&&!selected.trackCount){toast('Add a song before this playlist can be used.');return;}
  state.homeBusy=true;select.disabled=true;syncHomePicks();
  try {
    const playlistId = $('jukeboxPlaylistSelect').value;
    const playlist = (state.jukebox?.playlists || []).find((item) => item.id === playlistId);
    if (playlist && state.jukeboxDirtyPlaylistIds.has(playlist.id)) {
      await window.modLauncher.saveJukeboxPlaylist({
        id: playlist.id, name: playlist.name, trackKeys: [...state.jukeboxDraftKeys], deferBundle: true,
      });
    }
    renderJukebox(await window.modLauncher.activateJukeboxPlaylist(playlistId || null));
    toast('Playlist selected for your next game launch.');
  } catch (error) { renderJukebox(state.jukebox);toast(error.message, true); }
  finally { select.disabled=false;state.homeBusy=false;syncHomePicks();if(!state.launchStartedAt)renderHomeSetup(); }
}
$('deleteJukeboxPlaylist').addEventListener('click', async () => {
  const id = $('jukeboxPlaylistSelect').value;
  if (!id) return;
  const button = $('deleteJukeboxPlaylist');
  button.disabled = true;
  try {
    renderJukebox(await window.modLauncher.deleteJukeboxPlaylist(id));
    state.jukeboxDirtyPlaylistIds.delete(id);
    toast('Playlist deleted. No audio files were removed.');
  } catch (error) { toast(error.message, true); }
  finally { button.disabled = false; }
});
$('closeJukeboxEditor').addEventListener('click', closeJukeboxEditor);
$('cancelJukeboxEdit').addEventListener('click', closeJukeboxEditor);
$('saveJukeboxEdit').addEventListener('click', saveJukeboxEdit);

$('refreshPlayers').addEventListener('click', refreshPlayerResearch);
function previewPlayerEditorLayout() {
  openPlayerEditor({ source: 'layout-preview', name: 'Joel Embiid', firstName: 'Joel', lastName: 'Embiid',
    position: 'Center', positionId: 4, secondaryPositionId: 3, jerseyNumber: 21,
    teamName: 'Philadelphia 76ers', heightCm: 213.36, wingspanCm: 228.6, weightLbs: 280,
    bioLayoutVerified:true,birthDate:'1994-03-16',birthYear:1994,birthMonth:3,birthDay:16,yearsPro:12,
    overall: 96, potential: 96 });
  document.querySelectorAll('#playerEditorModal input:not([readonly]), #playerEditorModal select')
    .forEach(control => { control.disabled = false; });
  $('playerAudioFullName').disabled = true;
  $('savePlayerEdit').disabled = true;
  $('playerEditBoundary').textContent = 'Layout preview · Sample data only. Nothing is read from or saved to the game. Close to discard field changes.';
}
$('refreshTeams').addEventListener('click', refreshPlayerResearch);
$('exportPlayers').addEventListener('click', exportPlayerCatalog);
$('playerSearch').addEventListener('input', renderPlayerRows);
$('playerTeamFilter').addEventListener('change', renderPlayerRows);
$('playerRows').addEventListener('click', (event) => {
  const row = event.target.closest('.player-row');
  if (!row) return;
  const player = (state.playerResearch?.players || [])[Number(row.dataset.playerIndex)];
  if (player) openPlayerEditor(player);
});
$('closePlayerEditor').addEventListener('click', closePlayerEditor);
$('cancelPlayerEdit').addEventListener('click', closePlayerEditor);
$('savePlayerEdit').addEventListener('click', savePlayerEdit);
$('teamSearch').addEventListener('input', renderTeamRows);
$('teamRows').addEventListener('click', (event) => {
  const row = event.target.closest('.team-row');
  if (!row) return;
  const team = (state.playerResearch?.teams || []).find((item) => item.recordIndex === Number(row.dataset.recordIndex)
    && item.teamId === Number(row.dataset.teamId));
  if (team) openTeamEditor(team);
});
$('closeTeamEditor').addEventListener('click', closeTeamEditor);
$('cancelTeamEdit').addEventListener('click', closeTeamEditor);
$('saveTeamEdit').addEventListener('click', saveTeamEdit);
$('reviewTeamEdit').addEventListener('click', reviewTeamChanges);
$('teamEditArenaModel').addEventListener('change',()=>{
 const arena=(state.playerResearch?.arenas||[]).find(a=>a.arenaId===Number($('teamEditArenaModel').value));
 if(!arena)return;$('teamEditArenaId').value=String(arena.arenaId);$('teamEditArenaAssets').value=arena.arenaFile;$('teamEditArenaName').value=arena.name;
});
$('teamEditFloor').addEventListener('change',()=>{$('teamEditFloorAsset').value=$('teamEditFloor').value;});
$('teamDnaImport').addEventListener('click',()=>void transferTeamDna('import'));
$('teamDnaExport').addEventListener('click',()=>void transferTeamDna('export'));
document.querySelectorAll('[data-team-tab]').forEach((tab) => {
  tab.addEventListener('click', () => selectTeamEditorTab(tab.dataset.teamTab));
  tab.addEventListener('keydown', (event) => {
    if (!['ArrowUp', 'ArrowDown', 'Home', 'End'].includes(event.key)) return;
    event.preventDefault();
    const tabs = [...document.querySelectorAll('[data-team-tab]')];
    const current = tabs.indexOf(tab);
    const next = event.key === 'Home' ? 0 : event.key === 'End' ? tabs.length - 1
      : (current + (event.key === 'ArrowDown' ? 1 : -1) + tabs.length) % tabs.length;
    selectTeamEditorTab(tabs[next].dataset.teamTab);
    tabs[next].focus();
  });
});
$('createCustomArena').addEventListener('click', createCustomArena);
$('customArenaTarget').addEventListener('change', loadCustomArenaFields);
$('closeModTagEditor').addEventListener('click', closeModTagEditor);
$('cancelModTagEdit').addEventListener('click', closeModTagEditor);
$('saveModTagEdit').addEventListener('click', saveModTagEdit);
$('modTagModal').addEventListener('click', (event) => { if (event.target === $('modTagModal')) closeModTagEditor(); });
// Editors dismiss only through explicit controls or Escape. A text-selection
// drag ending on the backdrop can generate a click targeted at the backdrop.

document.addEventListener('keydown', (event) => {
  if (event.key === 'Escape' && !$('jukeboxNewPlaylistModal').classList.contains('hidden')) {
    closeJukeboxNewPlaylist();
    return;
  }
  if (event.key === 'Escape' && !$('jukeboxImportModal').classList.contains('hidden')) {
    closeJukeboxImport();
    return;
  }
  if (event.key === 'Escape' && !$('jukeboxEditorModal').classList.contains('hidden')) {
    closeJukeboxEditor();
    return;
  }
  if (event.key === 'Escape' && !$('modTagModal').classList.contains('hidden')) {
    closeModTagEditor();
    return;
  }
  if (event.key === 'Escape' && !$('playerEditorModal').classList.contains('hidden')) {
    closePlayerEditor();
    return;
  }
  if (event.key === 'Escape' && !$('teamEditorModal').classList.contains('hidden')) {
    closeTeamEditor();
    return;
  }
  if (!event.ctrlKey || !event.shiftKey || event.key.toLowerCase() !== 'd') return;
  event.preventDefault();
  const nav = $('developerNav');
  nav.classList.toggle('hidden');
  if (!nav.classList.contains('hidden')) toast('Developer diagnostics unlocked for this launcher window.');
});

for (const eventName of ['dragover', 'drop']) {
  document.addEventListener(eventName, (event) => {
    if (!Array.from(event.dataTransfer?.types || []).includes('Files')) return;
    event.preventDefault();
    event.stopPropagation();
    if (eventName === 'dragover') event.dataTransfer.dropEffect = 'none';
  }, true);
}

$('cameraRows').addEventListener('change', (event) => {
  const row = event.target.closest('.camera-row');
  if (!row) return;
  const camera = state.cameras?.cameras?.[Number.parseInt(row.dataset.index, 10)];
  if (!camera) return;
  if (event.target.dataset.field === 'name') camera.name = event.target.value;
  if (event.target.dataset.field === 'selected') camera.selected = event.target.checked;
  if (event.target.dataset.field === 'height') camera.height = Number.parseInt(event.target.value, 10);
  if (event.target.dataset.field === 'zoom') camera.zoom = Number.parseInt(event.target.value, 10);
  renderCameraStatus(state.cameras);
});
$('cameraRows').addEventListener('click', (event) => {
  const reset = event.target.closest('[data-reset-name]');
  if (reset) {
    const row = reset.closest('.camera-row');
    const camera = state.cameras?.cameras?.[Number.parseInt(row.dataset.index, 10)];
    if (camera) { camera.name = camera.defaultName || camera.name; row.querySelector('[data-field="name"]').value = camera.name; }
    return;
  }
  const button = event.target.closest('[data-move]');
  if (!button) return;
  const row = button.closest('.camera-row');
  const index = Number.parseInt(row.dataset.index, 10);
  const destination = button.dataset.move === 'up' ? index - 1 : index + 1;
  if (destination < 0 || destination >= state.cameras.cameras.length) return;
  const setup = cameraSetupFromUi();
  state.cameras.lockValues = setup.lockValues;
  state.cameras.cameras = setup.cameras.map((camera) => ({
    ...state.cameras.cameras.find((item) => item.id === camera.id),
    ...camera,
  }));
  [state.cameras.cameras[index], state.cameras.cameras[destination]] = [state.cameras.cameras[destination], state.cameras.cameras[index]];
  renderCameraRows();
});
$('lockCameraValues').addEventListener('change', () => { state.cameras.lockValues = $('lockCameraValues').checked; });
$('saveCameraSetup').addEventListener('click', async () => {
  const button = $('saveCameraSetup');
  button.disabled = true;
  button.textContent = 'Saving…';
  try {
    renderCameras(await window.modLauncher.saveCameras(cameraSetupFromUi()));
    toast('Camera setup saved. Names apply after restarting the game; live height and zoom update automatically.');
  } catch (error) { toast(error.message, true); }
  finally { button.disabled = false; button.textContent = 'Save camera setup'; }
});
$('stopCameras').addEventListener('click', async () => {
  const button = $('stopCameras');
  button.disabled = true;
  try {
    renderCameraStatus(await window.modLauncher.stopCameras());
    toast('Camera controller stopped and restoration requested.');
  } catch (error) { toast(error.message, true); }
  finally { button.disabled = false; }
});
$('exportSupportBundle').addEventListener('click',async()=>{
  const button=$('exportSupportBundle'),status=$('supportBundleStatus');button.disabled=true;button.textContent='Exporting…';
  try{const result=await window.modLauncher.exportSupportBundle();if(result){status.textContent=`Saved ${result.files} diagnostic file${result.files===1?'':'s'}${result.truncated?`; ${result.truncated} trimmed to the safe size limit`:''}.`;toast('Redacted support bundle exported.');}}
  catch(error){status.textContent=error.message;toast(error.message,true);}finally{button.disabled=false;button.textContent='Export support bundle';}
});
$('captureGameHang').addEventListener('click',async()=>{
  const button=$('captureGameHang');button.disabled=true;
  try{const result=await window.modLauncher.captureGameHang();$('gameCaptureStatus').textContent=`Snapshot saved: ${result.filePath}`;toast('Game snapshot saved locally.');}
  catch(error){$('gameCaptureStatus').textContent=error.message;toast(error.message,true);}
  finally{button.disabled=false;}
});
$('openGameDiagnostics').addEventListener('click',()=>window.modLauncher.openDiagnosticsFolder().catch(error=>toast(error.message,true)));
const paintGameCapture=state=>{$('gameCaptureStatus').textContent=state.message;$('captureGameHang').disabled=state.phase==='capturing';};
window.modLauncher.onGameCaptureStatus(paintGameCapture);
window.modLauncher.gameCaptureStatus().then(paintGameCapture).catch(()=>{});
$('retryCameras').addEventListener('click', async () => {
  const button = $('retryCameras');
  button.disabled = true;
  button.textContent = 'Retrying…';
  try {
    renderCameraStatus(await window.modLauncher.startCameras(cameraSetupFromUi()));
  } catch (error) { toast(error.message, true); }
  finally { button.disabled = false; button.textContent = 'Retry cameras'; }
});
$('installCameraNames').addEventListener('click', async () => {
  const button = $('installCameraNames');
  button.disabled = true;
  try {
    renderCameraStatus(await window.modLauncher.installCameraNames());
    toast('Camera names installed in the active mods folder. Restart the game to load them.');
  } catch (error) { toast(error.message, true); }
  finally { button.disabled = false; }
});

document.querySelectorAll('[data-choose]').forEach((button) => button.addEventListener('click', async () => {
  try {
    const value = await window.modLauncher.choosePath(button.dataset.choose);
    if (value) $(button.dataset.choose).value = value;
  } catch (error) { toast(error.message, true); }
}));

$('profileRows').addEventListener('change', async (event) => {
  const input = event.target.closest('[name="activeModsProfile"]');
  if (!input) return;
  try {
    const result = await window.modLauncher.saveSettings({ modsRoot: input.value });
    state.settings = result.settings;
    state.validation = result.validation;
    fillForm();
    renderValidation();
    await refreshPresentation();
    toast(`${state.settings.modsProfiles.find((profile) => profile.path.toLowerCase() === input.value.toLowerCase())?.name || 'Profile'} selected.`);
  } catch (error) { fillForm(); toast(error.message, true); }
});
$('homeModProfile').addEventListener('change', async () => {
  const selected=$('homeModProfile').value;
  if(state.homeBusy||state.homeLoading) return;
  state.homeBusy=true;syncHomePicks();
  try {
    const result=await window.modLauncher.saveSettings({modsRoot:selected});
    state.settings=result.settings;state.validation=result.validation;
    fillForm();renderValidation();await refreshPresentation();
    await state.homeCatalogPromise;
  } catch(error) {toast(error.message,true);}
  finally {state.homeBusy=false;fillForm();syncHomePicks();if(!state.launchStartedAt)renderHomeSetup();}
});
$('homeJukeboxPlaylist').addEventListener('change', async () => {
  const id=$('homeJukeboxPlaylist').value;
  if(state.homeBusy||state.homeLoading) return;
  if(state.jukeboxDirtyPlaylistIds.has(id)) {toast('Save this playlist in the Jukebox tab before selecting it here.',true);syncHomePicks();return;}
  state.homeBusy=true;syncHomePicks();
  try {renderJukebox(await window.modLauncher.activateJukeboxPlaylist(id||null));}
  catch(error){toast(error.message,true);}
  finally {state.homeBusy=false;syncHomePicks();if(!state.launchStartedAt)renderHomeSetup();}
});
$('homeJukeboxEnabled').addEventListener('change', async () => {
  if (state.homeBusy || state.homeLoading || profilesLocked() || !state.jukebox?.available) { syncHomePicks(); return; }
  const enabled=$('homeJukeboxEnabled').checked, playlists=state.jukebox.playlists||[];
  const id=enabled?(playlists.find(p=>p.id===state.jukebox.lastPlaylistId)?.id || playlists[0]?.id):null;
  if(enabled&&!id){syncHomePicks();return;}
  if(id&&state.jukeboxDirtyPlaylistIds.has(id)){toast('Save this playlist in Jukebox before enabling it.',true);syncHomePicks();return;}
  state.homeBusy=true;syncHomePicks();
  try {renderJukebox(await window.modLauncher.activateJukeboxPlaylist(id));}
  catch(error){toast(error.message,true);}
  finally {state.homeBusy=false;syncHomePicks();if(!state.launchStartedAt)renderHomeSetup();}
});
$('addModsProfile').addEventListener('click', async () => {
  if (profilesLocked() || state.profilePicking || $('profileDialog').open) return;
  state.profilePicking = true;
  renderSessionUi();
  try {
    const selected = await window.modLauncher.choosePath('modsRoot');
    if (selected) openProfileDialog('create', selected);
  } catch (error) { toast(error.message, true); }
  finally { state.profilePicking = false; renderSessionUi(); }
});

function profilesLocked() {
  return state.session?.phase === 'game-running'
    || ['preparing-loader', 'launching-game', 'waiting-for-game', 'loader-armed', 'activating-loader', 'loader-active', 'stopping-loader'].includes(state.loader?.phase);
}

function openProfileDialog(mode, selected) {
  const dialog = $('profileDialog');
  if (profilesLocked() || dialog.open) return;
  const defaultRoot = `${String(state.settings.gameRoot || '').replace(/[\\/]+$/, '')}\\mods`;
  if (mode === 'edit' && String(selected).toLowerCase() === defaultRoot.toLowerCase()) return;
  const profile = (state.settings.modsProfiles || []).find(item => item.path.toLowerCase() === selected.toLowerCase());
  if (mode === 'create' && profile) { toast('That folder is already saved. Select it from the list.'); return; }
  if (mode === 'edit' && !profile) return;
  dialog.dataset.mode = mode;
  dialog.dataset.path = selected;
  dialog.dataset.folder = selected;
  dialog.dataset.saving = 'false';
  dialog.dataset.picking = 'false';
  $('profileDialogTitle').textContent = mode === 'create' ? 'New profile' : 'Edit profile';
  $('profileDialogPath').textContent = selected;
  $('profileDialogFolderHelp').hidden = mode !== 'edit' || selected.toLowerCase() !== defaultRoot.toLowerCase();
  $('profileDialogFolderHelp').textContent = 'Choosing another folder adds a separate profile and keeps the game’s default folder.';
  $('profileDialogName').value = profile?.name || selected.replace(/[\\/]+$/, '').split(/[\\/]/).at(-1) || 'Mods';
  $('profileDialogName').disabled = false;
  $('profileDialogError').hidden = true;
  $('cancelProfileDialog').disabled = false;
  $('chooseProfileDialogFolder').disabled = false;
  $('saveProfileDialog').disabled = false;
  $('saveProfileDialog').textContent = mode === 'create' ? 'Create profile' : 'Save changes';
  dialog.showModal();
  $('profileDialogName').focus();
  $('profileDialogName').select();
}

$('cancelProfileDialog').addEventListener('click', () => {
  if ($('profileDialog').dataset.saving !== 'true' && $('profileDialog').dataset.picking !== 'true') $('profileDialog').close();
});
$('profileDialog').addEventListener('cancel', event => {
  if ($('profileDialog').dataset.saving === 'true' || $('profileDialog').dataset.picking === 'true') event.preventDefault();
});
$('chooseProfileDialogFolder').addEventListener('click', async () => {
  const dialog = $('profileDialog');
  if (!dialog.open || profilesLocked() || dialog.dataset.saving === 'true' || dialog.dataset.picking === 'true') return;
  dialog.dataset.picking = 'true';
  $('cancelProfileDialog').disabled = true;
  renderSessionUi();
  try {
    const selected = await window.modLauncher.choosePath('modsRoot');
    if (selected && dialog.open && !profilesLocked()) {
      dialog.dataset.folder = selected;
      $('profileDialogPath').textContent = selected;
      $('profileDialogError').hidden = true;
    }
  } catch (error) { $('profileDialogError').textContent = error.message; $('profileDialogError').hidden = false; }
  finally { dialog.dataset.picking = 'false'; $('cancelProfileDialog').disabled = false; renderSessionUi(); }
});
$('profileDialogForm').addEventListener('submit', async event => {
  event.preventDefault();
  const dialog = $('profileDialog'), name = $('profileDialogName').value.trim();
  if (!dialog.open || dialog.dataset.saving === 'true' || dialog.dataset.picking === 'true') return;
  const showError = message => { $('profileDialogError').textContent = message; $('profileDialogError').hidden = false; };
  if (profilesLocked()) { showError('Profiles are locked while your game session is active.'); return; }
  if (!name || name.length > 80) { showError('Enter a profile name of 1–80 characters.'); return; }
  const original = dialog.dataset.path, selected = dialog.dataset.folder, creating = dialog.dataset.mode === 'create';
  const profiles = state.settings.modsProfiles || [];
  const exists = profiles.some(profile => profile.path.toLowerCase() === original.toLowerCase());
  const duplicate = profiles.some(profile => profile.path.toLowerCase() === selected.toLowerCase()
    && (creating || profile.path.toLowerCase() !== original.toLowerCase()));
  if (duplicate) { showError('That folder is already saved in another profile. Choose a different folder.'); return; }
  if (!creating && !exists) { showError('This profile is no longer saved. Cancel and choose another profile.'); return; }
  const folderChanged = original.toLowerCase() !== selected.toLowerCase();
  const defaultRoot = `${String(state.settings.gameRoot || '').replace(/[\\/]+$/, '')}\\mods`;
  const keepDefault = !creating && folderChanged && original.toLowerCase() === defaultRoot.toLowerCase();
  const changesSelection = creating || (folderChanged && original.toLowerCase() === String(state.settings.modsRoot || '').toLowerCase());
  dialog.dataset.saving = 'true';
  $('saveProfileDialog').disabled = true;
  $('cancelProfileDialog').disabled = true;
  $('profileDialogName').disabled = true;
  $('profileDialogError').hidden = true;
  try {
    const modsProfiles = creating || keepDefault ? [...profiles, { name, path: selected }] : profiles.map((profile) => (
      profile.path.toLowerCase() === original.toLowerCase() ? { ...profile, name, path: selected } : profile
    ));
    const result = await window.modLauncher.saveSettings(changesSelection ? { modsRoot: selected, modsProfiles } : { modsProfiles });
    state.settings = result.settings;
    state.validation = result.validation;
    dialog.close();
    fillForm();
    renderValidation();
    toast(creating ? `${name} created and selected.` : 'Profile updated.');
    if (changesSelection) await refreshPresentation();
  } catch (error) { if (dialog.open) showError(error.message); else toast(error.message, true); }
  finally {
    dialog.dataset.saving = 'false';
    $('cancelProfileDialog').disabled = false;
    $('profileDialogName').disabled = false;
    renderSessionUi();
  }
});
async function removeProfileShortcut(selected) {
  const defaultRoot = `${String(state.settings.gameRoot || '').replace(/[\\/]+$/, '')}\\mods`;
  if (profilesLocked() || !selected || selected.toLowerCase() === defaultRoot.toLowerCase()
    || selected.toLowerCase() === String(state.settings.modsRoot || '').toLowerCase()) return;
  try {
    const modsProfiles = (state.settings.modsProfiles || []).filter((profile) => profile.path.toLowerCase() !== selected.toLowerCase());
    const result = await window.modLauncher.saveSettings({ modsProfiles });
    state.settings = result.settings;
    state.validation = result.validation;
    fillForm();
    renderValidation();
    toast('Profile shortcut removed. No mod files were deleted.');
  } catch (error) { toast(error.message, true); }
}

async function scanModCatalog({ startup = false } = {}) {
  const root = state.settings.modsRoot;
  const revision = (state.modScanRevision || 0) + 1;
  state.modScanRevision = revision;
  $('scanMods').disabled = true;
  $('scanModsLabel').textContent = 'Scanning…';
  try {
    const result = await window.modLauncher.scanMods();
    if (revision !== state.modScanRevision || root !== state.settings.modsRoot) return;
    state.homeSetupError=null;renderHomeSetup();
    renderCatalog(result);
    if (!startup) toast('Active mod library refreshed.');
  } catch (error) {
    if (revision !== state.modScanRevision || root !== state.settings.modsRoot) return;
    $('modResultSummary').textContent = `Scan failed: ${error.message}`;
    state.homeSetupError=error.message;renderHomeSetup();
    if(!startup)toast(error.message,true);
  }
  finally { if (revision === state.modScanRevision) { $('scanMods').disabled = false; $('scanModsLabel').textContent = 'Scan mods'; } }
}

$('saveSettings').addEventListener('click', saveSettings);
function renderUnlockStatus(id,value){
  const node=$(id),phase=value?.phase||'',message=(value?.message||'').trim();
  node.textContent=phase==='saved'||phase==='disabled'||/^Saved\b/.test(message)||message==='Unlock helper stopped safely. Restart the game before retrying.'?'':message;
}
$('unlockWnbaFreestyle').addEventListener('change', async () => {
  const input=$('unlockWnbaFreestyle'),enabled=input.checked;input.disabled=true;
  try {
    const result=await window.modLauncher.saveSettings({unlockWnbaFreestyle:enabled});state.settings=result.settings;
    $('freestyleUnlockStatus').textContent='';
  } catch(error) { input.checked=!enabled;toast(error.message,true); }
  finally { input.disabled=false; }
});
$('unlockWnbaBlacktop').addEventListener('change',async()=>{
  const input=$('unlockWnbaBlacktop');input.disabled=true;
  try{const result=await window.modLauncher.saveSettings({unlockWnbaBlacktop:input.checked});state.settings=result.settings;$('blacktopUnlockStatus').textContent='';}
  catch(error){input.checked=state.settings.unlockWnbaBlacktop===true;$('blacktopUnlockStatus').textContent=error.message;}
  finally{input.disabled=false;}
});
window.modLauncher.onFreestyleUnlockStatus(value=>renderUnlockStatus('freestyleUnlockStatus',value));
window.modLauncher.onBlacktopUnlockStatus(value=>renderUnlockStatus('blacktopUnlockStatus',value));
window.modLauncher.freestyleUnlockStatus().then(value=>renderUnlockStatus('freestyleUnlockStatus',value)).catch(()=>{});
$('autoOpenBlacktop').addEventListener('change', async () => {
  const input = $('autoOpenBlacktop'); const enabled = input.checked; input.disabled = true;
  try { await window.modLauncher.saveSettings({ autoOpenBlacktop: enabled }); state.settings.autoOpenBlacktop = enabled; }
  catch (error) { input.checked = !enabled; alert(error.message); }
  finally { input.disabled = false; }
});
$('usePlaylistInFreestyle').addEventListener('change', async () => {
  const checkbox=$('usePlaylistInFreestyle'),enabled=checkbox.checked;
  checkbox.disabled=true;
  try {
    const result=await window.modLauncher.saveSettings({usePlaylistInFreestyle:enabled});
    state.settings=result.settings;
    toast(enabled?'Playlist in Freestyle enabled for next launch.':'Freestyle will use its stock music next launch.');
  } catch(error) { checkbox.checked=!enabled;toast(error.message,true); }
  finally { checkbox.disabled=false; }
});
$('cachePreparedPlaylists').addEventListener('change',async()=>{
 const input=$('cachePreparedPlaylists'),enabled=input.checked;input.disabled=true;
 try{const result=await window.modLauncher.saveSettings({cachePreparedPlaylists:enabled});state.settings=result.settings;toast(enabled?'Faster-launch playlist cache enabled.':'Playlist cache disabled. Cleanup waits until the game is closed.');}
 catch(error){input.checked=state.settings?.cachePreparedPlaylists!==false;toast(error.message,true);}
 finally{input.disabled=false;}
});
for(const [id,key,label] of [['startupVideoEnabled','startupVideoEnabled','Startup video']]){
  $(id).addEventListener('change',async()=>{
    const input=$(id),enabled=input.checked;input.disabled=true;
    try{const result=await window.modLauncher.saveSettings({[key]:enabled});state.settings=result.settings;toast(`${label} ${enabled?'enabled':'disabled'} for the next launch.`);}
    catch(error){input.checked=!enabled;toast(error.message,true);}
    finally{renderPresentation(state.presentation);}
  });
}
async function applyPresentationChoice(action,success){
  try{const result=await action();if(!result)return;state.settings=result.settings;renderPresentation(result.presentation);toast(success);}
  catch(error){toast(error.message,true);}
}
$('choosePresentationMovie').addEventListener('click',async()=>{
  if(state.presentationImportBusy)return;state.presentationImportBusy=true;
  renderPresentation(state.presentation);
  try{await applyPresentationChoice(()=>window.modLauncher.choosePresentationMovie(),'MP4 added to the library.');}
  finally{state.presentationImportBusy=false;$('choosePresentationMovie').disabled=false;$('presentationImportProgress').classList.add('hidden');renderPresentation(state.presentation);}
});
window.modLauncher.onPresentationProgress(value=>{$('presentationImportProgress').classList.remove('hidden');$('presentationImportBar').value=value.percent;$('presentationImportMessage').textContent=value.message;});
$('cancelPresentationImport').addEventListener('click',()=>window.modLauncher.cancelPresentationImport());
$('openPresentationFolder').addEventListener('click',event=>{event.stopPropagation();window.modLauncher.openPresentationFolder().catch(error=>toast(error.message,true));});
$('modSearch').addEventListener('input', renderModRows);
document.querySelectorAll('[data-mod-sort]').forEach(button=>button.addEventListener('click',()=>{
  const key=button.dataset.modSort,current=state.modSort || {key:'title',direction:1};
  state.modSort={key,direction:current.key===key?-current.direction:1};
  for(const item of document.querySelectorAll('[data-mod-sort]')){
    item.setAttribute('aria-pressed',String(item===button));
    item.dataset.direction=item===button?(state.modSort.direction===1?'ascending':'descending'):'';
  }
  renderModRows();
}));
$('installMods').addEventListener('click',async()=>{
  const button=$('installMods');button.disabled=true;$('installModsLabel').textContent='Installing…';
  try{
    const result=await window.modLauncher.installMods();if(result?.cancelled)return;
    if(result?.issues?.length){const detail=result.issues.slice(0,3).map(issue=>`${issue.name}: ${issue.reason}`).join(' · ');toast(`Nothing installed. ${detail}${result.issues.length>3?' · More files were rejected.':''}`,true);return;}
    if(result?.catalog)renderCatalog(result.catalog);toast(`Installed ${result.installed} mod file${result.installed===1?'':'s'}${result.replaced?` · replaced ${result.replaced}`:''}${result.ignored?` · ignored ${result.ignored} non-mod file${result.ignored===1?'':'s'}`:''}.`);
  }catch(error){toast(error.message,true);}
  finally{button.disabled=false;$('installModsLabel').textContent='Install mods';}
});
$('scanMods').addEventListener('click', () => scanModCatalog());
$('openMods').addEventListener('click', () => window.modLauncher.openMods().catch((error) => toast(error.message, true)));
window.modLauncher.storageInfo().then(info=>{$('launcherStoragePath').textContent=info.path;}).catch(error=>{$('launcherStoragePath').textContent=error.message;});
$('openLauncherStorage').addEventListener('click',()=>window.modLauncher.openStorage().catch(error=>toast(error.message,true)));
$('rescanResolver').addEventListener('click', async () => {
  const button = $('rescanResolver');
  button.disabled = true;
  button.textContent = 'Scanning…';
  renderProviderLayout({ status: 'scanning', message: 'Resolving and validating the local executable…', report: null });
  try {
    const result = await window.modLauncher.rescanResolver();
    renderProviderLayout(result);
    toast(result.status === 'ready' ? 'Current game build resolved and cached.' : result.message, result.status !== 'ready');
  } catch (error) {
    renderProviderLayout({ status: 'error', message: error.message, report: null });
    toast(error.message, true);
  } finally {
    button.disabled = false;
    button.textContent = 'Rescan executable';
  }
});
$('copyResolver').addEventListener('click', async () => {
  try {
    await window.modLauncher.copyResolver();
    toast('Resolver report copied.');
  } catch (error) { toast(error.message, true); }
});
$('testProvider').addEventListener('click', async () => {
  const button = $('testProvider');
  const output = $('providerDiagnostic');
  button.disabled = true;
  button.textContent = 'Testing…';
  output.className = '';
  output.textContent = 'Resolving the file and running a private IPC round trip…';
  try {
    const result = await window.modLauncher.diagnoseProvider($('providerTestPath').value.trim());
    output.className = 'success';
    output.textContent = `PASS · ${result.resolver.logicalPath} · ${formatBytes(result.channel.fullBytesRead)} streamed in ${result.channel.readCalls} reads · user-only ${result.channel.security.currentUserOnly ? 'YES' : 'NO'} · bad token rejected ${result.channel.security.invalidTokenRejected ? 'YES' : 'NO'} · handle closed.`;
  } catch (error) {
    output.className = 'failure';
    output.textContent = `FAIL · ${error.message}`;
  } finally {
    button.disabled = false;
    button.textContent = 'Test provider core';
  }
});
$('auditBridge').addEventListener('click', async () => {
  const button = $('auditBridge');
  const output = $('bridgeDiagnostic');
  button.disabled = true;
  button.textContent = 'Auditing…';
  output.className = '';
  output.textContent = 'Hashing the game executable, reading the stock manifest, and checking file-device lifecycle plus the development-manifest gate…';
  try {
    const result = await window.modLauncher.auditBridge($('providerTestPath').value.trim());
    const build = result.findings.exactBuildVerified ? 'exact build YES' : 'exact build NO';
    const manifest = result.findings.stockManifestKnowsAsset ? 'stock manifest LISTED' : 'stock manifest ABSENT';
    const loose = result.findings.looseAssetPresent ? 'loose asset PRESENT' : 'loose asset MISSING';
    const receiver = result.findings.exactBuildHasPriorLiveReceiverEvidence ? 'live receiver evidence YES' : 'live receiver evidence NO';
    const deviceBoundary = result.findings.resourceDeviceBoundaryVerified ? 'file-device boundary VERIFIED' : 'file-device boundary NOT VERIFIED';
    const lifecycle = result.findings.managerLifecycleMapped ? 'register/unregister lifecycle MAPPED' : 'register/unregister lifecycle NOT MAPPED';
    const stockLifetime = result.findings.stockVtableFullyMapped && result.findings.stockAllocationBoundaryMapped
      ? 'stock allocation/vtable boundary MAPPED'
      : 'stock allocation/vtable boundary NOT MAPPED';
    const development = result.findings.developmentManifestBoundaryVerified
      ? (result.findings.stockDevelopmentLooseModRouteUsable ? 'development route USABLE' : 'development route FALLBACK-ONLY')
      : 'development route NOT VERIFIED';
    const delegation = result.findings.offlineDelegationPrototypeReady ? 'offline mod→stock delegation PASS' : 'offline delegation FAIL';
    const nativeHarness = result.findings.nativeAbiHarnessReady ? 'native ABI/rollback harness PASS' : 'native ABI harness FAIL';
    const crossModule = result.findings.crossModuleLifetimeReady ? 'cross-module lifetime PASS' : 'cross-module lifetime FAIL';
    const lifecycleAddon = result.findings.observeOnlyLifecycleAddonVerified ? 'lifecycle add-on VERIFIED' : 'lifecycle add-on NOT VERIFIED';
    const registrationProbe = result.findings.managerOwnedStreamProbeAvailable ? 'manager-owned stream probe READY' : 'manager-owned stream probe NOT READY';
    output.className = result.findings.stockDataOnlyRouteProven ? 'success' : 'warning';
    output.textContent = `READ-ONLY RESULT · ${build} · ${manifest} · ${loose} · ${receiver} · ${deviceBoundary} · ${lifecycle} · ${stockLifetime} · ${development} · ${delegation} · ${nativeHarness} · ${crossModule} · ${lifecycleAddon} · ${registrationProbe} · live registration result PENDING · ${result.findings.cleanRoomGameBridgeReady ? 'clean-room bridge READY' : 'clean-room game bridge PENDING'}.`;
  } catch (error) {
    output.className = 'failure';
    output.textContent = `FAIL · ${error.message}`;
  } finally {
    button.disabled = false;
    button.textContent = 'Run bridge audit';
  }
});
$('armProbe').addEventListener('click', async () => {
  try {
    const registrationProbe = $('registrationProbe').checked;
    renderProbe(await window.modLauncher.startProbe({
      registrationProbe,
      logicalPath: $('providerTestPath').value.trim(),
    }));
    toast(registrationProbe ? 'One-time default delegation test armed.' : 'Observe-only probe armed.');
  } catch (error) { toast(error.message, true); }
});
$('disarmProbe').addEventListener('click', async () => {
  try {
    renderProbe(await window.modLauncher.stopProbe());
    toast('Observe-only probe disarmed.');
  } catch (error) { toast(error.message, true); }
});
$('activateLiveRoute').addEventListener('click', async () => {
  const button = $('activateLiveRoute');
  const current = state.probe;
  const lifecycle = current?.lifecycle;
  let blocked = '';
  if (!current?.armed) blocked = 'Click Arm probe first.';
  else if (!lifecycle?.active) blocked = 'Wait until the game handshake says LIVE.';
  else if (current.registrationProbeArmed) {
    blocked = 'The older path-aware test is armed. Restart with that checkbox unchecked.';
  } else if (current.liveRouteArmed) blocked = 'The local fallback is already active.';
  else if (lifecycle.liveRoute?.passed) blocked = 'This fallback session already ended. Restart the probe for another session.';
  else if (lifecycle.liveRouteFailure) blocked = 'This fallback session ended with an error. Restart the probe to retry.';
  if (blocked) {
    const output = $('probeDiagnostic');
    output.className = 'failure';
    output.textContent = `ROUTE NOT ARMED · ${blocked}`;
    toast(blocked, true);
    return;
  }
  state.routePreparing = true;
  renderProbe(current);
  const output = $('probeDiagnostic');
  output.className = '';
  output.textContent = 'STARTING LOCAL FALLBACK · indexing every exact file path under mods and starting the current-user-only provider…';
  try {
    const updated = await window.modLauncher.activateLiveRoute();
    state.routePreparing = false;
    renderProbe(updated);
    const routeState = updated.lifecycle?.liveRouteState;
    if (updated.lifecycle?.liveRoute?.passed) {
      toast('Local fallback stopped cleanly.');
    } else if (routeState?.registrationActive && routeState?.defaultActive) {
      toast('Local fallback active for this game session.');
    } else {
      toast('Local fallback armed and waiting for the next game heartbeat.');
    }
  } catch (error) {
    state.routePreparing = false;
    renderProbe(state.probe);
    const failure = $('probeDiagnostic');
    failure.className = 'failure';
    failure.textContent = `ROUTE NOT ARMED · ${error.message}`;
    toast(error.message, true);
  }
});
$('openProbe').addEventListener('click', () => window.modLauncher.openProbe().catch((error) => toast(error.message, true)));
$('startSession').addEventListener('click', async () => {
  if(state.homeBusy||state.homeLoading||!state.validation?.ready||state.homeSetupBusy) return;
  const button = $('startSession');
  button.disabled = true;
  showLaunchOverlay('Checking the current game build and selected profile…');
  try {
    const result = await window.modLauncher.launchGameWithoutEac();
    toast(result.alreadyRunning ? 'NBA 2K27 is already running.' : `NBA 2K27 launched without EAC (Steam App ${result.appId}, ${result.launchType}).`);
  }
  catch (error) { hideLaunchOverlay(); toast(error.message, true); }
  finally { renderHomeSetup(); }
});
$('stopSession').addEventListener('click', async () => renderSession(await window.modLauncher.stopSession()));
window.modLauncher.onSessionStatus(renderSession);
window.modLauncher.onLoaderStatus(renderLoader);
window.modLauncher.onProbeStatus(renderProbe);
window.modLauncher.onCameraStatus(renderCameraStatus);

(async () => {
  try {
    if (rosterWorkspace) {
      const data = await window.modLauncher.rosterBootstrap();
      $('version').textContent = `v${data.version}`;
      window.refreshLauncherBuild?.();
      renderPlayerResearch(data.playerResearch);
      const section = new URLSearchParams(location.search).get('section') === 'teams' ? 'teams' : 'players';
      document.querySelectorAll('.page').forEach(page=>page.classList.toggle('active',page.id===`page-${section}`));
      document.querySelectorAll('.nav-item').forEach(item=>item.classList.toggle('active',item.dataset.page===section));
      $('pageTitle').textContent=section === 'teams' ? 'Team Editor' : 'Player Editor';
      return;
    }
    const data = await window.modLauncher.bootstrap();
    state.settings = data.settings;
    state.validation = data.validation;
    state.providers = data.providers;
    state.loader = data.loader;
    $('version').textContent = `v${data.version}`;
    window.refreshLauncherBuild?.();
    fillForm();
    renderProvider();
    renderValidation();
    renderSession(data.session);
    renderProbe(data.probe);
    renderCameras(data.cameras);
    renderJukebox(data.jukebox);
    renderPresentation(data.presentation);
    renderPlayerResearch(data.playerResearch);
    renderProviderLayout(data.providerLayout);
    $('appStartupMessage').textContent = 'Ready. Scanning your mod library in the background…';
    state.homeCatalogPromise=scanModCatalog({ startup: true });
  } catch (error) { state.homeSetupError=error.message;renderHomeSetup();toast(`Startup needs attention: ${error.message}`, true); }
  finally { $('appStartup').classList.add('hidden'); }
})();

// Help is passive; diagnostics navigation does not start a game or native action.
$('launcherHelp').addEventListener('click',()=>{$('launcherHelpDialog').returnValue='';$('launcherHelpDialog').showModal();});
$('closeLauncherHelp').addEventListener('click',()=>$('launcherHelpDialog').close());
$('launcherHelpDialog').addEventListener('close',()=>{if($('launcherHelpDialog').returnValue!=='diagnostics')$('launcherHelp').focus();});
$('openSessionDiagnostics').addEventListener('click',()=>{
  $('launcherHelpDialog').close('diagnostics');$('developerNav').click();
  document.querySelector('#page-developer .session-diagnostics').open=true;
  const summary=document.querySelector('#page-developer .session-diagnostics summary');summary.tabIndex=0;summary.focus();
});
