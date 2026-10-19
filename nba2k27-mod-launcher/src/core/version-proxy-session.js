const fs = require('node:fs');
const fsp = require('node:fs/promises');
const path = require('node:path');
const crypto = require('node:crypto');
const { fileKind } = require('./paths');
const { processIsRunning } = require('./session');
const { startProductionProvider } = require('./production-provider');
const { AudioRouteDiagnostics } = require('./audio-route-diagnostics');
const { selectAudioIndexOverride } = require('./jukebox-index-test');
const { useFreestylePlaylist, sampleFreestyleMask } = require('./freestyle-playlist');
const {cleanupDiagnostics}=require('./diagnostic-cleanup');

const CURRENT_PROXY_SHA256 = '11EA0FA0A866B33EF6E1743D998BE66FD3E82B266CA07D46BEF40964DEE45AB9';
const STARTUP_TRANSITION_PROXY_SHA256 = 'A25A3F325C2D0262BCE4D39EA8699917C5D43E2C2A5DD63B5313C5CD814951CC';
const SCOREBOARD_TEXT_PROXY_SHA256 = 'ECEF8C6C856A60AF3387BE50D76BD30B53A80F80F404476F5C31D5A5453E91DF';
const HIGHSCHOOL_DIGIT_PROXY_SHA256 = 'E8E224C0197776123E9D94299EB31D2E49FD010108484BDEBEAA777D67995DA8';
const FREESTYLE_TEST_PROXY_SHA256 = 'CB0DAAD6509A65E9EBE400112E959C2994B11D2E57C12E13308C78E4C3669C15';
const KNOWN_PROXY_HASHES = new Set([
  'B3C48C892F8009AD351829991D3246F5D187FE486A782BE9FAAC93CD1CBAA868',
  'E66EAC430FDC481B08D5E55058C64A2FC8B651DFE8A475826E21CF30610CA5E5',
  'D956EC637DE21666365E0481581C93B1634BBFB272A903ED225D385B3877F33C',
  'A62FB87AC7F3DA9BD2C3ADC2259BB889C0B7AD11C1FF95E29AFE7997CC10FB1A',
  'E4457DEF2FEAE6BC66A249974E03D1062B887229C6CEA659E59E5952A618C573',
  'B1BC6A2BEF51A199872440A49E08092D232BF9A4B7DD504072A96936C00BA875',
  '769F8B447094F3BC47F001D8F3850DED155EB064EE6FA6F18018116479C4D118',
  '17588DBCC090297A5B8AC9FA57BA8AEDB9CD8DBE62DA092AAD06C009315FDAE6',
  '986A09EE6484D8F67EB7734409A9E438AE89AA1C32C69F8872761DAFAD0BDBB5',
  'E445B8F80295AE8CB094EBE4E76A553775C983212BA0048C0F00197F61FB10F9',
  'C3B6C51BC3FD456729B67B8937BD6C99306717E22DB910F9C6060BBDB97BF3F9',
  'AC047999C6E8D1F6DE64E1A6D432099CFB4AE6A993828C03AE7596F6474B8707',
  'DAEFE2371D66D366A6236783869AE8D1563352FC42764619E022B46920202DF2',
  '308AFFD1FBBB382D45BFBBDE09FBEAF282CE2F22A27C08D4EC01F97D906B0E0B',
  FREESTYLE_TEST_PROXY_SHA256,
  '9F20C367C9FB64DFB562B6ABA661948192D04FD6801902B621FEED5E83B69FE7',
  'FD295800B6829F7A69D26D38F21593EF51B79DA85774589ACBF6820941C3802D',
  '97F3027A191A2448BD1027EDF6E9BB7BA086F947A5AC9D3EB0982E0D2DE583CD',
  'F348997B9E2E77084BD8CF2D75F6D562A69CBD80FD18015329CB952D1735ABB7',
  'FB87BD82DCDA709543C9DEFEB2C850B5261229D835FFF046E955087E44BBAB31',
  '9A86BA1E1DE37338364DB3C08E18B0DF955D470E14BFFA2CC9847C163007759F',
  'CFB2CB1ED47B5E80AE6851F5AA47968CDE07A9602CCE7FCF4CD5C3DD578D3854',
  'F6237A76068301AC82D99DB37D3A17F58B5D1CAB3F5944BF58F2D879538AC98F',
  'E0ADECE6462C97AFF585BF9E0E8A3FFF9F37B2FE80D935B4562135D56AE42266',
  'D15E3ED4EABFCCF00C362E25CAE3E3781A59804D547120C6AEDFB1A0334FFAD0',
  'A8A2EB8F445A76ECDBA1D95CDE1C6F84A6F4DCA4A7EBFB93A73C78CF0076DA6B',
  '712A45720A78BF7BFC9780DE0794C24865F7D8037B1FFFED4401C96301290C8B',
  '3CE990D497F28B7C51676620998651165F9BABECA305E2740D2B5037998C054E',
  'EC1C9AA91B05A8919159096DE0C9CFFC4558AE0ED9F4A46D23E6ACAC73ABA6AF',
  '49B3724769B76EFC7A78E6AB9FC9CD1C96693CB94662FFE07446E9B71BA9CB6A',
  '0B4634F6D8D236181CD3E5A1F887DC4E485F06285437667B872E990251E4E1FA',
  'DFB2A9817B29C6A33F31C32E9A3DFAE129BB6C255E6CB01775D87782B9D66554',
  'B6BB864560D96FCA1740EE634D7B819C91C9D6F82D767EDB310902E6BA5633E9',
  CURRENT_PROXY_SHA256,
  STARTUP_TRANSITION_PROXY_SHA256,
  'A8A5CD91C01C945E2115EC7FD463167637007B3FEAAB97B52CDE577B74A410AC',
  '5902021C05ED1DF4FB31F1819EEC8279A87BD769D48769C2602619680AC6C86E',
  '402BE0F86BC0573C9605039FE69969551E979134CB4FF6947217AB3A089C98F6',
  SCOREBOARD_TEXT_PROXY_SHA256,
  HIGHSCHOOL_DIGIT_PROXY_SHA256,
]);
const MAX_PREFLIGHT_BYTES = 64 * 1024 * 1024;
const portableLogDirectoryHex=value=>Array.from({length:value.length},(_,index)=>value.charCodeAt(index).toString(16).padStart(4,'0')).join('').toUpperCase();
const SESSION_FILES = Object.freeze({
  config: 'nba2k27_provider_audit.cfg',
  report: 'nba2k27_version_proxy.log',
  armed: 'nba2k27_version_proxy_armed.log',
  trigger: 'nba2k27_version_proxy_audit.trigger',
  trace: 'nba2k27_version_proxy_audit.trace',
});

