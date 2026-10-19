const path = require('node:path');
const { execFile } = require('node:child_process');
const { promisify } = require('node:util');
const { fileKind } = require('./paths');

const execFileAsync = promisify(execFile);
let nativeProcessProbe = null;
function createProcessChecker(execute=execFileAsync, now=Date.now, getNativeProbe=()=>null) {
 const processChecks = new Map();
 const checker = async function processIsRunning(imageName, {fresh=false}={}) {
  const key=String(imageName).toLowerCase();
  const existing=processChecks.get(key);
  if(existing&&(existing.pending||(!fresh&&now()-existing.finishedAt<500)))return existing.promise;
  const check={pending:true,finishedAt:0};
  const promise=Promise.resolve().then(async()=>{
   const probe=getNativeProbe();
   if(probe){const value=await probe(imageName);if(typeof value!=='boolean')throw Error('Native process query returned an invalid result.');return value;}
   const {stdout=''}=await execute(
    path.join(process.env.SystemRoot || 'C:\\Windows', 'System32', 'tasklist.exe'),
    ['/FO', 'CSV', '/NH'],
    { windowsHide: true, timeout: 5000, maxBuffer: 4 * 1024 * 1024 },
   );
    // A full listing always contains system processes. Empty/error output is
    // unknown, never proof that it is safe to tear down a running game.
    const lines=stdout.trim().split(/\r?\n/).filter(Boolean);
    const rows=lines.map(line=>line.match(/^"((?:[^"]|"")+)","(\d+)"(?:,|$)/));
    if(!rows.length||rows.some(row=>!row))throw Error('Windows returned an incomplete process list.');
    return rows.some(row=>row[1].replaceAll('""','"').toLowerCase()===key);
  }).catch(cause=>{
    if(processChecks.get(key)===check)processChecks.delete(key);
    const error=new Error(`Could not verify whether ${imageName} is running. Please try again.`,{cause});
    error.code='PROCESS_QUERY_FAILED';throw error;
  }).finally(()=>{check.pending=false;check.finishedAt=now();});
  check.promise=promise;processChecks.set(key,check);
  return promise;
 };
 checker.clearCache=()=>processChecks.clear();
 return checker;
}
const processIsRunning=createProcessChecker(execFileAsync,Date.now,()=>nativeProcessProbe);
function setNativeProcessProbe(probe){
 if(probe!==null&&typeof probe!=='function')throw Error('Invalid native process probe.');
 nativeProcessProbe=probe;processIsRunning.clearCache();
}

class SessionManager {
  constructor(onStatus = () => {}, startCameraController = null, isProcessRunning = processIsRunning) {
    this.onStatus = onStatus;
    this.startCameraController = startCameraController;
    this.isProcessRunning = isProcessRunning;
    this.timer = null;
    this.state = { phase: 'idle', message: 'No active session.' };
    this.settings = null;
    this.sawGame = false;
    this.cameraStarted = false;
    this.tickRunning = false;
  }

  emit(patch) {
    this.state = { ...this.state, ...patch, updatedAt: Date.now() };
    this.onStatus(this.state);
    return this.state;
  }

  async start(settings) {
    if (this.timer) throw new Error('A launch session is already active.');
    if ((await fileKind(path.join(settings.gameRoot || '', 'NBA2K27.exe'))) !== 'file') throw new Error('NBA2K27.exe was not found. Check the game folder in Settings.');

    this.settings = { ...settings, startCameras: true };
    this.sawGame = await this.isProcessRunning('NBA2K27.exe');
    this.cameraStarted = false;
    this.emit({
      phase: this.sawGame ? 'game-running' : 'waiting-for-game',
      message: this.sawGame
        ? 'NBA 2K27 is already running. Preparing the camera session.'
        : 'Local loader armed. Launch NBA 2K27 normally from Steam or your usual shortcut.',
      provider: 'Local unique-asset loader',
      modiumStartedByLauncher: false,
    });
    await this.tick();
    this.schedule();
    return this.state;
  }

  schedule() {
    clearTimeout(this.timer);
    if (!this.settings) return;
    this.timer=setTimeout(async()=>{
      if(this.tickRunning)return this.schedule();
      this.tickRunning=true;
      try{await this.tick();}finally{this.tickRunning=false;this.schedule();}
    },1000);
    this.timer.unref?.();
  }

  async startCameras() {
    if (this.cameraStarted) return;
    if (this.startCameraController) {
      try {
        await this.startCameraController(this.settings);
        this.cameraStarted = true;
        this.emit({
          phase: 'game-running',
          message: 'NBA 2K27 is running and the integrated camera setup is active.',
          cameraState: 'started',
        });
      } catch (error) {
        this.cameraStarted = true;
        this.emit({
          phase: 'game-running',
          message: `NBA 2K27 is running, but camera activation failed: ${error.message}`,
          cameraState: 'failed',
        });
      }
      return;
    }
    this.cameraStarted = true;
    this.emit({ phase: 'game-running', message: 'NBA 2K27 is running.', cameraState: 'disabled' });
  }

  async tick() {
    if (!this.settings) return;
    let running;
    try { running = await this.isProcessRunning('NBA2K27.exe'); }
    catch(error) { this.emit({processCheckError:error.message});return; }
    if(this.state.processCheckError)this.emit({processCheckError:null});
    if (running) {
      this.sawGame = true;
      await this.startCameras();
      return;
    }
    if (this.sawGame) {
      this.sawGame = false;
      this.cameraStarted = false;
      this.emit({ phase: 'waiting-for-game', message: 'NBA 2K27 closed. Local loader remains armed for the next normal launch.', cameraState: 'waiting' });
    }
  }

  stop(phase = 'stopped', message = 'Session monitoring stopped.') {
    if (this.timer) clearTimeout(this.timer);
    this.timer = null;
    this.settings = null;
    this.sawGame = false;
    this.cameraStarted = false;
    return this.emit({ phase, message });
  }
}

module.exports = { SessionManager, processIsRunning, createProcessChecker, setNativeProcessProbe };
