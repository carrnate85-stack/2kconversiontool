const path = require('node:path');
const fsp = require('node:fs/promises');
const { spawn } = require('node:child_process');
const { fileKind } = require('./paths');
const { processIsRunning } = require('./session');

function parseAppManifest(text) {
  const appId = /"appid"\s+"(\d+)"/i.exec(String(text || ''))?.[1];
  const name = /"name"\s+"([^"]+)"/i.exec(String(text || ''))?.[1];
  const installDir = /"installdir"\s+"([^"]+)"/i.exec(String(text || ''))?.[1];
  if (!appId || !name || !installDir) throw new Error('The Steam app manifest is incomplete.');
  return { appId, name, installDir };
}

async function resolveSteamGame(gameRoot) {
  const fullGameRoot = path.resolve(gameRoot || '');
  const commonRoot = path.dirname(fullGameRoot);
  const steamAppsRoot = path.dirname(commonRoot);
  const steamRoot = path.dirname(steamAppsRoot);
  const steamExe = path.join(steamRoot, 'steam.exe');
  if ((await fileKind(steamExe)) !== 'file') throw new Error('steam.exe was not found above the configured NBA 2K27 folder.');
  const entries = await fsp.readdir(steamAppsRoot, { withFileTypes: true });
  for (const entry of entries) {
    if (!entry.isFile() || !/^appmanifest_\d+\.acf$/i.test(entry.name)) continue;
    const manifestPath = path.join(steamAppsRoot, entry.name);
    const manifest = parseAppManifest(await fsp.readFile(manifestPath, 'utf8'));
    if (manifest.name.toLowerCase() === 'nba 2k27'
      || manifest.installDir.toLowerCase() === path.basename(fullGameRoot).toLowerCase()) {
      return { ...manifest, manifestPath, steamExe, gameRoot: fullGameRoot };
    }
  }
  throw new Error('NBA 2K27 was not found in the configured Steam library manifest.');
}

async function resolveWithoutEacLaunch(gameRoot) {
  const game = await resolveSteamGame(gameRoot);
  const executable = path.join(game.gameRoot, 'NBA2K27.exe');
  if ((await fileKind(executable)) !== 'file') {
    throw new Error('The NBA 2K27 without-EAC executable was not found in the configured game folder.');
  }
  return {
    ...game,
    executable,
    launchType: 'option1',
    launchLabel: 'NBA 2K27 without EAC (offline only)',
    steamUrl: `steam://launch/${game.appId}/option1`,
  };
}

async function launchWithoutEac(gameRoot, options = {}) {
  const launch = await resolveWithoutEacLaunch(gameRoot);
  const isRunning = options.processIsRunning || processIsRunning;
  if (await isRunning('NBA2K27.exe')) return { ...launch, alreadyRunning: true, launched: false };
  const spawnProcess = options.spawn || spawn;
  const args = [launch.steamUrl];
  const child = spawnProcess(launch.steamExe, args, {
    cwd: path.dirname(launch.steamExe),
    detached: true,
    stdio: 'ignore',
    windowsHide: false,
  });
  await new Promise((resolve, reject) => {
    child.once('spawn', resolve);
    child.once('error', reject);
  });
  child.unref();
  return {
    ...launch,
    alreadyRunning: false,
    launched: true,
    processId: child.pid,
    args,
  };
}

module.exports = {
  launchWithoutEac,
  parseAppManifest,
  resolveSteamGame,
  resolveWithoutEacLaunch,
};
