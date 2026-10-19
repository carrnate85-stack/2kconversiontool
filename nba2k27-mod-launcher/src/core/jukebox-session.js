'use strict';
const fs=require('node:fs/promises');
const path=require('node:path');
const crypto=require('node:crypto');
const {processIsRunning}=require('./session');
const {readMusicJson,buildMusicIff,audioLogicalPath}=require('./jukebox-manager');
const {wrapPcmWave}=require('./native-pcm');
const {prepareArtwork,validateArtworkRestore,restoreArtwork}=require('./jukebox-artwork-session');
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
const digest=async p=>hash(await fs.readFile(p));
const SCHEMA='nba2k27-jukebox-session-v1';
const RETAINED_AUDIO_BYTES=256*1024*1024;

function planPlaylist(bytes) {
  const music=readMusicJson(bytes),main=Array(71).fill('0'),park=Array(54).fill('0'),custom=Array(71).fill('0');
  if(!music.Tracks.length) throw new Error('Choose a nonempty playlist before Play.');
  const used=new Set();
  for(const t of music.Tracks){
    const c=Number(t.Collection),i=Number(t['Bank Index']);
    if(c!==1)throw new Error('Custom in-game playlists currently support main soundtrack songs only. Remove tracks from other collections, or use Default Playlist unchanged.');
    if(!Number.isInteger(i)||i<1||i>=999) throw new Error('Playlist contains an unsupported audio-bank entry.');
    if(i<=71){used.add(i);main[i-1]='1';}
  }
  const mappings=[],assigned=new Map(),uniqueIds=new Set(music.Tracks.filter(t=>Number(t['Bank Index'])<=71).map(t=>Number(t['Unique Id'])));
  const tracks=music.Tracks.map(t=>{
    if(Number(t.Collection)!==1||Number(t['Bank Index'])<=71)return {...t};
    const source=audioLogicalPath(t['Bank Index']);
    let slot=assigned.get(source);
    if(!slot){
      slot=Array.from({length:71},(_,i)=>i+1).find(i=>!used.has(i));
      if(!slot)throw new Error('This game build currently supports 71 main soundtrack positions per active playlist. Split this playlist; your library and shared playlists are unchanged.');
      used.add(slot);assigned.set(source,slot);main[slot-1]='1';custom[slot-1]='1';
      mappings.push({slot,source,target:audioLogicalPath(slot),title:String(t.Name||'Song')});
    }
    const uniqueId=uniqueIds.has(slot-1)?Array.from({length:125},(_,i)=>i).find(i=>!uniqueIds.has(i)):slot-1;
    if(uniqueId===undefined)throw new Error('No verified unique soundtrack ID is available.');
    uniqueIds.add(uniqueId);
    return {...t,'Bank Index':slot,'Unique Id':uniqueId};
  });
  return {music:buildMusicIff({...music,Tracks:tracks}),mappings,policy:{main:main.join(''),park:park.join(''),custom:custom.join('')}};
}