function delay(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

async function sha256File(filePath) {
  const hash = crypto.createHash('sha256');
  await new Promise((resolve, reject) => {
    const stream = fs.createReadStream(filePath);
    stream.on('data', (chunk) => hash.update(chunk));
    stream.once('error', reject);
    stream.once('end', resolve);
  });
  return hash.digest('hex').toUpperCase();
}

function safeLogicalPath(modsRoot, absolutePath) {
  const logicalPath = path.relative(modsRoot, absolutePath).replaceAll('\\', '/');
  if (!logicalPath || logicalPath.startsWith('../') || path.isAbsolute(logicalPath)
    || logicalPath.split('/').some((part) => !part || part === '.' || part === '..' || part.includes(':'))) {
    throw new Error('The preflight asset must remain inside the configured mods folder.');
  }
  return logicalPath;
}

async function selectPreflightAsset(modsRoot) {
  const preferred = path.join(modsRoot, 'char', 'sig', 'png10000.iff');
  const preferredStat = await fsp.stat(preferred).catch(() => null);
  if (preferredStat?.isFile() && preferredStat.size > 0 && preferredStat.size <= MAX_PREFLIGHT_BYTES) {
    return { absolutePath: preferred, logicalPath: 'char/sig/png10000.iff', size: preferredStat.size };
  }
  const pending = [path.resolve(modsRoot)];
  while (pending.length) {
    const directory = pending.shift();
    const entries = await fsp.readdir(directory, { withFileTypes: true });
    entries.sort((left, right) => left.name.localeCompare(right.name));
    for (const entry of entries) {
      const absolutePath = path.join(directory, entry.name);
      if (entry.isDirectory()) pending.push(absolutePath);
      else if (entry.isFile()) {
        const stat = await fsp.stat(absolutePath);
        if (stat.size > 0 && stat.size <= MAX_PREFLIGHT_BYTES) {
          return { absolutePath, logicalPath: safeLogicalPath(modsRoot, absolutePath), size: stat.size };
        }
      }
    }
  }
  throw new Error('The mods folder needs at least one file from 1 byte to 64 MB for the safe startup check.');
}

async function removeIfPresent(filePath) {
  await fsp.rm(filePath, { force: true }).catch(() => {});
}

class VersionProxySession {
  constructor({ onStatus = () => {}, isProcessRunning = processIsRunning,
    startProvider = startProductionProvider, activationDelayMs = 20000, onCleanup = async () => {}, audioDiagnosticsPath = null,
    startupReadinessCheck = null, startupEarlyReadinessCheck = null, nativeLocationHintPath = null, nativeLocationRoot = null, portableLogsPath = null,
    gamePresence = () => null, now = Date.now, sleep = delay } = {}) {
    this.onStatus = onStatus;
    this.isProcessRunning = isProcessRunning;
    this.gamePresence = gamePresence;
    this.now = now;
    this.sleep = sleep;
    this.absentSince = null;
    this.absentChecks = 0;
    this.startProvider = startProvider;
    this.activationDelayMs = activationDelayMs;
    this.onCleanup = onCleanup;
    this.audioDiagnosticsPath=audioDiagnosticsPath;
    this.nativeLocationHintPath=nativeLocationHintPath;
    this.nativeLocationRoot=nativeLocationRoot;
    this.portableLogsPath=portableLogsPath;
    this.startupReadinessCheck=startupReadinessCheck;
    this.startupEarlyReadinessCheck=startupEarlyReadinessCheck;
    this.earlyActivation=false;
    this.state = { phase: 'idle', message: 'Unique-asset route is ready to launch.' };
    this.provider = null;
    this.background = null;
    this.cancelled = false;
    this.paths = null;
    this.managedProxy = false;
    this.cleanupPromise = null;
  }

  emit(patch) {
    this.state = { ...this.state, ...patch, updatedAt: Date.now() };
    this.onStatus(this.state);
    return this.state;
  }

  async start({ gameRoot, modsRoot, proxySource, providerHost, launchGame, forcedOverrides = [], providerAliases = {}, mutableProviderAliases = [], selectorOverride = false, audioSlotCount = 0, audioPolicy = null, freestyleTest = false, freestylePlaylist = false, scoreboardTextTest = false, highschoolDigitTest = false }) {
    if (!Array.isArray(forcedOverrides) || forcedOverrides.length > 4) throw new Error('At most four explicit stock-file overrides are supported.');
    if(forcedOverrides.length===4&&(!scoreboardTextTest||highschoolDigitTest))throw new Error('Four explicit overrides require the current production loader.');
    const overrides = forcedOverrides.map(value=>String(value).replaceAll('\\','/'));
    // The opening transition is requested before audio becomes ready. Trigger
    // the native audit as soon as its armed report exists; the audit itself
    // already waits for the resource manager and fails closed if it cannot
    // become ready. Later-only assets retain the normal audio/delay gate.
    this.earlyActivation=overrides.includes('gooey_scenes/startup_boot_transition.iff');
    if(!providerAliases||typeof providerAliases!=='object'||Array.isArray(providerAliases)||Object.keys(providerAliases).length>8)throw new Error('Invalid provider aliases.');
    const aliases=Object.fromEntries(Object.entries(providerAliases).map(([target,source])=>[String(target).replaceAll('\\','/'),String(source).replaceAll('\\','/')]));
    if(!Array.isArray(mutableProviderAliases)||mutableProviderAliases.length>4)throw new Error('Invalid mutable provider aliases.');
    const mutableAliases=[...new Set(mutableProviderAliases.map(value=>String(value).replaceAll('\\','/')))];
    if(new Set(overrides).size!==overrides.length)throw new Error('Explicit stock-file overrides must be unique.');
    const startupTransitionTest=overrides.length===2&&overrides[0]==='movies/era_intro_current.mov'&&['gooey_scenes/startup_boot_transition.iff','bootup.iff','gooey/startup_core.iff'].includes(overrides[1]);
    if (typeof selectorOverride !== 'boolean' || (selectorOverride && (!scoreboardTextTest || highschoolDigitTest || freestyleTest))) throw new Error('Unsupported selector override loader.');
    if(typeof highschoolDigitTest!=='boolean'||(highschoolDigitTest&&(!scoreboardTextTest||freestyleTest)))throw new Error('Invalid High school digit test configuration.');
    if(typeof scoreboardTextTest!=='boolean'||(scoreboardTextTest&&freestyleTest))throw new Error('Invalid scoreboard test configuration.');
    if(typeof freestylePlaylist!=='boolean'||(freestylePlaylist&&(freestyleTest||audioSlotCount!==71||!audioPolicy)))throw new Error('Invalid Freestyle playlist configuration.');
    if(freestylePlaylist)useFreestylePlaylist({usePlaylistInFreestyle:true},{audioPolicy});
    if(typeof freestyleTest!=='boolean'||(freestyleTest&&(!audioPolicy||!/^0*1[01]*$/.test(audioPolicy.custom)||audioSlotCount!==71)))throw new Error('Freestyle test requires a prepared custom playlist.');
    if(audioPolicy && (audioSlotCount!==71 || !/^[01]{71}$/.test(audioPolicy.main)||!/^[01]{54}$/.test(audioPolicy.park)||!/^[01]{71}$/.test(audioPolicy.custom)
      || [...audioPolicy.custom].some((c,i)=>c==='1'&&audioPolicy.main[i]!=='1')))throw new Error('Invalid playlist audio policy.');
    if (!Number.isInteger(audioSlotCount) || audioSlotCount < 0 || audioSlotCount > 71) throw new Error('Invalid audio slot count.');
    if (audioSlotCount && process.env.NBA2K27_AUDIO_INDEX_TEST === '1') throw new Error('Audio slot and index tests cannot be combined.');
    if (this.background || this.provider) throw new Error('A unique-asset session is already active.');
    this.cleanupPromise=null;
    this.absentSince=null;this.absentChecks=0;
    this.playlistFiltered=Boolean(audioPolicy);
    if (await this.isProcessRunning('NBA2K27.exe')) {
      throw new Error('NBA 2K27 is already running. Close it, then use this Play button so the loader can start first.');
    }
    this.cancelled = false;
    this.emit({ phase: 'preparing-loader', message: 'Checking the game build and preparing the local missing-asset route…', error: null });
    const root = path.resolve(gameRoot);
    if(this.portableLogsPath&&(!path.isAbsolute(this.portableLogsPath)||this.portableLogsPath.includes('\n')||this.portableLogsPath.includes('\r')||this.portableLogsPath.length>400))throw Error('Invalid portable diagnostics directory.');
    if (this.nativeLocationHintPath) {
      const expected = this.nativeLocationRoot || path.join(root, '2k Launcher');
      const hint = await fsp.readFile(this.nativeLocationHintPath, 'utf8').catch(() => '');
      if (hint.replaceAll('\r\n', '\n').trim().toLowerCase()
          !== `[Launcher]\nRoot=${expected}`.toLowerCase()) {
        throw new Error('The launcher data location hint is missing or does not match its active library. Restart the launcher before Play.');
      }
    }
    const targetProxy = path.join(root, 'version.dll');
    this.paths = Object.fromEntries(Object.entries(SESSION_FILES).map(([key, name]) => [key, path.join(root, name)]));
    this.paths.targetProxy = targetProxy;
    if(!freestyleTest && this.audioDiagnosticsPath){
      this.diagnosticsDirectory=path.dirname(this.audioDiagnosticsPath);
      const directory=path.join(this.diagnosticsDirectory,'game-session');
      await fsp.mkdir(directory,{recursive:true});
      for(const key of ['report','armed','trace'])this.paths[key]=path.join(directory,SESSION_FILES[key]);
      await cleanupDiagnostics({directory:this.diagnosticsDirectory,gameRoot:root,isGameRunning:()=>this.isProcessRunning('NBA2K27.exe')});
    }

    try {
      if ((await fileKind(path.join(root, 'NBA2K27.exe'))) !== 'file') throw new Error('NBA2K27.exe was not found.');
      if ((await fileKind(modsRoot)) !== 'directory') throw new Error('The configured mods folder was not found.');
      if ((await fileKind(proxySource)) !== 'file') throw new Error('The bundled session loader is missing.');
      // DXGI/D3D12 add-ons are user-owned; leave them untouched. Only our
      // version.dll loader is subject to ownership and integrity checks below.
      const sourceHash = await sha256File(proxySource);
      if (sourceHash !== (freestyleTest?FREESTYLE_TEST_PROXY_SHA256:highschoolDigitTest?HIGHSCHOOL_DIGIT_PROXY_SHA256:scoreboardTextTest?SCOREBOARD_TEXT_PROXY_SHA256:startupTransitionTest?STARTUP_TRANSITION_PROXY_SHA256:CURRENT_PROXY_SHA256)) throw new Error('The bundled session loader failed its integrity check.');
      if ((await fileKind(targetProxy)) === 'file') {
        const installedHash = await sha256File(targetProxy);
        if (!KNOWN_PROXY_HASHES.has(installedHash)) {
          throw new Error('A different version.dll is installed. Nothing was overwritten.');
        }
      }
      const asset = await selectPreflightAsset(modsRoot);
      const assetHash = await sha256File(asset.absolutePath);
      // Diagnostic prototype only; keep OFF for normal launches. Freestyle audio
      // was subsequently identified as stock; custom playback is still unverified.
      const audioIndexOverride = process.env.NBA2K27_AUDIO_INDEX_TEST === '1'
        ? await selectAudioIndexOverride(modsRoot) : null;
      this.provider = await this.startProvider({ modsRoot, hostExecutable: providerHost, aliases, mutableAliases });
      this.audioDiagnostics=this.audioDiagnosticsPath?new AudioRouteDiagnostics(this.audioDiagnosticsPath):null;
      if(this.audioDiagnostics) {
        try {await this.audioDiagnostics.sample(this.provider);}
        catch {await this.audioDiagnostics.failed(this.provider.pid).catch(()=>{});}
      }
      const metadata = await this.provider.request({ operation: 'query', logicalPath: asset.logicalPath });
      if (metadata.size !== asset.size) throw new Error('The local provider index changed during startup.');
      if (audioIndexOverride) {
        const audioMetadata = await this.provider.request({ operation: 'query', logicalPath: audioIndexOverride });
        if (audioMetadata.size !== 4312) throw new Error('The verified jukebox test index is unavailable through the provider.');
      }
      if (selectorOverride) {
        for (const logicalPath of ['char/signatureconfigs_all.iff', 'char/signatureconfigs_editplayer.iff', 'char/fpplayer_global.iff']) {
          const file = path.join(modsRoot, ...logicalPath.split('/'));
          const stat = await fsp.stat(file);
          const info = await this.provider.request({ operation: 'query', logicalPath });
          if (!stat.isFile() || !stat.size || stat.size > 16 * 1024 * 1024 || info.size !== stat.size) throw new Error('Prepared player selector is unavailable through the provider.');
        }
      }
      for (const override of overrides) {
        const sourceLogical=aliases[override]||override;
        const overridePath = path.resolve(modsRoot, ...sourceLogical.split('/'));
        safeLogicalPath(modsRoot, overridePath);
        if ((await fileKind(overridePath)) !== 'file') throw new Error(`The explicit override ${override} was not found in the mods folder.`);
        const stat=await fsp.stat(overridePath),info=await this.provider.request({operation:'query',logicalPath:override});
        if(!stat.size||info.size!==stat.size)throw new Error(`The explicit override ${override} is unavailable through the provider.`);
      }

      if(scoreboardTextTest){
        const layouts=[...require('../data/scoreboard-text-adapters.json').layouts];
        if(highschoolDigitTest)layouts.push(require('../data/highschool-digit-probe.json'));
        for(const layout of layouts){
          const target=path.join(modsRoot,...layout.asset.split('/'));
          if((await fileKind(target))!=='file')throw new Error('Prepared scoreboard layout is missing.');
          const stat=await fsp.stat(target),info=await this.provider.request({operation:'query',logicalPath:layout.asset});
          if(!stat.size||info.size!==stat.size)throw new Error('Prepared scoreboard layout is unavailable through the provider.');
        }
      }

      await Promise.all(Object.keys(SESSION_FILES).map(key=>removeIfPresent(this.paths[key])));
      const config = [
        `PIPE=${this.provider.endpoint}`,
        `PATH=${asset.logicalPath}`,
        `SIZE=${asset.size}`,
        `SHA256=${assetHash}`,
        ...(this.portableLogsPath?[`PORTABLE_LOG_DIR_HEX=${portableLogDirectoryHex(this.portableLogsPath)}`]:[]),
        ...(overrides[0] ? [`OVERRIDE=${overrides[0]}`] : []),
        ...(overrides[1] ? [`OVERRIDE_2=${overrides[1]}`] : []),
        ...(overrides[2] ? [`OVERRIDE_3=${overrides[2]}`] : []),
        ...(overrides[3] ? [`OVERRIDE_4=${overrides[3]}`] : []),
        ...(selectorOverride ? ['SELECTOR_OVERRIDE=1'] : []),
        ...(scoreboardTextTest ? ['SCOREBOARD_TEXT_TEST=1'] : []),
        ...(highschoolDigitTest ? ['HIGHSCHOOL_DIGIT_TEST=7'] : []),
        ...(audioIndexOverride ? [`AUDIO_INDEX_OVERRIDE=${audioIndexOverride}`] : []),
        ...(audioSlotCount ? [`AUDIO_SLOT_COUNT=${audioSlotCount}`] : []),
        ...(audioPolicy ? [`AUDIO_ALLOWED_MAIN=${audioPolicy.main}`,`AUDIO_ALLOWED_PARK=${audioPolicy.park}`,`AUDIO_CUSTOM_MAIN=${audioPolicy.custom}`] : []),
        ...(freestyleTest ? ['FREESTYLE_EVENT_TEST=992'] : []),
        ...(freestylePlaylist ? ['FREESTYLE_PLAYLIST=992',`FREESTYLE_SELECTION=${sampleFreestyleMask(audioPolicy)}`] : []),
        '',
      ].join('\r\n');
      if(Buffer.byteLength(config,'ascii')>2047)throw Error('Session configuration exceeds the native size limit.');
      await fsp.copyFile(proxySource, targetProxy);
      this.managedProxy = true;
      await fsp.writeFile(this.paths.config, config, 'ascii');
      this.emit({ phase: 'launching-game', message: 'Local provider ready. Launching NBA 2K27 without EAC…', preflightAsset: asset.logicalPath });
      const launch = await launchGame();
      let monitorCompleted=false;
      this.background = this.activateAndMonitor().then(()=>{monitorCompleted=true;}).catch(async(error) => {
        await this.captureFailureDiagnostics(error).catch(()=>{});
        if (!this.cancelled) this.emit({ phase: 'loader-failed', message: `Unique-asset route stopped safely: ${error.message}`, error: error.message });
      }).finally(async () => {
        try {
          await this.cleanup();
          if(monitorCompleted&&!this.cancelled)this.emit({phase:'complete',message:'NBA 2K27 closed. The local provider and session files were restored cleanly.'});
        }
        catch (error) { this.emit({ phase: 'loader-failed', message: `Session cleanup needs attention: ${error.message}`, error: error.message }); }
        finally { this.background = null; }
      });
      return { ...launch, loader: this.state };
    } catch (error) {
      await this.cleanup();
      this.emit({ phase: 'loader-failed', message: error.message, error: error.message });
      throw error;
    }
  }

  async waitForFile(filePath, timeoutMs, failurePath = null, requireGameRunning = false) {
    const deadline = Date.now() + timeoutMs;
    while (!this.cancelled && Date.now() < deadline) {
      if ((await fileKind(filePath)) === 'file') return true;
      if (failurePath && (await fileKind(failurePath)) === 'file') {
        const report = await fsp.readFile(failurePath, 'utf8').catch(() => '');
        throw new Error(report.trim() || 'The session loader failed closed.');
      }
      if (requireGameRunning && !(await this.isProcessRunning('NBA2K27.exe'))) {
        throw new Error('NBA 2K27 exited before the missing-asset route finished activating.');
      }
      await delay(250);
    }
    if (this.cancelled) throw new Error('Session stopped.');
    return false;
  }

  async waitForStartupReadiness() {
    if(this.earlyActivation){
      this.emit({phase:'loader-armed',message:'Opening transition detected. Waiting for the earliest stable resource-manager boundary…'});
      if(this.startupEarlyReadinessCheck)await this.startupEarlyReadinessCheck();
      if (!(await this.isProcessRunning('NBA2K27.exe'))) throw new Error('NBA 2K27 exited before early startup activation.');
      return;
    }
    // Experimental callers may supply a read-only exact-build readiness gate.
    // Normal application launches retain the existing delay.
    if (this.startupReadinessCheck) {
      this.emit({phase:'loader-armed',message:'Waiting for verified file-manager readiness (read-only startup test)…'});
      await this.startupReadinessCheck();
      if (!(await this.isProcessRunning('NBA2K27.exe'))) throw new Error('NBA 2K27 exited during the readiness check.');
      return;
    }
    this.emit({ phase: 'loader-armed', message: `Missing-asset route armed. Waiting ${Math.ceil(this.activationDelayMs / 1000)} seconds for game startup to settle…` });
    const activationDeadline = Date.now() + this.activationDelayMs;
    while (!this.cancelled && Date.now() < activationDeadline) {
      if (!(await this.isProcessRunning('NBA2K27.exe'))) throw new Error('NBA 2K27 exited before the route activated.');
      await delay(Math.max(1, Math.min(1000, activationDeadline - Date.now())));
    }
  }

  async captureFailureDiagnostics(error){
    if(!this.diagnosticsDirectory)return;
    const artifacts={};
    for(const [name,file] of Object.entries({armed:this.paths.armed,trace:this.paths.trace,report:this.paths.report})){
      const bytes=await fsp.readFile(file).catch(()=>null);
      if(bytes)artifacts[name]=bytes.subarray(0,65536).toString('utf8');
    }
    await fsp.writeFile(path.join(this.diagnosticsDirectory,'last-loader-failure.json'),JSON.stringify({
      schema:'nba2k27-loader-failure-v1',recordedAt:new Date().toISOString(),message:String(error?.message||error).slice(0,1024),earlyActivation:this.earlyActivation,artifacts,
    },null,2),'utf8');
  }

  async activateAndMonitor() {
    this.emit({ phase: 'waiting-for-game', message: 'Waiting for NBA 2K27 to load the verified session proxy…' });
    const armed = await this.waitForFile(this.paths.armed, 90000, this.paths.report);
    if (!armed) throw new Error('NBA 2K27 did not produce an armed report within 90 seconds.');
    await this.waitForStartupReadiness();
    if (this.cancelled) return;
    await removeIfPresent(this.paths.trace);
    if ((await fileKind(this.paths.report)) !== 'file') await fsp.writeFile(this.paths.trigger, 'trigger\r\n', 'ascii');
    this.emit({ phase: 'activating-loader', message: 'Activating stock-first lookup for assets missing from the game manifest…' });
    const reported = await this.waitForFile(this.paths.report, 180000, null, true);
    if (!reported) throw new Error('The missing-asset route did not return an activation report.');
    const report = await fsp.readFile(this.paths.report, 'utf8');
    if (!/^LIVE_SESSION_ACTIVE=YES\s*$/m.test(report)) throw new Error('The proxy rejected session activation.');
    this.emit({ phase: 'loader-active', message: this.playlistFiltered
      ? 'Ready. Your selected playlist is active; unselected soundtrack songs are blocked.'
      : 'Unique assets are active. Stock files stay stock except explicitly enabled launcher assets such as camera names.' });
    let nextAudioSample=0;
    while (!this.cancelled && await this.gameMayBeRunning()) {
      if(this.audioDiagnostics && this.provider && Date.now()>=nextAudioSample) {
        nextAudioSample=Date.now()+2000;
        try {await this.audioDiagnostics.sample(this.provider);}
        catch(error) {
          this.state.audioDiagnosticsError=String(error.message).slice(0,512);
          await this.audioDiagnostics.failed(this.provider?.pid).catch(()=>{});
        }
      }
      await delay(500);
    }
  }

  async cleanup() {
    if(this.cleanupPromise)return this.cleanupPromise;
    this.cleanupPromise=this.cleanupOnce().catch(error=>{this.cleanupPromise=null;throw error;});
    return this.cleanupPromise;
  }

  async gameMayBeRunning() {
    try {
      // The read-only native watcher owns an exact process handle. Keep the
      // provider alive even if a process-list snapshot misses that process.
      const presence=this.gamePresence();
      if(presence===true){this.absentSince=null;this.absentChecks=0;this.state.processCheckError=null;return true;}
      const running=await this.isProcessRunning('NBA2K27.exe',{fresh:true});
      if(typeof running!=='boolean')throw Error('Game process state is unknown.');
      const latestPresence=this.gamePresence();
      this.state.processCheckError=null;
      if(running||latestPresence===true){this.absentSince=null;this.absentChecks=0;return true;}
      // A verified handle exit still needs a fresh global check: a new game
      // process may already have started. Without that monitor, require three
      // fresh absent observations spanning at least one second.
      if(latestPresence===false)return false;
      if(this.absentSince===null)this.absentSince=this.now();
      this.absentChecks++;
      return this.absentChecks<3||this.now()-this.absentSince<1000;
    } catch(error) {
      this.absentSince=null;this.absentChecks=0;
      this.state.processCheckError=error.message;
      // Retain the provider and recovery records until exit can be verified.
      return true;
    }
  }

  async waitForGameExit() {
    if(!await this.gameMayBeRunning())return;
    this.emit({phase:'waiting-for-game-exit',message:'Keeping game resources active until NBA 2K27 exit is confirmed.'});
    do { await this.sleep(500); } while(await this.gameMayBeRunning());
  }

  async cleanupOnce() {
    // Also guard retries after the provider was removed: recovery files and
    // dependent helpers must never be restored into a new live game session.
    await this.waitForGameExit();
    let evidenceSaved=true;
    if(this.diagnosticsDirectory){
      try{await require('./session-evidence').archiveSessionLogs({directory:this.diagnosticsDirectory,gameRoot:this.paths?path.dirname(this.paths.targetProxy):null});}
      catch(error){evidenceSaved=false;this.emit({diagnosticError:`Session evidence could not be saved: ${error.message}`});}
    }
    await this.waitForGameExit();
    const provider = this.provider;
    if (provider) {
      if(await provider.stop()===false)throw Error('Unique-asset provider cleanup is still pending.');
      if(this.provider===provider)this.provider=null;
    }
    let removalError=null;
    if (this.paths) {
      await Promise.all(Object.values(SESSION_FILES).filter(name=>evidenceSaved||!/[.](?:log|trace)$/.test(name)).map((name) => removeIfPresent(path.join(path.dirname(this.paths.config), name))));
      if (this.managedProxy && (await fileKind(this.paths.targetProxy)) === 'file') {
        const installedHash = await sha256File(this.paths.targetProxy).catch(() => '');
        if (KNOWN_PROXY_HASHES.has(installedHash)) {
          try { await fsp.rm(this.paths.targetProxy); }
          catch(error) { removalError=new Error(`The launcher-owned version.dll could not be removed: ${error.message}`); }
        }
      }
    }
    this.managedProxy = false;
    for(;;){
      await this.waitForGameExit();
      try { await this.onCleanup();break; }
      catch(error){if(error.code!=='GAME_SESSION_ACTIVE'&&error.code!=='PROCESS_QUERY_FAILED')throw error;await this.sleep(500);}
    }
    if(this.diagnosticsDirectory)await cleanupDiagnostics({directory:this.diagnosticsDirectory,gameRoot:path.dirname(this.paths.targetProxy),isGameRunning:()=>this.isProcessRunning('NBA2K27.exe')});
    if(removalError)throw removalError;
  }

  async stop() {
    this.cancelled = true;
    this.emit({ phase: 'stopping-loader', message: 'Stopping the local unique-asset provider…' });
    const background=this.background;
    await this.cleanup();
    if(background)await Promise.race([background.catch(()=>{}),delay(2000)]);
    this.emit({ phase: 'stopped', message: 'Unique-asset session stopped.' });
    return this.state;
  }
}

module.exports = {
  CURRENT_PROXY_SHA256,
  KNOWN_PROXY_HASHES,
  VersionProxySession,
  portableLogDirectoryHex,
  selectPreflightAsset,
  sha256File,
};
