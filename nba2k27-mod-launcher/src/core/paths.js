const path = require('node:path');
const fsp = require('node:fs/promises');

const DEFAULT_GAME_ROOT = 'C:\\Program Files (x86)\\Steam\\steamapps\\common\\NBA 2K27';
async function fileKind(target) {
  const stat = await fsp.stat(target).catch(() => null);
  if (!stat) return 'missing';
  if (stat.isFile()) return 'file';
  if (stat.isDirectory()) return 'directory';
  return 'other';
}

function defaults() {
  const gameRoot = DEFAULT_GAME_ROOT;
  const modsRoot = path.join(gameRoot, 'mods');
  return {
    gameRoot,
    modsRoot,
    modsProfiles: [{ name: 'Default mods', path: modsRoot }],
    provider: 'local',
    startCameras: true,
    usePlaylistInFreestyle: true,
    autoOpenBlacktop: true,
    unlockWnbaFreestyle: true,
    unlockWnbaBlacktop: false,
    blacktopPresentation: {drone:false,dj:false,mc:false},
    blacktopCrowdEnabled: true,
    startupVideoEnabled: false,
  };
}

function normalizeModProfiles(settings) {
  // Retired launcher music settings must not reactivate or leak back into UI.
  settings={...settings};
  settings.blacktopCrowdEnabled=settings.blacktopCrowdEnabled!==false;
  const removals=settings.blacktopPresentation;
  settings.blacktopPresentation={drone:removals?.drone===true,dj:removals?.dj===true,mc:removals?.mc===true};
  for(const key of ['usePlaylistInBlacktop','blacktopPlaylistId','blacktopMusicVolume','blacktopCourtSpeaker','blacktopCourtSpeakerStrength','startupSplashEnabled','startupTeamSplashEnabled','hideWhatsNewPanel','autoUnlockMultiConfigPlayers','replaceFrontMenuPlayers','frontMenuPlayerMappings','frontMenuPlayerConfigs','frontMenuActiveConfig','unlockGameRules'])delete settings[key];
  const gameRoot = path.resolve(settings.gameRoot || DEFAULT_GAME_ROOT);
  const defaultRoot = path.join(gameRoot, 'mods');
  const activeRoot = path.resolve(settings.modsRoot || defaultRoot);
  const savedProfiles = Array.isArray(settings.modsProfiles) ? settings.modsProfiles : [];
  const savedDefault = savedProfiles.find(profile => typeof profile?.path === 'string' && profile.path.trim()
    && path.resolve(profile.path).toLowerCase() === defaultRoot.toLowerCase());
  const candidates = [
    { name: savedDefault?.name || 'Default mods', path: defaultRoot },
    ...savedProfiles,
    { name: path.basename(activeRoot) || 'Mods', path: activeRoot },
  ];
  const seen = new Set();
  const modsProfiles = [];
  for (const candidate of candidates) {
    if (!candidate || typeof candidate.path !== 'string' || !candidate.path.trim()) continue;
    const profilePath = path.resolve(candidate.path);
    const key = profilePath.toLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    modsProfiles.push({
      name: String(candidate.name || path.basename(profilePath) || 'Mods').trim().slice(0, 80),
      path: profilePath,
    });
  }
  return { ...settings, gameRoot, modsRoot: activeRoot, modsProfiles };
}

async function validateSettings(settings) {
  const gameExe = path.join(settings.gameRoot || '', 'NBA2K27.exe');
  const modsRoot = settings.modsRoot || path.join(settings.gameRoot || '', 'mods');
  const checks = {
    gameRoot: await fileKind(settings.gameRoot || ''),
    gameExe: await fileKind(gameExe),
    modsRoot: await fileKind(modsRoot),
  };
  return {
    ...checks,
    ready: checks.gameRoot === 'directory' && checks.gameExe === 'file' && checks.modsRoot === 'directory',
    gameExe,
    normalizedModsRoot: path.resolve(modsRoot),
  };
}

module.exports = { DEFAULT_GAME_ROOT, defaults, fileKind, normalizeModProfiles, validateSettings };