class JukeboxSession {
  constructor({root,libraryPath=null,isGameRunning=()=>processIsRunning('NBA2K27.exe'),onProgress=()=>{}}){
    this.libraryPath=libraryPath;
    this.root=path.resolve(root);this.marker=path.join(this.root,'session.json');
    this.isGameRunning=isGameRunning;this.onProgress=onProgress;this.busy=false;this.active=false;
  }
  async save(s){await fs.mkdir(this.root,{recursive:true});await fs.writeFile(this.marker+'.tmp',JSON.stringify(s));await fs.rename(this.marker+'.tmp',this.marker);}
  async plain(root,relative){
    const parts=relative.split('/');let p=root;
    for(const piece of ['',...parts]){
      if(piece)p=path.join(p,piece);
      const stat=await fs.lstat(p).catch(e=>{if(e.code==='ENOENT')return null;throw e;});
      if(stat?.isSymbolicLink())throw new Error('Jukebox session refuses linked files or folders.');
    }
    return p;
  }
  async restore(){
    const raw=await fs.readFile(this.marker,'utf8').catch(e=>{if(e.code==='ENOENT')return null;throw e;});
    if(!raw){this.active=false;return;}
    this.active=true;
    if(await this.isGameRunning())throw new Error('Close NBA 2K27 before restoring its playlist session.');
    const s=JSON.parse(raw);
    if(s.schema!==SCHEMA||!path.isAbsolute(s.modsRoot)||!Array.isArray(s.mappings)||s.mappings.length>71
      ||!(s.originalHash===null||/^[a-f0-9]{64}$/.test(s.originalHash))||!/^[a-f0-9]{64}$/.test(s.testHash)
      ||s.mappings.some(m=>!Number.isInteger(m.slot)||m.slot<1||m.slot>71||m.target!==audioLogicalPath(m.slot)||!/^[a-f0-9]{64}$/.test(m.hash)))throw new Error('Invalid jukebox recovery journal; no files changed.');
    const music=await this.plain(s.modsRoot,'music.iff'),original=path.join(this.root,'original-music.iff');
    if(s.originalHash!==null&&await digest(original)!==s.originalHash)throw new Error('Original jukebox backup failed validation.');
    const current=await digest(music).catch(e=>{if(e.code==='ENOENT')return null;throw e;});
    if(current!==s.originalHash&&current!==s.testHash)throw new Error('Playlist changed outside the session; backup retained, no overwrite.');
    const owned=[];
    const artOptions={root:s.modsRoot,sessionRoot:this.root,plain:this.plain.bind(this)};
    await validateArtworkRestore(s.artworks||[],artOptions);
    for(const m of s.mappings){
      const target=await this.plain(s.modsRoot,m.target);
      const stat=await fs.stat(target).catch(e=>{if(e.code==='ENOENT')return null;throw e;});
      if(stat){if(await digest(target)!==m.hash)throw new Error('Temporary song changed outside this session; it was left untouched.');owned.push(target);}
    }
    if(current!==s.originalHash){if(s.originalHash===null)await fs.unlink(music);else await fs.copyFile(original,music);}
    await restoreArtwork(s.artworks||[],artOptions);
    for(const p of owned)await fs.unlink(p); // Exact hash-verified owned session copies.
    await fs.unlink(this.marker);this.active=false;
    await this.clearConversionCache();
  }
  async clearConversionCache(maxAudioBytes=RETAINED_AUDIO_BYTES){
    // Only content-addressed files in these two exact owned cache directories.
    // Unexpected names, links, or modified contents are retained, never swept.
    for(const name of ['native-pcm-v1','artwork-cache']){
      const directory=await this.plain(this.root,name);
      const entries=[];
      for(const file of await fs.readdir(directory).catch(e=>{if(e.code==='ENOENT')return [];throw e;})){
        if(!/^[a-f0-9]{64}\.(wav|tld)$/.test(file))continue;
        const target=await this.plain(this.root,`${name}/${file}`);
        const stat=await fs.lstat(target);if(!stat.isFile())continue;
        entries.push({target,file,size:stat.size,mtime:stat.mtimeMs});
      }
      // Retain only the newest 256 MiB of prepared audio; artwork remains temporary.
      let retained=0;
      for(const entry of entries.sort((a,b)=>b.mtime-a.mtime)){
        if(name==='native-pcm-v1' && retained+entry.size<=maxAudioBytes){retained+=entry.size;continue;}
        if(await digest(entry.target)===entry.file.split('.')[0])await fs.unlink(entry.target);
      }
    }
  }
  async prepare({modsRoot,enabled,gameRoot=null,playlistBytes=null,sourceRoot=null,libraryPath=this.libraryPath,preparedAudio=null}){
    if(this.busy)throw new Error('Jukebox preparation is already running.');
    this.busy=true;
    const started=Date.now(),timings={cacheHits:0,cacheMisses:0};
    try{
      if(await this.isGameRunning())throw new Error('Close NBA 2K27 before preparing the playlist.');
      await this.restore();
      if(!enabled)return {audioSlotCount:0,audioPolicy:null};
      const root=path.resolve(modsRoot),music=await this.plain(root,'music.iff');
      const original=await fs.readFile(music).catch(e=>{if(e.code==='ENOENT'&&playlistBytes)return null;throw e;});
      const selected=playlistBytes||original,plan=planPlaylist(selected);
      const artworkStarted=Date.now();
      const artwork=await prepareArtwork({libraryPath,gameRoot,modsRoot:root,sessionRoot:this.root,music:readMusicJson(selected),plain:this.plain.bind(this)});
      timings.artworkMs=Date.now()-artworkStarted;
      const remapped=readMusicJson(plan.music);
      remapped.Tracks.forEach((track,i)=>{track['Cover Name']=artwork.music.Tracks[i]['Cover Name'];});
      plan.music=buildMusicIff(remapped);
      const cacheRoot=path.join(this.root,'native-pcm-v1');await fs.mkdir(cacheRoot,{recursive:true});
      let total=0;
      const audioStarted=Date.now();
      for(const m of plan.mappings){
        this.onProgress(`Preparing song ${plan.mappings.indexOf(m)+1}/${plan.mappings.length}: ${m.title}`);
        const source=await this.plain(sourceRoot||root,m.source),target=await this.plain(root,m.target);
        if(await fs.lstat(target).catch(e=>{if(e.code==='ENOENT')return null;throw e;}))throw new Error(`An existing audio mod occupies soundtrack position ${m.slot}; it was left untouched.`);
        if(preparedAudio?.[m.source]){
          const cached=preparedAudio[m.source];
          if(!path.isAbsolute(cached.path)||!/^[a-f0-9]{64}$/.test(cached.hash)||!Number.isSafeInteger(cached.size)||cached.size<54||cached.size>256*1048576)throw new Error('Invalid prepared playlist audio.');
          const file=await this.plain(path.parse(cached.path).root,path.relative(path.parse(cached.path).root,cached.path).replaceAll('\\','/'));
          const stat=await fs.stat(file);if(!stat.isFile()||stat.size!==cached.size||await digest(file)!==cached.hash)throw new Error('Prepared playlist cache changed. Relaunch to rebuild it.');
          m.hash=cached.hash;m.cache=file;total+=cached.size;timings.cacheHits++;continue;
        }
        const stat=await fs.stat(source);
        if(!stat.isFile()||stat.size>256*1024*1024)throw new Error('Custom WAV is missing or oversized.');
        const input=await fs.readFile(source),native=wrapPcmWave(input);
        m.hash=hash(native);m.cache=await this.plain(this.root,'native-pcm-v1/'+m.hash+'.wav');total+=native.length;
        const cached=await fs.readFile(m.cache).catch(e=>{if(e.code==='ENOENT')return null;throw e;});
        if(!cached||hash(cached)!==m.hash){timings.cacheMisses++;const temp=await this.plain(this.root,'native-pcm-v1/'+m.hash+'.wav.tmp');await fs.writeFile(temp,native);await fs.rename(temp,m.cache);}
        else {timings.cacheHits++;const now=new Date();await fs.utimes(m.cache,now,now);}
      }
      timings.audioPreparationMs=Date.now()-audioStarted;
      const disk=await fs.statfs(root);if(disk.bavail*disk.bsize<total+16*1048576)throw new Error('Not enough disk space for the temporary playlist files.');
      const originalHash=original?hash(original):null;
      if(await digest(music).catch(e=>{if(e.code==='ENOENT')return null;throw e;})!==originalHash)throw new Error('Playlist changed while preparing audio.');
      if(original)await fs.writeFile(path.join(this.root,'original-music.iff'),original);
      const state={schema:SCHEMA,modsRoot:root,originalHash,testHash:hash(plan.music),mappings:plan.mappings,artworks:artwork.entries};
      await this.save(state);this.active=true;
      const publishStarted=Date.now();
      try{
        for(const e of artwork.entries){
          const target=await this.plain(root,e.target);await fs.mkdir(path.dirname(target),{recursive:true});
          const current=await fs.readFile(target).catch(error=>{if(error.code==='ENOENT')return null;throw error;});
          if((current?hash(current):null)!==e.originalHash)throw new Error('Artwork slot changed during preparation.');
          await fs.copyFile(e.cache,target);
        }
        for(const m of plan.mappings){
          const target=await this.plain(root,m.target);await fs.mkdir(path.dirname(target),{recursive:true});
          await fs.copyFile(m.cache,target,require('node:fs').constants.COPYFILE_EXCL);
          if(await digest(target)!==m.hash)throw new Error('Temporary audio integrity check failed.');
        }
        await fs.writeFile(music,plan.music);
      }catch(error){await this.restore().catch(()=>{});throw error;}
      timings.publishMs=Date.now()-publishStarted;timings.totalMs=Date.now()-started;
      return {audioSlotCount:71,audioPolicy:plan.policy,preparationTimings:timings};
    }finally{this.busy=false;if(!this.active)await this.clearConversionCache();}
  }
}
module.exports={JukeboxSession,planPlaylist};
