const fs = require('node:fs');
const fsp = require('node:fs/promises');
const path = require('node:path');
const crypto = require('node:crypto');

const PRESENTATION_DIRECTORY = 'presentation';
const MOVIE_FILE = 'startup.mov';
const MAX_MOVIE_BYTES = 2 * 1024 * 1024 * 1024;

function presentationPaths(modsRoot) {
  const root = path.resolve(modsRoot);
  const directory = path.join(root, PRESENTATION_DIRECTORY);
  return { root, directory, movie: path.join(directory, MOVIE_FILE) };
}

async function plainFile(filePath) {
  const stat = await fsp.lstat(filePath).catch(() => null);
  return stat?.isFile() && !stat.isSymbolicLink() ? stat : null;
}

async function atomicCopy(source, target) {
  await fsp.mkdir(path.dirname(target), { recursive: true });
  const temporary = `${target}.${process.pid}.${crypto.randomUUID()}.tmp`;
  try {
    await fsp.copyFile(source, temporary, fs.constants.COPYFILE_EXCL);
    await fsp.rename(temporary, target);
  } finally { await fsp.rm(temporary, { force: true }).catch(() => {}); }
}

async function inspectNativeMovie(filePath) {
  const stat = await plainFile(filePath);
  if (!stat || stat.size < 0x4000 || stat.size > MAX_MOVIE_BYTES) {
    throw new Error('The prepared native movie must be between 16 KB and 2 GB.');
  }
  const handle = await fsp.open(filePath, 'r');
  try {
    const header = Buffer.alloc(64);
    if ((await handle.read(header, 0, header.length, 0)).bytesRead !== header.length) throw new Error('The native movie header is truncated.');
    const version = header.readUInt16LE(0);
    const codecId = header.readUInt16LE(2);
    const width = header.readUInt32LE(4);
    const height = header.readUInt32LE(8);
    const frames = header.readUInt32LE(12);
    const numerator = header.readUInt32LE(16);
    const denominator = header.readUInt32LE(20);
    const blockCapacity = header.readUInt32LE(0x28);
    let size = header.readUInt32LE(0x30);
    if (![2, 3, 4].includes(version)) throw new Error('This is not a recognized native NBA 2K movie. Renaming an MP4 to .mov will not work.');
    if (!width || width > 8192 || !height || height > 8192 || !frames || frames > 1_000_000) throw new Error('The native movie dimensions or frame count are invalid.');
    if (!numerator || !denominator || numerator / denominator < 1 || numerator / denominator > 240) throw new Error('The native movie frame rate is invalid.');
    if (blockCapacity < 24 || blockCapacity > 0x100000) throw new Error('The native movie block capacity is invalid.');
    let offset = version >= 4 ? 0x4000 : 0x1000;
    let totalFrames = 0;
    let blocks = 0;
    const blockHeader = Buffer.alloc(8);
    while (size) {
      if (++blocks > 100000 || size < 24 || size > 0x100000 || size > blockCapacity || offset + size > stat.size) throw new Error('The native movie block chain is unsafe or truncated.');
      if ((await handle.read(blockHeader, 0, blockHeader.length, offset)).bytesRead !== blockHeader.length) throw new Error('The native movie block header is truncated.');
      const nextSize = blockHeader.readUInt32LE(0);
      const count = blockHeader.readUInt32LE(4);
      if (count > 0x1000 || totalFrames + count > frames) throw new Error('The native movie frame table is invalid.');
      totalFrames += count;
      offset += size;
      size = nextSize;
    }
    if (!blocks || totalFrames !== frames) throw new Error('The native movie frame total does not match its header.');
    return {
      nativeReady: true, version, codecId, width, height, frames, fps: numerator / denominator,
      durationSeconds: frames * denominator / numerator, audioChannels: header[0x18],
      bytes: stat.size, blocks, trailingBytes: stat.size - offset,
    };
  } finally { await handle.close(); }
}

class PresentationManager {
  constructor({ isGameRunning = async () => false, convertMp4, onProgress=()=>{}, resolveLibraryRoot=async root=>root,libraryDirectory=null } = {}) { this.isGameRunning = isGameRunning; this.convertMp4=convertMp4; this.onProgress=onProgress;this.busy=false;this.controller=null;this.resolveLibraryRoot=resolveLibraryRoot;this.libraryDirectory=libraryDirectory; }

  paths(root){return this.libraryDirectory?{root:path.resolve(root),directory:this.libraryDirectory,movie:path.join(this.libraryDirectory,MOVIE_FILE)}:presentationPaths(root);}

  cancel(){this.controller?.abort();}

  async library(modsRoot) {
    modsRoot=await this.resolveLibraryRoot(modsRoot);
    const file=path.join(this.paths(modsRoot).directory,'library.json');
    let data;try{data=JSON.parse(await fsp.readFile(file,'utf8'));}catch(error){if(error.code==='ENOENT')return {selected:null,items:[]};throw Error('The video library could not be read. Restore library.json before editing it.');}
    if(!Array.isArray(data.items)||data.items.some(item=>!/^[-a-f0-9]{36}$/.test(item.id)||typeof item.name!=='string'))throw Error('Invalid video library.');
    return data;
  }

