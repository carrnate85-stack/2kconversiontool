const path = require('node:path');
const fsp = require('node:fs/promises');
const {randomUUID}=require('node:crypto');
const { processIsRunning } = require('./session');

const MARKER_SCHEMA = 'nba2k27-session-mod-profile-v1';
const BACKUP_NAME = '.nba2k27-mod-launcher-default-mods';

function samePath(left, right) {
  return path.resolve(left || '').toLowerCase() === path.resolve(right || '').toLowerCase();
}

function isInside(parent, child) {
  const relative = path.relative(path.resolve(parent), path.resolve(child));
  return Boolean(relative) && !relative.startsWith('..') && !path.isAbsolute(relative);
}

async function kind(target) {
  const stat = await fsp.lstat(target).catch(error => {if(error.code==='ENOENT')return null;throw error;});
  if (!stat) return 'missing';
  if (stat.isSymbolicLink()) return 'link';
  if (stat.isDirectory()) return 'directory';
  return 'other';
}

async function directoryIdentity(target) {
  const stat=await fsp.lstat(target,{bigint:true});
  if(!stat.isDirectory()||stat.isSymbolicLink())throw Error('The original mods folder is not a regular directory.');
  return {dev:String(stat.dev),ino:String(stat.ino),birth:String(stat.birthtimeNs)};
}
async function matchesIdentity(target,identity) {
  if(!identity)return false;
  const actual=await directoryIdentity(target);
  return ['dev','ino','birth'].every(key=>actual[key]===identity[key]);
}

class ModProfileManager {
  constructor({ markerPath, isGameRunning = () => processIsRunning('NBA2K27.exe') }) {
    this.markerPath = markerPath;
    this.isGameRunning = isGameRunning;
  }

  async readMarker() {
    try {
      const marker = JSON.parse(await fsp.readFile(this.markerPath, 'utf8'));
      if (marker?.schema !== MARKER_SCHEMA) throw new Error('The mod-profile recovery marker is not recognized.');
      if(!path.isAbsolute(marker.gameRoot||'')||!path.isAbsolute(marker.selectedModsRoot||'')||
        !samePath(marker.liveMods,path.join(marker.gameRoot,'mods'))||
        marker.backupPath!==null&&!samePath(marker.backupPath,path.join(marker.gameRoot,BACKUP_NAME))||
        !['preparing','moved','active','restoring'].includes(marker.stage))throw Error('The mod-profile recovery paths are invalid.');
      return marker;
    } catch (error) {
      if (error.code === 'ENOENT') return null;
      throw error;
    }
  }

  async writeMarker(marker) {
    await fsp.mkdir(path.dirname(this.markerPath), { recursive: true });
    const temporary=`${this.markerPath}.${randomUUID()}.tmp`;
    try {
      const file=await fsp.open(temporary,'wx');
      try{await file.writeFile(`${JSON.stringify(marker,null,2)}\n`,'utf8');await file.sync();}finally{await file.close();}
      await fsp.rename(temporary,this.markerPath);
    }finally{await fsp.rm(temporary,{force:true});}
  }

  async status() {
    const marker = await this.readMarker();
    return marker ? { active: marker.stage === 'active', ...marker } : { active: false };
  }

