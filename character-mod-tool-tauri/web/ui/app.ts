const TEMPLATE_STORAGE_KEY = 'characterModTargetTemplatesV1';
const NATIVE_HAIR_FAVORITES_STORAGE_KEY = 'characterModNativeHairFavoritesV1';
const NATIVE_HAIR_CAMERA_STORAGE_KEY = 'characterModNativeHairCameraV1';
const MANIFEST_BATCH_SIZE = 250;
const NATIVE_HAIR_ASSET_CACHE_LIMIT = 12;
const NATIVE_HAIR_HEAD_CACHE_LIMIT = 4;
const TEMPLATE_SLOTS = [
  ['clean', 'Clean', 'No hair, facial hair, or headband'],
  ['hair', 'Hair', 'Hair only'],
  ['facial-hair', 'Facial Hair', 'Facial hair only'],
  ['hair-facial', 'Hair + Facial Hair', 'Hair and facial hair'],
  ['headband', 'Headband', 'Headband only'],
  ['headband-hair', 'Headband + Hair', 'Headband and hair'],
  ['headband-facial', 'Headband + Facial Hair', 'Headband and facial hair'],
  ['full-accessories', 'Headband + Hair + Facial Hair', 'Headband, hair, and facial hair'],
].map(([key, label, description]) => ({ key, label, description }));

const state = {
  bootstrap: null,
  activeView: 'player',
  playerWorkflow: 'player',
  viewRequestId: 0,
  engineLabel: 'Connecting to conversion engine...',
  cli: {
    game: 'NBA2K27',
    player: null,
    language: '',
  },
  health: {
    loading: false,
    report: null,
  },
  source: null,
  target: null,
  workingSource: null,
  workingBaseline: null,
  workingPlayer: null,
  workingRequestId: 0,
  workingGeneration: 0,
  outputDir: '',
  activeJob: '',
  startingJob: false,
  selectionBusy: false,
  stagePlayer: { busy: false, requestId: 0 },
  resetting: false,
  pendingTerminalEvent: null,
  lastOutput: '',
  manifestRole: 'source',
  manifestGame: 'NBA 2K27',
  manifestGames: {
    source: 'NBA 2K27',
    target: 'NBA 2K27',
    template: 'NBA 2K27',
  },
  templateAssignmentSlot: '',
  templates: {},
  catalog: [],
  selectedManifest: null,
  manifestVisibleLimit: MANIFEST_BATCH_SIZE,
  hair: {
    sourcePath: '',
    generation: '',
    sourceKey: '',
    candidates: [],
    targetGame: '',
    slots: [],
    loading: false,
    requestId: 0,
  },
  facialHair: {
    manualChoice: false,
    sourcePath: '',
    generation: '',
    sourceKey: '',
    candidates: [],
    slots: [],
    targetGame: '',
    route: '',
    message: '',
    loading: false,
    requestId: 0,
  },
  body: {
    sourcePlayer: null,
    targetPlayer: null,
    fallbackSource: null,
    targetRole: 'Selected Target',
    sourceBodyFit: {},
    targetBodyFit: {},
    targetDraft: {},
    dirtyFields: new Set<string>(),
    invalidFields: new Set<string>(),
    loading: false,
    requestId: 0,
    operation: '',
  },
  headband: {
    sourceHeadbands: [],
    configs: [],
    styles: [],
    loading: false,
    applying: false,
    requestId: 0,
  },
  preview: {
    loading: false,
    blenderBusy: false,
    blenderSessionId: '',
    requestId: 0,
    editRequestId: 0,
    loadedPath: '',
    dirty: true,
    wireframe: false,
    jersey: false,
    jerseyAvailable: false,
    chestCorrectionBusy: false,
    closeSeamsBusy: false,
    blenderHair: false,
    blenderFacialHair: false,
    blenderTextures: false,
    bodyFit: {},
    bodyFitTarget: '',
    bodyFitDraft: {},
    bodyFitDirtyFields: new Set<string>(),
    bodyFitInvalidInputs: new Map<string, string>(),
    bodyFitBusy: false,
    bodyFitOpen: false,
  },
  nativeHair: {
    assetType: 'hair' as import('../contracts').AssetType,
    assets: [],
    selectedId: '',
    slots: [],
    loading: false,
    catalogLoaded: false,
    catalogSourceKey: '',
    catalogRevision: '',
    catalogRequest: null as { assetType: import('../contracts').AssetType; sourceKey: string; promise: Promise<void> } | null,
    targetRequestId: 0,
    targetLoading: false,
    targetContext: '',
    previewRequestId: 0,
    activePreviewKey: '',
    displayedPreviewKey: '',
    baseRequestId: 0,
    baseLoadedPath: '',
    basePreview: null,
    previewCache: new Map(),
    headCache: new Map(),
    cameraState: null,
    favorites: { hair: new Set(), facialhair: new Set() },
    favoritesOnly: false,
  },
};

function $<K extends keyof UIElements>(id: K): UIElements[K];
function $(id: string): HTMLElement;
function $(id: string): HTMLElement { return document.getElementById(id); }
const rendererLoads = new Map();

function ensureRenderer(globalName, query) {
  if (window[globalName]) return Promise.resolve(window[globalName]);
  if (rendererLoads.has(globalName)) return rendererLoads.get(globalName);
  const loading = new Promise((resolve, reject) => {
    const script = document.createElement('script');
    script.type = 'module';
    script.src = `model-preview.js?${query}`;
    script.addEventListener('load', () => {
      if (window[globalName]) resolve(window[globalName]);
      else reject(new Error(`${globalName} did not initialize.`));
    }, { once: true });
    script.addEventListener('error', () => {
      reject(new Error(`Could not load ${globalName}.`));
    }, { once: true });
    document.head.append(script);
  }).catch((error) => {
    rendererLoads.delete(globalName);
    throw error;
  });
  rendererLoads.set(globalName, loading);
  return loading;
}

function ensureModelPreviewRenderer() {
  return ensureRenderer('modelPreview', 'canvas=previewCanvas&global=modelPreview');
}

function ensureHairPreviewRenderers() {
  return Promise.all([
    ensureRenderer('hairBasePreview', 'canvas=nativeHairBaseCanvas&global=hairBasePreview&broadcast=hairCatalog&picker=1'),
    ensureRenderer('hairAssetPreview', 'canvas=nativeHairPreviewCanvas&global=hairAssetPreview&transparent=1&interactive=0&follow=hairCatalog&picker=1'),
  ]);
}

function finishStartupLoading() {
  const loading = $('startupLoading');
  if (!loading) return;
  loading.classList.add('finished');
  setTimeout(() => { loading.hidden = true; }, 180);
}

function basename(filePath) {
  return String(filePath || '').split(/[\\/]/).filter(Boolean).pop() || '';
}

