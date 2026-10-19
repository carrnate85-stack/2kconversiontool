'use strict';
const fs = require('node:fs/promises');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawn } = require('node:child_process');
const { stopChild } = require('./child-lifecycle');
const MAX_REPLY = 8 * 1024 * 1024, MAX_PAYLOAD = 32 * 1024 * 1024;
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
// Owned Python helpers use extended Windows paths, including on systems where
// LongPathsEnabled is off. Compare their logical paths with the managed roots.
const localPath = value => path.resolve(value.startsWith('\\\\?\\UNC\\') ? '\\\\' + value.slice(8) : value.startsWith('\\\\?\\') ? value.slice(4) : value);
const inside = (root, file) => { const relative = path.relative(root, file); return !!relative && !relative.startsWith('..') && !path.isAbsolute(relative); };
async function plain(file, directory = false) {
 const stat = await fs.lstat(file);
 if (stat.isSymbolicLink() || (directory ? !stat.isDirectory() : !stat.isFile()) || path.resolve(await fs.realpath(file)).toLowerCase() !== path.resolve(file).toLowerCase()) throw Error('Animation preview refuses redirected files or folders.');
 return stat;
}
async function plainChain(file) {
 const parsed = path.parse(path.resolve(file)); let current = parsed.root;
 for (const part of path.resolve(file).slice(current.length).split(path.sep)) { current = path.join(current, part); const stat = await fs.lstat(current).catch(error => { if (error.code !== 'ENOENT') throw error; }); if (stat) await plain(current, stat.isDirectory()); }
}
class LineProcess {
 constructor(command, args, { spawnProcess = spawn, timeout = 120000, startupTimeout = timeout, progress = () => {} } = {}) {
  this.child = spawnProcess(command, args, { windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
  this.sequence = 0; this.pending = new Map(); this.buffer = Buffer.alloc(0); this.logs = ''; this.timeout = timeout; this.tail = Promise.resolve();
  this.first = new Promise((resolve, reject) => { this.initial = { resolve, reject }; this.initial.timer = setTimeout(() => this.fail(Error('Animation preview preparation timed out.')), startupTimeout); });
  this.child.stdout.on('data', chunk => {
   this.buffer = Buffer.concat([this.buffer, chunk]);
   if (this.buffer.length > MAX_REPLY && !this.buffer.includes(10)) { this.fail(Error('Animation worker reply exceeds its byte budget.')); return; }
   let newline; while ((newline = this.buffer.indexOf(10)) >= 0) {
    const line = this.buffer.subarray(0, newline); this.buffer = this.buffer.subarray(newline + 1);
    if (line.length > MAX_REPLY) { this.fail(Error('Animation worker reply exceeds its byte budget.')); return; }
    let row; try { row = JSON.parse(line.toString()); } catch { this.fail(Error('Invalid animation worker reply.')); return; }
    if (row.progress) { progress(row.progress); continue; }
    if (this.initial) { const initial = this.initial; this.initial = null; clearTimeout(initial.timer); initial.resolve(row); continue; }
    const request = this.pending.get(row.id);
    if (!request) { this.fail(Error('Animation worker reply belongs to another request.')); return; }
    this.pending.delete(row.id); clearTimeout(request.timer);
    row.error ? request.reject(Error(row.error.message || 'Animation unavailable.')) : request.resolve(row.result);
   }
  });
  this.child.stderr.on('data', chunk => { this.logs = (this.logs + chunk).slice(-3000); });
  this.child.on('error', error => this.fail(error));
  this.child.on('exit', code => { this.exited = true; this.fail(Error(this.logs.trim() || `Animation worker stopped (${code}). Reopen preview to retry.`)); });
  this.child.stdin.on('error', error => this.fail(error));
 }
 fail(error) {
  this.failure ||= error;
  if (this.initial) { clearTimeout(this.initial.timer); this.initial.reject(error); this.initial = null; }
  for (const request of this.pending.values()) { clearTimeout(request.timer); request.reject(error); } this.pending.clear();
  if (!this.exited) void this.stop();
 }
 call(op, values = {}) {
  const run = this.tail.then(() => {
   if (this.failure || this.exited) throw this.failure || Error('Animation worker stopped.');
   return new Promise((resolve, reject) => {
    const id = ++this.sequence, bytes = Buffer.from(JSON.stringify({ id, op, ...values }) + '\n');
    if (bytes.length > 64 * 1024) { reject(Error('Preview request exceeds its byte budget.')); return; }
    const timer = setTimeout(() => this.fail(Error('Animation decode timed out. Reopen preview to retry.')), this.timeout);
    this.pending.set(id, { resolve, reject, timer }); this.child.stdin.write(bytes);
   });
  }); this.tail = run.catch(() => {}); return run;
 }
 stop() {
  return this.stopping ||= (async () => {
   // EOF handles normal worker and preparation cancellation. If a wedged
   // extractor survives, terminate only this owned live process tree.
   if (await stopChild(this.child, { timeoutMs: 3000, forceKill: false })) return true;
   if (process.platform === 'win32' && this.child.pid && this.child.exitCode == null) {
    const killer = spawn('taskkill.exe', ['/PID', String(this.child.pid), '/T', '/F'], { windowsHide: true, stdio: 'ignore' });
    await stopChild(killer, { timeoutMs: 3000 });
   }
   return stopChild(this.child, { timeoutMs: 1000 });
  })();
 }
 setBackground(background) {
  if(this.exited || this.child.exitCode != null || !this.child.pid)return;
  try { const os=require('node:os');os.setPriority(this.child.pid,background?os.constants.priority.PRIORITY_BELOW_NORMAL:os.constants.priority.PRIORITY_NORMAL); } catch {}
 }
}
class PlayerAnimationPreview {
 constructor({ runtimeRoot, cacheRoot, settings, createProcess, idleMs = 30000 }) {
  Object.assign(this, { runtimeRoot: path.resolve(runtimeRoot), cacheRoot: path.resolve(cacheRoot), settings, idleMs });
  this.createProcess = createProcess || ((exe, args, options) => new LineProcess(exe, args, options));
  this.sessions = new Map(); this.tokens = new Map(); this.warmOwners = new Map(); this.progressListeners = new Set(); this.closing = false;
 }
 session(owner, id) {
  const session = this.sessions.get(id);
  if (!session || session.owner !== owner || session.closed || this.closing) throw Error('Animation preview expired. Reopen it to retry.');
  return session;
 }
 open(owner) {
  if (this.closing || this.sessions.size >= 8) throw Error('Close another animation preview before opening this one.');
  clearTimeout(this.idleTimer);this.worker?.setBackground?.(false); const id = crypto.randomUUID(); this.sessions.set(id, { owner, closed: false }); return { session: id };
 }
 needed() { return !this.closing && !!(this.sessions.size || this.warmOwners.size); }
 publishProgress(row) { this.lastProgress = row; for(const listener of this.progressListeners) { try { listener(row); } catch {} } }
 setWarm(owner, enabled) {
  if(typeof enabled !== 'boolean') throw Error('Invalid animation warmup request.');
  if(!enabled) { this.warmOwners.delete(owner); this.armIdle(); return { warming: false }; }
  if(this.closing) throw Error('Animation preview is closing.');
  clearTimeout(this.idleTimer);
  if(this.warmOwners.has(owner) && (this.starting || (this.worker && !this.worker.failure))) return { warming: true };
  if(!this.warmOwners.has(owner) && this.warmOwners.size >= 64) throw Error('Close another editor first.');
  const lease = {}; this.warmOwners.set(owner, lease);
  lease.promise = (async()=>{
   const {gameRoot} = await this.settings();
   if(this.closing || this.warmOwners.get(owner) !== lease) return;
   if(typeof gameRoot !== 'string' || !path.isAbsolute(gameRoot)) throw Error('Select your NBA 2K27 folder in Settings first.');
   await this.ensure(gameRoot);
  })().catch(error=>{if(this.warmOwners.get(owner)!==lease||this.closing)return;this.lastWarmError=error.message;console.warn('Animation background preparation:',error.message);});
  return { warming: true };
 }
 armIdle() {
  if(this.needed()) return;
  clearTimeout(this.idleTimer);
  if(this.starting) { void this.stopWorker().catch(error=>console.warn('Animation preparation cancellation:',error.message)); return; }
  this.idleTimer=setTimeout(()=>{if(!this.needed())void this.stopWorker().catch(error=>console.warn('Animation idle cleanup:',error.message));},this.idleMs);this.idleTimer.unref?.();
 }
 async verifyRuntime() {
  await plainChain(this.runtimeRoot); await plainChain(this.cacheRoot);
  await fs.mkdir(this.cacheRoot, { recursive: true }); await plain(this.cacheRoot, true);
  const manifest = JSON.parse(await fs.readFile(path.join(this.runtimeRoot, 'manifest.json'), 'utf8'));
  if (manifest.schema !== 1 || !Array.isArray(manifest.files) || !manifest.files.length) throw Error('Animation preview runtime is missing.');
  for (const item of manifest.files) {
   if (typeof item.path !== 'string' || item.path.includes('\\') || item.path.split('/').some(p => !p || p === '.' || p === '..') || path.isAbsolute(item.path)) throw Error('Invalid animation runtime manifest.');
   const file = path.resolve(this.runtimeRoot, item.path);
   if (!inside(this.runtimeRoot, file)) throw Error('Animation runtime escaped its bundle.');
   await plainChain(file); const stat = await plain(file);
   if (stat.size !== item.size || hash(await fs.readFile(file)) !== item.sha256.toLowerCase()) throw Error('Animation preview runtime is damaged.');
  }
 }
 ensure(gameRoot, progress = () => {}) {
  this.progressListeners.add(progress);if(this.lastProgress)progress(this.lastProgress);
  return this.ensureWorker(gameRoot).finally(()=>this.progressListeners.delete(progress));
 }
 ensureWorker(gameRoot) {
  if (this.workerStopping) return this.workerStopping.then(() => this.ensureWorker(gameRoot));
  if (this.worker && !this.worker.failure && this.gameRoot === gameRoot) return Promise.resolve(this.catalog);
  if (this.starting) { if (this.startingGame !== gameRoot) return Promise.reject(Error('Game folder changed. Close the preview and retry.')); return this.starting; }
  this.startingGame = gameRoot;
  return this.starting = (async () => {
   if (this.worker) await this.stopWorker();
   await this.verifyRuntime(); if (!this.needed()) throw Error('Animation preview closed.');
   const exe = path.join(this.runtimeRoot, 'PlayerAnimationPreview.exe'), bundle = path.join(this.runtimeRoot, '_internal', 'bundle');
   this.publishProgress({phase:'checking',percent:0});
   // One owned process prepares, validates, indexes and decodes. Integrity
   // proofs stay in that process, rather than trusting a disk verification flag.
   const worker = this.createProcess(exe, ['--worker', '--launcher-cache', '--game-root', gameRoot, '--cache', this.cacheRoot, '--bundle', bundle], { timeout: 120000, startupTimeout:1800000, progress:row=>this.publishProgress(row) }); this.worker = worker;
   worker.setBackground?.(!this.sessions.size);
   const handshake = await worker.first;
   if (handshake.ready !== true || handshake.protocol !== 2 || !/^[a-f0-9]{64}$/.test(handshake.profile_id || '')) throw Error('Animation worker handshake failed.');
   const profile=path.join(this.cacheRoot,'profiles',handshake.profile_id);await plain(profile,true);
   if(!this.needed())throw Error('Animation preview closed.');
   const runtime = localPath(handshake.runtime);
   if (path.dirname(runtime).toLowerCase() !== path.join(this.cacheRoot, 'runtime').toLowerCase() || !/^[a-f0-9]{32}$/.test(path.basename(runtime))) throw Error('Animation payload cache escaped its owner.');
   await plain(runtime, true); this.runtime = runtime; this.workerRuntime = runtime; this.profile = handshake.profile_id;
   const catalog = await worker.call('catalog');
   if (!Array.isArray(catalog.fields) || catalog.fields.length !== 61 || catalog.profile_id !== this.profile || catalog.release_timing?.choices?.length !== 5) throw Error('Unsupported animation catalog revision.');
   this.catalog = catalog; this.gameRoot = gameRoot; return catalog;
  })().catch(async error => { if (this.worker) await this.stopWorker(); throw error; }).finally(() => { this.starting = null; });
 }
 async capabilities(owner, id, progress) {
  this.session(owner, id); const { gameRoot } = await this.settings();
  if (typeof gameRoot !== 'string' || !path.isAbsolute(gameRoot)) throw Error('Select your NBA 2K27 folder in Settings first.');
  const catalog = await this.ensure(gameRoot, progress); this.session(owner, id); return catalog;
 }
 async request(owner, op, args) {
  if (!['resolve','load','shot-part','character','stats'].includes(op)) throw Error('Unsupported animation operation.');
  const { session, field, value, variant = 0, request = 0 } = args || {}; this.session(owner, session);
  const catalog = await this.capabilities(owner, session, () => {}); this.session(owner, session);
  if (op !== 'character' && op !== 'stats') {
   const item = catalog.fields.find(f => f.key === field);
   if (!item || !Number.isSafeInteger(value) || value < 0 || value > 65535) throw Error('Invalid animation field or roster value.');
   if (op === 'shot-part' && !['ShotJumper','ShotRelease','ShotRelease2'].includes(field)) throw Error('Invalid jump-shot part.');
  }
  if (!Number.isSafeInteger(request) || request < 0 || !Number.isSafeInteger(variant) || variant < 0 || variant > 10000) throw Error('Invalid animation request.');
  const worker = this.worker, result = await worker.call(op, { session, field, value, variant, request }); this.session(owner, session);
  if (!['load','shot-part','character'].includes(op)) return result;
  if (worker !== this.worker || result.session_id !== session || result.request_id !== request || result.profile_id !== this.profile || !/^[a-f0-9]{32}$/.test(result.handle || '') || !Number.isSafeInteger(result.bytes) || result.bytes < 1 || result.bytes > MAX_PAYLOAD || !/^[a-f0-9]{64}$/.test(result.sha256 || '')) throw Error('Invalid animation payload identity.');
  const row = await worker.call('payload', { handle: result.handle, session }); this.session(owner, session);
  const file = localPath(row.path || '');
  if (file !== path.join(this.runtime, result.handle + '.json') || row.session !== session || row.size !== result.bytes) throw Error('Animation payload belongs to another owner.');
  await plainChain(file); const bytes = await fs.readFile(file);
  if (bytes.length !== result.bytes || hash(bytes) !== result.sha256) throw Error('Animation payload failed verification.');
  for (const [token, saved] of this.tokens) if (saved.expires < Date.now()) this.tokens.delete(token);
  while (this.tokens.size >= 16) this.tokens.delete(this.tokens.keys().next().value);
  const token = crypto.randomBytes(24).toString('hex'); this.tokens.set(token, { owner, session, file, bytes: result.bytes, hash: result.sha256, expires: Date.now() + 120000 });
  return { url: 'preview-image://images/__animation/' + token, session, request, profile: this.profile };
 }
 async readToken(token, owner = null) {
  const row = this.tokens.get(token);
  if (!row || (owner !== null && row.owner !== owner) || row.expires < Date.now() || !this.sessions.has(row.session) || this.closing) return null;
  try { await plainChain(row.file); const bytes = await fs.readFile(row.file); if (bytes.length !== row.bytes || hash(bytes) !== row.hash) return null; return bytes; } catch { return null; }
 }
 async close(owner, id) {
  const session = this.sessions.get(id); if (!session) return { closed: true }; this.session(owner, id);
  session.closed = true; this.sessions.delete(id);
  for (const [token, row] of this.tokens) if (row.session === id) this.tokens.delete(token);
  if (this.worker && !this.worker.failure && !this.starting) await this.worker.call('close', { session: id }).catch(() => {});
  if(!this.sessions.size)this.worker?.setBackground?.(true);
  this.armIdle();
  return { closed: true };
 }
 async closeOwner(owner) { this.warmOwners.delete(owner);await Promise.all([...this.sessions].filter(([,s]) => s.owner === owner).map(([id]) => this.close(owner, id)));this.armIdle(); }
 stopWorker() {
  // A warm reopen can race the idle timer. Join that exact cleanup before
  // creating a replacement so the old cleanup cannot clear a new owner.
  return this.workerStopping ||= this.releaseWorker().finally(() => { this.workerStopping = null; });
 }
 async releaseWorker() {
  if (this.preparing && !await this.preparing.stop()) throw Error('Animation extractor cleanup is still pending.');
  if (this.worker && !await this.worker.stop()) throw Error('Animation worker cleanup is still pending.');
  this.worker = null; this.catalog = null;
  // This exact runtime came from our owned child's verified handshake. Never
  // recursively delete a cache root or an unknown/replaced runtime directory.
  if (this.workerRuntime) {
   const runtime = this.workerRuntime;
   try {
    await plainChain(runtime); await plain(runtime, true);
    const files = await fs.readdir(runtime);
    for (const name of files) { if (!/^[a-f0-9]{32}\.json$/.test(name)) throw Error('Unexpected animation payload; cleanup paused.'); await plain(path.join(runtime,name)); }
    for (const name of files) await fs.unlink(path.join(runtime,name));
    await fs.rmdir(runtime);
   } catch (error) { if (error.code !== 'ENOENT') throw error; }
   this.workerRuntime = null;
  }
 }
 async stop() { this.closing = true; clearTimeout(this.idleTimer); this.tokens.clear(); this.sessions.clear();this.warmOwners.clear(); await this.stopWorker(); if (this.starting) await this.starting.catch(() => {});this.progressListeners.clear(); return true; }
}
module.exports = { PlayerAnimationPreview, LineProcess, plainChain };
