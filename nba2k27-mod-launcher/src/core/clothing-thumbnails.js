'use strict';
const fs = require('node:fs/promises');
const path = require('node:path');
const crypto = require('node:crypto');
const { promisify } = require('node:util');
const run = promisify(require('node:child_process').execFile);
const { readZipArchive } = require('./jukebox-manager');
const catalog = require('../data/streetwear-components.json');
const { PreviewQueue } = require('./preview-queue');
const ALLOWED = new Set([...catalog.presets.filter(p=>['male','female'].includes(p.sourceGroup.pool)).flatMap(p=>p.original),...(catalog.additionalShoes||[]).map(s=>s.asset),...(catalog.additionalAccessories||[]).map(s=>s.asset)]);
const PNG = Buffer.from('89504e470d0a1a0a', 'hex');
function validPng(b) {
  return b.length > 24 && b.length <= 1024 * 1024 && b.subarray(0,8).equals(PNG)
    && b.readUInt32BE(16) > 0 && b.readUInt32BE(16) <= 512 && b.readUInt32BE(20) > 0 && b.readUInt32BE(20) <= 512;
}
class ClothingThumbnails {
  constructor({ cacheRoot, decoderPath, texconvPath, execute = run }) {
    Object.assign(this, { cacheRoot: path.resolve(cacheRoot), decoderPath, texconvPath, execute });
    this.indexes = new Map(); this.pending = new Map(); this.queue = new PreviewQueue(4); this.count = 0;
    this.missing = new Set(); this.pruning = null;this.conversions=new Map();
    this.memory = new Map(); this.memoryBytes = 0;
  }
  async index(gameRoot) {
    const root = path.resolve(gameRoot), file = path.join(root, 'manifest');
    const stat = await fs.stat(file); const stamp = `${root}:${stat.size}:${stat.mtimeMs}`;
    if (!this.indexes.has(stamp)) {
      this.indexes.clear();
      const job = (async () => {
        const raw = await fs.readFile(file); const entries = new Map(),shoeModels=new Map();
        for (const line of raw.toString('utf8').split(/\r?\n/)) {
          const shoe=/^(shoes\/shoe_fe_(\d{4})\.iff),([A-Z0-9]{2}),(\d+),(\d+)$/.exec(line);
          if(shoe){const offset=Number(shoe[4]),size=Number(shoe[5]);if(Number.isSafeInteger(offset)&&offset>=0&&Number.isSafeInteger(size)&&size>0&&size<=4*1024*1024)
            shoeModels.set(shoe[2],{archive:shoe[3],offset,size,sourcePath:shoe[1],previewKind:'shoe-model'});}
          const m = /^store\/thumbnails\/([a-zA-Z0-9_]+)\.iff,([A-Z0-9]{2}),(\d+),(\d+)$/.exec(line);
          if (!m || !ALLOWED.has(m[1])) continue;
          const offset=Number(m[3]), size=Number(m[4]);
          if (Number.isSafeInteger(offset) && offset>=0 && Number.isSafeInteger(size) && size>0 && size<=4*1024*1024) entries.set(m[1], {archive:m[2],offset,size});
        }
        for(const asset of ALLOWED){const shoe=/^shoe_(\d{4})_[a-zA-Z0-9_]+$/.exec(asset);
          if(shoe&&!entries.has(asset)&&shoeModels.has(shoe[1]))entries.set(asset,shoeModels.get(shoe[1]));}
        return { root, entries, fingerprint:crypto.createHash('sha256').update(root).update(raw).digest('hex') };
      })();
      this.indexes.set(stamp, job);
      job.catch(() => { if(this.indexes.get(stamp)===job) this.indexes.delete(stamp); });
    }
    return this.indexes.get(stamp);
  }
  get(gameRoot, asset, { background=false } = {}) {
    if (typeof asset !== 'string' || !ALLOWED.has(asset)) return Promise.reject(new Error('Unknown clothing preview.'));
    const key = `${path.resolve(gameRoot)}:${asset}`;
    if (this.pending.has(key)) return this.pending.get(key);
    if (this.pending.size >= 64) return Promise.resolve({ state:'busy' });
    // Cached files must never wait behind a slow first-time conversion.
    const job = this.load(gameRoot, asset, background);
    this.pending.set(key,job);
    job.finally(() => this.pending.delete(key)).catch(() => {});
    return job;
  }
  getCached(gameRoot, asset) {
    if (typeof asset !== 'string' || !ALLOWED.has(asset)) return Promise.reject(new Error('Unknown clothing preview.'));
    return this.load(gameRoot, asset, true, true);
  }
  remember(key,data,details={}) {
    const result={state:'ready',dataUrl:`data:image/png;base64,${data.toString('base64')}`,...details};
    const bytes=result.dataUrl.length*2;
    if(this.memory.has(key))this.memoryBytes-=this.memory.get(key).bytes;
    this.memory.delete(key);this.memory.set(key,{result,bytes});this.memoryBytes+=bytes;
    while(this.memory.size>128||this.memoryBytes>16*1024*1024) {
      const oldest=this.memory.keys().next().value;
      this.memoryBytes-=this.memory.get(oldest).bytes;this.memory.delete(oldest);
    }
    return result;
  }
  async load(gameRoot, asset, background, cachedOnly=false) {
    const index = await this.index(gameRoot), entry = index.entries.get(asset);
    if (!entry) return {state:'missing'};
    const archivePath=path.join(index.root,entry.archive), archiveStat=await fs.stat(archivePath);
    if (entry.offset+entry.size > archiveStat.size) throw new Error('Thumbnail archive range is invalid.');
    const key=crypto.createHash('sha256').update(`${index.fingerprint}:${entry.previewKind==='shoe-model'?entry.sourcePath:asset}:${archiveStat.size}:${archiveStat.mtimeMs}:v1`).digest('hex');
    const details=entry.previewKind==='shoe-model'?{previewKind:'shoe-model',note:'Model preview · colors may differ'}:{};
    if(this.missing.has(key)) return {state:'missing'};
    if(this.memory.has(key)) {
      const cached=this.memory.get(key);this.memory.delete(key);this.memory.set(key,cached);return cached.result;
    }
    await fs.mkdir(this.cacheRoot,{recursive:true});
    const target=path.join(this.cacheRoot,`${key}.png`);
    try { const data=await fs.readFile(target); if(validPng(data)) return this.remember(key,data,details); }
    catch(error) { if(error.code!=='ENOENT') throw error; }
    if(cachedOnly) return {state:'uncached'};
    if(this.conversions.has(key))return this.conversions.get(key);
    const job=this.queue.run(()=>this.convert(index,entry,archivePath,target,key),background);this.conversions.set(key,job);
    job.finally(()=>this.conversions.delete(key)).catch(()=>{});return job;
  }
  async convert(index,entry,archivePath,target,key) {
    const temporary=await fs.mkdtemp(path.join(this.cacheRoot,'decode-'));
    try {
      const output=path.join(temporary,'thumbnail.iff');
      await this.execute(this.decoderPath,[archivePath,String(entry.offset),String(entry.size),path.join(index.root,'data','oodle','oo2core_9_win64.dll'),output],{windowsHide:true,timeout:20000,maxBuffer:1024*1024});
      if((await fs.stat(output)).size>4*1024*1024) throw new Error('Clothing preview is too large.');
      const entries=readZipArchive(await fs.readFile(output));
      const textures=[...entries].filter(([name])=>name.toLowerCase().endsWith('.dds'));
      if(textures.length!==1) {
        if(this.missing.size>=4096)this.missing.delete(this.missing.values().next().value);
        this.missing.add(key);return {state:'missing'};
      }
      const dds=textures[0][1];
      if(dds.length<128 || dds.length>2*1024*1024 || dds.subarray(0,4).toString()!=='DDS '
        || dds.readUInt32LE(12)>512 || dds.readUInt32LE(16)>512) throw new Error('Unsupported clothing preview texture.');
      const input=path.join(temporary,'preview.dds'); await fs.writeFile(input,dds,{flag:'wx'});
      await this.execute(this.texconvPath,['-nologo','-y','-ft','png','-f','R8G8B8A8_UNORM','-o',temporary,input],{windowsHide:true,timeout:15000,maxBuffer:1024*1024});
      const data=await fs.readFile(path.join(temporary,'preview.png'));
      if(!validPng(data)) throw new Error('Clothing preview conversion failed.');
      await fs.writeFile(target,data); // Only our generated cache; never a game file.
      if(++this.count%32===0 && !this.pruning) {
        this.pruning=this.prune().catch(()=>{}).finally(()=>{this.pruning=null;});
      }
      return this.remember(key,data,entry.previewKind==='shoe-model'?{previewKind:'shoe-model',note:'Model preview · colors may differ'}:{});
    } finally {
      // mkdtemp returned a direct child of the explicit app-owned cache root.
      if(path.dirname(temporary)===this.cacheRoot && path.basename(temporary).startsWith('decode-')) await fs.rm(temporary,{recursive:true,force:true});
    }
  }
  async prune() {
    const entries=[];
    for(const name of await fs.readdir(this.cacheRoot)) if(/^[a-f0-9]{64}\.png$/.test(name)) {
      const file=path.join(this.cacheRoot,name), stat=await fs.lstat(file);
      if(stat.isFile() && !stat.isSymbolicLink()) entries.push({file,size:stat.size,time:stat.mtimeMs});
    }
    let total=entries.reduce((n,e)=>n+e.size,0);
    // Optional full wardrobe cache needs room for all current known previews.
    for(const e of entries.sort((a,b)=>a.time-b.time)) { if(total<=2*1024*1024*1024) break; await fs.unlink(e.file); total-=e.size; }
  }
}
module.exports={ClothingThumbnails,validPng};
