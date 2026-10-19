const path = require('node:path');
const fs = require('node:fs');
const fsp = require('node:fs/promises');
const { spawn } = require('node:child_process');
const {waitForExit}=require('./child-lifecycle');
const crypto = require('node:crypto');
const {CameraNamesSession}=require('./camera-names-session');
const {cameraNames,LABELS}=require('./camera-localization');

const CURATED_CAMERAS = Object.freeze([
  { id: 51, name: 'Broadcast Low 2K18' },
  { id: 53, name: 'Press 2K17' },
  { id: 54, name: 'Press 2K27' },
  { id: 52, name: 'Beluba 2K17' },
  { id: 57, name: 'Beluba 2K27' },
  { id: 59, name: 'Side' },
  { id: 55, name: 'Nosebleeds' },
]);
const CAMERA_IDS = new Set(CURATED_CAMERAS.map((camera) => camera.id));
const LEGACY_MANAGED_NAMES_HASHES = new Set([
  '19A8F875EDC3F27CA7DE4177D2064F5E8AF97D97110B73A25865DD4D21F27EF2',
]);

function classifyNamesMod(sourceHash, targetHash, legacyHashes = LEGACY_MANAGED_NAMES_HASHES) {
  const installed = sourceHash === targetHash;
  const upgradeAvailable = Boolean(targetHash && legacyHashes.has(targetHash));
  return { installed, upgradeAvailable, conflict: Boolean(targetHash && !installed && !upgradeAvailable) };
}

function parseIni(text) {
  const result = {};
  let section = '';
  for (const raw of String(text || '').split(/\r?\n/)) {
    const line = raw.trim();
    if (!line || line.startsWith(';') || line.startsWith('#')) continue;
    const heading = /^\[([^\]]+)\]$/.exec(line);
    if (heading) {
      section = heading[1];
      if (!result[section]) result[section] = {};
      continue;
    }
    const equals = line.indexOf('=');
    if (section && equals > 0) result[section][line.slice(0, equals).trim()] = line.slice(equals + 1).trim();
  }
  return result;
}

function serializeIni(ini) {
  return `${Object.entries(ini).map(([section, values]) => {
    const rows = Object.entries(values || {}).map(([key, value]) => `${key}=${value}`);
    return `[${section}]\r\n${rows.join('\r\n')}`;
  }).join('\r\n\r\n')}\r\n`;
}

function addNosebleeds(ini) {
  if (ini.Managed?.NosebleedsAdded === '1') return false;
  const order = [...new Set(String(ini.CameraOrder?.IDs || '51,53,54,52,57,59').split(',').filter(Boolean))].filter(id => id !== '55');
  if (!order.includes('59')) order.push('59');
  order.splice(order.indexOf('59') + 1, 0, '55');
  ini.CameraOrder = { ...ini.CameraOrder, IDs: order.join(',') };
  ini.Unlocks = { ...ini.Unlocks, 55: '1' };
  ini.UnlockValues = { ...ini.UnlockValues, 55: '1,1' };
  ini.Managed = { ...ini.Managed, NosebleedsAdded: '1' };
  return true;
}

function parseInteger(value, fallback = 1) {
  const parsed = Number.parseInt(value, 10);
  return Number.isInteger(parsed) ? parsed : fallback;
}

async function sha256(target) {
  const content = await fsp.readFile(target);
  return crypto.createHash('sha256').update(content).digest('hex').toUpperCase();
}

function parseStatus(text) {
  const values = {};
  for (const line of String(text || '').split(/\r?\n/)) {
    const equals = line.indexOf('=');
    if (equals > 0) values[line.slice(0, equals)] = line.slice(equals + 1);
  }
  return {
    state: values.STATE || 'STOPPED',
    gamePid: parseInteger(values.GAME_PID, 0),
    cameraIds: (values.CAMERAS || '').split(',').map((value) => parseInteger(value, -1)).filter((id) => CAMERA_IDS.has(id)),
    message: values.MESSAGE || 'Camera controller is stopped.',
    updatedAt: values.UPDATED || null,
  };
}

