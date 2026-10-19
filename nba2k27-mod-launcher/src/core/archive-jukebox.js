'use strict';
// Game-facing files are published by JukeboxSession only. Drafts are recoverable;
// an exact, verified archive member is required before any draft is discarded.
const fs = require('node:fs/promises');
const sync = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const {JukeboxManager, audioLogicalPath, trackKey, buildMusicIff, readMusicJson,
  readZipArchive, readZipMemberFromFile, findManifestEntry} = require('./jukebox-manager');
const {openPlaylistContainer, readPlaylistSegment} = require('./playlist-container');
const {boundedWork} = require('./bounded-work');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const optional = p => fs.readFile(p).catch(e => { if(e.code === 'ENOENT') return null; throw e; });
const custom = t => Number(t.Collection) === 1 && Number(t['Bank Index']) > 71;
async function plain(p) {
  const absolute = path.resolve(p), parsed = path.parse(absolute);
  let current = parsed.root;
  for(const part of absolute.slice(parsed.root.length).split(path.sep).filter(Boolean)) {
    current = path.join(current, part);
    const s = await fs.lstat(current).catch(e => {if(e.code === 'ENOENT') return null; throw e;});
    if(s?.isSymbolicLink()) throw new Error('Playlist storage refuses linked files or folders.');
  }
  return absolute;
}
async function put(p, bytes) {
  await plain(p); await fs.mkdir(path.dirname(p), {recursive:true});
  const tmp = `${p}.${crypto.randomUUID()}.tmp`;
  try { await fs.writeFile(tmp, bytes, {flag:'wx'}); await fs.rename(tmp,p); }
  finally { await fs.unlink(tmp).catch(e => {if(e.code !== 'ENOENT') throw e;}); }
}
async function* archiveParts(file) {
  const container = await openPlaylistContainer(file);
  if(container) {
    for(let i=0;i<container.parts.length;i++) yield {segment:i, entries:readZipArchive(await readPlaylistSegment(container,i))};
  } else yield {segment:null, entries:readZipArchive(await fs.readFile(file))};
}