function formatBytes(bytes) {
  const value = Number(bytes) || 0;
  if (value >= 1024 ** 2) return `${(value / 1024 ** 2).toFixed(1)} MB`;
  if (value >= 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${value} B`;
}

function nowLabel() {
  return new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

function loadNativeHairFavorites() {
  try {
    const stored = JSON.parse(localStorage.getItem(NATIVE_HAIR_FAVORITES_STORAGE_KEY) || '{}');
    state.nativeHair.favorites.hair = new Set(Array.isArray(stored.hair) ? stored.hair : []);
    state.nativeHair.favorites.facialhair = new Set(Array.isArray(stored.facialhair) ? stored.facialhair : []);
  } catch {
    state.nativeHair.favorites = { hair: new Set(), facialhair: new Set() };
  }
}

function saveNativeHairFavorites() {
  localStorage.setItem(NATIVE_HAIR_FAVORITES_STORAGE_KEY, JSON.stringify({
    hair: [...state.nativeHair.favorites.hair],
    facialhair: [...state.nativeHair.favorites.facialhair],
  }));
}

function loadNativeHairCamera() {
  try {
    const saved = JSON.parse(localStorage.getItem(NATIVE_HAIR_CAMERA_STORAGE_KEY) || 'null');
    state.nativeHair.cameraState = saved && Array.isArray(saved.target) ? saved : null;
  } catch {
    state.nativeHair.cameraState = null;
  }
}

function saveNativeHairCamera(camera) {
  if (!camera || !Array.isArray(camera.target)) return;
  state.nativeHair.cameraState = camera;
  localStorage.setItem(NATIVE_HAIR_CAMERA_STORAGE_KEY, JSON.stringify(camera));
}

function nativeHairFavoriteKey(asset) {
  return String(asset?.id || `${asset?.sourcePng || ''}:${asset?.key || ''}`);
}

function currentNativeHairFavorites() {
  return state.nativeHair.favorites[state.nativeHair.assetType];
}

function renderNativeHairFavoritesFilter() {
  const button = $('nativeHairFavoritesOnly');
  button.classList.toggle('active', state.nativeHair.favoritesOnly);
  button.setAttribute('aria-pressed', String(state.nativeHair.favoritesOnly));
  button.title = state.nativeHair.favoritesOnly ? 'Show all assets' : 'Show favorites only';
  button.querySelector('span').textContent = state.nativeHair.favoritesOnly ? '\u2605' : '\u2606';
}

function toggleNativeHairFavorite(asset) {
  const favorites = currentNativeHairFavorites();
  const key = nativeHairFavoriteKey(asset);
  if (favorites.has(key)) favorites.delete(key);
  else favorites.add(key);
  saveNativeHairFavorites();
  renderNativeHairRows();
}

function addLog(message, kind = 'info') {
  const text = String(message || '').trim();
  if (!text) return;
  const entry = document.createElement('div');
  entry.className = `log-entry ${kind}`;
  const strong = document.createElement('strong');
  strong.textContent = text;
  const stamp = document.createElement('span');
  stamp.textContent = nowLabel();
  entry.append(strong, stamp);
  $('activityLog').prepend(entry);
}

function setProgress(title, mode = 'idle', value = '') {
  $('progressTitle').textContent = title;
  $('progressValue').textContent = value;
  $('jobProgress').className = `job-progress ${mode}`;
  if (mode === 'complete') $('progressFill').style.width = '100%';
  else if (mode !== 'running') $('progressFill').style.width = '0';
}

function healthStatusLabel(status, label = '') {
  if (status === 'info' && ['Info', 'Unverified'].includes(label)) return label;
  return {
    pass: 'Passed',
    warning: 'Warning',
    error: 'Error',
    info: 'Optional',
  }[status] || 'Info';
}

function renderHealthReport() {
  const report = state.health.report;
  const container = $('healthGroups');
  container.replaceChildren();
  $('healthLoading').hidden = !state.health.loading;
  $('runHealthCheck').disabled = state.health.loading;
  $('runHealthCheck').textContent = state.health.loading ? 'Checking...' : 'Run Health Check';
  if (!report) {
    const empty = document.createElement('div');
    empty.className = 'health-empty';
    empty.textContent = state.health.loading ? 'Reading local components...' : 'No health report has been run.';
    container.append(empty);
    return;
  }

  const counts = report.counts || {};
  const overall = report.overall || 'info';
  $('healthSummary').dataset.status = overall;
  $('healthSummaryMark').textContent = overall === 'pass' ? '\u2713' : overall === 'warning' ? '!' : '\u00d7';
  $('healthOverall').textContent = overall === 'pass' ? 'Healthy' : overall === 'warning' ? 'Ready with warnings' : 'Needs attention';
  $('healthSummaryText').textContent = overall === 'pass'
    ? 'All required local components passed.'
    : overall === 'warning'
      ? 'Required components work, but one or more items should be reviewed.'
      : 'One or more required components need repair.';
  $('healthPassCount').textContent = counts.pass || 0;
  $('healthWarningCount').textContent = counts.warning || 0;
  $('healthErrorCount').textContent = counts.error || 0;
  const generated = new Date(report.generatedAt);
  $('healthLastRun').textContent = Number.isNaN(generated.getTime())
    ? 'Check complete'
    : `Last checked ${generated.toLocaleString()}`;

  for (const group of report.groups || []) {
    const section = document.createElement('section');
    section.className = 'health-group';
    const heading = document.createElement('header');
    const title = document.createElement('h3');
    title.textContent = group.label || 'Checks';
    const tally = document.createElement('span');
    tally.textContent = `${(group.checks || []).length} checks`;
    heading.append(title, tally);
    section.append(heading);
    for (const check of group.checks || []) {
      const row = document.createElement('div');
      row.className = `health-check ${check.status || 'info'}`;
      const marker = document.createElement('span');
      marker.className = 'health-check-marker';
      marker.textContent = check.status === 'pass' ? '\u2713' : check.status === 'warning' ? '!' : check.status === 'error' ? '\u00d7' : 'i';
      const copy = document.createElement('div');
      copy.className = 'health-check-copy';
      const label = document.createElement('strong');
      label.textContent = check.label || 'Check';
      const message = document.createElement('span');
      message.textContent = check.message || '';
      copy.append(label, message);
      if (check.path) {
        const checkPath = document.createElement('small');
        checkPath.textContent = check.path;
        checkPath.title = check.path;
        copy.append(checkPath);
      }
      const status = document.createElement('span');
      status.className = 'health-check-status';
      status.textContent = healthStatusLabel(check.status, check.statusLabel);
      row.append(marker, copy, status);
      if (check.repair?.action) {
        const action = document.createElement('button');
        action.type = 'button';
        action.className = 'button secondary health-repair';
        action.textContent = check.repair.label || 'Repair';
        action.disabled = state.health.loading;
        action.addEventListener('click', () => repairHealthCheck(check));
        row.append(action);
      }
      section.append(row);
    }
    container.append(section);
  }
}

function applyHealthBootstrap(bootstrap) {
  if (!bootstrap?.settings) return;
  state.bootstrap = { ...state.bootstrap, ...bootstrap };
  state.outputDir = bootstrap.settings.output_dir || '';
  fillSettings(bootstrap.settings);
  $('outputPath').textContent = state.outputDir || 'Not configured';
  $('game26Dot').classList.toggle('ready', Boolean(bootstrap.games?.['NBA 2K26']));
  $('game27Dot').classList.toggle('ready', Boolean(bootstrap.games?.['NBA 2K27']));
}

async function runHealthCheck() {
  if (state.health.loading) return;
  state.health.loading = true;
  renderHealthReport();
  try {
    state.health.report = await window.characterTool.runHealthCheck();
    addLog('Local health check completed.', state.health.report.overall === 'error' ? 'error' : 'success');
  } catch (error) {
    state.health.report = null;
    $('healthOverall').textContent = 'Health check failed';
    $('healthSummaryText').textContent = error.message;
    $('healthSummary').dataset.status = 'error';
    addLog(`Health check: ${error.message}`, 'error');
  } finally {
    state.health.loading = false;
    renderHealthReport();
  }
}

async function repairHealthCheck(check) {
  const repair = check?.repair || {};
  if (repair.action === 'settings') {
    $('settingsDialog').showModal();
    return;
  }
  if (repair.action === 'open-path') {
    await window.characterTool.openPath(repair.path || check.path);
    return;
  }
  if (state.health.loading) return;
  state.health.loading = true;
  renderHealthReport();
  try {
    const result = await window.characterTool.repairHealth({ action: repair.action });
    state.health.report = result.report;
    applyHealthBootstrap(result.bootstrap);
    addLog(`${repair.label || check.label} complete.`, 'success');
  } catch (error) {
    addLog(`Health repair: ${error.message}`, 'error');
  } finally {
    state.health.loading = false;
    renderHealthReport();
  }
}

function setActiveView(view) {
  const requestId = ++state.viewRequestId;
  state.activeView = view;
  if (view === 'player' || view === 'full-swap') state.playerWorkflow = view;
  const isCurrent = () => requestId === state.viewRequestId && state.activeView === view;
  const titles = {
    player: ['Player', 'NBA 2K27'],
    configs: ['Configs', 'Staged player configurations'],
    'full-swap': ['Player', state.engineLabel],
    body: ['Body', 'Selected source and target body-fit values'],
    hair: ['Hair', 'NBA 2K27 manifest hair and facial hair'],
    headband: ['Headbands', 'Transfer or add a fitted headband'],
    preview: ['Preview', 'Interactive staged-player render'],
    cli: ['CLI', 'NBA 2K27 selected-player reader'],
    health: ['Health', 'Local installation and cache diagnostics'],
  };
  $('fullSwapView').hidden = view !== 'full-swap';
  $('selectPlayerView').hidden = view !== 'player';
  $('selectPlayerTab').classList.toggle('active', view === 'player');
  $('selectPlayerTab').setAttribute('aria-pressed', String(view === 'player'));
  $('fullSwapChoice').classList.toggle('active', view === 'full-swap');
  $('fullSwapChoice').setAttribute('aria-pressed', String(view === 'full-swap'));
  $('playerChoices').hidden = view !== 'player' && view !== 'full-swap';
  $('stagePlayer').hidden = view === 'player' || view === 'full-swap';
  $('configsView').hidden = view !== 'configs';
  $('configsTab').classList.toggle('active', view === 'configs');
  $('bodyView').hidden = view !== 'body';
  $('hairView').hidden = view !== 'hair';
  $('headbandView').hidden = view !== 'headband';
  $('previewView').hidden = view !== 'preview';
  $('cliView').hidden = view !== 'cli';
  $('healthView').hidden = view !== 'health';
  $('fullSwapTab').classList.toggle('active', view === 'player' || view === 'full-swap');
  $('bodyTab').classList.toggle('active', view === 'body');
  $('hairTab').classList.toggle('active', view === 'hair');
  $('headbandTab').classList.toggle('active', view === 'headband');
  $('previewTab').classList.toggle('active', view === 'preview');
  $('cliTab').classList.toggle('active', view === 'cli');
  $('healthTab').classList.toggle('active', view === 'health');
  $('workspaceTitle').textContent = titles[view][0];
  $('engineStatus').textContent = titles[view][1];
  $('newSwap').hidden = view !== 'full-swap';
  $('openOutput').hidden = view === 'cli' || view === 'health';
  if (view === 'body') refreshBodyFit({ preserveDraft: true });
  if (view === 'configs') window.configsEditor?.refresh();
  if (view === 'hair') {
    ensureHairPreviewRenderers()
      .then(() => ensureNativeHairReady())
      .catch((error) => addLog(`Hair preview: ${error.message}`, 'error'));
  }
  if (view === 'headband') refreshHeadbandOptions();
  if (view === 'preview') {
    if (!window.modelPreview) setPreviewStatus('Loading the local preview renderer...');
    ensureModelPreviewRenderer()
      .then(() => {
        if (!isCurrent()) return;
        requestAnimationFrame(() => { if (isCurrent()) window.modelPreview?.resize(); });
        return loadStagedPreview();
      })
      .catch((error) => { if (isCurrent()) setPreviewStatus(error.message, 'error'); });
  }
  if (view === 'cli') loadRosterLanguage();
  if (view === 'health' && !state.health.report && !state.health.loading) runHealthCheck();
}

function rosterPlayerName(player) {
  return [player?.fields?.FirstName, player?.fields?.LastName].filter(Boolean).join(' ');
}

function renderRosterPlayer(player) {
  const fields = player?.fields || {};
  const display = player?.display || {};
  $('cliCyberfaceId').textContent = display.CyberfaceID || '----';
  $('cliFirstName').textContent = display.FirstName || '--';
  $('cliLastName').textContent = display.LastName || '--';
  $('cliPlayerKey').textContent = display.PlayerKey || '--';
  $('cliHeadshotId').textContent = display.HeadshotID || '--';
  $('cliPortraitId').textContent = display.PortraitID || '--';
  $('cliNickname').textContent = display.Nickname || '--';
  $('cliJerseyNickname').textContent = display.JerseyNickname || '--';
  $('cliPlayerSummary').textContent = player
    ? [rosterPlayerName(player), fields.PlayerKey, player.game].filter(Boolean).join(' | ')
    : 'No player read';
}

function syncRosterEditValue() {
  const field = $('cliEditField').value;
  $('cliEditValue').value = state.cli.player?.fields?.[field] || '';
  $('cliEditValue').placeholder = field === 'CyberfaceID' ? '01759' : `New ${$('cliEditField').selectedOptions[0].textContent}`;
  updateRosterFieldAction();
}

function updateRosterFieldAction() {
  const field = $('cliEditField').value;
  const value = $('cliEditValue').value.trim();
  const current = state.cli.player?.fields?.[field];
  const numeric = ['CyberfaceID', 'HeadshotID', 'PortraitID'].includes(field);
  const valid = value.length > 0 && (!numeric || (/^\d+$/.test(value) && Number(value) <= 65535));
  const changed = numeric ? Number(value) !== Number(current) : value !== current;
  $('applyRosterField').disabled = !state.cli.player || !valid || !changed;
}

async function readRosterPlayer() {
  const button = $('readRosterPlayer');
  const game = $('cliGame').value;
  button.disabled = true;
  button.textContent = 'Reading...';
  $('cliGame').disabled = true;
  $('cliState').textContent = 'Reading';
  $('cliStatus').className = '';
  $('cliStatus').textContent = 'Reading the selected player...';
  try {
    const result = await window.characterTool.readRosterPlayer({ game });
    state.cli.game = game;
    state.cli.player = result;
    renderRosterPlayer(result);
    syncRosterEditValue();
    $('cliState').textContent = 'Read';
    $('cliState').classList.add('ready');
    $('cliStatus').className = 'success';
    $('cliStatus').textContent = `${rosterPlayerName(result) || 'Selected player'} | ${result.display.CyberfaceID}`;
  } catch (error) {
    state.cli.player = null;
    renderRosterPlayer(null);
    $('cliState').textContent = 'Unavailable';
    $('cliState').classList.remove('ready');
    $('cliStatus').className = 'error';
    $('cliStatus').textContent = error.message;
  } finally {
    button.disabled = false;
    $('cliGame').disabled = false;
    button.textContent = 'Read Selected Player';
    updateRosterFieldAction();
  }
}

async function applyRosterField() {
  const player = state.cli.player;
  const field = $('cliEditField').value;
  const value = $('cliEditValue').value.trim();
  if (!player || !value) return;
  const readButton = $('readRosterPlayer');
  const assignButton = $('applyRosterField');
  readButton.disabled = true;
  $('cliGame').disabled = true;
  $('cliEditField').disabled = true;
  $('cliEditValue').disabled = true;
  assignButton.disabled = true;
  assignButton.textContent = 'Applying...';
  $('cliState').textContent = 'Writing';
  $('cliStatus').className = '';
  $('cliStatus').textContent = `Verifying and updating ${field}...`;
  try {
    const result = await window.characterTool.setRosterPlayerField({
      game: state.cli.game,
      field,
      value,
      expectedIdentity: player.identity,
      expectedValue: player.fields[field],
    });
    state.cli.player = result;
    renderRosterPlayer(result);
    syncRosterEditValue();
    $('cliState').textContent = 'Assigned';
    $('cliState').classList.add('ready');
    $('cliStatus').className = 'success';
    $('cliStatus').textContent = `${field}: ${result.previousDisplay} -> ${result.valueDisplay} | Readback verified`;
  } catch (error) {
    $('cliState').textContent = 'Not assigned';
    $('cliState').classList.remove('ready');
    $('cliStatus').className = 'error';
    $('cliStatus').textContent = error.message;
  } finally {
    readButton.disabled = false;
    $('cliGame').disabled = false;
    $('cliEditField').disabled = false;
    $('cliEditValue').disabled = false;
    assignButton.textContent = 'Apply Field';
    updateRosterFieldAction();
  }
}

async function loadRosterLanguage() {
  try {
    const result = await window.characterTool.getRosterLanguage();
    state.cli.language = result.language;
    $('cliLanguage').value = result.language;
  } catch {
    // Player editing remains available when the optional language read fails.
  }
}

async function saveRosterLanguage() {
  const button = $('saveCliLanguage');
  button.disabled = true;
  button.textContent = 'Saving...';
  try {
    const result = await window.characterTool.setRosterLanguage({ language: $('cliLanguage').value });
    state.cli.language = result.language;
    $('cliStatus').className = 'success';
    $('cliStatus').textContent = `Roster CLI language: ${result.language}`;
  } catch (error) {
    $('cliStatus').className = 'error';
    $('cliStatus').textContent = error.message;
  } finally {
    button.disabled = false;
    button.textContent = 'Save Language';
  }
}

function renderCharacter(role) {
  const data = state[role];
  const surface = $(`${role}Surface`);
  const badge = $(`${role}State`);
  if (!data) {
    surface.classList.add('empty');
    badge.classList.remove('ready');
    badge.textContent = 'Not selected';
    $(`${role}Name`).textContent = role === 'source'
      ? 'Choose a character package'
      : 'Choose a target player slot';
    $(`${role}Path`).textContent = role === 'source'
      ? 'IFF, ZIP or RAR'
      : 'NBA 2K23, NBA 2K26 or NBA 2K27';
    return;
  }
  surface.classList.remove('empty');
  badge.classList.add('ready');
  badge.textContent = 'Ready';
  $(`${role}Name`).textContent = data.name || basename(data.path);
  $(`${role}Path`).textContent = `${data.path} | ${data.entries || 0} entries | ${formatBytes(data.size)}`;
  updateReadiness();
}

function selectedHairSlot() {
  return state.hair.slots.find((slot) => slot.key === $('hairSlot').value) || null;
}

function setHairStatus(message, kind = '') {
  $('hairStatus').textContent = message;
  $('hairStatus').className = `hair-status ${kind}`.trim();
}

function renderHairControls() {
  const charactersReady = Boolean(state.source && state.target);
  const sourceReady = Boolean(state.hair.sourcePath);
  const slotsReady = state.hair.slots.length > 0;
  const enabled = $('includeHair').checked;
  $('includeHair').disabled = !charactersReady || state.hair.loading || !sourceReady || !slotsReady;
  $('detectHair').disabled = !charactersReady || state.hair.loading;
  $('browseHair').disabled = !charactersReady || state.hair.loading;
  $('hairSource').disabled = !charactersReady || state.hair.loading || !state.hair.candidates.length;
  if (state.hair.sourcePath) $('hairSource').value = state.hair.sourcePath;
  $('hairSource').title = state.hair.sourcePath;
  $('hairSlot').disabled = !charactersReady || state.hair.loading || !enabled || !slotsReady;
}

async function refreshHairOptions({ explicitSource = '', autoEnable = false } = {}) {
  const requestId = ++state.hair.requestId;
  if (!state.source || !state.target) {
    state.hair.sourcePath = '';
    state.hair.generation = '';
    state.hair.sourceKey = '';
    state.hair.candidates = [];
    state.hair.slots = [];
    $('includeHair').checked = false;
    $('hairSource').replaceChildren(new Option('No source hair detected', ''));
    $('hairSlot').replaceChildren(new Option('No target slots loaded', ''));
    setHairStatus('Select a source and target character to inspect hair.');
    renderHairControls();
    updateReadiness();
    return;
  }

  if (state.target.gameLabel === 'NBA 2K23') {
    state.hair.sourcePath = '';
    state.hair.generation = '';
    state.hair.sourceKey = '';
    state.hair.candidates = [];
    state.hair.slots = [];
    state.hair.targetGame = 'NBA 2K23';
    $('includeHair').checked = false;
    $('hairSource').replaceChildren(new Option('Hair transfer not included in this route', ''));
    $('hairSlot').replaceChildren(new Option('Uses the NBA 2K23 target hair', ''));
    setHairStatus('NBA 2K18 to NBA 2K23 currently preserves the target hair. Hair conversion will be added after the player route is verified.', 'warning');
    renderHairControls();
    updateReadiness();
    return;
  }

  const previousSource = explicitSource || state.hair.sourcePath;
  const previousSlot = $('hairSlot').value;
  state.hair.loading = true;
  setHairStatus('Inspecting source hair and target appearance slots...');
  renderHairControls();
  updateReadiness();
  try {
    const result = await window.characterTool.loadHairOptions({
      source: state.source.path,
      target: state.target.path,
      sourceGame: state.source.gameLabel || '',
      targetGame: state.target.gameLabel || '',
      hairSource: previousSource,
    });
    if (requestId !== state.hair.requestId) return;
    const candidate = result.candidates.find((item) => item.path === previousSource)
      || result.candidates[0]
      || null;
    state.hair.candidates = result.candidates || [];
    state.hair.sourcePath = candidate?.path || '';
    state.hair.generation = candidate?.generation || '';
    state.hair.sourceKey = candidate?.sourceKey || '';
    state.hair.targetGame = result.targetGame || state.target.gameLabel || '';
    state.hair.slots = result.slots || [];

    const sourceSelect = $('hairSource');
    sourceSelect.replaceChildren();
    state.hair.candidates.forEach((item) => {
      sourceSelect.add(new Option(`${item.generation} ${item.sourceKey} | ${item.name}`, item.path));
    });
    if (candidate) sourceSelect.value = candidate.path;
    else sourceSelect.add(new Option('No source hair detected', ''));

    const slotSelect = $('hairSlot');
    slotSelect.replaceChildren();
    state.hair.slots.forEach((slot) => slotSelect.add(new Option(slot.label, slot.key)));
    const selectedSlot = state.hair.slots.find((slot) => slot.key === previousSlot)
      || state.hair.slots.find((slot) => slot.default)
      || state.hair.slots[0];
    if (selectedSlot) slotSelect.value = selectedSlot.key;
    else slotSelect.add(new Option('No hair slots found', ''));

    if (autoEnable && candidate && selectedSlot) $('includeHair').checked = true;
    if (!candidate) {
      $('includeHair').checked = false;
      setHairStatus('No supported 2K18/2K19/2K20/2K23/2K25/2K26 source hair geometry was found beside this character.', 'warning');
    } else if (!selectedSlot) {
      $('includeHair').checked = false;
      setHairStatus('The target appearance contains no selectable hair slot.', 'warning');
    } else {
      setHairStatus(
        `${candidate.generation} ${candidate.sourceKey} -> ${state.hair.targetGame} native slot ${selectedSlot.key}.`,
        'ready',
      );
    }
  } catch (error) {
    if (requestId !== state.hair.requestId) return;
    state.hair.sourcePath = '';
    state.hair.generation = '';
    state.hair.sourceKey = '';
    state.hair.candidates = [];
    state.hair.slots = [];
    $('includeHair').checked = false;
    $('hairSource').replaceChildren(new Option('No source hair detected', ''));
    $('hairSlot').replaceChildren(new Option('No target slots loaded', ''));
    setHairStatus(error.message, 'warning');
    addLog(`Hair inspection: ${error.message}`, 'warning');
  } finally {
    if (requestId === state.hair.requestId) {
      state.hair.loading = false;
      renderHairControls();
      updateReadiness();
    }
  }
}

function refreshHairStatus() {
  const slot = selectedHairSlot();
  if (!state.hair.sourcePath || !slot) return;
  setHairStatus(
    `${state.hair.generation} ${state.hair.sourceKey} -> ${state.hair.targetGame} native slot ${slot.key}.`,
    'ready',
  );
}

function selectedFacialHairCandidate() {
  return state.facialHair.candidates.find((item) => item.path === state.facialHair.sourcePath) || null;
}

function selectedFacialHairSlot() {
  return state.facialHair.slots.find((slot) => slot.key === $('facialHairSlot').value) || null;
}

function facialHairSelectionReady() {
  return !$('includeFacialHair').checked
    || Boolean(selectedFacialHairCandidate() && selectedFacialHairSlot() && !state.facialHair.loading);
}

function setFacialHairStatus(message, kind = '') {
  $('facialHairStatus').textContent = message;
  $('facialHairStatus').className = `hair-status ${kind}`.trim();
}

function renderFacialHairControls() {
  const charactersReady = Boolean(state.source && state.target);
  const busy = state.facialHair.loading || Boolean(state.startingJob || state.activeJob);
  const sourceReady = Boolean(selectedFacialHairCandidate());
  const slotsReady = state.facialHair.slots.length > 0;
  $('includeFacialHair').disabled = !charactersReady || busy || !sourceReady || !slotsReady;
  $('detectFacialHair').disabled = !charactersReady || busy;
  $('browseFacialHair').disabled = !charactersReady || busy;
  $('facialHairSource').disabled = !charactersReady || busy || !state.facialHair.candidates.length;
  if (sourceReady) $('facialHairSource').value = state.facialHair.sourcePath;
  $('facialHairSource').title = state.facialHair.sourcePath;
  $('facialHairSlot').disabled = !charactersReady || busy || !$('includeFacialHair').checked || !slotsReady;
}

function resetFacialHairSelection() {
  state.facialHair.manualChoice = null;
  state.facialHair.requestId += 1;
  state.facialHair.sourcePath = '';
  state.facialHair.generation = '';
  state.facialHair.sourceKey = '';
  state.facialHair.candidates = [];
  state.facialHair.slots = [];
  state.facialHair.targetGame = '';
  state.facialHair.route = '';
  state.facialHair.message = '';
  state.facialHair.loading = false;
  $('includeFacialHair').checked = false;
  $('facialHairSource').replaceChildren(new Option('No source facial hair detected', ''));
  $('facialHairSlot').replaceChildren(new Option('No target slots loaded', ''));
  setFacialHairStatus('Select a source and target character to inspect facial hair.');
  renderFacialHairControls();
  updateReadiness();
}

function refreshFacialHairStatus() {
  const candidate = selectedFacialHairCandidate();
  const slot = selectedFacialHairSlot();
  if (!candidate || !slot) return;
  const route = state.facialHair.route || `${candidate.generation} -> ${state.facialHair.targetGame}`;
  setFacialHairStatus(`${route} | ${candidate.sourceKey} -> ${slot.key}.`, 'ready');
}

async function refreshFacialHairOptions({ explicitSource = '' } = {}) {
  if (!state.source || !state.target) {
    resetFacialHairSelection();
    return;
  }
  if (state.target.gameLabel === 'NBA 2K23') {
    resetFacialHairSelection();
    state.facialHair.targetGame = 'NBA 2K23';
    $('facialHairSource').replaceChildren(new Option('Facial hair transfer not included in this route', ''));
    $('facialHairSlot').replaceChildren(new Option('Uses the NBA 2K23 target facial hair', ''));
    setFacialHairStatus('NBA 2K18 to NBA 2K23 currently preserves the target facial hair.', 'warning');
    return;
  }
  const requestId = ++state.facialHair.requestId;
  const source = state.source;
  const target = state.target;
  const isCurrent = () => requestId === state.facialHair.requestId
    && source === state.source && target === state.target;
  const previousSource = explicitSource || state.facialHair.sourcePath;
  const previousSlot = $('facialHairSlot').value;
  state.facialHair.loading = true;
  setFacialHairStatus('Inspecting source facial hair and target appearance slots...');
  updateReadiness();
  try {
    const result = await window.characterTool.loadFacialHairOptions({
      source: source.path,
      target: target.path,
      sourceGame: source.gameLabel || '',
      targetGame: target.gameLabel || '',
      facialHairSource: previousSource,
    });
    if (!isCurrent()) return;
    const candidates = result.candidates || [];
    const slots = result.slots || [];
    const candidate = candidates.find((item) => item.path === previousSource) || candidates[0] || null;
    const slot = slots.find((item) => item.key === previousSlot)
      || slots.find((item) => item.default) || slots[0] || null;
    state.facialHair.candidates = candidates;
    state.facialHair.slots = slots;
    state.facialHair.sourcePath = candidate?.path || '';
    state.facialHair.generation = candidate?.generation || '';
    state.facialHair.sourceKey = candidate?.sourceKey || '';
    state.facialHair.targetGame = result.targetGame || target.gameLabel || '';
    state.facialHair.route = result.route || '';
    state.facialHair.message = result.message || '';
    const sourceSelect = $('facialHairSource');
    sourceSelect.replaceChildren();
    candidates.forEach((item) => sourceSelect.add(new Option(
      `${item.generation} ${item.sourceKey} | ${item.name}`, item.path,
    )));
    if (candidate) sourceSelect.value = candidate.path;
    else sourceSelect.add(new Option('No source facial hair detected', ''));
    const slotSelect = $('facialHairSlot');
    slotSelect.replaceChildren();
    slots.forEach((item) => slotSelect.add(new Option(item.label, item.key)));
    if (slot) slotSelect.value = slot.key;
    else slotSelect.add(new Option('No supported facial hair slots found', ''));
    if (!candidate || !slot) {
      $('includeFacialHair').checked = false;
      setFacialHairStatus(result.message || (!candidate
        ? 'No supported source facial hair was found beside this character.'
        : 'The NBA 2K27 target has no facial hair slot with supported regular or tangent-space geometry.'), 'warning');
    } else {
      $('includeFacialHair').checked = state.facialHair.manualChoice !== false;
      refreshFacialHairStatus();
    }
  } catch (error) {
    if (!isCurrent()) return;
    resetFacialHairSelection();
    setFacialHairStatus(error.message, 'warning');
    addLog(`Facial hair inspection: ${error.message}`, 'warning');
  } finally {
    if (isCurrent()) {
      state.facialHair.loading = false;
      updateReadiness();
    }
  }
}

function resetFullSwapOutputNames() {
  $('hairOutputName').value = '';
  $('facialHairOutputName').value = '';
}

function updateReadiness() {
  updateStagePlayerControls();
  updatePreviewSeamAction();
  $('browseWorkingPlayer').disabled = Boolean(stagePlayerBlockReason());
  const hairReady = !$('includeHair').checked
    || Boolean(state.hair.sourcePath && $('hairSlot').value && !state.hair.loading);
  const facialHairReady = facialHairSelectionReady();
  const jobBusy = Boolean(state.startingJob || state.activeJob || state.selectionBusy);
  document.querySelectorAll<HTMLButtonElement>('[data-browse], [data-manifest]').forEach((button) => {
    button.disabled = jobBusy || state.hair.loading || state.facialHair.loading;
  });
  const namesBusy = !state.source || !state.target || jobBusy;
  $('hairOutputName').disabled = namesBusy || state.hair.loading || !$('includeHair').checked;
  $('facialHairOutputName').disabled = namesBusy || state.facialHair.loading || !$('includeFacialHair').checked;
  const targetRouteReady = !state.target || state.target.gameLabel !== 'NBA 2K23'
    || state.source?.gameLabel === 'NBA 2K18';
  const ready = Boolean(state.source && state.target && state.outputDir && hairReady && facialHairReady && targetRouteReady && !jobBusy);
  renderFacialHairControls();
  $('runSwap').disabled = !ready;
  $('newSwap').disabled = jobBusy || state.hair.loading || state.facialHair.loading;
  $('readinessDot').classList.toggle('ready', ready);
  $('readinessText').textContent = state.selectionBusy
    ? 'Loading player selection...'
    : jobBusy ? 'Conversion running'
    : ready
      ? 'Ready to run Full Swap'
      : !targetRouteReady
        ? 'NBA 2K23 targets currently require an NBA 2K18 source'
      : $('includeHair').checked && !hairReady
        ? 'Complete the hair source and target slot'
        : !facialHairReady
          ? 'Complete the facial hair source and target slot'
          : 'Choose source, target, and output folder';
}

function renderPlayerEditControls() {
  updateStagePlayerControls();
  renderBodyFitRows();
  renderPreviewControls();
  renderPreviewBodyFitControls();
  renderHeadbandControls();
  updateNativeHairInstallState();
}

async function recoverFailedReset() {
  state.body.requestId += 1;
  state.body.loading = false;
  state.body.operation = '';
  $('saveBodyFit').textContent = 'Apply Changes';
  $('applyChestFix').textContent = 'Apply Chest Fix';
  state.preview.requestId += 1;
  state.preview.editRequestId += 1;
  state.preview.loading = false;
  state.preview.bodyFitBusy = false;
  state.preview.chestCorrectionBusy = false;
  state.preview.closeSeamsBusy = false;
  state.preview.dirty = true;
  $('previewLoading').hidden = true;
  state.headband.requestId += 1;
  state.headband.loading = false;
  state.headband.applying = false;
  const recovering = [window.configsEditor?.refresh()];
  if (state.body.targetPlayer) recovering.push(refreshBodyFit({ preserveDraft: true }));
  if (state.activeView === 'preview' && state.workingPlayer) recovering.push(loadStagedPreview(true));
  if (state.activeView === 'headband') recovering.push(refreshHeadbandOptions());
  const results = await Promise.allSettled(recovering);
  for (const result of results) if (result.status === 'rejected') addLog(`Could not refresh retained player: ${result.reason?.message || result.reason}`, 'warning');
}

async function resetFullSwapSession() {
  if (state.startingJob || state.activeJob || state.selectionBusy || state.hair.loading || state.facialHair.loading) return;
  if (state.body.dirtyFields.size && !window.confirm('Discard unapplied Body changes and start a new swap?')) return;

  state.selectionBusy = true;
  state.resetting = true;
  renderPlayerEditControls();
  updateReadiness();
  try {
    state.workingRequestId += 1;
    state.workingGeneration += 1;
    await window.characterTool.resetSession();
  } catch (error) {
    addLog(`Could not clear the previous selection cache: ${error.message}`, 'warning');
    try { await recoverFailedReset(); }
    catch (recoveryError) { addLog(`Could not refresh retained player: ${recoveryError.message}`, 'warning'); }
    finally {
      state.resetting = false;
      state.selectionBusy = false;
      renderPlayerEditControls();
      updateReadiness();
    }
    return;
  }

  state.pendingTerminalEvent = null;
  state.source = null;
  state.target = null;
  state.workingSource = null;
  state.workingBaseline = null;
  state.workingPlayer = null;
  window.configsEditor?.reset();
  state.lastOutput = '';
  state.hair.requestId += 1;
  state.hair.sourcePath = '';
  state.hair.generation = '';
  state.hair.sourceKey = '';
  state.hair.candidates = [];
  state.hair.targetGame = '';
  state.hair.slots = [];
  state.hair.loading = false;
  resetFullSwapOutputNames();
  resetFacialHairSelection();
  state.nativeHair.selectedId = '';
  invalidateNativeHairTarget();
  state.nativeHair.baseRequestId += 1;
  state.nativeHair.previewRequestId += 1;
  state.nativeHair.activePreviewKey = '';
  state.nativeHair.displayedPreviewKey = '';
  state.nativeHair.baseLoadedPath = '';
  state.nativeHair.basePreview = null;
  window.hairBasePreview?.clear();
  window.hairAssetPreview?.clear();
  state.headband.requestId += 1;
  state.headband.sourceHeadbands = [];
  state.headband.configs = [];
  state.headband.styles = [];
  state.headband.loading = false;
  state.headband.applying = false;
  resetPreviewState();
  localStorage.removeItem('characterModWorkingPlayer');
  localStorage.removeItem('characterModWorkingSource');
  localStorage.removeItem('characterModWorkingBaseline');

  $('includeHair').checked = false;
  $('hairSource').replaceChildren(new Option('No source hair detected', ''));
  $('hairSlot').replaceChildren(new Option('No target slots loaded', ''));
  $('renamePackage').checked = false;
  $('renameId').value = '';
  $('renameId').disabled = true;
  $('runSwap').hidden = false;
  $('runSwap').textContent = 'Run Full Swap';
  $('cancelSwap').disabled = true;
  $('openLastOutput').disabled = true;
  renderCharacter('source');
  renderCharacter('target');
  resetBodyFitSession();
  renderWorkingPlayer();
  renderHairControls();
  renderHeadbandControls();
  setHairStatus('Select a source and target character to inspect hair.');
  setProgress('Ready', 'idle');
  setActiveView('full-swap');
  state.selectionBusy = false;
  state.resetting = false;
  renderPlayerEditControls();
  updateReadiness();
  addLog('New swap session started.', 'success');
}

async function selectFile(role) {
  if (state.selectionBusy || state.startingJob || state.activeJob || state.hair.loading || state.facialHair.loading) return;
  state.selectionBusy = true;
  updateReadiness();
  try {
    const filePath = await window.characterTool.chooseCharacter(role);
    if (!filePath) return;
    const inspected = await window.characterTool.inspectCharacter(filePath);
    state[role] = inspected;
    resetFullSwapOutputNames();
    renderCharacter(role);
    if (role === 'source') {
      state.hair.sourcePath = '';
      $('includeHair').checked = false;
      resetFacialHairSelection();
    }
    await refreshFacialHairOptions();
    await useSelectedPlayersForBodyFit();
    await refreshHairOptions({ autoEnable: true });
    addLog(`${role === 'source' ? 'Source' : 'Target'} selected: ${inspected.name}`, 'success');
  } catch (error) {
    addLog(error.message, 'error');
  } finally {
    state.selectionBusy = false;
    updateReadiness();
  }
}

function fillSettings(settings) {
  $('settings26').value = settings.game_root || '';
  $('settings27').value = settings.game_root_2k27 || '';
  $('settingsBlender').value = settings.blender_exe || '';
  $('settingsAddon').value = settings.head_swap_tool || '';
  $('settingsMdt').value = settings.mesh_data_transfer || '';
  $('settingsOutput').value = settings.output_dir || '';
}

async function browseSetting(button) {
  if (button.disabled) return;
  const input = $(button.dataset.settingBrowse);
  const buttons = [...document.querySelectorAll<HTMLButtonElement>('[data-setting-browse]')];
  buttons.forEach((item) => { item.disabled = true; });
  $('saveSettings').disabled = true;
  $('settingsStatus').textContent = '';
  try {
    const selected = await window.characterTool.chooseSetting({
      field: button.dataset.settingBrowse, currentPath: input.value,
    });
    if (selected) input.value = selected;
  } catch (error) {
    $('settingsStatus').textContent = error.message;
  } finally {
    buttons.forEach((item) => { item.disabled = false; });
    $('saveSettings').disabled = false;
    input.focus();
  }
}

async function saveSettings() {
  try {
    const result = await window.characterTool.saveSettings({
      game_root: $('settings26').value,
      game_root_2k27: $('settings27').value,
      blender_exe: $('settingsBlender').value,
      head_swap_tool: $('settingsAddon').value,
      mesh_data_transfer: $('settingsMdt').value,
      output_dir: $('settingsOutput').value,
    });
    state.bootstrap.settings = result.settings;
    if (state.nativeHair.catalogRequest) {
      state.nativeHair.catalogRequest = null;
      state.nativeHair.loading = false;
      setNativeHairCatalogLoading(false);
    }
    syncNativeHairSource(nativeHairSourceKey(), '');
    state.nativeHair.catalogLoaded = false;
    state.outputDir = result.settings.output_dir;
    $('outputPath').textContent = state.outputDir || 'Not configured';
    fillSettings(result.settings);
    $('settingsDialog').close();
    if (state.activeView === 'hair') await ensureNativeHairReady();
    addLog('Settings saved.', 'success');
    updateReadiness();
  } catch (error) {
    addLog(error.message, 'error');
  }
}

async function chooseOutput() {
  const folder = await window.characterTool.chooseFolder('output');
  if (!folder) return;
  state.outputDir = folder;
  $('outputPath').textContent = folder;
  $('settingsOutput').value = folder;
  await window.characterTool.saveSettings({ output_dir: folder });
  updateReadiness();
}

async function loadManifest(gameLabel) {
  if (state.manifestRole === 'target' && !['NBA 2K23', 'NBA 2K26', 'NBA 2K27'].includes(gameLabel)) return;
  state.manifestGame = gameLabel;
  state.manifestGames[state.manifestRole] = gameLabel;
  state.catalog = [];
  state.selectedManifest = null;
  state.manifestVisibleLimit = MANIFEST_BATCH_SIZE;
  $('manifestTableWrap').scrollTop = 0;
  $('useManifestPlayer').disabled = true;
  $('catalogEmpty').hidden = false;
  $('catalogEmpty').textContent = `Loading ${gameLabel} players...`;
  document.querySelectorAll<HTMLButtonElement>('[data-game]').forEach((button) => {
    button.classList.toggle('active', button.dataset.game === gameLabel);
  });
  try {
    const result = await window.characterTool.loadCatalog(gameLabel);
    state.catalog = result.rows;
    $('manifestSupportHeading').textContent = result.supportLabel || 'Configs';
    $('manifestSummary').textContent = `${result.rows.length.toLocaleString()} players | ${result.gameRoot}`;
    renderManifestRows();
  } catch (error) {
    $('catalogEmpty').textContent = error.message;
    addLog(error.message, 'error');
  }
}

function renderManifestRows() {
  const query = $('manifestSearch').value.trim().toLowerCase();
  const filtered = state.catalog.filter((row) => {
    return !query || row.name.toLowerCase().includes(query) || row.png.includes(query.replace(/^png/, ''));
  });
  const visible = filtered.slice(0, state.manifestVisibleLimit);
  const body = $('manifestRows');
  body.replaceChildren();
  visible.forEach((row) => {
    const tr = document.createElement('tr');
    tr.dataset.png = row.png;
    tr.classList.toggle('selected', state.selectedManifest?.png === row.png);
    [row.name, `png${row.png}`, row.support, row.hair, row.facialhair, row.headband].forEach((value) => {
      const td = document.createElement('td');
      td.textContent = value;
      tr.append(td);
    });
    tr.addEventListener('click', () => {
      body.querySelectorAll('tr').forEach((item) => item.classList.remove('selected'));
      tr.classList.add('selected');
      state.selectedManifest = row;
      $('useManifestPlayer').disabled = false;
    });
    tr.addEventListener('dblclick', useSelectedManifestPlayer);
    body.append(tr);
  });
  $('catalogEmpty').hidden = visible.length > 0;
  if (!visible.length) $('catalogEmpty').textContent = 'No matching players.';
  $('manifestCount').textContent = filtered.length > visible.length
    ? `${visible.length.toLocaleString()} of ${filtered.length.toLocaleString()} matches | scroll for more`
    : `${filtered.length.toLocaleString()} matches | all shown`;
}

function stagePlayerBlockReason() {
  if (state.stagePlayer.busy) return 'Staging player...';
  if (!state.bootstrap) return 'Wait for startup to finish.';
  if (state.resetting || state.selectionBusy || state.startingJob || state.activeJob
    || state.hair.loading || state.facialHair.loading || state.preview.loading
    || state.nativeHair.loading || state.headband.applying
    || bodyFitEditBusy(state.body.targetPlayer?.path) || bodyFitEditBusy(state.workingPlayer?.path)) {
    return 'Wait for the current operation to finish.';
  }
  if (previewBlenderSessions.size) return 'Finish the Blender round trip before staging another player.';
  if (state.body.dirtyFields.size || state.body.invalidFields.size
    || state.preview.bodyFitDirtyFields.size || state.preview.bodyFitInvalidInputs.size) {
    return 'Save or revert body-fit changes before staging another player.';
  }
  return window.configsEditor?.stagingBlockReason?.() || '';
}

function updateStagePlayerControls() {
  const button = document.getElementById('stagePlayer') as HTMLButtonElement;
  if (!button) return;
  const reason = stagePlayerBlockReason();
  button.disabled = Boolean(reason);
  button.title = reason || 'Stage an existing player for editing';
  button.setAttribute('aria-busy', String(state.stagePlayer.busy));
  button.querySelector('span').textContent = state.stagePlayer.busy ? 'Staging...' : 'Stage Player';
  document.querySelectorAll<HTMLButtonElement>('.sidebar .nav-item').forEach(tab => { tab.disabled = state.stagePlayer.busy; });
  document.querySelectorAll<HTMLButtonElement>('.player-start-choices button').forEach(choice => { choice.disabled = state.stagePlayer.busy; });
  for (const id of ['selectPlayerFile', 'selectPlayerManifest']) {
    const entry = document.getElementById(id) as HTMLButtonElement;
    if (entry) { entry.disabled = Boolean(reason); entry.title = reason; }
  }
  const current = document.getElementById('selectedPlayerName');
  if (current) {
    current.textContent = state.workingPlayer?.name || 'No player selected';
    document.getElementById('selectedPlayerPath').textContent = state.workingPlayer?.path || '';
    (document.getElementById('selectedPlayerPreview') as HTMLButtonElement).disabled = !state.workingPlayer || state.stagePlayer.busy;
    const status = document.getElementById('selectPlayerStatus');
    if (state.stagePlayer.busy) status.textContent = 'Staging player...';
    else status.textContent = document.getElementById('stagePlayerStatus')?.textContent || '';
  }
  for (const id of ['stagePlayerFile', 'stagePlayerManifest', 'stagePlayerClose']) {
    const control = document.getElementById(id) as HTMLButtonElement;
    if (control) control.disabled = state.stagePlayer.busy;
  }
  const close = $('manifestDialog').querySelector<HTMLButtonElement>('button[value="cancel"]');
  if (close) close.disabled = state.stagePlayer.busy;
  if (state.manifestRole === 'stage') {
    $('useManifestPlayer').disabled = state.stagePlayer.busy || !state.selectedManifest;
    $('useManifestPlayer').textContent = state.stagePlayer.busy ? 'Staging...' : 'Stage Selected Player';
  }
}

function openStagePlayer() {
  const reason = stagePlayerBlockReason();
  if (reason) { addLog(reason, 'warning'); return; }
  document.getElementById('stagePlayerStatus').textContent = '';
  (document.getElementById('stagePlayerDialog') as HTMLDialogElement).showModal();
}

async function stagePlayerSelection(select: () => Promise<{ filePath: string; gameLabel?: string } | null>) {
  const reason = stagePlayerBlockReason();
  if (reason) { addLog(reason, 'warning'); return; }
  const requestId = ++state.stagePlayer.requestId, previousWorkingId = state.workingRequestId;
  const ownsRequest = () => requestId === state.stagePlayer.requestId && !state.resetting;
  const ownsSelection = () => ownsRequest() && previousWorkingId === state.workingRequestId;
  state.stagePlayer.busy = true;
  state.selectionBusy = true;
  updateReadiness();
  const status = document.getElementById('stagePlayerStatus');
  if (status) status.textContent = 'Selecting player...';
  try {
    const selected = await select();
    if (!ownsSelection()) return;
    if (!selected) { if (status) status.textContent = ''; return; }
    if (status) status.textContent = 'Creating editable player copy...';
    const result = await window.characterTool.stagePlayer({ ...selected, outputDir: state.outputDir || undefined });
    if (!ownsSelection()) return;
    result.warnings.forEach(warning => addLog(warning, 'warning'));
    const staged = await setWorkingPlayer(result.mainPath);
    if (!staged || !ownsRequest() || !samePlayerPath(result.mainPath, state.workingPlayer?.path)) return;
    (document.getElementById('stagePlayerDialog') as HTMLDialogElement)?.close();
    $('manifestDialog').close();
    addLog(`Staged ${basename(result.mainPath)} with ${result.companions} companion files. Original unchanged.`, 'success');
    addLog(`Editable player: ${result.mainPath}`, 'info');
    if (status) status.textContent = '';
    setActiveView('preview');
  } catch (error) {
    if (!ownsRequest()) return;
    if (status) status.textContent = error.message;
    if (state.manifestRole === 'stage' && $('manifestDialog').open) $('manifestSummary').textContent = error.message;
    addLog(`Could not stage player: ${error.message}`, 'error');
  } finally {
    if (requestId === state.stagePlayer.requestId) {
      state.stagePlayer.busy = false;
      state.selectionBusy = false;
      updateReadiness();
    }
  }
}

function setupStagePlayer() {
  const button = document.createElement('button');
  button.id = 'stagePlayer'; button.type = 'button'; button.className = 'button primary';
  button.innerHTML = '<i data-lucide="user-round"></i><span>Stage Player</span>';
  button.addEventListener('click', openStagePlayer);
  $('newSwap').before(button);
  const dialog = document.createElement('dialog');
  dialog.id = 'stagePlayerDialog'; dialog.className = 'modal stage-player-dialog';
  dialog.setAttribute('aria-labelledby', 'stagePlayerTitle');
  dialog.innerHTML = '<header><h2 id="stagePlayerTitle">Stage Player</h2><button id="stagePlayerClose" class="icon-button" type="button" aria-label="Close" title="Close"><i data-lucide="x"></i></button></header><div class="stage-player-choices"><button id="stagePlayerFile" class="button secondary" type="button"><i data-lucide="folder-open"></i>Browse Files</button><button id="stagePlayerManifest" class="button secondary" type="button"><i data-lucide="list"></i>From Manifest</button></div><p id="stagePlayerStatus" role="status" aria-live="polite"></p>';
  document.body.append(dialog);
  const entryTab = document.createElement('button');
  entryTab.id = 'selectPlayerTab'; entryTab.className = 'button secondary'; entryTab.type = 'button';
  entryTab.innerHTML = '<i data-lucide="user-round"></i>Select Player';
  entryTab.title = 'Select Player';
  entryTab.addEventListener('click', () => setActiveView('player'));
  const choices = document.createElement('div');
  choices.id = 'playerChoices';
  choices.className = 'player-start-choices';
  choices.setAttribute('role', 'group'); choices.setAttribute('aria-label', 'Select an existing player or convert a player');
  const divider = document.createElement('div');
  divider.className = 'player-start-or'; divider.textContent = 'OR';
  const swapChoice = document.createElement('button');
  swapChoice.id = 'fullSwapChoice'; swapChoice.className = 'button secondary'; swapChoice.type = 'button';
  swapChoice.innerHTML = '<i data-lucide="arrow-left-right"></i>Convert Player';
  swapChoice.addEventListener('click', () => setActiveView('full-swap'));
  choices.append(entryTab, divider, swapChoice);
  document.querySelector('.topbar-actions').before(choices);
  $('fullSwapTab').innerHTML = '<i data-lucide="contact"></i>Player';
  $('fullSwapTab').title = 'Player';
  $('fullSwapTab').addEventListener('click', () => setActiveView(state.playerWorkflow));
  const entry = document.createElement('main');
  entry.id = 'selectPlayerView'; entry.className = 'select-player-view'; entry.hidden = true;
  entry.innerHTML = '<section class="select-player-workspace"><header><span class="select-player-symbol"><i data-lucide="user-round"></i></span><div><h2 id="selectedPlayerName">No player selected</h2><p id="selectedPlayerPath"></p></div></header><div class="select-player-actions"><button id="selectPlayerFile" type="button" class="button primary"><i data-lucide="folder-open"></i>Browse Files</button><button id="selectPlayerManifest" type="button" class="button secondary"><i data-lucide="list"></i>From Manifest</button><button id="selectedPlayerPreview" type="button" class="button secondary" disabled><i data-lucide="box"></i>Open Preview</button></div><p id="selectPlayerStatus" role="status" aria-live="polite"></p></section>';
  $('fullSwapView').before(entry);
  entry.querySelector('#selectPlayerFile').addEventListener('click', () => stagePlayerSelection(async () => {
    const filePath = await window.characterTool.chooseCharacter('player to edit');
    return filePath ? {filePath} : null;
  }));
  entry.querySelector('#selectPlayerManifest').addEventListener('click', () => openManifest('stage'));
  entry.querySelector('#selectedPlayerPreview').addEventListener('click', () => setActiveView('preview'));
  dialog.querySelector('#stagePlayerClose').addEventListener('click', () => { if (!state.stagePlayer.busy) dialog.close(); });
  dialog.querySelector('#stagePlayerFile').addEventListener('click', () => stagePlayerSelection(async () => {
    const filePath = await window.characterTool.chooseCharacter('player to edit');
    return filePath ? { filePath } : null;
  }));
  dialog.querySelector('#stagePlayerManifest').addEventListener('click', () => {
    if (state.stagePlayer.busy) return;
    dialog.close();
    openManifest('stage');
  });
  const preventBusyClose = event => { if (state.stagePlayer.busy) event.preventDefault(); };
  dialog.addEventListener('cancel', preventBusyClose);
  $('manifestDialog').addEventListener('cancel', preventBusyClose);
  const empty = $('previewEmpty');
  empty.querySelector('span').textContent = '';
  const emptyButton = button.cloneNode(true) as HTMLButtonElement;
  emptyButton.removeAttribute('id'); emptyButton.disabled = false;
  emptyButton.addEventListener('click', openStagePlayer);
  empty.append(emptyButton);
  $('browseWorkingPlayer').textContent = 'Stage Player';
  window.lucide?.createIcons();
  updateStagePlayerControls();
}

async function openManifest(role) {
  if (role === 'stage' && stagePlayerBlockReason()) return;
  if (state.selectionBusy || state.startingJob || state.activeJob || state.hair.loading || state.facialHair.loading) return;
  state.manifestRole = role;
  const templateSlot = TEMPLATE_SLOTS.find((slot) => slot.key === state.templateAssignmentSlot);
  $('manifestTitle').textContent = role === 'template'
    ? `Assign ${templateSlot?.label || 'Target'} Template`
    : role === 'stage' ? 'Stage Player from Manifest' : `Select ${role === 'source' ? 'Source' : 'Target'} Player`;
  $('manifestSearch').value = '';
  document.querySelectorAll<HTMLButtonElement>('[data-game]').forEach((button) => {
    button.hidden = role === 'template' || role === 'stage'
      ? button.dataset.game !== 'NBA 2K27'
      : role === 'target'
        ? !['NBA 2K23', 'NBA 2K26', 'NBA 2K27'].includes(button.dataset.game || '')
        : false;
  });
  $('useManifestPlayer').textContent = role === 'template' ? 'Assign Template' : role === 'stage' ? 'Stage Selected Player' : 'Use Selected Player';
  const preferredOrder = role === 'source'
    ? ['NBA 2K27', 'NBA 2K26', 'NBA 2K25', 'NBA 2K23', 'NBA 2K20', 'NBA 2K19', 'NBA 2K18']
    : role === 'template' || role === 'stage' ? ['NBA 2K27'] : ['NBA 2K27', 'NBA 2K26', 'NBA 2K23'];
  const remembered = state.manifestGames[role];
  const gameLabel = preferredOrder.includes(remembered) && state.bootstrap?.games?.[remembered]
    ? remembered
    : preferredOrder.find((label) => state.bootstrap?.games?.[label]) || preferredOrder[0];
  $('manifestDialog').showModal();
  await loadManifest(gameLabel);
}

async function useSelectedManifestPlayer() {
  if (state.selectionBusy || state.startingJob || state.activeJob || state.hair.loading || state.facialHair.loading) return;
  const row = state.selectedManifest;
  const role = state.manifestRole;
  const gameLabel = state.manifestGame;
  if (!row) return;
  if (role === 'stage') {
    await stagePlayerSelection(async () => {
      const extracted = await window.characterTool.extractPlayer({ gameLabel, png: row.png, role: 'working' });
      extracted.warnings.forEach(warning => addLog(warning, 'warning'));
      return { filePath: extracted.mainPath, gameLabel: extracted.gameLabel };
    });
    return;
  }
  if (state.manifestRole === 'template') {
    const slot = TEMPLATE_SLOTS.find((item) => item.key === state.templateAssignmentSlot);
    if (!slot) return;
    const templates = {
      ...state.templates,
      [slot.key]: {
      png: row.png,
      name: row.name,
      hair: row.hair || '',
      facialhair: row.facialhair || '',
      headband: row.headband || '',
      },
    };
    try {
      state.templates = await persistTemplates(templates);
    } catch (error) {
      addLog(`Could not save the ${slot.label} template: ${error.message}`, 'error');
      return;
    }
    $('manifestDialog').close();
    state.templateAssignmentSlot = '';
    renderTemplateList();
    $('templateDialog').showModal();
    addLog(`png${row.png} assigned to the ${slot.label} target template.`, 'success');
    return;
  }
  state.selectionBusy = true;
  updateReadiness();
  $('useManifestPlayer').disabled = true;
  $('useManifestPlayer').textContent = 'Extracting...';
  try {
    const result = await window.characterTool.extractPlayer({
      gameLabel,
      png: row.png,
      role,
    });
    const inspected = result.inspected || await window.characterTool.inspectCharacter(result.mainPath);
    inspected.name = `${result.name} (png${result.png})`;
    inspected.gameLabel = result.gameLabel;
    state[role] = inspected;
    resetFullSwapOutputNames();
    renderCharacter(role);
    if (role === 'source') {
      state.hair.sourcePath = '';
      $('includeHair').checked = false;
      resetFacialHairSelection();
    }
    await refreshFacialHairOptions();
    await useSelectedPlayersForBodyFit();
    await refreshHairOptions({ autoEnable: true });
    $('manifestDialog').close();
    addLog(`${result.gameLabel} ${inspected.name} extracted with ${result.companions} companion files.`, 'success');
    result.warnings.forEach((warning) => addLog(warning, 'warning'));
  } catch (error) {
    addLog(error.message, 'error');
  } finally {
    state.selectionBusy = false;
    updateReadiness();
    $('useManifestPlayer').textContent = 'Use Selected Player';
    $('useManifestPlayer').disabled = !state.selectedManifest;
  }
}

function readLegacyTemplates() {
  try {
    const stored = JSON.parse(localStorage.getItem(TEMPLATE_STORAGE_KEY) || '{}');
    return stored && typeof stored === 'object' && !Array.isArray(stored) ? stored : {};
  } catch {
    return {};
  }
}

async function persistTemplates(templates) {
  const saved = await window.characterTool.saveTemplates(templates);
  const persisted = saved?.templates && typeof saved.templates === 'object' ? saved.templates : {};
  localStorage.setItem(TEMPLATE_STORAGE_KEY, JSON.stringify(persisted));
  return persisted;
}

async function loadTemplates() {
  const legacy = readLegacyTemplates();
  try {
    const saved = await window.characterTool.loadTemplates();
    if (!saved?.exists && Object.keys(legacy).length) {
      state.templates = await persistTemplates(legacy);
      addLog('Existing template players were migrated to durable app storage.', 'success');
      return;
    }
    state.templates = saved?.templates && typeof saved.templates === 'object' ? saved.templates : {};
    localStorage.setItem(TEMPLATE_STORAGE_KEY, JSON.stringify(state.templates));
  } catch (error) {
    state.templates = legacy;
    addLog(`Template storage could not be loaded; using the browser backup: ${error.message}`, 'warning');
  }
}

function templateAssetSummary(template) {
  if (!template) return 'Not assigned';
  const assets = [];
  if (template.hair && template.hair !== '-') assets.push('Hair');
  if (template.facialhair && template.facialhair !== '-') assets.push('Facial hair');
  if (template.headband && template.headband !== '-') assets.push('Headband');
  return assets.length ? assets.join(' | ') : 'No detected accessories';
}

function renderTemplateList() {
  const list = $('templateList');
  list.replaceChildren();
  let assigned = 0;
  TEMPLATE_SLOTS.forEach((slot) => {
    const template = state.templates[slot.key];
    if (template) assigned += 1;
    const row = document.createElement('div');
    row.className = `template-row${template ? ' assigned' : ''}`;

    const copy = document.createElement('div');
    copy.className = 'template-copy';
    const label = document.createElement('strong');
    label.textContent = slot.label;
    const description = document.createElement('small');
    description.textContent = slot.description;
    copy.append(label, description);

    const player = document.createElement('div');
    player.className = 'template-player';
    const playerName = document.createElement('strong');
    playerName.textContent = template ? `${template.name} (png${template.png})` : 'No player assigned';
    const assets = document.createElement('small');
    assets.textContent = templateAssetSummary(template);
    player.append(playerName, assets);

    const actions = document.createElement('div');
    actions.className = 'template-actions';
    const assign = document.createElement('button');
    assign.type = 'button';
    assign.className = 'button secondary';
    assign.textContent = template ? 'Change Player' : 'Assign Player';
    assign.addEventListener('click', async () => {
      state.templateAssignmentSlot = slot.key;
      $('templateDialog').close();
      await openManifest('template');
    });
    const use = document.createElement('button');
    use.type = 'button';
    use.className = 'button primary';
    use.textContent = 'Use Template';
    use.disabled = !template;
    use.addEventListener('click', () => useTemplate(slot.key, use));
    actions.append(assign, use);
    if (template) {
      const clear = document.createElement('button');
      clear.type = 'button';
      clear.className = 'icon-button template-clear';
      clear.textContent = 'X';
      clear.title = `Clear ${slot.label} template`;
      clear.setAttribute('aria-label', clear.title);
      clear.addEventListener('click', async () => {
        const templates = { ...state.templates };
        delete templates[slot.key];
        clear.disabled = true;
        try {
          state.templates = await persistTemplates(templates);
          renderTemplateList();
        } catch (error) {
          clear.disabled = false;
          addLog(`Could not clear the ${slot.label} template: ${error.message}`, 'error');
        }
      });
      actions.prepend(clear);
    }
    row.append(copy, player, actions);
    list.append(row);
  });
  $('templateSummary').textContent = `${assigned} of ${TEMPLATE_SLOTS.length} template players assigned`;
}

async function useTemplate(slotKey, button) {
  if (state.selectionBusy || state.startingJob || state.activeJob || state.hair.loading || state.facialHair.loading) return;
  const slot = TEMPLATE_SLOTS.find((item) => item.key === slotKey);
  const template = state.templates[slotKey];
  if (!slot || !template) return;
  state.selectionBusy = true;
  updateReadiness();
  button.disabled = true;
  button.textContent = 'Extracting...';
  try {
    const result = await window.characterTool.extractPlayer({
      gameLabel: 'NBA 2K27',
      png: template.png,
      role: 'target',
    });
    const inspected = result.inspected || await window.characterTool.inspectCharacter(result.mainPath);
    inspected.name = `${result.name} (png${result.png})`;
    inspected.gameLabel = result.gameLabel;
    state.target = inspected;
    resetFullSwapOutputNames();
    renderCharacter('target');
    await refreshFacialHairOptions();
    await useSelectedPlayersForBodyFit();
    await refreshHairOptions({ autoEnable: true });
    $('templateDialog').close();
    addLog(`${slot.label} template loaded from NBA 2K27 png${result.png}.`, 'success');
    result.warnings.forEach((warning) => addLog(warning, 'warning'));
  } catch (error) {
    addLog(error.message, 'error');
  } finally {
    state.selectionBusy = false;
    updateReadiness();
    button.textContent = 'Use Template';
    button.disabled = false;
  }
}

function openTemplatePicker() {
  renderTemplateList();
  $('templateDialog').showModal();
}

function renderWorkingPlayer() {
  const player = state.workingPlayer;
  const bodySource = state.body.sourcePlayer;
  const bodyTarget = state.body.targetPlayer;
  $('workingPlayerName').textContent = player?.name || 'No staged player';
  $('workingPlayerPath').textContent = player?.path
    || 'No staged player';
  $('workingPlayerState').textContent = player ? 'Active' : 'Not selected';
  $('workingPlayerState').classList.toggle('ready', Boolean(player));
  $('openWorkingPlayer').disabled = !player;
  $('headbandWorkingName').textContent = player?.name || 'No staged player';
  $('headbandWorkingPath').textContent = player?.path || 'No staged player';
  $('headbandWorkingState').textContent = player ? 'Active' : 'Not selected';
  $('headbandWorkingState').classList.toggle('ready', Boolean(player));
  $('previewWorkingName').textContent = player?.name || 'No staged player';
  $('previewWorkingPath').textContent = player?.path || 'No staged player';
  $('previewWorkingState').textContent = player ? 'Active' : 'Not selected';
  $('previewWorkingState').classList.toggle('ready', Boolean(player));
  $('refreshPreview').disabled = !player || state.preview.loading;
  $('bodySourceName').textContent = bodySource?.name || 'No source selected';
  $('bodySourceRole').textContent = 'Source';
  $('bodySourcePath').textContent = bodySource?.path
    || 'Select a source player in Full Swap.';
  $('bodyTargetRole').textContent = state.body.targetRole;
  $('bodyTargetName').textContent = bodyTarget?.name || 'No target selected';
  $('bodyTargetPath').textContent = bodyTarget?.path
    || 'Select a target player in Full Swap.';
  $('bodyState').textContent = bodyTarget ? 'Ready' : 'Not loaded';
  $('bodyState').classList.toggle('ready', Boolean(bodyTarget));
}

function clearBodyFitValues() {
  state.body.operation = '';
  $('saveBodyFit').textContent = 'Apply Changes';
  $('applyChestFix').textContent = 'Apply Chest Fix';
  state.body.sourceBodyFit = {};
  state.body.targetBodyFit = {};
  state.body.targetDraft = {};
  state.body.dirtyFields.clear();
  state.body.invalidFields.clear();
}

function resetBodyFitSession() {
  state.body.requestId += 1;
  state.body.loading = false;
  state.body.sourcePlayer = null;
  state.body.targetPlayer = null;
  state.body.fallbackSource = null;
  state.body.targetRole = 'Selected Target';
  clearBodyFitValues();
  renderWorkingPlayer();
  renderBodyFitRows();
  $('bodyFitStatus').className = '';
  $('bodyFitStatus').textContent = 'Select both players to load body-fit values.';
}

async function useSelectedPlayersForBodyFit() {
  state.body.requestId += 1;
  state.body.loading = false;
  state.body.sourcePlayer = state.source;
  state.body.targetPlayer = state.target;
  state.body.fallbackSource = null;
  state.body.targetRole = 'Selected Target';
  clearBodyFitValues();
  renderWorkingPlayer();
  if (state.source && state.target) {
    await refreshBodyFit();
  } else {
    renderBodyFitRows();
    $('bodyFitStatus').className = '';
    $('bodyFitStatus').textContent = 'Select both players to load body-fit values.';
  }
}

async function setWorkingPlayer(filePath, source = null, baseline = null) {
  if (state.resetting) throw new Error('Wait for New Swap cleanup to finish before staging another player.');
  if (!filePath) throw new Error('No player IFF path was supplied for staging.');
  const requestId = ++state.workingRequestId;
  const isCurrent = () => requestId === state.workingRequestId;
  let inspected;
  try { inspected = await window.characterTool.inspectCharacter(filePath); }
  catch (error) { if (!isCurrent()) return false; throw error; }
  if (!isCurrent()) return false;
  inspected.name = basename(filePath);
  inspected.gameLabel = 'NBA 2K27';
  state.workingGeneration += 1;
  state.workingPlayer = inspected;
  invalidateNativeHairTarget();
  state.workingSource = source || null;
  state.workingBaseline = baseline || null;
  state.body.requestId += 1;
  state.body.loading = false;
  state.body.sourcePlayer = source || null;
  state.body.targetPlayer = inspected;
  state.body.fallbackSource = baseline || null;
  state.body.targetRole = source ? 'Staged Output' : 'Opened Player';
  localStorage.setItem('characterModWorkingPlayer', inspected.path);
  if (state.workingSource?.path) localStorage.setItem('characterModWorkingSource', state.workingSource.path);
  else localStorage.removeItem('characterModWorkingSource');
  if (state.workingBaseline?.path) localStorage.setItem('characterModWorkingBaseline', state.workingBaseline.path);
  else localStorage.removeItem('characterModWorkingBaseline');
  state.nativeHair.baseRequestId += 1;
  state.nativeHair.baseLoadedPath = '';
  state.nativeHair.basePreview = null;
  state.preview.requestId += 1;
  window.characterTool.cancelStagedPreview?.().catch(() => {});
  state.preview.loading = false;
  state.preview.loadedPath = '';
  state.preview.dirty = true;
  state.preview.editRequestId += 1;
  state.preview.bodyFitBusy = false;
  state.preview.chestCorrectionBusy = false;
  state.preview.closeSeamsBusy = false;
  state.preview.bodyFit = {};
  state.preview.bodyFitTarget = '';
  state.preview.bodyFitDraft = {};
  state.preview.bodyFitDirtyFields.clear();
  state.preview.bodyFitInvalidInputs.clear();
  state.preview.bodyFitOpen = false;
  state.preview.jerseyAvailable = false;
  $('previewLoading').hidden = true;
  window.modelPreview?.clear();
  state.headband.requestId += 1;
  state.headband.loading = false;
  state.headband.applying = false;
  state.headband.sourceHeadbands = [];
  state.headband.configs = [];
  state.headband.styles = [];
  clearBodyFitValues();
  renderWorkingPlayer();
  renderHeadbandControls();
  syncPreviewBlenderState();
  renderPreviewControls();
  renderPreviewBodyFitControls();
  try {
    await window.configsEditor?.refresh();
    if (!isCurrent()) return false;
    const view = state.activeView;
    if (view === 'body') await refreshBodyFit();
    else if (view === 'hair') await refreshNativeHairTarget();
    else if (view === 'headband') await refreshHeadbandOptions();
    else if (view === 'preview') await loadStagedPreview(true);
  } catch (error) { if (!isCurrent()) return false; throw error; }
  return isCurrent();
}

function formatBodyFitValue(value) {
  if (value === undefined) return '';
  if (typeof value === 'number') return Number.isInteger(value) ? String(value) : String(Number(value.toFixed(6)));
  return String(value);
}

function bodyFitValuesEqual(left, right) {
  if (typeof left === 'number' && typeof right === 'number') {
    return Math.abs(left - right) <= 0.000001;
  }
  return formatBodyFitValue(left) === formatBodyFitValue(right);
}

function currentTargetBodyFitValue(key) {
  return Object.hasOwn(state.body.targetDraft, key)
    ? state.body.targetDraft[key]
    : state.body.targetBodyFit[key];
}

function bodyFitComparison(key) {
  if (state.body.invalidFields.has(key)) return 'invalid';
  const hasSource = Object.hasOwn(state.body.sourceBodyFit, key);
  const hasTarget = Object.hasOwn(state.body.targetBodyFit, key);
  if (!hasTarget) return 'missing-target';
  if (!hasSource) return 'target-only';
  return bodyFitValuesEqual(state.body.sourceBodyFit[key], currentTargetBodyFitValue(key))
    ? 'matching'
    : 'different';
}

function bodyFitComparisonLabel(comparison) {
  return {
    matching: 'Matching',
    different: 'Different',
    'missing-target': 'Missing in Target',
    'target-only': 'No Source Match',
    invalid: 'Invalid Value',
  }[comparison] || comparison;
}

function bodyFitEditBusy(path, includeReads = true) {
  if (!path) return false;
  return Boolean((state.body.loading && (includeReads || state.body.operation)
    && samePlayerPath(path, state.body.targetPlayer?.path))
    || ((state.preview.bodyFitBusy || state.preview.chestCorrectionBusy || state.preview.closeSeamsBusy)
      && samePlayerPath(path, state.workingPlayer?.path)));
}

function updateBodyFitActions(visibleCount = null) {
  updateStagePlayerControls();
  updatePreviewSeamAction();
  const busy = state.resetting || bodyFitEditBusy(state.body.targetPlayer?.path);
  $('refreshBodyFit').disabled = busy;
  $('bodyFitRows').querySelectorAll<HTMLInputElement>('input').forEach((input) => { input.disabled = busy; });
  const keys = [...new Set([
    ...Object.keys(state.body.sourceBodyFit),
    ...Object.keys(state.body.targetBodyFit),
  ])];
  const counts = { matching: 0, different: 0, missing: 0 };
  keys.forEach((key) => {
    const comparison = bodyFitComparison(key);
    if (comparison === 'matching') counts.matching += 1;
    else if (comparison === 'different') counts.different += 1;
    else if (comparison === 'missing-target' || comparison === 'target-only') counts.missing += 1;
  });
  const targetCount = Object.keys(state.body.targetBodyFit).length;
  const sourceOnlyCount = keys.filter((key) => (
    Object.hasOwn(state.body.sourceBodyFit, key)
    && !Object.hasOwn(state.body.targetBodyFit, key)
  )).length;
  const shown = visibleCount === null ? targetCount : visibleCount;
  $('bodyFitCount').textContent = `${shown.toLocaleString()} shown | ${targetCount.toLocaleString()} target areas | ${counts.matching} matching | ${counts.different} different | ${counts.missing} unmatched | ${sourceOnlyCount} source-only | ${state.body.dirtyFields.size} edited`;
  $('saveBodyFit').disabled = !state.body.targetPlayer
    || state.resetting
    || state.body.loading
    || busy
    || state.body.dirtyFields.size === 0
    || state.body.invalidFields.size > 0;
  const hasChest = Object.hasOwn(state.body.targetBodyFit, 'chestTHIN')
    && Object.hasOwn(state.body.targetBodyFit, 'chestTHICK');
  $('applyChestFix').disabled = !state.body.targetPlayer
    || state.resetting
    || state.body.loading
    || busy
    || !hasChest
    || state.body.dirtyFields.size > 0;
}

function handleBodyFitInput(key, input, row, badge) {
  if (state.resetting || bodyFitEditBusy(state.body.targetPlayer?.path)) return;
  const raw = input.value.trim();
  const numeric = Number(raw);
  if (raw === '' || !Number.isFinite(numeric) || !Number.isFinite(Math.fround(numeric))) {
    state.body.targetDraft[key] = raw;
    state.body.dirtyFields.add(key);
    state.body.invalidFields.add(key);
  } else {
    state.body.invalidFields.delete(key);
    if (bodyFitValuesEqual(numeric, state.body.targetBodyFit[key])) {
      delete state.body.targetDraft[key];
      state.body.dirtyFields.delete(key);
    } else {
      state.body.targetDraft[key] = numeric;
      state.body.dirtyFields.add(key);
    }
  }
  const comparison = bodyFitComparison(key);
  row.className = comparison;
  if (state.body.dirtyFields.has(key)) row.classList.add('edited');
  input.classList.toggle('invalid', comparison === 'invalid');
  badge.className = `body-fit-comparison ${comparison}`;
  badge.textContent = bodyFitComparisonLabel(comparison);
  updateBodyFitActions();
  syncPreviewBodyFitFromBody(key);
}

function renderBodyFitRows() {
  const query = $('bodyFitSearch').value.trim().toLowerCase();
  const filter = $('bodyFitFilter').value;
  const source = state.body.sourceBodyFit;
  const target = state.body.targetBodyFit;
  const keys = [...new Set([...Object.keys(source), ...Object.keys(target)])]
    .filter((key) => !query || key.toLowerCase().includes(query))
    .filter((key) => {
      const comparison = bodyFitComparison(key);
      if (filter === 'target') return Object.hasOwn(target, key);
      if (filter === 'edited') return state.body.dirtyFields.has(key);
      if (filter === 'missing') return comparison === 'missing-target' || comparison === 'target-only';
      return comparison === filter;
    })
    .sort((left, right) => left.localeCompare(right));
  const body = $('bodyFitRows');
  body.replaceChildren();
  keys.forEach((key) => {
    const tr = document.createElement('tr');
    const comparison = bodyFitComparison(key);
    tr.className = comparison;
    if (state.body.dirtyFields.has(key)) tr.classList.add('edited');

    const fieldCell = document.createElement('td');
    fieldCell.textContent = key;
    const sourceCell = document.createElement('td');
    sourceCell.textContent = Object.hasOwn(source, key) ? formatBodyFitValue(source[key]) : '--';
    sourceCell.classList.toggle('missing-value', !Object.hasOwn(source, key));
    const targetCell = document.createElement('td');
    if (Object.hasOwn(target, key)) {
      const input = document.createElement('input');
      input.type = 'number';
      input.step = 'any';
      input.value = formatBodyFitValue(currentTargetBodyFitValue(key));
      input.setAttribute('aria-label', `Target ${key}`);
      input.disabled = state.body.loading || state.resetting;
      targetCell.append(input);
      const statusCell = document.createElement('td');
      const badge = document.createElement('span');
      badge.className = `body-fit-comparison ${comparison}`;
      badge.textContent = bodyFitComparisonLabel(comparison);
      statusCell.append(badge);
      input.addEventListener('input', () => handleBodyFitInput(key, input, tr, badge));
      tr.append(fieldCell, sourceCell, targetCell, statusCell);
    } else {
      targetCell.textContent = '--';
      targetCell.className = 'missing-value';
      const statusCell = document.createElement('td');
      const badge = document.createElement('span');
      badge.className = `body-fit-comparison ${comparison}`;
      badge.textContent = bodyFitComparisonLabel(comparison);
      statusCell.append(badge);
      tr.append(fieldCell, sourceCell, targetCell, statusCell);
    }
    body.append(tr);
  });
  $('bodyFitEmpty').hidden = keys.length > 0;
  updateBodyFitActions(keys.length);
}

async function refreshBodyFit(options: { preserveDraft?: boolean } = {}) {
  if (bodyFitEditBusy(state.body.targetPlayer?.path, false)) return;
  const requestId = ++state.body.requestId;
  if (!state.body.targetPlayer) {
    clearBodyFitValues();
    renderWorkingPlayer();
    renderBodyFitRows();
    $('bodyFitStatus').className = '';
    $('bodyFitStatus').textContent = 'Select both players to load body-fit values.';
    return;
  }
  state.body.loading = true;
  renderBodyFitRows();
  updatePreviewBodyFitActions();
  $('refreshBodyFit').disabled = true;
  $('bodyFitStatus').className = '';
  $('bodyFitStatus').textContent = 'Reading body-fit values...';
  try {
    const result = await window.characterTool.loadBodyFit({
      source: state.body.sourcePlayer?.path || '',
      fallbackSource: state.body.fallbackSource?.path || '',
      target: state.body.targetPlayer.path,
    });
    if (requestId !== state.body.requestId) return;
    const draft = options.preserveDraft === true ? { ...state.body.targetDraft } : {};
    state.body.sourceBodyFit = result.sourceBodyFit || {};
    state.body.targetBodyFit = result.targetBodyFit || {};
    state.body.targetDraft = {};
    state.body.dirtyFields.clear();
    state.body.invalidFields.clear();
    for (const [key, value] of Object.entries(draft)) {
      if (!Object.hasOwn(state.body.targetBodyFit, key) || bodyFitValuesEqual(value, state.body.targetBodyFit[key])) continue;
      state.body.targetDraft[key] = value;
      state.body.dirtyFields.add(key);
      if (typeof value !== 'number' || !Number.isFinite(value)) state.body.invalidFields.add(key);
    }
    syncLoadedPreviewBodyFit(state.body.targetBodyFit, options.preserveDraft === true);
    for (const key of state.body.dirtyFields) syncPreviewBodyFitFromBody(key);
    $('bodySourceRole').textContent = result.sourceRole || 'Source';
    $('bodySourceName').textContent = result.source ? basename(result.source) : 'No comparison source';
    $('bodySourcePath').textContent = result.source || 'No source or original-target body-fit data is available.';
    renderBodyFitRows();
    $('bodyFitStatus').className = 'success';
    $('bodyFitStatus').textContent = `${result.sourceEntry || 'No source'} | ${result.targetEntry}`;
  } catch (error) {
    if (requestId !== state.body.requestId) return;
    $('saveBodyFit').disabled = true;
    $('bodyFitStatus').className = 'error';
    $('bodyFitStatus').textContent = error.message;
    addLog(error.message, 'error');
  } finally {
    if (requestId !== state.body.requestId) return;
    state.body.loading = false;
    $('refreshBodyFit').disabled = false;
    renderBodyFitRows();
    updatePreviewBodyFitActions();
  }
}

async function saveBodyFit() {
  if (state.resetting || !state.body.targetPlayer || bodyFitEditBusy(state.body.targetPlayer.path) || state.body.invalidFields.size) return;
  const changes = {};
  state.body.dirtyFields.forEach((key) => {
    changes[key] = state.body.targetDraft[key];
  });
  if (!Object.keys(changes).length) return;
  const target = state.body.targetPlayer;
  const requestId = ++state.body.requestId, stageId = state.workingGeneration;
  const isCurrent = () => requestId === state.body.requestId && target === state.body.targetPlayer && stageId === state.workingGeneration;
  const button = $('saveBodyFit');
  state.body.loading = true;
  state.body.operation = 'save';
  button.disabled = true;
  button.textContent = 'Saving...';
  renderBodyFitRows();
  updatePreviewBodyFitActions();
  try {
    const result = await window.characterTool.saveBodyFit({
      target: target.path,
      changes,
    });
    invalidatePlayerRender(target.path);
    if (!isCurrent()) return;
    state.body.targetBodyFit = result.targetBodyFit || state.body.targetBodyFit;
    state.body.targetDraft = {};
    state.body.dirtyFields.clear();
    state.body.invalidFields.clear();
    syncLoadedPreviewBodyFit(result.targetBodyFit || {});
    renderBodyFitRows();
    $('bodyFitStatus').className = 'success';
    $('bodyFitStatus').textContent = `Saved ${result.changed.length} target value${result.changed.length === 1 ? '' : 's'}.`;
    addLog(`Saved ${result.changed.length} body-fit value${result.changed.length === 1 ? '' : 's'} to ${basename(target.path)}.`, 'success');
  } catch (error) {
    if (!isCurrent()) return;
    $('bodyFitStatus').className = 'error';
    $('bodyFitStatus').textContent = error.message;
    addLog(error.message, 'error');
  } finally {
    if (!isCurrent()) return;
    state.body.loading = false;
    state.body.operation = '';
    button.textContent = 'Apply Changes';
    renderBodyFitRows();
    updatePreviewBodyFitActions();
  }
}

async function applyChestFix() {
  if (state.resetting || !state.body.targetPlayer || bodyFitEditBusy(state.body.targetPlayer.path) || state.body.dirtyFields.size) return;
  const target = state.body.targetPlayer;
  const requestId = ++state.body.requestId, stageId = state.workingGeneration;
  const isCurrent = () => requestId === state.body.requestId && target === state.body.targetPlayer && stageId === state.workingGeneration;
  state.body.loading = true;
  state.body.operation = 'chest';
  const button = $('applyChestFix');
  button.disabled = true;
  button.textContent = 'Applying...';
  renderBodyFitRows();
  updatePreviewBodyFitActions();
  try {
    const result = await window.characterTool.applyChestFix({ target: target.path });
    invalidatePlayerRender(target.path);
    if (!isCurrent()) return;
    addLog(
      `Chest fix: chestTHIN ${result.oldThin} -> ${result.newThin}; chestTHICK ${result.oldThick} -> ${result.newThick}.`,
      'success',
    );
    state.body.loading = false;
    state.body.operation = '';
    button.textContent = 'Apply Chest Fix';
    await refreshBodyFit();
  } catch (error) {
    if (!isCurrent()) return;
    $('bodyFitStatus').className = 'error';
    $('bodyFitStatus').textContent = error.message;
    addLog(error.message, 'error');
  } finally {
    if (!isCurrent()) return;
    state.body.loading = false;
    state.body.operation = '';
    button.textContent = 'Apply Chest Fix';
    renderBodyFitRows();
    updatePreviewBodyFitActions();
  }
}

function selectedNativeHair() {
  return state.nativeHair.assets.find((asset) => asset.id === state.nativeHair.selectedId) || null;
}

function renderSelectedNativeHair() {
  const asset = selectedNativeHair();
  const description = asset
    ? `${asset.key} | ${asset.playerName} | png${asset.sourcePng} | ${asset.tangentspace ? 'Tangent-space' : 'Regular'}`
    : 'Hair asset preview';
  $('nativeHairPreviewCanvas').setAttribute('aria-label', description);
  $('nativeHairPreviewCanvas').closest<HTMLElement>('.native-hair-preview-shell').title = description;
  const facialHair = state.nativeHair.assetType === 'facialhair';
  $('nativeHairCopyItem').checked = Boolean(asset?.hasItem);
  $('nativeHairCopyItem').disabled = facialHair || Boolean(asset && !asset.hasItem);
  $('nativeHairCopyHint').textContent = facialHair && asset?.hasItem
    ? 'Facial hair color and tangent textures are copied automatically'
    : asset?.hasItem
      ? 'Use the selected NBA 2K27 texture item'
      : 'This manifest asset has no matching item';
  updateNativeHairInstallState();
}

function setNativeHairPreviewState(message = '', loading = false, loadingLabel = '') {
  const empty = $('nativeHairPreviewEmpty');
  const spinner = $('nativeHairPreviewLoading');
  spinner.hidden = !loading;
  spinner.querySelector('strong').textContent = loadingLabel || 'Loading Preview...';
  empty.hidden = loading || !message;
  if (message) empty.querySelector('span').textContent = message;
}

function nativeHairCached(cache, key) {
  if (!cache.has(key)) return null;
  const value = cache.get(key);
  cache.delete(key);
  cache.set(key, value);
  return value;
}

function cacheNativeHairPreview(cache, key, value, maximum) {
  cache.delete(key);
  cache.set(key, value);
  while (cache.size > maximum) cache.delete(cache.keys().next().value);
}

async function previewNativeHairAsset(asset) {
  if (!asset) return;
  const sourceKey = nativeHairSourceKey(), revision = state.nativeHair.catalogRevision;
  const assetType = state.nativeHair.assetType;
  const cacheKey = `${assetType}:${asset.id}`;
  const headKey = String(asset.sourcePng || '');
  if (state.nativeHair.activePreviewKey === cacheKey
    || state.nativeHair.displayedPreviewKey === cacheKey && !state.nativeHair.activePreviewKey) return;
  state.nativeHair.activePreviewKey = cacheKey;
  const requestId = ++state.nativeHair.previewRequestId;
  const loadingLabel = assetType === 'hair' ? 'Loading Hair...' : 'Loading Facial Hair...';
  setNativeHairPreviewState('', true, loadingLabel);
  try {
    let assetPreview = nativeHairCached(state.nativeHair.previewCache, cacheKey);
    let basePreview = nativeHairCached(state.nativeHair.headCache, headKey);
    if (!assetPreview || !basePreview) {
      const preview = await window.characterTool.previewNativeHair({
        assetType,
        assetId: asset.id,
        includeBase: !basePreview,
      });
      if (sourceKey !== nativeHairSourceKey() || revision !== state.nativeHair.catalogRevision) return;
      assetPreview = preview.asset || assetPreview;
      basePreview = preview.base || basePreview;
      if (preview.asset) cacheNativeHairPreview(
        state.nativeHair.previewCache, cacheKey, preview.asset, NATIVE_HAIR_ASSET_CACHE_LIMIT,
      );
      if (preview.base) cacheNativeHairPreview(
        state.nativeHair.headCache, headKey, preview.base, NATIVE_HAIR_HEAD_CACHE_LIMIT,
      );
    }
    if (requestId !== state.nativeHair.previewRequestId) return;
    if (!assetPreview || !basePreview) throw new Error('The local hair preview cache returned an incomplete asset.');
    if (!window.hairBasePreview) throw new Error('The local hair preview renderer did not initialize.');
    const previousCamera = state.nativeHair.basePreview ? window.hairBasePreview.cameraState() : null;
    if (state.nativeHair.baseLoadedPath !== headKey) {
      await window.hairBasePreview.load(basePreview);
      if (requestId !== state.nativeHair.previewRequestId) return;
      state.nativeHair.basePreview = basePreview;
      state.nativeHair.baseLoadedPath = headKey;
      window.hairBasePreview.setJerseyVisible(false);
      window.hairBasePreview.setEyelashesVisible(true);
      if (previousCamera) {
        window.hairBasePreview.setCameraState(previousCamera);
      } else if (state.nativeHair.cameraState) {
        window.hairBasePreview.setCameraState(state.nativeHair.cameraState);
      } else {
        window.hairBasePreview.focusHead();
        const portrait = window.hairBasePreview.cameraState();
        const initialCamera = {
          ...portrait,
          yaw: 0.18,
          pitch: 0.02,
          distance: portrait.distance * 1.08,
          fittedDistance: portrait.fittedDistance * 1.08,
        };
        window.hairBasePreview.setCameraState(initialCamera);
        saveNativeHairCamera(initialCamera);
      }
    }
    window.hairAssetPreview?.clear();
    await window.hairAssetPreview?.load(assetPreview);
    if (requestId !== state.nativeHair.previewRequestId) return;
    window.hairAssetPreview?.setJerseyVisible(false);
    window.hairAssetPreview?.setOcclusionBase(basePreview);
    window.hairAssetPreview?.setEyelashesVisible(false);
    const catalogCamera = window.hairBasePreview.cameraState();
    window.hairBasePreview.fitCamera(window.hairAssetPreview?.bounds());
    const combinedCamera = window.hairBasePreview.cameraState();
    window.hairBasePreview.setCameraState({
      ...combinedCamera,
      yaw: catalogCamera.yaw,
      pitch: catalogCamera.pitch,
      distance: combinedCamera.distance * Math.min(2, Math.max(0.5, catalogCamera.distance / catalogCamera.fittedDistance)),
    });
    window.hairAssetPreview?.setCameraState(window.hairBasePreview.cameraState());
    state.nativeHair.displayedPreviewKey = cacheKey;
    setNativeHairPreviewState();
  } catch (error) {
    if (requestId !== state.nativeHair.previewRequestId) return;
    setNativeHairPreviewState(error.message || 'This asset could not be previewed.');
  } finally {
    if (requestId === state.nativeHair.previewRequestId) state.nativeHair.activePreviewKey = '';
  }
}

let nativeHairTable = null;
let nativeHairFilterKey = '';
let nativeHairFilterAssets = null;
function renderNativeHairRows() {
  const query = $('nativeHairSearch').value.trim().toLowerCase();
  const favorites = currentNativeHairFavorites();
  const filtered = state.nativeHair.assets.filter((asset) => {
    const favoriteMatch = !state.nativeHair.favoritesOnly || favorites.has(nativeHairFavoriteKey(asset));
    const searchMatch = !query || [asset.playerName, asset.sourcePng, asset.key].some((value) => String(value).toLowerCase().includes(query));
    return favoriteMatch && searchMatch;
  });
  if (!nativeHairTable) nativeHairTable = new window.VirtualTable<import('../contracts').NativeAsset>($('nativeHairRows'), (asset) => {
    const tr = document.createElement('tr');
    const favorites = currentNativeHairFavorites();
    tr.dataset.assetId = asset.id;
    tr.tabIndex = 0;
    tr.classList.toggle('selected', asset.id === state.nativeHair.selectedId);
    const favoriteCell = document.createElement('td');
    const favoriteButton = document.createElement('button');
    const isFavorite = favorites.has(nativeHairFavoriteKey(asset));
    favoriteButton.type = 'button';
    favoriteButton.className = 'native-hair-favorite';
    favoriteButton.classList.toggle('active', isFavorite);
    favoriteButton.textContent = isFavorite ? '\u2605' : '\u2606';
    favoriteButton.title = isFavorite ? 'Remove from favorites' : 'Add to favorites';
    favoriteButton.setAttribute('aria-label', favoriteButton.title);
    favoriteButton.addEventListener('click', (event) => {
      event.stopPropagation();
      toggleNativeHairFavorite(asset);
    });
    favoriteCell.append(favoriteButton);
    tr.append(favoriteCell);
    [asset.playerName, `png${asset.sourcePng}`, asset.key, asset.hasItem ? 'Yes' : 'No'].forEach((value) => {
      const td = document.createElement('td');
      td.textContent = value;
      td.title = value;
      tr.append(td);
    });
    tr.addEventListener('click', () => {
      state.nativeHair.selectedId = asset.id;
      nativeHairTable.render();
      renderSelectedNativeHair();
      previewNativeHairAsset(asset);
    });
    tr.addEventListener('keydown', (event) => {
      if (event.target === tr && (event.key === 'Enter' || event.key === ' ')) {
        event.preventDefault();
        tr.click();
      }
    });
    tr.addEventListener('dblclick', () => {
      state.nativeHair.selectedId = asset.id;
      renderSelectedNativeHair();
      installNativeHair();
    });
    return tr;
  });
  const filterKey = `${state.nativeHair.assetType}:${query}:${state.nativeHair.favoritesOnly}`;
  nativeHairTable.setItems(filtered, filterKey !== nativeHairFilterKey || nativeHairFilterAssets !== state.nativeHair.assets);
  nativeHairFilterKey = filterKey;
  nativeHairFilterAssets = state.nativeHair.assets;
  $('nativeHairEmpty').hidden = filtered.length > 0;
  $('nativeHairCount').textContent = `${filtered.length.toLocaleString()} assets`;
}

function setNativeHairCatalogLoading(loading) {
  const overlay = $('nativeHairCatalogLoading');
  overlay.hidden = !loading;
  overlay.querySelector('strong').textContent = state.nativeHair.assetType === 'hair'
    ? 'Loading Hair...'
    : 'Loading Facial Hair...';
  $('nativeHeadHair').disabled = loading;
  $('nativeFacialHair').disabled = loading;
}

function nativeHairSourceKey() {
  return String(state.bootstrap?.settings?.game_root_2k27 || '');
}

function syncNativeHairSource(sourceKey, revision) {
  if (state.nativeHair.catalogSourceKey === sourceKey && state.nativeHair.catalogRevision === revision) return false;
  const rootChanged = state.nativeHair.catalogSourceKey !== sourceKey;
  state.nativeHair.catalogSourceKey = sourceKey;
  state.nativeHair.catalogRevision = revision;
  state.nativeHair.catalogLoaded = false;
  state.nativeHair.previewRequestId += 1;
  state.nativeHair.previewCache.clear();
  state.nativeHair.headCache.clear();
  state.nativeHair.activePreviewKey = '';
  state.nativeHair.displayedPreviewKey = '';
  state.nativeHair.baseLoadedPath = '';
  state.nativeHair.basePreview = null;
  window.hairBasePreview?.clear();
  window.hairAssetPreview?.clear();
  setNativeHairPreviewState('Select a catalog row to inspect it.');
  if (rootChanged) invalidateNativeHairTarget();
  return true;
}

async function loadNativeHairCatalog(force = false) {
  const assetType = state.nativeHair.assetType;
  const sourceKey = nativeHairSourceKey();
  if (state.nativeHair.catalogSourceKey !== sourceKey) syncNativeHairSource(sourceKey, '');
  const pending = state.nativeHair.catalogRequest;
  if (pending?.assetType === assetType && pending.sourceKey === sourceKey) return pending.promise;
  if ((state.nativeHair.loading && !pending) || (state.nativeHair.catalogLoaded && !force)) return;
  const request = { assetType, sourceKey, promise: Promise.resolve() };
  state.nativeHair.catalogRequest = request;
  state.nativeHair.loading = true;
  setNativeHairCatalogLoading(true);
  $('nativeHairEmpty').hidden = false;
  $('nativeHairEmpty').textContent = 'Reading NBA 2K27 manifest...';
  updateNativeHairInstallState();
  const isCurrent = () => state.nativeHair.catalogRequest === request
    && state.nativeHair.assetType === assetType && nativeHairSourceKey() === sourceKey;
  request.promise = (async () => {
    try {
      const result = await window.characterTool.loadNativeHairCatalog({ assetType });
      if (!isCurrent()) return;
      const sourceChanged = syncNativeHairSource(sourceKey, result.sourceRevision || '');
      state.nativeHair.assets = result.assets || [];
      if (!state.nativeHair.assets.some(asset => asset.id === state.nativeHair.selectedId)) {
        state.nativeHair.selectedId = '';
      }
      state.nativeHair.catalogLoaded = true;
      renderNativeHairRows();
      renderSelectedNativeHair();
      if (sourceChanged && selectedNativeHair()) await previewNativeHairAsset(selectedNativeHair());
    } catch (error) {
      if (!isCurrent()) return;
      state.nativeHair.assets = [];
      state.nativeHair.catalogLoaded = false;
      $('nativeHairEmpty').hidden = false;
      $('nativeHairEmpty').textContent = error.message;
      $('nativeHairStatus').className = 'native-hair-status error';
      $('nativeHairStatus').textContent = error.message;
      addLog(error.message, 'error');
    } finally {
      if (state.nativeHair.catalogRequest === request) {
        state.nativeHair.catalogRequest = null;
        state.nativeHair.loading = false;
        setNativeHairCatalogLoading(false);
        updateNativeHairInstallState();
      }
    }
  })();
  return request.promise;
}

function nativeHairTargetKey() {
  return JSON.stringify([
    state.workingPlayer?.path || '',
    window.configsEditor?.active() || '',
    state.nativeHair.assetType,
    nativeHairSourceKey(),
  ]);
}

function invalidateNativeHairTarget() {
  state.nativeHair.targetRequestId += 1;
  state.nativeHair.targetLoading = false;
  state.nativeHair.targetContext = '';
  state.nativeHair.slots = [];
  const select = $('nativeHairSlot');
  select.replaceChildren(new Option('No working player loaded', ''));
  select.disabled = true;
  updateNativeHairInstallState();
}

async function refreshNativeHairTarget() {
  invalidateNativeHairTarget();
  const requestId = state.nativeHair.targetRequestId;
  const target = state.workingPlayer?.path;
  const config = window.configsEditor?.active();
  const assetType = state.nativeHair.assetType;
  const context = nativeHairTargetKey();
  const isCurrent = () => requestId === state.nativeHair.targetRequestId
    && context === nativeHairTargetKey();
  const select = $('nativeHairSlot');
  if (!target) {
    $('nativeHairStatus').className = 'native-hair-status';
    $('nativeHairStatus').textContent = 'No working player loaded';
    return;
  }
  state.nativeHair.targetLoading = true;
  select.replaceChildren(new Option('Loading appearance slots...', ''));
  try {
    const result = await window.characterTool.loadNativeHairTarget({
      target,
      configScoped: true,
      config,
      assetType,
    });
    if (!isCurrent()) return;
    state.nativeHair.slots = result.slots || [];
    state.nativeHair.targetContext = context;
    select.replaceChildren();
    state.nativeHair.slots.forEach((slot) => select.add(new Option(slot.label, slot.key)));
    if (!state.nativeHair.slots.length) select.add(new Option('No compatible appearance slots', ''));
    select.disabled = !state.nativeHair.slots.length;
    $('nativeHairStatus').className = 'native-hair-status';
    $('nativeHairStatus').textContent = state.nativeHair.slots.length
      ? `Staged png${result.targetPng} | ${state.nativeHair.slots.length} compatible slot(s)`
      : `Staged png${result.targetPng} has no ${assetType} slot.`;
  } catch (error) {
    if (!isCurrent()) return;
    select.disabled = true;
    $('nativeHairStatus').className = 'native-hair-status error';
    $('nativeHairStatus').textContent = error.message;
  } finally {
    if (requestId === state.nativeHair.targetRequestId) {
      state.nativeHair.targetLoading = false;
      updateNativeHairInstallState();
    }
  }
}

async function ensureNativeHairReady() {
  renderWorkingPlayer();
  await Promise.all([loadNativeHairCatalog(true), refreshNativeHairTarget()]);
}

function nativeHairInstallReady() {
  return Boolean(
    state.workingPlayer
    && selectedNativeHair()
    && state.nativeHair.catalogLoaded
    && state.nativeHair.catalogSourceKey === nativeHairSourceKey()
    && state.nativeHair.slots.some(slot => slot.key === $('nativeHairSlot').value)
    && !state.nativeHair.loading
    && !state.resetting
    && !state.nativeHair.targetLoading
    && state.nativeHair.targetContext === nativeHairTargetKey()
  );
}

function updateNativeHairInstallState() {
  $('installNativeHair').disabled = !nativeHairInstallReady();
  $('nativeHeadHair').disabled = state.nativeHair.loading;
  $('nativeFacialHair').disabled = state.nativeHair.loading;
  $('installNativeHair').textContent = state.nativeHair.assetType === 'hair' ? 'Install Hair' : 'Install Facial Hair';
}

async function setNativeHairType(assetType) {
  if (state.nativeHair.loading && !state.nativeHair.catalogRequest) return;
  if (state.nativeHair.assetType === assetType && state.nativeHair.catalogLoaded) return;
  state.nativeHair.assetType = assetType;
  state.nativeHair.catalogLoaded = false;
  state.nativeHair.assets = [];
  state.nativeHair.selectedId = '';
  state.nativeHair.previewRequestId += 1;
  state.nativeHair.activePreviewKey = '';
  state.nativeHair.displayedPreviewKey = '';
  window.characterTool.cancelNativeHairPreview().catch(() => {});
  state.nativeHair.baseLoadedPath = '';
  state.nativeHair.basePreview = null;
  window.hairBasePreview?.setMaterialTargetVisible('hair', true);
  window.hairBasePreview?.setMaterialTargetVisible('facialhair', true);
  window.hairBasePreview?.clear();
  window.hairAssetPreview?.clear();
  setNativeHairPreviewState('Select a catalog row to inspect it.');
  $('nativeHeadHair').classList.toggle('active', assetType === 'hair');
  $('nativeFacialHair').classList.toggle('active', assetType === 'facialhair');
  renderSelectedNativeHair();
  await Promise.all([loadNativeHairCatalog(), refreshNativeHairTarget()]);
}

async function installNativeHair() {
  if (!nativeHairInstallReady()) return;
  const asset = selectedNativeHair();
  const targetKey = $('nativeHairSlot').value;
  const target = state.workingPlayer.path;
  const context = nativeHairTargetKey();
  state.nativeHair.loading = true;
  updateNativeHairInstallState();
  $('nativeHairStatus').className = 'native-hair-status';
  $('nativeHairStatus').textContent = `Installing ${asset.key}...`;
  try {
    const result = await window.characterTool.installNativeHair({
      target,
      configScoped: true,
      config: window.configsEditor?.active(),
      assetType: state.nativeHair.assetType,
      assetId: asset.id,
      targetKey,
      copyItem: state.nativeHair.assetType === 'facialhair' || $('nativeHairCopyItem').checked,
    });
    result.outputs.forEach((output) => addLog(`Created ${basename(output)}`, 'success'));
    result.backups.forEach((backup) => addLog(`Backup ${basename(backup)}`, 'info'));
    (result.warnings || []).forEach((warning) => addLog(warning, 'warning'));
    invalidatePlayerRender(target);
    if (context !== nativeHairTargetKey()) return;
    await window.configsEditor?.refresh();
    if (context !== nativeHairTargetKey()) return;
    $('nativeHairStatus').className = 'native-hair-status success';
    $('nativeHairStatus').textContent = `Installed ${result.sourceKey} as ${result.targetKey} | ${result.outputs.length} file(s)`;
    window.hairAssetPreview?.clear();
    state.nativeHair.displayedPreviewKey = '';
    window.hairBasePreview?.setMaterialTargetVisible('hair', true);
    window.hairBasePreview?.setMaterialTargetVisible('facialhair', true);
    setNativeHairPreviewState('Installed on the staged player. The preview remains on the original source head.');
  } catch (error) {
    if (context === nativeHairTargetKey()) {
      $('nativeHairStatus').className = 'native-hair-status error';
      $('nativeHairStatus').textContent = error.message;
    }
    addLog(error.message, 'error');
  } finally {
    state.nativeHair.loading = false;
    updateNativeHairInstallState();
  }
}

function setPreviewStatus(message, kind = '') {
  $('previewStatus').className = `player-preview-status ${kind}`.trim();
  $('previewStatus').textContent = message;
}

const BODY_FIT_AREA_ORDER = [
  'forearmupper', 'forearmlower', 'forearm', 'upperarm', 'trap', 'neck', 'chest', 'breast', 'abs',
  'head', 'shoulder', 'hips', 'glutes', 'thigh', 'calf',
  'calfupper', 'calflower',
];

function samePlayerPath(left, right) {
  return String(left || '').replaceAll('\\', '/').toLowerCase()
    === String(right || '').replaceAll('\\', '/').toLowerCase();
}

function invalidatePlayerRender(target) {
  if (!samePlayerPath(target, state.workingPlayer?.path)) return false;
  state.preview.dirty = true;
  if (!state.preview.loading) return true;
  state.preview.requestId += 1;
  state.preview.loading = false;
  state.preview.loadedPath = '';
  window.characterTool.cancelStagedPreview?.().catch(() => {});
  window.modelPreview?.clear();
  $('previewLoading').hidden = true;
  $('previewEmpty').hidden = false;
  $('previewEmpty').querySelector('strong').textContent = 'Player updated';
  $('previewEmpty').querySelector('span').textContent = basename(target);
  resetPreviewDetails();
  renderWorkingPlayer();
  renderPreviewControls();
  if (state.activeView === 'preview') {
    const requestId = state.preview.requestId, stageId = state.workingGeneration;
    queueMicrotask(() => {
      if (state.activeView === 'preview' && requestId === state.preview.requestId
        && stageId === state.workingGeneration && samePlayerPath(target, state.workingPlayer?.path)
        && !state.resetting && !state.preview.loading && !state.preview.bodyFitBusy && !state.preview.chestCorrectionBusy && !state.preview.closeSeamsBusy) {
        loadStagedPreview(true);
      }
    });
  }
  return true;
}

function currentPreviewBodyFit() {
  return { ...state.preview.bodyFit, ...state.preview.bodyFitDraft };
}

function previewBodyFitAreas() {
  const areas = new Map();
  Object.keys(state.preview.bodyFit).forEach((key) => {
    const match = key.match(/^(.*?)(THIN|THICK)$/i);
    if (!match) return;
    const token = match[1].toLowerCase();
    const area = areas.get(token) || { token, label: match[1], thin: '', thick: '' };
    area[match[2].toLowerCase()] = key;
    areas.set(token, area);
  });
  return [...areas.values()].sort((left, right) => {
    const leftIndex = BODY_FIT_AREA_ORDER.indexOf(left.token);
    const rightIndex = BODY_FIT_AREA_ORDER.indexOf(right.token);
    return (leftIndex < 0 ? 999 : leftIndex) - (rightIndex < 0 ? 999 : rightIndex)
      || left.token.localeCompare(right.token);
  });
}

function previewBodyDimensionGroups() {
  const groups = new Map();
  Object.keys(state.preview.bodyFit).forEach((key) => {
    const match = key.match(/^(.*?)\.t([xyz])$/i);
    if (!match) return;
    const token = match[1].toLowerCase();
    const group = groups.get(token) || { token, label: match[1], x: '', y: '', z: '' };
    group[match[2].toLowerCase()] = key;
    groups.set(token, group);
  });
  return [...groups.values()].sort((left, right) => left.token.localeCompare(right.token));
}

function previewFitLabel(token) {
  return token.replace(/([a-z])([A-Z])/g, '$1 $2').replaceAll('_', ' ');
}

function setPreviewBodyFitStatus(message, kind = '') {
  $('previewBodyFitStatus').className = `preview-body-fit-status ${kind}`.trim();
  $('previewBodyFitStatus').textContent = message;
}

function updatePreviewBodyFitActions() {
  updatePreviewSeamAction();
  const loaded = Boolean(state.preview.loadedPath && Object.keys(state.preview.bodyFit).length);
  const busy = state.resetting || state.preview.loading || bodyFitEditBusy(state.preview.bodyFitTarget || state.preview.loadedPath);
  const invalid = state.preview.bodyFitInvalidInputs.size > 0;
  $('previewBodyFitControls').querySelectorAll<HTMLInputElement>('input').forEach((input) => { input.disabled = !loaded || busy; });
  $('previewBodyFitToggle').disabled = !loaded || busy;
  $('previewBodyFitToggle').classList.toggle('active', loaded && state.preview.bodyFitOpen);
  $('previewBodyFitPanel').hidden = !loaded || !state.preview.bodyFitOpen;
  $('previewCalculateFit').disabled = !loaded || busy || invalid || !state.preview.jerseyAvailable;
  $('previewRevertFit').disabled = !loaded || busy || (!state.preview.bodyFitDirtyFields.size && !invalid);
  $('previewApplyFit').disabled = !loaded || busy || invalid || !state.preview.bodyFitDirtyFields.size;
  $('previewBodyFitCount').textContent = `${previewBodyFitAreas().length} fit areas | ${previewBodyDimensionGroups().length} dimensions`;
}

function previewSeamBlockReason() {
  const target = state.workingPlayer?.path;
  if (!target) return 'Stage a saved player first.';
  if (state.preview.closeSeamsBusy) return 'Closing seams...';
  if (state.resetting || state.startingJob || state.activeJob || state.selectionBusy
    || state.preview.loading || bodyFitEditBusy(target) || state.headband.applying || state.nativeHair.loading) {
    return 'Wait for the current operation to finish.';
  }
  if (previewBlenderSessions.has(previewBlenderTarget(target))) return 'Finish this player\'s Blender round trip first.';
  if ((samePlayerPath(target, state.body.targetPlayer?.path) && (state.body.dirtyFields.size || state.body.invalidFields.size))
    || (samePlayerPath(target, state.preview.bodyFitTarget || state.preview.loadedPath)
      && (state.preview.bodyFitDirtyFields.size || state.preview.bodyFitInvalidInputs.size))) {
    return 'Apply or revert the unsaved body-fit changes first.';
  }
  return '';
}

function updatePreviewSeamAction() {
  const button = $('previewCloseSeams') as HTMLButtonElement;
  if (!button?.isConnected) return;
  const reason = previewSeamBlockReason();
  button.disabled = Boolean(reason);
  button.title = reason || 'Close seams on the saved staged player';
  button.setAttribute('aria-busy', String(state.preview.closeSeamsBusy));
  button.querySelector('span').textContent = state.preview.closeSeamsBusy ? 'Closing...' : 'Close Seams';
}

function setPreviewBodyFitOpen(value) {
  const loaded = Boolean(state.preview.loadedPath && Object.keys(state.preview.bodyFit).length);
  state.preview.bodyFitOpen = Boolean(value) && loaded;
  if (state.preview.bodyFitOpen && state.preview.jerseyAvailable && !state.preview.jersey) {
    state.preview.jersey = true;
    $('previewJersey').checked = true;
    window.modelPreview?.setJerseyVisible(true);
  }
  updatePreviewBodyFitActions();
}

function syncBodyFitDraftFromPreview(key, value) {
  if (!state.body.targetPlayer
    || !samePlayerPath(state.body.targetPlayer.path, state.preview.loadedPath)
    || !Object.hasOwn(state.body.targetBodyFit, key)) return;
  const invalid = typeof value !== 'number' || !Number.isFinite(value) || !Number.isFinite(Math.fround(value));
  if (!invalid && bodyFitValuesEqual(value, state.body.targetBodyFit[key])) {
    delete state.body.targetDraft[key];
    state.body.dirtyFields.delete(key);
  } else {
    state.body.targetDraft[key] = value;
    state.body.dirtyFields.add(key);
  }
  if (invalid) state.body.invalidFields.add(key);
  else state.body.invalidFields.delete(key);
  renderBodyFitRows();
}

function syncPreviewBodyFitFromBody(key) {
  const owner = state.preview.bodyFitTarget || state.preview.loadedPath;
  if (!owner
    || !state.body.targetPlayer
    || !samePlayerPath(state.body.targetPlayer.path, owner)
    || !Object.hasOwn(state.preview.bodyFit, key)) return;
  if (state.body.invalidFields.has(key)) {
    state.preview.bodyFitInvalidInputs.set(key, String(currentTargetBodyFitValue(key)));
    renderPreviewBodyFitControls();
    setPreviewBodyFitStatus('Enter a finite value within the 32-bit game-float range.', 'error');
    return;
  }
  const value = currentTargetBodyFitValue(key);
  state.preview.bodyFitInvalidInputs.delete(key);
  if (bodyFitValuesEqual(value, state.preview.bodyFit[key])) {
    delete state.preview.bodyFitDraft[key];
    state.preview.bodyFitDirtyFields.delete(key);
  } else {
    state.preview.bodyFitDraft[key] = value;
    state.preview.bodyFitDirtyFields.add(key);
  }
  if (!state.preview.loading) window.modelPreview?.setBodyFit(currentPreviewBodyFit());
  renderPreviewBodyFitControls();
}

function syncLoadedPreviewBodyFit(values, preserveDraft = false) {
  const owner = state.preview.bodyFitTarget || state.preview.loadedPath;
  if (!owner
    || !state.body.targetPlayer
    || !samePlayerPath(state.body.targetPlayer.path, owner)) return;
  const draft = preserveDraft ? previewBodyFitDraft(state.preview.bodyFitDraft, values) : {};
  state.preview.bodyFit = { ...values };
  state.preview.bodyFitInvalidInputs.clear();
  state.preview.bodyFitDraft = Object.fromEntries(Object.entries(draft).filter(([key, value]) => !bodyFitValuesEqual(value, values[key])));
  state.preview.bodyFitDirtyFields = new Set(Object.keys(state.preview.bodyFitDraft));
  if (!state.preview.loading) window.modelPreview?.setBodyFit(currentPreviewBodyFit());
  renderPreviewBodyFitControls();
}

function setPreviewFitValue(key, value, output = null) {
  if (state.resetting || bodyFitEditBusy(state.preview.bodyFitTarget || state.preview.loadedPath)) return false;
  if (!Object.hasOwn(state.preview.bodyFit, key)) return false;
  const numeric = typeof value === 'number' ? value : typeof value === 'string' && value.trim() ? Number(value) : NaN;
  if (!Number.isFinite(numeric) || !Number.isFinite(Math.fround(numeric))) {
    state.preview.bodyFitInvalidInputs.set(key, typeof value === 'string' ? value : '');
    syncBodyFitDraftFromPreview(key, typeof value === 'string' ? value : '');
    updatePreviewBodyFitActions();
    setPreviewBodyFitStatus('Enter a finite value within the 32-bit game-float range.', 'error');
    return false;
  }
  state.preview.bodyFitInvalidInputs.delete(key);
  const next = Number(numeric.toFixed(6));
  if (bodyFitValuesEqual(next, state.preview.bodyFit[key])) {
    delete state.preview.bodyFitDraft[key];
    state.preview.bodyFitDirtyFields.delete(key);
  } else {
    state.preview.bodyFitDraft[key] = next;
    state.preview.bodyFitDirtyFields.add(key);
  }
  if (output) output.value = next.toFixed(3);
  syncBodyFitDraftFromPreview(key, next);
  window.modelPreview?.setBodyFit(currentPreviewBodyFit());
  updatePreviewBodyFitActions();
  setPreviewBodyFitStatus(
    state.preview.bodyFitDirtyFields.size
      ? `${state.preview.bodyFitDirtyFields.size} staged change${state.preview.bodyFitDirtyFields.size === 1 ? '' : 's'}.`
      : 'Live preview matches the staged player values.',
  );
  return true;
}

function renderPreviewBodyFitControls() {
  const container = $('previewBodyFitControls');
  container.replaceChildren();
  const values = currentPreviewBodyFit();
  const areas = previewBodyFitAreas();
  areas.forEach((area) => {
    const section = document.createElement('section');
    section.className = 'preview-fit-area';
    const heading = document.createElement('strong');
    heading.textContent = previewFitLabel(area.label);
    section.append(heading);
    for (const channel of ['thin', 'thick']) {
      const key = area[channel];
      if (!key) continue;
      const row = document.createElement('div');
      row.className = 'preview-fit-channel';
      const label = document.createElement('label');
      label.textContent = channel;
      const slider = document.createElement('input');
      slider.type = 'range';
      slider.min = String(Math.min(-2, Number(values[key]) || 0));
      slider.max = String(Math.max(2, Number(values[key]) || 2));
      slider.step = '0.001';
      slider.value = String(values[key]);
      slider.disabled = state.resetting || state.preview.loading || state.preview.bodyFitBusy || state.preview.chestCorrectionBusy || state.preview.closeSeamsBusy;
      slider.setAttribute('aria-label', `${previewFitLabel(area.label)} ${channel}`);
      const output = document.createElement('output');
      output.value = Number(values[key]).toFixed(3);
      slider.addEventListener('input', () => setPreviewFitValue(key, slider.value, output));
      row.append(label, slider, output);
      section.append(row);
    }
    container.append(section);
  });
  const dimensionGroups = previewBodyDimensionGroups();
  if (dimensionGroups.length) {
    const divider = document.createElement('div');
    divider.className = 'preview-fit-divider';
    divider.textContent = 'Body dimensions';
    container.append(divider);
  }
  dimensionGroups.forEach((group) => {
    const section = document.createElement('section');
    section.className = 'preview-fit-area preview-dimension-area';
    const heading = document.createElement('strong');
    heading.textContent = previewFitLabel(group.label);
    section.append(heading);
    for (const axis of ['x', 'y', 'z']) {
      const key = group[axis];
      if (!key) continue;
      const current = Number(values[key]) || 0;
      const baseline = Number(state.preview.bodyFit[key]) || 0;
      const span = Math.max(2, Math.abs(baseline) * 0.5, Math.abs(current - baseline) * 1.25);
      const row = document.createElement('div');
      row.className = 'preview-fit-channel preview-dimension-channel';
      const label = document.createElement('label');
      label.textContent = axis;
      const slider = document.createElement('input');
      slider.type = 'range';
      slider.min = String(baseline - span);
      slider.max = String(baseline + span);
      slider.step = '0.01';
      slider.value = String(current);
      slider.disabled = state.resetting || state.preview.loading || state.preview.bodyFitBusy || state.preview.chestCorrectionBusy || state.preview.closeSeamsBusy;
      slider.setAttribute('aria-label', `${previewFitLabel(group.label)} ${axis.toUpperCase()}`);
      const input = document.createElement('input');
      input.type = 'number';
      input.step = '0.01';
      input.value = state.preview.bodyFitInvalidInputs.get(key) ?? formatBodyFitValue(current);
      input.setAttribute('aria-invalid', String(state.preview.bodyFitInvalidInputs.has(key)));
      input.disabled = slider.disabled;
      input.setAttribute('aria-label', `${previewFitLabel(group.label)} ${axis.toUpperCase()} value`);
      slider.addEventListener('input', () => {
        input.value = formatBodyFitValue(Number(slider.value));
        setPreviewFitValue(key, slider.value);
        input.setAttribute('aria-invalid', 'false');
      });
      input.addEventListener('input', () => {
        const valid = setPreviewFitValue(key, input.value);
        input.setAttribute('aria-invalid', String(!valid));
        if (!valid) return;
        const next = Number(input.value);
        if (next < Number(slider.min)) slider.min = String(next);
        if (next > Number(slider.max)) slider.max = String(next);
        slider.value = String(next);
      });
      row.append(label, slider, input);
      section.append(row);
    }
    container.append(section);
  });
  if (!areas.length && !dimensionGroups.length) {
    const empty = document.createElement('div');
    empty.className = 'preview-item-list';
    empty.textContent = 'No editable body-fit values found.';
    container.append(empty);
  }
  updatePreviewBodyFitActions();
}

function revertPreviewBodyFit() {
  if (state.resetting || bodyFitEditBusy(state.preview.bodyFitTarget || state.preview.loadedPath)) return;
  const changed = [...new Set([...state.preview.bodyFitDirtyFields, ...state.preview.bodyFitInvalidInputs.keys()])];
  state.preview.bodyFitDraft = {};
  state.preview.bodyFitDirtyFields.clear();
  state.preview.bodyFitInvalidInputs.clear();
  changed.forEach((key) => syncBodyFitDraftFromPreview(key, state.preview.bodyFit[key]));
  window.modelPreview?.setBodyFit(currentPreviewBodyFit());
  renderPreviewBodyFitControls();
  setPreviewBodyFitStatus('Reverted to the staged player values.');
}

function calculatePreviewBodyFit() {
  if (!state.preview.loadedPath || !state.preview.jerseyAvailable || state.preview.bodyFitInvalidInputs.size || bodyFitEditBusy(state.preview.loadedPath)) return;
  const result = window.modelPreview?.suggestBodyFit(currentPreviewBodyFit());
  if (!result) {
    setPreviewBodyFitStatus('The preview could not measure body-to-jersey clearance.', 'error');
    return;
  }
  for (const [key, value] of Object.entries(result.suggested || {})) {
    if (!Object.hasOwn(state.preview.bodyFit, key)) continue;
    setPreviewFitValue(key, value);
  }
  renderPreviewBodyFitControls();
  setPreviewBodyFitStatus(
    result.changed
      ? `Suggested ${result.changed} value${result.changed === 1 ? '' : 's'} from ${result.evaluated} jersey-covered area${result.evaluated === 1 ? '' : 's'}. Review before applying.`
      : `${result.evaluated} jersey-covered area${result.evaluated === 1 ? '' : 's'} measured with no tight-clearance correction needed.`,
    result.changed ? 'success' : '',
  );
}

async function applyPreviewBodyFit() {
  if (state.resetting || !state.workingPlayer || bodyFitEditBusy(state.workingPlayer.path) || state.preview.bodyFitInvalidInputs.size || !state.preview.bodyFitDirtyFields.size) return;
  const target = state.workingPlayer, stageId = state.workingGeneration;
  const editId = ++state.preview.editRequestId, previewId = state.preview.requestId;
  const ownsEdit = () => editId === state.preview.editRequestId && target === state.workingPlayer && stageId === state.workingGeneration;
  const isCurrent = () => ownsEdit() && previewId === state.preview.requestId;
  const changes = {};
  state.preview.bodyFitDirtyFields.forEach((key) => { changes[key] = state.preview.bodyFitDraft[key]; });
  state.preview.bodyFitBusy = true;
  renderPreviewBodyFitControls();
  renderPreviewControls();
  updateBodyFitActions();
  setPreviewBodyFitStatus('Writing body-fit changes...');
  try {
    const result = await window.characterTool.saveBodyFit({
      target: target.path,
      changes,
    });
    invalidatePlayerRender(target.path);
    if (!isCurrent()) { if (ownsEdit()) state.preview.dirty = true; return; }
    state.preview.bodyFit = result.targetBodyFit || currentPreviewBodyFit();
    state.preview.bodyFitDraft = {};
    state.preview.bodyFitDirtyFields.clear();
    if (state.body.targetPlayer && samePlayerPath(state.body.targetPlayer.path, state.workingPlayer.path)) {
      state.body.targetBodyFit = { ...state.preview.bodyFit };
      state.body.targetDraft = {};
      state.body.dirtyFields.clear();
      state.body.invalidFields.clear();
      renderBodyFitRows();
    }
    setPreviewBodyFitStatus(`Saved ${result.changed.length} body-fit value${result.changed.length === 1 ? '' : 's'}.`, 'success');
    addLog(`Saved ${result.changed.length} preview body-fit value${result.changed.length === 1 ? '' : 's'} to ${target.name}.`, 'success');
    setPreviewBodyFitOpen(false);
  } catch (error) {
    if (!isCurrent()) return;
    setPreviewBodyFitStatus(error.message, 'error');
    addLog(error.message, 'error');
  } finally {
    if (!ownsEdit()) return;
    state.preview.bodyFitBusy = false;
    renderPreviewBodyFitControls();
    renderPreviewControls();
    updateBodyFitActions();
  }
}

function resetPreviewDetails() {
  $('previewConfig').textContent = 'No configuration';
  $('previewMeshCount').textContent = '0';
  $('previewTriangleCount').textContent = '0';
  $('previewTextureCount').textContent = '0';
  $('previewCompanionCount').textContent = '0';
  $('previewActiveItems').textContent = 'None loaded';
  $('previewSelectedMesh').textContent = 'Click a mesh in the render';
  $('previewSelectedTarget').textContent = '';
  syncPreviewBlenderState();
  $('previewWarnings').replaceChildren();
  $('previewWarnings').hidden = true;
  renderPreviewBodyFitControls();
}

function resetPreviewState() {
  window.characterTool.cancelStagedPreview?.().catch(() => {});
  state.preview.requestId += 1;
  state.preview.editRequestId += 1;
  state.preview.loading = false;
  state.preview.blenderBusy = false;
  state.preview.blenderSessionId = '';
  state.preview.loadedPath = '';
  state.preview.dirty = true;
  state.preview.wireframe = false;
  state.preview.jersey = false;
  state.preview.jerseyAvailable = false;
  state.preview.chestCorrectionBusy = false;
  state.preview.closeSeamsBusy = false;
  state.preview.blenderHair = false;
  state.preview.blenderFacialHair = false;
  state.preview.blenderTextures = false;
  state.preview.bodyFit = {};
  state.preview.bodyFitTarget = '';
  state.preview.bodyFitDraft = {};
  state.preview.bodyFitDirtyFields.clear();
  state.preview.bodyFitBusy = false;
  state.preview.bodyFitOpen = false;
  state.preview.bodyFitInvalidInputs.clear();
  $('previewWireframe').classList.remove('active');
  $('previewEyelashes').checked = false;
  $('previewJersey').checked = false;
  $('previewBlenderHair').checked = false;
  $('previewBlenderFacialHair').checked = false;
  $('previewBlenderTextures').checked = false;
  $('previewLoading').hidden = true;
  $('previewEmpty').hidden = false;
  resetPreviewDetails();
  window.modelPreview?.clear();
  setPreviewStatus('Waiting for a staged player.');
}

type PreviewBlenderSession = { sessionId: string; handoffId: string; phase: string; title: string; detail: string };
const previewBlenderSessions = new Map<string, PreviewBlenderSession>();
const previewBlenderTarget = (target: string) => String(target || '').replaceAll('/', '\\').toLowerCase();

function syncPreviewBlenderState() {
  const session = previewBlenderSessions.get(previewBlenderTarget(state.workingPlayer?.path));
  state.preview.blenderBusy = Boolean(session);
  state.preview.blenderSessionId = session?.sessionId || '';
  setPreviewBlenderStatus(session?.title || 'Ready', session?.detail || 'Open the staged player for fine adjustments.');
}

async function handlePreviewBlenderEvent(event: import('../contracts').EngineEvent) {
  const target = previewBlenderTarget(String(event.requestedTarget || event.target || ''));
  const session = previewBlenderSessions.get(target);
  if (!session || !['opened', 'returning', 'complete', 'closed', 'error'].includes(event.state || '')) return;
  const handoffId = String(event.handoffId || ''), sessionId = String(event.sessionId || '');
  if (session.handoffId && session.handoffId !== handoffId || session.sessionId && sessionId && session.sessionId !== sessionId) return;
  if (handoffId) session.handoffId = handoffId;
  if (sessionId) session.sessionId = sessionId;
  const terminal = ['complete', 'closed', 'error'].includes(event.state);
  if (terminal) previewBlenderSessions.delete(target);
  else {
    session.phase = event.state;
    session.title = event.state === 'returning' ? 'Applying Blender changes' : 'Editing in Blender';
    session.detail = event.state === 'returning' ? event.message || '' : 'Use Send Tweaks Back to Character Mod Tool when finished.';
  }
  if (target !== previewBlenderTarget(state.workingPlayer?.path)) return;
  const stageId = state.workingGeneration;
  syncPreviewBlenderState();
  if (event.state === 'returning') setPreviewStatus(event.message);
  else if (event.state === 'complete') {
    invalidatePlayerRender(String(event.requestedTarget || event.target || ''));
    setPreviewBlenderStatus('Changes received', event.eyesRecalculated
      ? 'The staged IFF, eye centers, mathnodes, and preview were updated.' : 'The staged IFF and preview were updated.');
    addLog(event.message, 'success');
    await loadStagedPreview(true);
  } else if (event.state === 'closed' || event.state === 'error') {
    const failed = event.state === 'error';
    setPreviewBlenderStatus(failed ? 'Blender return failed' : 'No changes received', event.message);
    setPreviewStatus(event.message, failed ? 'error' : '');
    addLog(failed ? `Blender: ${event.message}` : event.message, failed ? 'error' : 'warning');
  }
  if (stageId === state.workingGeneration && target === previewBlenderTarget(state.workingPlayer?.path)) renderPreviewControls();
}

function renderPreviewControls() {
  updateStagePlayerControls();
  window.configsEditor?.renderPreviewSelector();
  const enabled = Boolean(!state.resetting && state.workingPlayer && !state.preview.loading && state.preview.loadedPath);
  $('refreshPreview').disabled = !state.workingPlayer || state.resetting || state.preview.loading
    || state.preview.bodyFitBusy || state.preview.chestCorrectionBusy || Boolean(state.preview.closeSeamsBusy);
  $('openPreviewInBlender').disabled = !enabled || state.preview.blenderBusy || state.preview.closeSeamsBusy;
  $('openPreviewInBlender').textContent = state.preview.blenderBusy
    ? 'Waiting for Blender...'
    : 'Open in Blender 5.1';
  $('previewFitView').disabled = !enabled;
  $('previewWireframe').disabled = !enabled;
  $('previewEyelashes').disabled = !enabled;
  $('previewJersey').disabled = !enabled || !state.preview.jerseyAvailable;
  $('previewBlenderHair').disabled = !enabled;
  $('previewBlenderFacialHair').disabled = !enabled;
  $('previewBlenderTextures').disabled = !enabled;
  $('previewApplyChestCorrection').disabled = (
    !enabled
    || !state.preview.jerseyAvailable
    || state.preview.chestCorrectionBusy
    || state.preview.bodyFitBusy
    || state.preview.closeSeamsBusy
  );
  $('previewApplyChestCorrection').textContent = state.preview.chestCorrectionBusy
    ? 'Applying in Blender...'
    : 'Apply Chest Fix';
  updatePreviewBodyFitActions();
}

function setPreviewBlenderStatus(title, detail) {
  $('previewBlenderStatus').textContent = title;
  $('previewBlenderDetail').textContent = detail;
}

async function openPreviewInBlender() {
  if (state.resetting || !state.workingPlayer || state.preview.closeSeamsBusy) return;
  const player = state.workingPlayer, target = previewBlenderTarget(player.path);
  if (previewBlenderSessions.has(target)) return;
  const session: PreviewBlenderSession = { sessionId: '', handoffId: '', phase: 'opening', title: 'Opening Blender 5.1', detail: 'Preparing the staged player and return session.' };
  previewBlenderSessions.set(target, session);
  const ownsSession = () => previewBlenderSessions.get(target) === session;
  const isVisible = () => target === previewBlenderTarget(state.workingPlayer?.path);
  syncPreviewBlenderState();
  setPreviewStatus(`Opening ${player.name} in Blender 5.1...`);
  renderPreviewControls();
  try {
    const result = await window.characterTool.openPreviewInBlender({
      target: player.path,
      config: window.configsEditor?.active(),
      includeJersey: state.preview.jersey,
      includeHair: state.preview.blenderHair,
      includeFacialHair: state.preview.blenderFacialHair,
      includeTextures: state.preview.blenderTextures,
    });
    if (!ownsSession()) return;
    session.sessionId = result.sessionId;
    session.handoffId = String(result.handoffId || session.handoffId);
    if (session.phase === 'returning') return;
    session.phase = 'opened';
    session.title = 'Editing in Blender';
    session.detail = result.referenceCount
        ? `Loaded ${result.referenceCount} selected hair/facial-hair reference mesh(es). They are excluded from export.`
        : result.textureCount
          ? `Loaded ${result.textureCount} staged texture(s) on the editable player. Geometry export remains unchanged.`
        : result.jerseyLoaded
          ? 'Static jersey loaded for inspection. It will not be exported with the player.'
        : 'Use the Character Tool sidebar to send your adjustments back.';
    if (!isVisible()) return;
    syncPreviewBlenderState();
    setPreviewStatus(
      result.referenceCount
        ? 'Blender is open with the selected staged-player reference assets.'
        : result.textureCount
          ? 'Blender is open with staged-player preview textures applied.'
        : result.jerseyLoaded
          ? 'Blender is open with the static jersey reference. It is excluded from Send Tweaks Back.'
        : 'Blender is open. The app will refresh automatically when changes return.',
    );
    addLog(`${player.name} opened in Blender 5.1.`, 'success');
  } catch (error) {
    if (!ownsSession()) return;
    previewBlenderSessions.delete(target);
    if (!isVisible()) return;
    syncPreviewBlenderState();
    setPreviewBlenderStatus('Could not open Blender', error.message);
    setPreviewStatus(error.message, 'error');
    addLog(`Blender: ${error.message}`, 'error');
  } finally {
    if (isVisible()) renderPreviewControls();
  }
}

function setupPreviewSeamAction() {
  const button = document.createElement('button');
  button.id = 'previewCloseSeams';
  button.type = 'button';
  button.className = 'button secondary';
  button.disabled = true;
  const label = document.createElement('span');
  label.textContent = 'Close Seams';
  button.append(label);
  $('previewApplyChestCorrection').before(button);
  button.addEventListener('click', closePreviewSeams);
  updatePreviewSeamAction();
}

async function closePreviewSeams() {
  if (previewSeamBlockReason()) return;
  const target = state.workingPlayer, stageId = state.workingGeneration;
  const editId = ++state.preview.editRequestId, previewId = state.preview.requestId;
  const ownsEdit = () => editId === state.preview.editRequestId && target === state.workingPlayer
    && stageId === state.workingGeneration && !state.resetting;
  state.preview.closeSeamsBusy = true;
  renderPreviewControls();
  updateBodyFitActions();
  setPreviewStatus(`Closing seams on ${target.name}...`);
  try {
    const result = await window.characterTool.closePreviewSeams({ target: target.path });
    invalidatePlayerRender(target.path);
    if (!ownsEdit() || previewId !== state.preview.requestId) return;
    const movement = typeof result.maxVertexMove === 'number' ? ` Largest vertex move ${result.maxVertexMove.toPrecision(4)}.` : '';
    const summary = result.applied
      ? `Closed seams on ${target.name}: ${result.correctedVertices.toLocaleString()} vertices, ${result.pairs.toLocaleString()} pairs. Gap ${result.maxGapBefore.toPrecision(4)} to ${result.maxGapAfter.toPrecision(4)}.${movement}`
      : result.pairs > 0
        ? `Close Seams: no changes needed for ${target.name}.`
        : `Close Seams: no safe seam pairs found for ${target.name}; no changes made.`;
    addLog(summary, result.applied ? 'success' : 'info');
    if (result.backup) addLog(`Seam backup: ${result.backup}`, 'info');
    result.warnings.forEach(warning => addLog(`Close Seams: ${warning}`, 'warning'));
    const reload = loadStagedPreview(true), reloadId = state.preview.requestId;
    await reload;
    if (!ownsEdit() || reloadId !== state.preview.requestId) return;
    // Keep a preview failure visible even when the file edit itself succeeded.
    if (!state.preview.dirty && samePlayerPath(state.preview.loadedPath, target.path)) {
      setPreviewStatus([summary, ...result.warnings].join(' '), result.warnings.length ? 'warning' : result.applied ? 'success' : '');
    }
  } catch (error) {
    if (!ownsEdit()) return;
    setPreviewStatus(error.message, 'error');
    addLog(`Close Seams: ${error.message}`, 'error');
  } finally {
    if (!ownsEdit()) return;
    state.preview.closeSeamsBusy = false;
    renderPreviewControls();
    updateBodyFitActions();
  }
}

async function applyPreviewChestCorrection() {
  if (state.resetting || !state.workingPlayer || bodyFitEditBusy(state.workingPlayer.path) || !state.preview.jerseyAvailable) return;
  const target = state.workingPlayer, stageId = state.workingGeneration;
  const editId = ++state.preview.editRequestId, previewId = state.preview.requestId;
  const ownsEdit = () => editId === state.preview.editRequestId && target === state.workingPlayer && stageId === state.workingGeneration;
  const isCurrent = () => ownsEdit() && previewId === state.preview.requestId;
  state.preview.chestCorrectionBusy = true;
  renderPreviewControls();
  updateBodyFitActions();
  setPreviewStatus('Blender is fitting the jersey-covered body surface inside the static jersey...');
  try {
    const result = await window.characterTool.applyPreviewChestCorrection({
      target: target.path,
    });
    invalidatePlayerRender(target.path);
    if (!isCurrent()) { if (ownsEdit()) state.preview.dirty = true; return; }
    state.preview.dirty = true;
    const reload = loadStagedPreview(true), reloadId = state.preview.requestId;
    await reload;
    if (!ownsEdit() || reloadId !== state.preview.requestId) return;
    setPreviewStatus(
      `Blender fitted ${Number(result.corrected || 0).toLocaleString()} jersey-covered vertices and reloaded the player.`,
      'success',
    );
    addLog(`Applied the static-jersey chest fix to ${target.name}.`, 'success');
  } catch (error) {
    if (!ownsEdit()) return;
    setPreviewStatus(error.message, 'error');
    addLog(`Chest correction: ${error.message}`, 'error');
  } finally {
    if (!ownsEdit()) return;
    state.preview.chestCorrectionBusy = false;
    renderPreviewControls();
    updateBodyFitActions();
  }
}

function renderPreviewWarnings(messages) {
  const warnings = $('previewWarnings');
  warnings.replaceChildren();
  const visible = (messages || []).slice(0, 4);
  visible.forEach((message) => {
    const line = document.createElement('p');
    line.textContent = message;
    warnings.append(line);
  });
  if ((messages || []).length > visible.length) {
    const line = document.createElement('p');
    line.textContent = `${messages.length - visible.length} additional preview warning(s).`;
    warnings.append(line);
  }
  warnings.hidden = !warnings.childElementCount;
}

function previewBodyFitDraft(draft: Record<string, unknown>, baseline: Record<string, number>) {
  return Object.fromEntries(Object.entries(draft).filter((entry): entry is [string, number] => {
    const [key, value] = entry;
    return Object.hasOwn(baseline, key) && typeof value === 'number'
      && Number.isFinite(value) && Number.isFinite(Math.fround(value));
  }));
}

async function loadStagedPreview(force = false) {
  if (!state.workingPlayer) {
    resetPreviewState();
    renderWorkingPlayer();
    return;
  }
  if (
    !force
    && !state.preview.dirty
    && state.preview.loadedPath === state.workingPlayer.path
  ) {
    requestAnimationFrame(() => window.modelPreview?.resize());
    renderPreviewControls();
    return;
  }

  const requestId = ++state.preview.requestId;
  state.preview.loading = true;
  $('previewEmpty').hidden = true;
  $('previewLoading').hidden = false;
  $('previewLoadingDetail').textContent = 'Reading the active appearance and local companions.';
  setPreviewStatus(`Rendering ${state.workingPlayer.name}...`);
  renderWorkingPlayer();
  renderPreviewControls();
  try {
    const preview = await window.characterTool.loadStagedPreview({
      target: state.workingPlayer.path,
      config: window.configsEditor?.active(),
    });
    if (requestId !== state.preview.requestId) return;
    if (!window.modelPreview) throw new Error('The local Three.js preview renderer did not initialize.');
    const ownsPreviewDraft = samePlayerPath(state.preview.bodyFitTarget || state.preview.loadedPath, state.workingPlayer.path);
    const previousDraft = ownsPreviewDraft
      ? { ...state.preview.bodyFitDraft }
      : {};
    const stagedBodyFit = { ...(preview.player?.bodyFit || {}) };
    const bodyDraft = state.body.targetPlayer
      && samePlayerPath(state.body.targetPlayer.path, state.workingPlayer.path)
      ? state.body.targetDraft
      : {};
    const finiteBodyDraft = previewBodyFitDraft(bodyDraft, stagedBodyFit);
    // Invalid text must not replace the last finite value used by the renderer.
    const activeDraft = { ...previewBodyFitDraft(previousDraft, stagedBodyFit), ...finiteBodyDraft };
    state.preview.bodyFit = stagedBodyFit;
    state.preview.bodyFitTarget = state.workingPlayer.path;
    state.preview.bodyFitInvalidInputs = new Map([...state.preview.bodyFitInvalidInputs]
      .filter(([key]) => ownsPreviewDraft && Object.hasOwn(stagedBodyFit, key)));
    for (const [key, value] of Object.entries(bodyDraft)) {
      if (!Object.hasOwn(stagedBodyFit, key)) continue;
      if (Object.hasOwn(finiteBodyDraft, key)) state.preview.bodyFitInvalidInputs.delete(key);
      else state.preview.bodyFitInvalidInputs.set(key, typeof value === 'string' ? value : String(value ?? ''));
    }
    state.preview.bodyFitDraft = {};
    state.preview.bodyFitDirtyFields.clear();
    for (const [key, value] of Object.entries(activeDraft)) {
      if (!Object.hasOwn(stagedBodyFit, key) || bodyFitValuesEqual(value, stagedBodyFit[key])) continue;
      state.preview.bodyFitDraft[key] = value;
      state.preview.bodyFitDirtyFields.add(key);
    }
    $('previewLoadingDetail').textContent = 'Building geometry and applying decoded materials.';
    const loaded = await window.modelPreview.load(preview, (progress) => {
      if (requestId !== state.preview.requestId) return;
      const total = Number(progress.total) || 0;
      $('previewLoadingDetail').textContent = total
        ? `${progress.phase === 'textures' ? 'Applying textures' : 'Building meshes'} ${progress.completed}/${total}`
        : 'Building staged player render.';
    });
    if (requestId !== state.preview.requestId) return;
    if (loaded.cancelled) {
      state.preview.dirty = true;
      state.preview.loadedPath = '';
      $('previewEmpty').hidden = false;
      resetPreviewDetails();
      return;
    }
    if (!preview.meshes.length) throw new Error('No supported player meshes were found in the staged package.');
    // Geometry must establish its staged baseline before applying the latest draft.
    const displayBodyFit = { ...stagedBodyFit, ...previewBodyFitDraft(currentPreviewBodyFit(), stagedBodyFit) };
    if (Object.keys(stagedBodyFit).some((key) => !bodyFitValuesEqual(displayBodyFit[key], stagedBodyFit[key]))) {
      window.modelPreview.setBodyFit(displayBodyFit);
    }
    state.preview.loadedPath = state.workingPlayer.path;
    state.preview.dirty = false;
    state.preview.jerseyAvailable = Boolean(preview.player?.jerseyAvailable);
    if (!state.preview.jerseyAvailable) {
      state.preview.jersey = false;
      $('previewJersey').checked = false;
    }
    window.modelPreview.setJerseyVisible(state.preview.jersey);
    window.modelPreview.setChestCorrection(false);
    renderPreviewBodyFitControls();
    setPreviewBodyFitStatus(
      state.preview.bodyFitInvalidInputs.size
        ? 'Enter a finite value within the 32-bit game-float range.'
        : state.preview.bodyFitDirtyFields.size
        ? `${state.preview.bodyFitDirtyFields.size} staged change${state.preview.bodyFitDirtyFields.size === 1 ? '' : 's'}.`
        : 'Live preview matches the staged player values.',
      state.preview.bodyFitInvalidInputs.size ? 'error' : '',
    );
    $('previewConfig').textContent = preview.player?.defaultConfig || 'Default configuration';
    $('previewMeshCount').textContent = Number(preview.stats.modelsDecoded || 0).toLocaleString();
    $('previewTriangleCount').textContent = Number(preview.stats.triangles || 0).toLocaleString();
    $('previewTextureCount').textContent = Number(loaded.decodedTextures || 0).toLocaleString();
    $('previewCompanionCount').textContent = Number(preview.stats.companions || 0).toLocaleString();
    $('previewActiveItems').textContent = preview.activeItems?.length
      ? preview.activeItems.join(' | ')
      : 'Base player only';
    renderPreviewWarnings(preview.skipped);
    $('previewEmpty').hidden = true;
    setPreviewStatus(`Rendered ${preview.name} with ${preview.stats.companions} active companion file(s).`, 'success');
    requestAnimationFrame(() => window.modelPreview.resize());
  } catch (error) {
    if (requestId !== state.preview.requestId) return;
    state.preview.loadedPath = '';
    $('previewEmpty').hidden = false;
    $('previewEmpty').querySelector('strong').textContent = 'Preview unavailable';
    $('previewEmpty').querySelector('span').textContent = error.message;
    setPreviewStatus(error.message, 'error');
    addLog(`Preview: ${error.message}`, 'error');
  } finally {
    if (requestId !== state.preview.requestId) return;
    state.preview.loading = false;
    $('previewLoading').hidden = true;
    renderWorkingPlayer();
    renderPreviewControls();
  }
}

function selectedSourceHeadband() {
  return state.headband.sourceHeadbands.find((item) => item.path === $('headbandSource').value) || null;
}

function renderHeadbandControls() {
  const ready = Boolean(state.workingPlayer && state.headband.configs.length && state.headband.styles.length);
  const source = selectedSourceHeadband();
  $('refreshHeadbands').disabled = state.headband.loading;
  $('headbandSource').disabled = state.headband.loading || !state.headband.sourceHeadbands.length;
  $('headbandStyle').disabled = state.headband.loading || !state.headband.styles.length;
  $('headbandConfig').disabled = state.headband.loading || !state.headband.configs.length;
  $('headbandColor').disabled = state.headband.loading;
  $('headbandLogo').disabled = state.headband.loading;
  $('headbandLogoUpsideDown').disabled = state.headband.loading || !$('headbandLogo').checked;
  $('transferHeadband').disabled = state.resetting || state.headband.loading || !ready || !source;
  $('addHeadband').disabled = state.resetting || state.headband.loading || !ready;
  $('headbandSourceDetails').textContent = source
    ? `${source.name} | ${source.details}`
    : state.workingPlayer
      ? 'No transferable headband was found beside the Full Swap source.'
      : 'Select a staged player to inspect its Full Swap source.';
}

async function refreshHeadbandOptions() {
  if (state.headband.applying) return;
  const requestId = ++state.headband.requestId;
  state.headband.sourceHeadbands = [];
  state.headband.configs = [];
  state.headband.styles = [];
  if (!state.workingPlayer) {
    $('headbandSource').replaceChildren(new Option('No source headband detected', ''));
    $('headbandStyle').replaceChildren();
    $('headbandConfig').replaceChildren();
    $('headbandSourceCount').textContent = 'Not loaded';
    $('headbandStatus').className = 'headband-status';
    $('headbandStatus').textContent = 'Waiting for a staged player.';
    renderWorkingPlayer();
    renderHeadbandControls();
    return;
  }
  state.headband.loading = true;
  $('headbandStatus').className = 'headband-status';
  $('headbandStatus').textContent = 'Inspecting staged player and source headbands...';
  renderHeadbandControls();
  try {
    const result = await window.characterTool.loadHeadbandOptions({
      target: state.workingPlayer.path,
      source: state.workingSource?.path || '',
    });
    if (requestId !== state.headband.requestId) return;
    state.headband.sourceHeadbands = result.sourceHeadbands || [];
    state.headband.configs = result.configs || [];
    state.headband.styles = result.styles || [];
    const sourceSelect = $('headbandSource');
    sourceSelect.replaceChildren();
    state.headband.sourceHeadbands.forEach((item) => sourceSelect.add(new Option(item.name, item.path)));
    if (!state.headband.sourceHeadbands.length) sourceSelect.add(new Option('No source headband detected', ''));
    const styleSelect = $('headbandStyle');
    styleSelect.replaceChildren();
    state.headband.styles.forEach((style) => styleSelect.add(new Option(style, style)));
    const configSelect = $('headbandConfig');
    configSelect.replaceChildren();
    state.headband.configs.forEach((config) => configSelect.add(new Option(config, config)));
    if (result.defaultConfig && state.headband.configs.includes(result.defaultConfig)) configSelect.value = result.defaultConfig;
    if (state.headband.configs.includes(window.configsEditor?.active())) configSelect.value = window.configsEditor.active();
    const source = selectedSourceHeadband();
    if (source?.styleHint && state.headband.styles.includes(source.styleHint)) styleSelect.value = source.styleHint;
    $('headbandSourceCount').textContent = `${state.headband.sourceHeadbands.length} detected`;
    $('headbandStatus').className = 'headband-status success';
    $('headbandStatus').textContent = state.headband.configs.length
      ? `${state.headband.configs.length} target configuration${state.headband.configs.length === 1 ? '' : 's'} ready.`
      : 'The staged player has no usable appearance configurations.';
  } catch (error) {
    if (requestId !== state.headband.requestId) return;
    $('headbandStatus').className = 'headband-status error';
    $('headbandStatus').textContent = error.message;
    addLog(error.message, 'error');
  } finally {
    if (requestId !== state.headband.requestId) return;
    state.headband.loading = false;
    renderHeadbandControls();
  }
}

async function applyHeadband(mode) {
  const source = selectedSourceHeadband();
  if (state.resetting || !state.workingPlayer || state.headband.loading || (mode === 'transfer' && !source)) return;
  const target = state.workingPlayer, stageId = state.workingGeneration;
  const requestId = ++state.headband.requestId;
  const ownsStage = () => target === state.workingPlayer && stageId === state.workingGeneration;
  const isCurrent = () => ownsStage() && requestId === state.headband.requestId;
  let finalClass = '';
  let finalMessage = '';
  state.headband.loading = true;
  state.headband.applying = true;
  renderHeadbandControls();
  $('headbandStatus').className = 'headband-status';
  $('headbandStatus').textContent = mode === 'transfer'
    ? 'Adding the modern target headband and transferring the source shape...'
    : 'Adding and fitting the selected headband...';
  try {
    const result = await window.characterTool.applyHeadband({
      mode,
      target: target.path,
      sourceHeadband: source?.path || '',
      style: $('headbandStyle').value,
      config: $('headbandConfig').value,
      configScoped: true,
      color: $('headbandColor').value.toUpperCase(),
      addLogo: $('headbandLogo').checked,
      upsideDown: $('headbandLogoUpsideDown').checked,
    });
    invalidatePlayerRender(target.path);
    if (!isCurrent()) return;
    finalClass = 'success';
    finalMessage = mode === 'transfer'
      ? `Transferred ${basename(result.sourceHeadband)} to ${basename(result.headband)}.`
      : `Added ${result.style} as ${basename(result.headband)}.`;
    state.preview.dirty = true;
    addLog(`Updated ${basename(result.target)} and created ${basename(result.headband)}.`, 'success');
    await window.configsEditor?.refresh();
  } catch (error) {
    if (!isCurrent()) return;
    finalClass = 'error';
    finalMessage = error.message;
    addLog(error.message, 'error');
  } finally {
    if (!isCurrent()) return;
    state.headband.loading = false;
    state.headband.applying = false;
    const refresh = refreshHeadbandOptions(), refreshId = state.headband.requestId;
    await refresh;
    if (ownsStage() && refreshId === state.headband.requestId && finalMessage) {
      $('headbandStatus').className = `headband-status ${finalClass}`;
      $('headbandStatus').textContent = finalMessage;
    }
  }
}

async function runSwap() {
  if (!state.source || !state.target || !state.outputDir || state.startingJob || state.activeJob) return;
  if (!facialHairSelectionReady()) {
    updateReadiness();
    return;
  }
  state.lastOutput = '';
  state.startingJob = true;
  state.pendingTerminalEvent = null;
  $('openLastOutput').disabled = true;
  $('cancelSwap').disabled = true;
  setProgress('Starting Full Swap...', 'running');
  updateReadiness();
  try {
    const job = await window.characterTool.startSwap({
      source: state.source.path,
      target: state.target.path,
      outputDir: state.outputDir,
      sourceGame: state.source.gameLabel || '',
      targetGame: state.target.gameLabel || '',
      shrinkwrapBody: $('shrinkwrapBody').checked,
      renameId: $('renamePackage').checked ? $('renameId').value.trim() : '',
      hairEnabled: $('includeHair').checked,
      hairSource: state.hair.sourcePath,
      hairTargetKey: $('hairSlot').value,
      hairOutputName: $('includeHair').checked ? $('hairOutputName').value.trim() : '',
      facialHairEnabled: $('includeFacialHair').checked,
      facialHairSource: state.facialHair.sourcePath,
      facialHairTargetKey: $('facialHairSlot').value,
      facialHairOutputName: $('includeFacialHair').checked ? $('facialHairOutputName').value.trim() : '',
    });
    state.activeJob = job.jobId;
    state.startingJob = false;
    $('cancelSwap').disabled = false;
    addLog('Full Swap started.', 'success');
    if ($('includeHair').checked) {
      addLog(
        `Hair included: ${state.hair.generation} ${state.hair.sourceKey} -> ${state.hair.targetGame} ${$('hairSlot').value}.`,
        'success',
      );
    }
    if ($('includeFacialHair').checked) {
      addLog(
        `Facial hair included: ${state.facialHair.generation} ${state.facialHair.sourceKey} -> ${state.facialHair.targetGame} ${$('facialHairSlot').value}.`,
        'success',
      );
    }
    updateReadiness();

    const terminal = state.pendingTerminalEvent;
    state.pendingTerminalEvent = null;
    if (terminal?.event?.jobId === state.activeJob) {
      if (terminal.type === 'complete') await handleJobComplete(terminal.event);
      else handleJobFailed(terminal.event);
    }
  } catch (error) {
    state.startingJob = false;
    state.activeJob = '';
    state.pendingTerminalEvent = null;
    $('cancelSwap').disabled = true;
    setProgress('Full Swap could not start', 'idle');
    addLog(error.message, 'error');
    updateReadiness();
  }
}

async function cancelSwap() {
  if (!state.activeJob) return;
  await window.characterTool.cancelSwap(state.activeJob);
  addLog('Cancellation requested.', 'warning');
}

async function handleJobComplete(event) {
  if (event.jobId !== state.activeJob) return;
  state.activeJob = '';
  state.startingJob = false;
  state.lastOutput = event.output || event.result?.output || '';
  $('cancelSwap').disabled = true;
  $('openLastOutput').disabled = !state.lastOutput;
  setProgress('Full Swap completed', 'complete', '100%');
  addLog(`Created ${basename(state.lastOutput)}`, 'success');
  try {
    await setWorkingPlayer(state.lastOutput, state.source, state.target);
    addLog(`${basename(state.lastOutput)} is now the active working player.`, 'success');
  } catch (error) {
    addLog(`Could not stage the Full Swap result: ${error.message}`, 'error');
  }
  updateReadiness();
}

function handleJobFailed(event) {
  if (event.jobId !== state.activeJob) return;
  state.activeJob = '';
  state.startingJob = false;
  $('cancelSwap').disabled = true;
  $('runSwap').hidden = false;
  $('runSwap').textContent = 'Run Full Swap';
  setProgress(event.cancelled ? 'Cancelled' : 'Full Swap failed', 'idle');
  addLog(event.message, event.cancelled ? 'warning' : 'error');
  updateReadiness();
}

function bindEvents() {
  setupStagePlayer();
  setupPreviewSeamAction();
  $('bodyTab').addEventListener('click', () => setActiveView('body'));
  $('hairTab').addEventListener('click', () => setActiveView('hair'));
  $('headbandTab').addEventListener('click', () => setActiveView('headband'));
  $('previewTab').addEventListener('click', () => setActiveView('preview'));
  $('cliTab').addEventListener('click', () => setActiveView('cli'));
  $('healthTab').addEventListener('click', () => setActiveView('health'));
  $('runHealthCheck').addEventListener('click', runHealthCheck);
  $('newSwap').addEventListener('click', resetFullSwapSession);
  $('readRosterPlayer').addEventListener('click', readRosterPlayer);
  $('cliGame').addEventListener('change', () => {
    state.cli.game = $('cliGame').value;
    state.cli.player = null;
    renderRosterPlayer(null);
    syncRosterEditValue();
  });
  $('cliEditField').addEventListener('change', syncRosterEditValue);
  $('cliEditValue').addEventListener('input', updateRosterFieldAction);
  $('cliEditValue').addEventListener('keydown', (event) => {
    if (event.key === 'Enter' && !$('applyRosterField').disabled) applyRosterField();
  });
  $('applyRosterField').addEventListener('click', applyRosterField);
  $('saveCliLanguage').addEventListener('click', saveRosterLanguage);
  document.querySelectorAll<HTMLButtonElement>('[data-browse]').forEach((button) => {
    button.addEventListener('click', () => selectFile(button.dataset.browse));
  });
  document.querySelectorAll<HTMLButtonElement>('[data-manifest]').forEach((button) => {
    button.addEventListener('click', () => openManifest(button.dataset.manifest));
  });
  $('selectTemplate').addEventListener('click', openTemplatePicker);
  document.querySelectorAll<HTMLButtonElement>('[data-game]').forEach((button) => {
    button.addEventListener('click', () => {
      if (!button.disabled) loadManifest(button.dataset.game);
    });
  });
  $('browseWorkingPlayer').addEventListener('click', openStagePlayer);
  $('openWorkingPlayer').addEventListener('click', () => {
    if (state.workingPlayer) window.characterTool.showItem(state.workingPlayer.path);
  });
  $('bodyFitSearch').addEventListener('input', renderBodyFitRows);
  $('bodyFitFilter').addEventListener('change', renderBodyFitRows);
  $('refreshBodyFit').addEventListener('click', () => refreshBodyFit());
  $('saveBodyFit').addEventListener('click', saveBodyFit);
  $('applyChestFix').addEventListener('click', applyChestFix);
  $('nativeHeadHair').addEventListener('click', () => setNativeHairType('hair'));
  $('nativeFacialHair').addEventListener('click', () => setNativeHairType('facialhair'));
  $('nativeHairSearch').addEventListener('input', renderNativeHairRows);
  $('nativeHairFavoritesOnly').addEventListener('click', () => {
    state.nativeHair.favoritesOnly = !state.nativeHair.favoritesOnly;
    renderNativeHairFavoritesFilter();
    renderNativeHairRows();
  });
  window.addEventListener('model-preview:camera:hairCatalog', (event) => {
    if (state.activeView === 'hair') saveNativeHairCamera(event.detail);
  });
  $('nativeHairSlot').addEventListener('change', updateNativeHairInstallState);
  $('nativeHairCopyItem').addEventListener('change', updateNativeHairInstallState);
  $('installNativeHair').addEventListener('click', installNativeHair);
  $('refreshHeadbands').addEventListener('click', refreshHeadbandOptions);
  $('headbandSource').addEventListener('change', () => {
    const source = selectedSourceHeadband();
    if (source?.styleHint && state.headband.styles.includes(source.styleHint)) {
      $('headbandStyle').value = source.styleHint;
    }
    renderHeadbandControls();
  });
  $('headbandLogo').addEventListener('change', renderHeadbandControls);
  $('transferHeadband').addEventListener('click', () => applyHeadband('transfer'));
  $('addHeadband').addEventListener('click', () => applyHeadband('add'));
  $('refreshPreview').addEventListener('click', () => {
    if (state.resetting || state.preview.loading || bodyFitEditBusy(state.workingPlayer?.path)) return;
    loadStagedPreview(true);
  });
  $('openPreviewInBlender').addEventListener('click', openPreviewInBlender);
  $('previewFitView').addEventListener('click', () => window.modelPreview?.fitCamera());
  $('previewWireframe').addEventListener('click', () => {
    state.preview.wireframe = !state.preview.wireframe;
    $('previewWireframe').classList.toggle('active', state.preview.wireframe);
    window.modelPreview?.setWireframe(state.preview.wireframe);
  });
  $('previewEyelashes').addEventListener('change', () => {
    window.modelPreview?.setEyelashesVisible($('previewEyelashes').checked);
  });
  $('previewJersey').addEventListener('change', () => {
    state.preview.jersey = $('previewJersey').checked;
    window.modelPreview?.setJerseyVisible(state.preview.jersey);
  });
  $('previewBlenderHair').addEventListener('change', () => {
    state.preview.blenderHair = $('previewBlenderHair').checked;
  });
  $('previewBlenderFacialHair').addEventListener('change', () => {
    state.preview.blenderFacialHair = $('previewBlenderFacialHair').checked;
  });
  $('previewBlenderTextures').addEventListener('change', () => {
    state.preview.blenderTextures = $('previewBlenderTextures').checked;
  });
  $('previewApplyChestCorrection').addEventListener('click', applyPreviewChestCorrection);
  $('previewBodyFitToggle').addEventListener('click', () => {
    setPreviewBodyFitOpen(!state.preview.bodyFitOpen);
  });
  $('previewCloseBodyFit').addEventListener('click', () => setPreviewBodyFitOpen(false));
  $('previewCalculateFit').addEventListener('click', calculatePreviewBodyFit);
  $('previewRevertFit').addEventListener('click', revertPreviewBodyFit);
  $('previewApplyFit').addEventListener('click', applyPreviewBodyFit);
  window.addEventListener('model-preview:pick', (event) => {
    const detail = event.detail;
    if (detail?.viewer && detail.viewer !== 'modelPreview') return;
    $('previewSelectedMesh').textContent = detail?.name || 'Click a mesh in the render';
    $('previewSelectedTarget').textContent = detail?.target || detail?.model || '';
  });
  $('manifestSearch').addEventListener('input', () => {
    state.manifestVisibleLimit = MANIFEST_BATCH_SIZE;
    $('manifestTableWrap').scrollTop = 0;
    renderManifestRows();
  });
  $('manifestTableWrap').addEventListener('scroll', (event) => {
    const tableWrap = event.currentTarget as HTMLElement;
    const nearBottom = tableWrap.scrollTop + tableWrap.clientHeight >= tableWrap.scrollHeight - 160;
    if (!nearBottom) return;
    const previousLimit = state.manifestVisibleLimit;
    state.manifestVisibleLimit = Math.min(state.catalog.length, previousLimit + MANIFEST_BATCH_SIZE);
    if (state.manifestVisibleLimit !== previousLimit) renderManifestRows();
  });
  $('useManifestPlayer').addEventListener('click', useSelectedManifestPlayer);
  $('chooseOutput').addEventListener('click', chooseOutput);
  $('openOutput').addEventListener('click', () => state.outputDir && window.characterTool.openPath(state.outputDir));
  $('openSettings').addEventListener('click', () => $('settingsDialog').showModal());
  $('saveSettings').addEventListener('click', saveSettings);
  document.querySelectorAll<HTMLButtonElement>('[data-setting-browse]').forEach((button) => {
    button.addEventListener('click', () => browseSetting(button));
  });
  $('runSwap').addEventListener('click', runSwap);
  $('cancelSwap').addEventListener('click', cancelSwap);
  $('openLastOutput').addEventListener('click', () => state.lastOutput && window.characterTool.openPath(state.lastOutput));
  $('clearActivity').addEventListener('click', () => $('activityLog').replaceChildren());
  $('detectHair').addEventListener('click', async () => {
    state.hair.sourcePath = '';
    await refreshHairOptions({ autoEnable: true });
  });
  $('browseHair').addEventListener('click', async () => {
    const filePath = await window.characterTool.chooseHair();
    if (filePath) await refreshHairOptions({ explicitSource: filePath, autoEnable: true });
  });
  $('hairSource').addEventListener('change', () => {
    const candidate = state.hair.candidates.find((item) => item.path === $('hairSource').value);
    state.hair.sourcePath = candidate?.path || '';
    state.hair.generation = candidate?.generation || '';
    state.hair.sourceKey = candidate?.sourceKey || '';
    $('hairSource').title = state.hair.sourcePath;
    refreshHairStatus();
    renderHairControls();
    updateReadiness();
  });
  $('includeHair').addEventListener('change', () => {
    renderHairControls();
    refreshHairStatus();
    updateReadiness();
  });
  $('hairSlot').addEventListener('change', () => {
    refreshHairStatus();
    updateReadiness();
  });
  $('detectFacialHair').addEventListener('click', async () => {
    state.facialHair.sourcePath = '';
    await refreshFacialHairOptions();
  });
  $('browseFacialHair').addEventListener('click', async () => {
    const source = state.source;
    const target = state.target;
    const requestId = state.facialHair.requestId;
    const filePath = await window.characterTool.chooseHair();
    if (source !== state.source || target !== state.target || requestId !== state.facialHair.requestId) return;
    if (filePath) await refreshFacialHairOptions({ explicitSource: filePath });
  });
  $('facialHairSource').addEventListener('change', () => {
    const candidate = state.facialHair.candidates.find((item) => item.path === $('facialHairSource').value);
    state.facialHair.sourcePath = candidate?.path || '';
    state.facialHair.generation = candidate?.generation || '';
    state.facialHair.sourceKey = candidate?.sourceKey || '';
    refreshFacialHairStatus();
    updateReadiness();
  });
  $('includeFacialHair').addEventListener('change', () => {
    state.facialHair.manualChoice = $('includeFacialHair').checked;
    refreshFacialHairStatus();
    updateReadiness();
  });
  $('facialHairSlot').addEventListener('change', () => {
    refreshFacialHairStatus();
    updateReadiness();
  });
  $('renamePackage').addEventListener('change', () => {
    $('renameId').disabled = !$('renamePackage').checked;
    if (!$('renamePackage').checked) $('renameId').value = '';
  });

  window.characterTool.onJobEvent((event) => {
    if (event.jobId !== state.activeJob) return;
    if (event.type === 'progress') setProgress(event.message || 'Working...', 'running');
    addLog(event.message, event.type === 'error' ? 'error' : event.type === 'warning' ? 'warning' : 'info');
  });
  window.characterTool.onJobComplete(async (event) => {
    if (!state.activeJob && state.startingJob) {
      state.pendingTerminalEvent = { type: 'complete', event };
      return;
    }
    await handleJobComplete(event);
  });
  window.characterTool.onJobFailed((event) => {
    if (!state.activeJob && state.startingJob) {
      state.pendingTerminalEvent = { type: 'failed', event };
      return;
    }
    handleJobFailed(event);
  });
  window.characterTool.onBlenderEvent(handlePreviewBlenderEvent);
}

async function bootstrap() {
  bindEvents();
  try {
    await new Promise((resolve) => requestAnimationFrame(resolve));
    const templatesLoading = loadTemplates();
    state.bootstrap = await window.characterTool.bootstrap();
    const settings = state.bootstrap.settings;
    state.outputDir = settings.output_dir || '';
    state.engineLabel = `Engine ${state.bootstrap.engineVersion} | Tauri ${state.bootstrap.electronVersion}`;
    $('engineStatus').textContent = state.engineLabel;
    $('outputPath').textContent = state.outputDir || 'Not configured';
    $('game26Dot').classList.toggle('ready', state.bootstrap.games['NBA 2K26']);
    $('game27Dot').classList.toggle('ready', state.bootstrap.games['NBA 2K27']);
    document.querySelectorAll<HTMLButtonElement>('[data-game]').forEach((button) => {
      const available = Boolean(state.bootstrap.games[button.dataset.game]);
      button.disabled = !available;
      button.title = available ? '' : `${button.dataset.game} installation not detected`;
    });
    fillSettings(settings);
    $('startupLoadingMessage').textContent = 'Loading saved templates...';
    await templatesLoading;
    loadNativeHairFavorites();
    renderNativeHairFavoritesFilter();
    loadNativeHairCamera();
    renderHairControls();
    renderWorkingPlayer();
    localStorage.removeItem('characterModWorkingPlayer');
    localStorage.removeItem('characterModWorkingSource');
    localStorage.removeItem('characterModWorkingBaseline');
    resetBodyFitSession();
    setActiveView(state.activeView);
    addLog(`Conversion engine ${state.bootstrap.engineVersion} connected.`, 'success');
    updateReadiness();
  } catch (error) {
    state.engineLabel = 'Conversion engine unavailable';
    $('engineStatus').textContent = state.engineLabel;
    addLog(error.message, 'error');
  } finally {
    finishStartupLoading();
  }
}

bootstrap();