class CameraManager {
  constructor({ managedRoot, templateRoot, onStatus = () => {}, managedNames=false, decoderPath=null, isGameRunning=async()=>false, diagnosticsPath=null }) {
    this.managedRoot = managedRoot;
    this.templateRoot = templateRoot;
    this.onStatus = onStatus;
    this.child = null;
    this.timer = null;
    this.publishing=false;
    this.isGameRunning=isGameRunning;
    this.diagnosticsPath=diagnosticsPath;
    this.lastDiagnostic='';
    this.restartAttempts=0;
    this.intentionalStop=false;
    this.namesSession=managedNames?new CameraNamesSession({root:managedRoot,template:path.join(templateRoot,'Camera Names Mod','loc','english.iff'),decoderPath,isGameRunning}):null;
  }

  get controllerPath() { return path.join(this.managedRoot, 'NBA2K27CameraController.exe'); }
  get libraryPath() { return path.join(this.managedRoot, 'CameraLibrary.ini'); }
  get statusPath() { return path.join(this.managedRoot, 'camera-status.ini'); }
  get stopPath() { return path.join(this.managedRoot, 'camera-stop.signal'); }

  async ensureInstalled() {
    await fsp.mkdir(this.managedRoot, { recursive: true });
    const controllerSource = path.join(this.templateRoot, 'NBA2K27CameraController.exe');
    const [sourceHash, installedHash] = await Promise.all([sha256(controllerSource), sha256(this.controllerPath).catch(() => null)]);
    if (sourceHash !== installedHash) await fsp.copyFile(controllerSource, this.controllerPath);
    for(const file of ['CameraProfiles.ini','CameraPresets.ini']){
      if(!fs.existsSync(path.join(this.managedRoot,file)))await fsp.copyFile(path.join(this.templateRoot,file),path.join(this.managedRoot,file));
    }
    if(this.namesSession)await this.namesSession.seedKeys();
    try {
      await fsp.access(this.libraryPath);
    } catch {
      await fsp.copyFile(path.join(this.templateRoot, 'CameraLibrary.ini'), this.libraryPath);
    }
    const ini = parseIni(await fsp.readFile(this.libraryPath, 'utf8'));
    const addedCamera=addNosebleeds(ini);
    const missingLock=ini.Managed?.LockValues===undefined;
    if(missingLock)ini.Managed={...(ini.Managed||{}),LockValues:'0'};
    if (addedCamera||missingLock) {
      const temporary = `${this.libraryPath}.${crypto.randomUUID()}.tmp`;
      try { await fsp.writeFile(temporary, serializeIni(ini), 'utf8'); await fsp.rename(temporary, this.libraryPath); }
      finally { await fsp.unlink(temporary).catch(error => { if (error.code !== 'ENOENT') throw error; }); }
    }
  }

  async readLibrary() {
    await this.ensureInstalled();
    const ini = parseIni(await fsp.readFile(this.libraryPath, 'utf8'));
    const order = (ini.CameraOrder?.IDs || CURATED_CAMERAS.map((camera) => camera.id).join(','))
      .split(',').map((value) => parseInteger(value, -1)).filter((id) => CAMERA_IDS.has(id));
    const uniqueOrder = [...new Set(order), ...CURATED_CAMERAS.map((camera) => camera.id).filter((id) => !order.includes(id))];
    const byId = new Map(CURATED_CAMERAS.map((camera) => [camera.id, camera]));
    const names = cameraNames(ini.DisplayNames || {});
    const cameras = uniqueOrder.map((id) => {
      const [heightText = '1', zoomText = '1'] = String(ini.UnlockValues?.[id] || '1,1').split(',');
      return {
        ...byId.get(id),
        name: names[id],
        defaultName: byId.get(id).name,
        nameLocked: Boolean(LABELS.find(label=>label.id===id).locked),
        nameMaxLength: (LABELS.find(label=>label.id===id).donorSource || LABELS.find(label=>label.id===id).source).length,
        selected: ini.Unlocks?.[id] === '1',
        height: parseInteger(heightText, 1),
        zoom: parseInteger(zoomText, 1),
      };
    });
    return { cameras, lockValues: ini.Managed?.LockValues === '1' };
  }