class ArchiveProfile extends JukeboxManager {
  constructor(options, modsRoot) {
    const root = path.resolve(modsRoot);
    const profile = sha(Buffer.from(root.toLowerCase())).slice(0,16);
    const directory = path.join(path.dirname(options.statePath),'archive-library',profile);
    super({...options,statePath:path.join(directory,'library.json')});
    this.modsRoot=root; this.directory=directory; this.legacyStatePath=options.statePath;
    this.draftRoot=path.join(directory,'drafts'); this.ready=null;
    this.onProgress=options.onProgress||(()=>{});
    this.isGameRunning=options.isGameRunning||(()=>false);
    this.getGameRoot=options.getGameRoot;
    this.verified=new Map();
    this.opusEncoder=options.opusEncoder||null;
    this.launchCache=new (require('./playlist-launch-cache').PlaylistLaunchCache)(path.join(directory,'launch-ready-v1'));
  }
  musicPath() { return path.join(this.draftRoot,'music.iff'); }
  audioPath(_root, bank) { return path.join(this.draftRoot,...audioLogicalPath(bank).split('/')); }
  artifactPath(bank,kind) {
    return kind==='audio' ? this.audioPath(null,bank) : this.artworkPreviewPath(this.modsRoot,bank).replace(/\.png$/i,kind==='artwork'?'.tld':'.png');
  }
  archivedAssetAvailable(state,_root,bank,kind) {
    const ref=state.archiveAssets?.[bank]?.[kind];
    return Boolean(ref && sync.existsSync(ref.bundle));
  }
  viewFromState(state) {
    // Base view uses this directory as the profile identity, not as a write target.
    // Automatic recovery archives protect orphaned imports; they are not user
    // playlists. Keep them in storage but out of both playlist dropdowns.
    // No legacy-active exception: loadState also clears hidden selections, and
    // activatePlaylist rejects them so view and future playback remain aligned.
    const display={...state,playlists:state.playlists.filter(p=>!p.recoveryPlaylist)};
    return {...super.viewFromState(display,path.join(this.modsRoot,'music.iff')),storage:'playlist-archives',musicIff:this.musicPath()};
  }
  async selectAvailableArtworkSlot({_unused,bankIndex,state}) {
    if(!Number.isInteger(bankIndex)||bankIndex<1) throw new Error('Invalid artwork identity.');
    const coverName=String(state.music.Tracks.find(t=>Number(t['Bank Index'])===bankIndex)?.['Cover Name']||'001_helicopter');
    return {coverName,logicalPath:`library-artwork/${bankIndex}.tld`,absolutePath:this.artifactPath(bankIndex,'artwork'),storage:'library-v2'};
  }
  async readRef(ref) {
    if(!ref || !path.resolve(ref.bundle).startsWith(path.resolve(this.playlistFolder)+path.sep)
      || !/^[a-f0-9]{64}$/.test(ref.hash)) throw new Error('Invalid playlist asset reference.');
    await plain(ref.bundle);
    // Seek to just this member; thumbnails don't decompress a whole audio segment.
    const bytes=await readZipMemberFromFile(ref.bundle,ref.member,ref.segment);
    if(!bytes || sha(bytes)!==(ref.storedHash||ref.hash))throw new Error('Saved playlist asset failed integrity verification; no files were removed.');
    const decoded=await this.audioCodec.decode(bytes,ref);
    if(sha(decoded)!==ref.hash)throw new Error('Saved playlist decoded asset failed integrity verification.');
    return decoded;
  }
  async materialize(state,bank,kinds=['audio','artwork','preview']) {
    for(const kind of kinds) {
      const target=this.artifactPath(bank,kind);
      if(await optional(target))continue; // This may be a newer unsaved draft.
      const ref=state.archiveAssets?.[bank]?.[kind];
      if(ref)await put(target,await this.readRef(ref));
    }
  }
  async relocateManagedPaths(state) {
    // Older portable libraries stored absolute paths. Recover only the same
    // profile's managed archive directory, never arbitrary external files.
    const sources=new Set();
    const profile=path.basename(this.directory).toLowerCase();
    const addSource=directory=>{
      if(typeof directory==='string'&&path.isAbsolute(directory)&&
        path.basename(directory).toLowerCase()===profile&&
        path.basename(path.dirname(directory)).toLowerCase()==='archive-library')
        sources.add(path.resolve(directory));
    };
    addSource(state.archiveDirectory);
    const bundles=[...(state.playlists||[]).flatMap(p=>[p.bundlePath,...(p.bundlePaths||[])]),
      ...Object.values(state.archiveAssets||{}).flatMap(assets=>Object.values(assets).map(ref=>ref?.bundle))];
    for(const bundle of bundles) {
      if(typeof bundle==='string'&&path.isAbsolute(bundle)&&path.basename(path.dirname(bundle)).toLowerCase()==='playlists')
        addSource(path.dirname(path.dirname(bundle)));
    }
    const rebase=value=>{
      if(typeof value!=='string'||!path.isAbsolute(value))return value;
      for(const source of sources) {
        const relative=path.relative(source,value);
        if(relative&&!relative.startsWith('..')&&!path.isAbsolute(relative))return path.join(this.directory,relative);
      }
      return value;
    };
    const before=JSON.stringify(state);
    for(const playlist of state.playlists||[]) {
      playlist.bundlePath=rebase(playlist.bundlePath);
      if(playlist.bundlePaths)playlist.bundlePaths=playlist.bundlePaths.map(rebase);
    }
    for(const assets of Object.values(state.archiveAssets||{}))for(const ref of Object.values(assets))if(ref)ref.bundle=rebase(ref.bundle);
    for(const artwork of state.artworkOverrides||[]) {
      artwork.absolutePath=rebase(artwork.absolutePath);
      artwork.previewPath=rebase(artwork.previewPath);
    }
    // Migration ownership records deliberately retain their original targets.
    state.archiveDirectory=this.directory;
    if(JSON.stringify(state)!==before)await this.writeState(state);
  }
  async initialize() {
    const stored=await optional(this.statePath);
    if(stored) {
      const state=JSON.parse(stored);
      if(state.archiveStorageVersion===1) {
        await this.relocateManagedPaths(state);
        if(!await optional(this.musicPath()))await put(this.musicPath(),buildMusicIff(state.music));
        if(!await this.isGameRunning())await this.cleanupMigration(state);
        await this.prune(state); return;
      }
    }
    if(await this.isGameRunning())throw new Error('Close NBA 2K27 once so the launcher can safely migrate your playlists into app storage.');
    this.onProgress('Moving the jukebox into app-managed playlist storage…');
    const legacyRaw=await optional(this.legacyStatePath);
    const legacy=legacyRaw ? JSON.parse(legacyRaw) : null;
    const original=await optional(path.join(this.modsRoot,'music.iff'));
    let backup=await optional(this.defaultBackupPath)||await optional(path.join(path.dirname(this.legacyStatePath),'default-music.iff'));
    let seed=original||backup||(legacy?.music ? buildMusicIff(legacy.music) : null);
    if(!seed){
      if(!this.getGameRoot)throw new Error('Select the installed NBA 2K27 game folder to initialize the jukebox.');
      this.onProgress('Reading the default jukebox from the installed game…');
      const gameRoot=path.resolve(await this.getGameRoot());
      const entry=await findManifestEntry(gameRoot,'music.iff');
      const outputPath=path.join(this.directory,`stock-music-${crypto.randomUUID()}.decoded`);
      await fs.mkdir(this.directory,{recursive:true});
      try{
        await this.decodeEntry({archivePath:path.join(gameRoot,entry.archive),offset:entry.offset,size:entry.size,oodlePath:path.join(gameRoot,'data','oodle','oo2core_9_win64.dll'),outputPath});
        seed=await fs.readFile(outputPath);readMusicJson(seed);
        await put(this.defaultBackupPath,seed);backup=seed;
      }finally{await fs.unlink(outputPath).catch(e=>{if(e.code!=='ENOENT')throw e;});}
    }
    readMusicJson(seed); await put(this.musicPath(),seed);
    let state=stored ? JSON.parse(stored) : legacy?.music ? structuredClone(legacy) :
      {schema:'nba2k27-jukebox-library-v1',music:readMusicJson(seed),playlists:[],activePlaylistId:null,artworkOverrides:[]};
    state.playlists ||= []; state.artworkOverrides ||= []; state.archiveAssets ||= {};
    state.applied=true; // The complete metadata library is authoritative, not a legacy active music.iff.
    state.migrationFiles ||= [];
    if(legacyRaw)await put(path.join(this.directory,'legacy-library.json'),legacyRaw);
    if(original)await put(path.join(this.directory,'legacy-music.iff'),original);
    if(backup)await put(this.defaultBackupPath,backup);
    // Copy only catalogued files; never scan/delete arbitrary mod contents.
    for(const t of state.music.Tracks.filter(custom)) {
      const bank=Number(t['Bank Index']), source=path.join(this.modsRoot,...audioLogicalPath(bank).split('/'));
      await plain(source); const bytes=await optional(source);
      if(bytes) {
        await put(this.audioPath(null,bank),bytes);
        state.migrationFiles.push({path:source,hash:sha(bytes),bank,kind:'audio'});
      } else if(!await optional(this.audioPath(null,bank))) {
        // Prior share files can recover missing legacy loose audio.
        for(const playlist of state.playlists.filter(p=>p.trackKeys.includes(trackKey(t)))) {
          let recovered=false;
          for(const file of (playlist.bundlePaths?.length?playlist.bundlePaths:[playlist.bundlePath]).filter(Boolean)) {
            try {
              for await(const {entries} of archiveParts(file)) {
                const manifest=JSON.parse(entries.get('playlist.json').toString());
                const record=manifest.tracks.find(r=>r.sourceKey===trackKey(t));
                const audio=record?.audioFile&&entries.get(record.audioFile);
                if(audio){await put(this.audioPath(null,bank),await this.audioCodec.decode(audio,record));recovered=true;break;}
              }
            }catch(error){this.onProgress(`Recovery file unavailable: ${path.basename(file)} (${error.message})`);}
            if(recovered)break;
          }
          if(recovered)break;
        }
      }
    }
    const records=[];
    for(const r of state.artworkOverrides.filter(r=>path.resolve(r.modsRoot||'')===this.modsRoot)) {
      const record={...r,storage:'library-v2',logicalPath:`library-artwork/${r.bankIndex}.tld`,absolutePath:this.artifactPath(r.bankIndex,'artwork'),previewPath:r.previewPath?this.artifactPath(r.bankIndex,'preview'):null};
      for(const [kind,source] of [['artwork',r.absolutePath],['preview',r.previewPath]]) {
        if(!source)continue;
        const resolved=path.resolve(source);
        const privateRoot=path.join(path.dirname(this.legacyStatePath),'artwork-previews')+path.sep;
        if(!resolved.startsWith(this.modsRoot+path.sep)&&!resolved.startsWith(privateRoot)&&!resolved.startsWith(this.directory+path.sep))throw new Error('Legacy artwork points outside managed storage.');
        await plain(resolved); const bytes=await optional(resolved);
        if(bytes){await put(this.artifactPath(r.bankIndex,kind),bytes);if(resolved!==this.artifactPath(r.bankIndex,kind))state.migrationFiles.push({path:resolved,hash:sha(bytes),bank:r.bankIndex,kind});}
      }
      records.push(record);
    }
    state.artworkOverrides=records;
    // Old exports stay intact as recovery copies; newly managed files have their own directory.
    for(const p of state.playlists){p.bundlePath=null;p.bundlePaths=[];}
    await this.writeState(state);
    await this.ensureRecovered(state);
    for(const playlist of state.playlists) await this.exportPlaylistBundle(this.modsRoot,state,playlist);
    state.archiveStorageVersion=1;
    state.archiveDirectory=this.directory;
    if(original&&legacy?.music){
      const active=legacy.playlists?.find(p=>p.id===legacy.activePlaylistId);
      const expected={...legacy.music,Tracks:legacy.music.Tracks.filter(t=>active?active.trackKeys.includes(trackKey(t)):!custom(t))};
      // A library file existing elsewhere does not establish ownership of this profile's music mod.
      if(JSON.stringify(readMusicJson(original))===JSON.stringify(expected))
        state.migrationMusic={path:path.join(this.modsRoot,'music.iff'),hash:sha(original)};
    }
    await this.writeState(state);
    await this.cleanupMigration(state);
    await this.prune(state);
  }
  async cleanupMigration(state) {
    let changed=false;
    for(const entry of state.migrationFiles||[]) {
      if(entry.removed)continue;
      const target=path.resolve(entry.path||'');
      const legacyPreviewRoot=path.join(path.dirname(this.legacyStatePath),'artwork-previews')+path.sep;
      if(!/^[a-f0-9]{64}$/.test(entry.hash)||!Number.isInteger(Number(entry.bank))||
        !['audio','artwork','preview'].includes(entry.kind)||
        (entry.kind==='audio' ? target!==path.join(this.modsRoot,...audioLogicalPath(entry.bank).split('/')) :
          !target.startsWith(this.modsRoot+path.sep)&&!target.startsWith(legacyPreviewRoot)))
        throw new Error('Invalid playlist migration cleanup entry; originals retained.');
      const ref=state.archiveAssets?.[entry.bank]?.[entry.kind];
      if(!ref||ref.hash!==entry.hash)continue;
      // Recoverable from verified .2kplaylist. Changed or linked originals stay untouched.
      await this.readRef(ref); await plain(entry.path);
      const current=await optional(entry.path);
      if(current&&sha(current)!==entry.hash)continue;
      if(current)await fs.unlink(entry.path);
      entry.removed=true;changed=true;
    }
    const music=state.migrationMusic;
    if(music&&!music.removed){
      if(path.resolve(music.path)!==path.join(this.modsRoot,'music.iff')||!/^[a-f0-9]{64}$/.test(music.hash))throw new Error('Invalid playlist metadata migration entry.');
      await plain(music.path);const current=await optional(music.path);
      if(!current||sha(current)===music.hash){if(current)await fs.unlink(music.path);music.removed=true;changed=true;}
    }
    if(changed)await this.writeState(state);
  }
  async loadState(root) {
    if(!this.ready)this.ready=this.initialize().catch(error=>{this.ready=null;throw error;});
    await this.ready;
    const result=await super.loadState(root);
    if(result.state.playlists.some(p=>p.recoveryPlaylist&&p.id===result.state.activePlaylistId)){
      result.state.activePlaylistId=null;
      // Only app-managed draft metadata changes, never live game-facing files.
      await this.writeActiveMusic(result.state,this.musicPath());
    }
    return result;
  }
  async activatePlaylist(root,id) {
    const {state}=await this.loadState(root);
    if(id&&state.playlists.some(p=>p.id===id&&p.recoveryPlaylist))throw new Error('This is a recovery backup, not a selectable playlist.');
    return super.activatePlaylist(root,id);
  }
  async ensureRecovered(state) {
    const assigned=new Set(state.playlists.flatMap(p=>p.trackKeys));
    const unassigned=state.music.Tracks.filter(t=>custom(t)&&!assigned.has(trackKey(t))&&
      (sync.existsSync(this.audioPath(null,t['Bank Index']))||state.archiveAssets?.[t['Bank Index']]?.audio)).map(trackKey);
    if(unassigned.length){
      let p=state.playlists.find(p=>p.recoveryPlaylist);
      if(!p){p={id:crypto.randomUUID(),name:'Recovered songs',trackKeys:[],recoveryPlaylist:true};state.playlists.push(p);}
      p.trackKeys=[...new Set([...p.trackKeys,...unassigned])];
      return p;
    }
    return null;
  }
  async exportPlaylistBundle(root,state,playlist) {
    const destination=this.playlistBundlePath(playlist);
    const replacedPaths=new Set([destination,playlist.bundlePath,...(playlist.bundlePaths||[])].filter(Boolean));
    // Preserve sources whose old member positions are about to be replaced.
    for(const [bank,assets] of Object.entries(state.archiveAssets||{})) {
      if(Object.values(assets).some(r=>replacedPaths.has(r.bundle)))await this.materialize(state,Number(bank));
    }
    for(const t of state.music.Tracks.filter(t=>playlist.trackKeys.includes(trackKey(t))))await this.materialize(state,Number(t['Bank Index']));
    if(this.opusEncoder){
      const tracks=state.music.Tracks.filter(t=>custom(t)&&playlist.trackKeys.includes(trackKey(t)));
      for(let i=0;i<tracks.length;i++){
        const t=tracks[i],file=this.audioPath(null,Number(t['Bank Index'])),bytes=await fs.readFile(file);
        if(!require('./native-opus').isNativeOpus(bytes)){
          this.onProgress(`Compressing ${playlist.name}: ${i+1}/${tracks.length} — ${t.Name}`);
          const encoded=await this.opusEncoder(bytes);require('./native-opus').parse(encoded);await put(file,encoded);
        }
      }
    }
    const report=await super.exportPlaylistBundle(root,state,playlist);
    state.archiveAssets ||= {};
    for(const assets of Object.values(state.archiveAssets))for(const [kind,ref] of Object.entries(assets)) {
      if(replacedPaths.has(ref.bundle))delete assets[kind];
    }
    // Read back once per bounded segment before committing references or pruning drafts.
    for await(const {segment,entries} of archiveParts(report.path)) {
      const manifest=JSON.parse(entries.get('playlist.json').toString());
      for(const r of manifest.tracks) {
        const bank=Number(r.metadata['Bank Index']);
        for(const [kind,member] of [['audio',r.audioFile],['artwork',r.artworkFile],['preview',r.artworkPreviewFile]]) {
          if(!member)continue;const bytes=entries.get(member);
          if(!bytes)throw new Error('Saved playlist failed read-back verification. Drafts were retained.');
          const draft=await fs.readFile(this.artifactPath(bank,kind));
          const decoded=kind==='audio'?await this.audioCodec.decode(bytes,r):bytes;
          if(sha(decoded)!==sha(draft))throw new Error('Saved playlist differs from its draft. No drafts were removed.');
          state.archiveAssets[bank] ||= {};
          state.archiveAssets[bank][kind]={bundle:report.path,segment,member,hash:sha(decoded),...(kind==='audio'&&r.audioCodec==='flac'?{storedHash:sha(bytes),audioCodec:'flac',audioWaveSha256:r.audioWaveSha256,audioPcmBytes:r.audioPcmBytes}:kind==='audio'&&r.audioCodec==='native-opus'?{audioCodec:'native-opus',audioNativeSha256:r.audioNativeSha256}:{})};
        }
      }
    }
    const stat=await fs.stat(report.path);this.verified.set(report.path,{size:stat.size,mtimeMs:stat.mtimeMs});
    playlist.archiveMetadataHash=this.metadataHash(state,playlist);
    await this.writeState(state);
    return report;
  }
  metadataHash(state,playlist) {
    return sha(Buffer.from(JSON.stringify({name:playlist.name,keys:playlist.trackKeys,
      tracks:playlist.trackKeys.map(k=>state.music.Tracks.find(t=>trackKey(t)===k))})));
  }
  async needsSave(state,playlist) {
    if(!playlist.bundlePath||!sync.existsSync(playlist.bundlePath)||playlist.archiveMetadataHash!==this.metadataHash(state,playlist))return true;
    for(const t of state.music.Tracks.filter(t=>playlist.trackKeys.includes(trackKey(t))&&custom(t))) {
      const bank=Number(t['Bank Index']);if(!state.archiveAssets?.[bank]?.audio)return true;
      for(const kind of ['audio','artwork','preview']) {
        const bytes=await optional(this.artifactPath(bank,kind));
        if(bytes&&sha(bytes)!==state.archiveAssets?.[bank]?.[kind]?.hash)return true;
      }
    }
    return false;
  }
  async prune(state) {
    for(const [bank,assets] of Object.entries(state.archiveAssets||{}))for(const [kind,ref] of Object.entries(assets)) {
      const target=this.artifactPath(Number(bank),kind);await plain(target);const bytes=await optional(target);
      if(bytes&&sha(bytes)===ref.hash){
        const stat=await fs.stat(ref.bundle),verified=this.verified.get(ref.bundle);
        if(!verified||verified.size!==stat.size||verified.mtimeMs!==stat.mtimeMs)await this.readRef(ref);
        await fs.unlink(target);
        if(kind==='audio'){
          const preview=target+'.ogg';await plain(preview);
          const ogg=await optional(preview);
          if(ogg&&require('./native-opus').isNativeOpus(bytes)&&ogg.equals(require('./native-opus').toOgg(bytes)))await fs.unlink(preview);
        }
      }
    }
  }
  async settle() {
    const {state}=await this.loadState(this.modsRoot);
    const recovered=await this.ensureRecovered(state);
    if(recovered)await this.exportPlaylistBundle(this.modsRoot,state,recovered);
    await this.writeState(state); await this.prune(state);
  }
  async refreshPlaylistBundle(root,id) {
    const {state,musicIff}=await this.loadState(root);
    const p=state.playlists.find(p=>p.id===id);if(!p)throw new Error('Playlist no longer exists.');
    const bundleReport=await this.exportPlaylistBundle(root,state,p);
    await this.settle(); return {...this.viewFromState(state,musicIff),savedPlaylistId:id,bundleReport};
  }
  async previewTrack(root,key) {
    const {state}=await this.loadState(root),t=state.music.Tracks.find(t=>trackKey(t)===key);
    if(t)await this.materialize(state,Number(t['Bank Index']),['audio']);
    return super.previewTrack(root,key);
  }
  async previewArtwork(root,key) {
    const {state}=await this.loadState(root),t=state.music.Tracks.find(t=>trackKey(t)===key);
    if(t)await this.materialize(state,Number(t['Bank Index']),['preview']);
    return super.previewArtwork(root,key);
  }
  async deletePlaylist(root,id) {
    const {state}=await this.loadState(root),p=state.playlists.find(p=>p.id===id);
    if(!p)return this.viewFromState(state);
    // Keep its songs in a recovery playlist rather than deleting their only originals.
    state.playlists=state.playlists.filter(p=>p.id!==id);
    if(state.activePlaylistId===id)state.activePlaylistId=null;
    await this.ensureRecovered(state);
    for(const remaining of state.playlists)await this.exportPlaylistBundle(root,state,remaining);
    await this.writeActiveMusic(state,this.musicPath());
    const stillUsed=new Set(Object.values(state.archiveAssets||{}).flatMap(a=>Object.values(a).map(r=>r.bundle)));
    if(p.bundlePath&&!stillUsed.has(p.bundlePath)&&path.dirname(p.bundlePath)===this.playlistFolder){await plain(p.bundlePath);await fs.unlink(p.bundlePath).catch(e=>{if(e.code!=='ENOENT')throw e;});}
    await this.prune(state);return this.viewFromState(state);
  }
  async restoreDefault(root) {
    // Selecting stock music must never discard the custom archives or their metadata.
    const view=await this.activatePlaylist(root,null);
    return {...view,restoreReport:{source:'Unmodified stock game soundtrack',trackCount:view.visibleTrackCount}};
  }
  async repairMissingCustomTracks(root) {
    const {state}=await this.loadState(root);
    // Missing archives are an error to repair, not permission to delete their track records.
    const missing=state.music.Tracks.filter(t=>custom(t)&&!sync.existsSync(this.audioPath(root,t['Bank Index']))&&!this.archivedAssetAvailable(state,root,t['Bank Index'],'audio'));
    return {...this.viewFromState(state),repairReport:{removed:0,missing:missing.length}};
  }
  async launchSource({cachePreparedPlaylists=true}={}) {
    await this.settle();
    const {state}=await this.loadState(this.modsRoot);
    const p=state.playlists.find(p=>p.id===state.activePlaylistId);
    if(!p)return {enabled:false};
    // Save pending changes before creating game-facing copies.
    if(await this.needsSave(state,p))await this.exportPlaylistBundle(this.modsRoot,state,p);
    const banks=[...new Set(state.music.Tracks.filter(t=>p.trackKeys.includes(trackKey(t))).map(t=>Number(t['Bank Index'])))];
    if(!cachePreparedPlaylists){
      await this.launchCache.retain([]);
      await boundedWork(banks,2,bank=>this.materialize(state,bank,['audio','artwork']));
      const music={...state.music,Tracks:state.music.Tracks.filter(t=>p.trackKeys.includes(trackKey(t)))};
      return {enabled:true,playlistBytes:buildMusicIff(music),sourceRoot:this.draftRoot,libraryPath:this.statePath};
    }
    const preparedAudio={},cacheKeys=[],launchCacheTimings={hits:0,misses:0,bytes:0};
    const started=Date.now();
    await boundedWork(banks,2,async bank=>{
      await this.materialize(state,bank,['artwork']);
      if(bank<=71)return;
      const ref=state.archiveAssets?.[bank]?.audio;if(!ref)throw new Error('Selected song has no saved audio archive.');
      // An unchanged decoded-source hash reuses its already wrapped native audio.
      const cached=await this.launchCache.get(ref.hash,()=>this.readRef(ref));
      preparedAudio[audioLogicalPath(bank)]=cached;cacheKeys.push(ref.hash);
      launchCacheTimings[cached.hit?'hits':'misses']++;launchCacheTimings.bytes+=cached.size;
    });
    await this.launchCache.retain(cacheKeys);
    launchCacheTimings.totalMs=Date.now()-started;
    const music={...state.music,Tracks:state.music.Tracks.filter(t=>p.trackKeys.includes(trackKey(t)))};
    return {enabled:true,playlistBytes:buildMusicIff(music),sourceRoot:this.draftRoot,libraryPath:this.statePath,preparedAudio,launchCacheTimings};
  }
}