  async activate({ gameRoot, modsRoot }) {
    const game = path.resolve(gameRoot || '');
    const selected = path.resolve(modsRoot || '');
    const liveMods = path.join(game, 'mods');
    if (await this.readMarker()) {
      const result=await this.restoreIfSafe();
      if(result.reason==='game-running')throw new Error('Close NBA 2K27 before changing the active mod profile.');
    }
    if (samePath(selected, liveMods)) return { active: false, selectedModsRoot: selected, liveMods };
    if (await this.isGameRunning()) throw new Error('Close NBA 2K27 before changing the active mod profile.');
    if ((await kind(selected)) !== 'directory') throw new Error('The selected mod-profile folder does not exist.');
    if (isInside(liveMods, selected)) throw new Error('A mod profile cannot be stored inside the live mods folder. Choose a sibling such as modsfiba.');

    const liveKind = await kind(liveMods);
    if (!['directory', 'missing'].includes(liveKind)) {
      throw new Error('The game mods path is already a link or non-folder and was left untouched.');
    }
    const backupPath = path.join(game, BACKUP_NAME);
    if (liveKind === 'directory' && (await kind(backupPath)) !== 'missing') {
      throw new Error(`${BACKUP_NAME} already exists. The launcher will not overwrite it.`);
    }
    const marker = {
      schema: MARKER_SCHEMA,
      stage: 'preparing',
      gameRoot: game,
      liveMods,
      selectedModsRoot: selected,
      backupPath: liveKind === 'directory' ? backupPath : null,
      originalIdentity: liveKind === 'directory' ? await directoryIdentity(liveMods) : null,
      activatedAt: new Date().toISOString(),
    };
    let journalSaved=false;
    try {
      // Commit recovery ownership before moving any user folder.
      await this.writeMarker(marker);
      journalSaved=true;
      if (marker.backupPath) await fsp.rename(liveMods, marker.backupPath);
      marker.stage = 'moved';
      await this.writeMarker(marker);
      await fsp.symlink(selected, liveMods, 'junction');
      marker.stage = 'active';
      await this.writeMarker(marker);
      return { active: true, selectedModsRoot: selected, liveMods, backupPath: marker.backupPath };
    } catch (error) {
      try {
        if(journalSaved) {
          const result=await this.restoreIfSafe();
          if(result.reason==='game-running')throw Error('the game opened before rollback; recovery is saved for game exit');
        }
      } catch (rollbackError) {
        throw new Error(`Profile activation stopped: ${error.message}. Automatic rollback also stopped safely: ${rollbackError.message}`);
      }
      throw new Error(`The selected mod profile could not be activated safely: ${error.message}`);
    }
  }

  async restoreIfSafe({ ignoreRunning = false } = {}) {
    const marker = await this.readMarker();
    if (!marker) return { restored: false, reason: 'not-active' };
    if (!ignoreRunning && await this.isGameRunning()) return { restored: false, reason: 'game-running' };
    const liveKind = await kind(marker.liveMods);
    const backupKind=marker.backupPath?await kind(marker.backupPath):'missing';
    // The process may have stopped just before moving the original, or just
    // after restoring it. Only the exact original directory is accepted here.
    if(liveKind==='directory'&&backupKind==='missing'&&await matchesIdentity(marker.liveMods,marker.originalIdentity)) {
      await fsp.rm(this.markerPath,{force:true});
      return {restored:true,selectedModsRoot:marker.selectedModsRoot};
    }
    if(marker.backupPath&&(backupKind!=='directory'||marker.originalIdentity&&!await matchesIdentity(marker.backupPath,marker.originalIdentity)))
      throw Error('The preserved default mods folder is missing or changed; recovery left the live folder untouched.');
    if (liveKind === 'link') {
      const resolved = await fsp.realpath(marker.liveMods).catch(() => '');
      const selected = await fsp.realpath(marker.selectedModsRoot).catch(() => marker.selectedModsRoot);
      if (!samePath(resolved, selected)) {
        throw new Error('The live mods link no longer points to the recorded profile; it was left untouched.');
      }
      marker.stage='restoring';await this.writeMarker(marker);
      await fsp.rm(marker.liveMods, { force: true });
    } else if (liveKind !== 'missing') {
      throw new Error('The live mods path changed during the session; it was left untouched.');
    }
    if (marker.backupPath) {
      marker.stage='restoring';await this.writeMarker(marker);
      await fsp.rename(marker.backupPath, marker.liveMods);
    }
    await fsp.rm(this.markerPath, { force: true });
    return { restored: true, selectedModsRoot: marker.selectedModsRoot };
  }
}

module.exports = { BACKUP_NAME, MARKER_SCHEMA, ModProfileManager, isInside, samePath };