  async saveLibrary(payload) {
    await this.ensureInstalled();
    const incoming = Array.isArray(payload?.cameras) ? payload.cameras : [];
    if (incoming.length !== CURATED_CAMERAS.length) throw new Error('The camera setup must contain all seven cameras.');
    const ids = incoming.map((camera) => parseInteger(camera.id, -1));
    if (new Set(ids).size !== CURATED_CAMERAS.length || ids.some((id) => !CAMERA_IDS.has(id))) throw new Error('The camera order is invalid.');
    const selected = incoming.filter((camera) => camera.selected);
    if (selected.length < 1 || selected.length > 9) throw new Error('Select between one and nine cameras.');
    for (const camera of incoming) {
      if (!Number.isInteger(camera.height) || camera.height < -100 || camera.height > 100
        || !Number.isInteger(camera.zoom) || camera.zoom < -100 || camera.zoom > 100) {
        throw new Error('Camera height and zoom must be whole values from -100 to 100.');
      }
    }
    const ini = parseIni(await fsp.readFile(this.libraryPath, 'utf8'));
    ini.Unlocks = { ...(ini.Unlocks || {}) };
    ini.UnlockValues = { ...(ini.UnlockValues || {}) };
    ini.Names = { ...(ini.Names || {}) };
    const names = cameraNames(Object.fromEntries(incoming.map(camera => [camera.id, camera.name === undefined
      ? (ini.DisplayNames?.[camera.id] || CURATED_CAMERAS.find(item => item.id === camera.id).name) : camera.name])));
    ini.DisplayNames = names;
    for (const camera of incoming) {
      if (camera.selected) ini.Unlocks[camera.id] = '1'; else delete ini.Unlocks[camera.id];
      ini.UnlockValues[camera.id] = `${camera.height},${camera.zoom}`;
      ini.Names[camera.id] = names[camera.id];
    }
    ini.CameraOrder = { ...(ini.CameraOrder || {}), IDs: incoming.filter((camera) => camera.selected).map((camera) => camera.id).join(',') };
    ini.Managed = { ...(ini.Managed || {}), LockValues: payload?.lockValues === true ? '1' : '0' };
    await fsp.writeFile(this.libraryPath, serializeIni(ini), 'utf8');
    return this.view();
  }

  async namesModStatus(modsRoot) {
    if(this.namesSession)return this.namesSession.status(modsRoot);
    const source = path.join(this.templateRoot, 'Camera Names Mod', 'loc', 'english.iff');
    const target = path.join(modsRoot || '', 'loc', 'english.iff');
    const [sourceHash, targetHash] = await Promise.all([
      sha256(source).catch(() => null),
      sha256(target).catch(() => null),
    ]);
    if (!sourceHash) {
      return {
        available: false,
        installed: false,
        upgradeAvailable: false,
        conflict: Boolean(targetHash),
        target,
      };
    }
    return { available: true, ...classifyNamesMod(sourceHash, targetHash), target };
  }

  async installNamesMod(modsRoot) {
    if(this.namesSession){await this.ensureInstalled();return this.namesSession.status(modsRoot);}
    await this.ensureInstalled();
    const status = await this.namesModStatus(modsRoot);
    if (!status.available) throw new Error('The optional camera-name file is not included in this launcher build. Camera startup can continue without it.');
    if (status.installed) return status;
    if (status.conflict) throw new Error('A different loc/english.iff is already installed. It was left untouched so another text mod is not overwritten.');
    await fsp.mkdir(path.dirname(status.target), { recursive: true });
    await fsp.copyFile(path.join(this.templateRoot, 'Camera Names Mod', 'loc', 'english.iff'), status.target);
    return this.namesModStatus(modsRoot);
  }