class ArchiveJukeboxManager {
  constructor(options){this.options=options;this.profiles=new Map();this.tail=Promise.resolve();this.last=null;}
  profile(root){const key=path.resolve(this.options.libraryRoot || root);if(!this.profiles.has(key))this.profiles.set(key,new ArchiveProfile(this.options,key));this.last=this.profiles.get(key);return this.last;}
  run(fn){const next=this.tail.then(fn);this.tail=next.catch(()=>{});return next;}
  async ensurePlaylistFolder(){return this.last ? this.last.ensurePlaylistFolder() : path.dirname(this.options.statePath);}
  async launchSource(root,options){return this.run(()=>this.profile(root).launchSource(options));}
  async clearLaunchCache(root){return this.run(async()=>{if(await this.options.isGameRunning?.())return false;await this.profile(root).launchCache.retain([]);return true;});}
  async releaseCache(){return this.run(async()=>{for(const p of this.profiles.values())if(p.ready)await p.settle();});}
}
for(const method of ['view','repairMissingCustomTracks','importWav','importWaveBytes','refreshPlaylistBundle','previewTrack','previewArtwork','restoreDefault','saveTrack','savePlaylist','activatePlaylist','deletePlaylist','importPlaylistBundle']) {
  ArchiveJukeboxManager.prototype[method]=function(root,...args){return this.run(async()=>{
    const p=this.profile(root);const result=await p[method](p.modsRoot,...args);
    // Preview files stay alive until the next non-preview operation or session cleanup.
    if(!['view','previewTrack','previewArtwork'].includes(method))await p.settle();
    return result;
  });};
}
module.exports={ArchiveJukeboxManager,ArchiveProfile};