  async saveLibrary(modsRoot,data){
    modsRoot=await this.resolveLibraryRoot(modsRoot);
    const target=path.join(this.paths(modsRoot).directory,'library.json'),temp=target+'.'+crypto.randomUUID()+'.tmp';
    await fsp.mkdir(path.dirname(target),{recursive:true});
    try{await fsp.writeFile(temp,JSON.stringify(data,null,2),{flag:'wx'});await fsp.rename(temp,target);}finally{await fsp.rm(temp,{force:true});}
  }

  async editLibrary(modsRoot,action,id,name){
    modsRoot=await this.resolveLibraryRoot(modsRoot);
    if(this.busy)throw Error('Wait for the video operation to finish.');
    this.busy=true;
    try{
      await this.assertEditable();const data=await this.library(modsRoot),item=data.items.find(item=>item.id===id);
      if(!item)throw Error('Video not found. Refresh the library.');
      const paths=this.paths(modsRoot),file=path.join(paths.directory,'videos',id+'.mov');
      if(action==='rename'){
        if(typeof name!=='string'||!name.trim()||name.trim().length>120)throw Error('Enter a video name between 1 and 120 characters.');
        item.name=name.trim();
      }else if(action==='select'){
        await inspectNativeMovie(file);await this.assertEditable();await atomicCopy(file,paths.movie);data.selected=id;
      }else if(action==='remove'){
        if(data.selected===id)throw Error('Select another startup video before removing this one.');
        data.items=data.items.filter(item=>item.id!==id);
      }else throw Error('Invalid video operation.');
      await this.saveLibrary(modsRoot,data);
      if(action==='remove')await fsp.rm(file,{force:true});
      return await this.view(modsRoot);
    }finally{this.busy=false;}
  }

  async assertEditable() {
    if (await this.isGameRunning()) throw new Error('Close NBA 2K27 before changing presentation files.');
  }

  async view(modsRoot) {
    modsRoot=await this.resolveLibraryRoot(modsRoot);
    const paths = this.paths(modsRoot);
    const movie = await plainFile(paths.movie);
    let movieInfo = null;
    let movieError = null;
    if (movie) {
      try { movieInfo = await inspectNativeMovie(paths.movie); }
      catch (error) { movieError = error.message; }
    }
    return {
      directory: paths.directory,
      library: await this.library(modsRoot),
      movie: movie ? { present: true, path: paths.movie, bytes: movie.size, info: movieInfo, error: movieError } : { present: false, path: paths.movie },
    };
  }

  async importMovie(modsRoot, sourcePath, {addToLibrary=false}={}) {
    modsRoot=await this.resolveLibraryRoot(modsRoot);
    if(this.busy)throw new Error('A movie is already being imported.');
    this.busy=true;this.controller=new AbortController();
    const source=path.resolve(sourcePath),paths=this.paths(modsRoot);let work;
    try{
      await this.assertEditable();
      let prepared=source;
      if(path.extname(source).toLowerCase()==='.mp4'){
        if(!this.convertMp4)throw Error('The MP4 converter is unavailable.');
        await fsp.mkdir(paths.directory,{recursive:true});work=await fsp.mkdtemp(path.join(paths.directory,'.movie-import-'));prepared=path.join(work,'converted.mov');
        await this.convertMp4({sourcePath:source,outputPath:prepared,onProgress:this.onProgress,signal:this.controller.signal});
      }
      await inspectNativeMovie(prepared);
      await this.assertEditable();
      if(this.controller.signal.aborted)throw Error('Video import was cancelled.');
      if(addToLibrary){
        const data=await this.library(modsRoot);
        // Preserve the previous single-video setup when the library is first used.
        if(!data.items.length&&await plainFile(paths.movie)){
          const oldInfo=await inspectNativeMovie(paths.movie),oldId=crypto.randomUUID();
          await atomicCopy(paths.movie,path.join(paths.directory,'videos',oldId+'.mov'));
          data.items.push({id:oldId,name:'Previous startup video',...oldInfo});data.selected=oldId;
        }
        const id=crypto.randomUUID(),info=await inspectNativeMovie(prepared);
        await atomicCopy(prepared,path.join(paths.directory,'videos',id+'.mov'));
        data.items.push({id,name:path.basename(source,path.extname(source)).slice(0,120),...info});
        // Adding a video does not replace an existing selection.
        if(!data.selected){await atomicCopy(prepared,paths.movie);data.selected=id;}
        await this.saveLibrary(modsRoot,data);
        return await this.view(modsRoot);
      }
      if(prepared.toLowerCase()!==paths.movie.toLowerCase())await atomicCopy(prepared,paths.movie);
      return await this.view(modsRoot);
    }finally{try{if(work)await fsp.rm(work,{recursive:true,force:true});}finally{this.busy=false;this.controller=null;}}
  }

  async remove(modsRoot, kind) {
    modsRoot=await this.resolveLibraryRoot(modsRoot);
    if(this.busy)throw new Error('Wait for the movie import to finish.');
    await this.assertEditable();
    const paths = this.paths(modsRoot);
    if (kind !== 'movie') throw new Error('Only the startup movie is supported.');
    await fsp.rm(paths.movie, { force: true });
    return this.view(modsRoot);
  }
}

module.exports = { MAX_MOVIE_BYTES, MOVIE_FILE, PRESENTATION_DIRECTORY, PresentationManager, inspectNativeMovie, presentationPaths };