  async serviceStatus() {
    const text = await fsp.readFile(this.statusPath, 'utf8').catch(() => '');
    const status = parseStatus(text);
    if (this.child && this.child.exitCode === null) status.controllerPid = this.child.pid;
    return status;
  }

  async view(modsRoot) {
    const [setup, service, namesMod] = await Promise.all([
      this.readLibrary(),
      this.serviceStatus(),
      this.namesModStatus(modsRoot).catch(() => ({ installed: false, conflict: false, target: path.join(modsRoot || '', 'loc', 'english.iff') })),
    ]);
    return { ...setup, service, namesMod, controllerReady: fs.existsSync(this.controllerPath) };
  }

  async start(modsRoot) {
    await this.ensureInstalled();
    if (this.child && this.child.exitCode === null) return this.view(modsRoot);
    this.intentionalStop=false;
    await Promise.all([fsp.rm(this.stopPath, { force: true }), fsp.rm(this.statusPath, { force: true })]);
    const child=this.child = spawn(this.controllerPath, ['--managed-service', this.statusPath, this.stopPath, String(process.pid)], {
      cwd: this.managedRoot,
      windowsHide: true,
      stdio: 'ignore',
    });
    child.once('exit', async () => {
      if(this.child===child)this.child = null;
      if (this.timer) clearInterval(this.timer);
      this.timer = null;
      const value=await this.publish(modsRoot).catch(()=>null);
      if(!this.intentionalStop&&value?.service?.state!=='GAME_CLOSED'&&this.restartAttempts<2&&await this.isGameRunning().catch(()=>false)){
        this.restartAttempts++;
        setTimeout(()=>this.start(modsRoot).catch(()=>{}),750).unref?.();
      }
    });
    if (this.timer) clearInterval(this.timer);
    this.timer = setInterval(async()=>{if(this.publishing)return;this.publishing=true;try{await this.publish(modsRoot);}catch{}finally{this.publishing=false;}}, 750);
    this.timer.unref?.();
    await new Promise((resolve) => setTimeout(resolve, 250));
    return this.publish(modsRoot);
  }

  async publish(modsRoot) {
    const value = await this.view(modsRoot);
    if(value.service.state==='LIVE')this.restartAttempts=0;
    await this.recordService(value.service).catch(()=>{});
    this.onStatus(value);
    return value;
  }

  async recordService(service){
    if(!this.diagnosticsPath)return;const key=[service.state,service.gamePid,service.message].join('|');if(key===this.lastDiagnostic)return;this.lastDiagnostic=key;
    await fsp.mkdir(path.dirname(this.diagnosticsPath),{recursive:true});let rows=[];try{const stat=await fsp.stat(this.diagnosticsPath);if(stat.size<=128*1024){const parsed=JSON.parse(await fsp.readFile(this.diagnosticsPath,'utf8'));if(Array.isArray(parsed))rows=parsed;}}catch(error){if(error.code!=='ENOENT')throw error;}
    rows.push({recordedAt:new Date().toISOString(),state:service.state,gamePid:service.gamePid,message:service.message,cameraIds:service.cameraIds});rows=rows.slice(-80);const temporary=this.diagnosticsPath+'.tmp';await fsp.writeFile(temporary,JSON.stringify(rows,null,2));await fsp.rename(temporary,this.diagnosticsPath);
  }

  async stop(modsRoot) {
    await this.ensureInstalled();
    if (!this.child || this.child.exitCode !== null) return this.view(modsRoot);
    this.intentionalStop=true;
    await fsp.writeFile(this.stopPath, 'stop\n', 'utf8');
    const child=this.child;
    if(!await waitForExit(child,2000)){try{child.kill();}catch{}await waitForExit(child,1000);}
    return this.publish(modsRoot);
  }

  stopSync() {
    if (this.child && this.child.exitCode === null) {
      this.intentionalStop=true;
      try { fs.writeFileSync(this.stopPath, 'stop\n', 'utf8'); } catch {}
    }
  }
}

module.exports = { CameraManager, CURATED_CAMERAS, addNosebleeds, classifyNamesMod, parseIni, parseStatus, serializeIni };
