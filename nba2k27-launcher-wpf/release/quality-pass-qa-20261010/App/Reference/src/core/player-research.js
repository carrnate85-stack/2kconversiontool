'use strict';

const { execFile } = require('node:child_process');
const fsSync = require('node:fs');
const fs = require('node:fs/promises');
const path = require('node:path');
const crypto = require('node:crypto');
const readline = require('node:readline');
const { promisify } = require('node:util');

const execFileAsync = promisify(execFile);
const TEAM_UNIFORM_COLOR_FIELDS = Object.freeze([
  ['uniformPrimaryColor', 'uniform-primary'], ['uniformSecondaryColor', 'uniform-secondary'],
  ['primaryColor', 'primary'], ['secondaryColor', 'secondary'], ['tertiaryColor', 'tertiary'],
  ['quaternaryColor', 'quaternary'], ['quintaryColor', 'quintary'], ['sextaryColor', 'sextary'],
  ['primaryAccessoryColor', 'primary-accessory'], ['secondaryAccessoryColor', 'secondary-accessory'],
  ['tertiaryAccessoryColor', 'tertiary-accessory'], ['quaternaryAccessoryColor', 'quaternary-accessory'],
  ['quintaryAccessoryColor', 'quintary-accessory'],
  ['underShirtColorHome', 'undershirt-home'], ['underShirtColorAway', 'undershirt-away'],
  ['shoePrimaryColorAway', 'away-shoe-primary'],
  ['primarySockColor', 'primary-sock'], ['secondarySockColor', 'secondary-sock'],
  ['tertiarySockColor', 'tertiary-sock'], ['quaternarySockColor', 'quaternary-sock'],
  ['quintarySockColor', 'quintary-sock'],
]);
const TEAM_UNIFORM_PACKED_FIELDS = Object.freeze([
  ['typeId', 'type', 0, 20], ['logoTypeId', 'logo-type', 0, 5], ['logoBrandId', 'logo-brand', 0, 3],
  ['dominantColorId', 'dominant-color', 0, 20], ['scorebugUsesUniformColor', 'scorebug-uniform-color', 0, 1],
  ['disablePhoto', 'disable-photo', 0, 1], ['usesTeamLogo', 'use-team-logo', 0, 1],
  ['homePriority', 'home-priority', 0, 15], ['awayPriority', 'away-priority', 0, 15],
  ['topNamePlateStyleId', 'top-nameplate', 0, 3], ['bottomNamePlateStyleId', 'bottom-nameplate', 0, 3],
  ['sideId', 'side', 0, 1], ['overrideNbaPatch', 'override-nba-patch', 0, 1],
  ['hasNumberOnShorts', 'number-on-shorts', 0, 1], ['isAlternate', 'alternate', 0, 1],
  ['primaryLogoSockColor', 'primary-logo-sock', 0, 1], ['secondaryLogoSockColor', 'secondary-logo-sock', 0, 1],
  ['tertiaryLogoSockColor', 'tertiary-logo-sock', 0, 1], ['quaternaryLogoSockColor', 'quaternary-logo-sock', 0, 1],
  ['quintaryLogoSockColor', 'quintary-logo-sock', 0, 1], ['rightAlignFrontNumber', 'right-align-front-number', 0, 1],
  ['legAccessoryColorHome', 'leg-home', 0, 7], ['legAccessoryColorAway', 'leg-away', 0, 7],
  ['armItemColorHome', 'arm-home', 0, 7], ['armItemColorAway', 'arm-away', 0, 7],
  ['sockColorHome', 'sock-home', 0, 7], ['sockColorAway', 'sock-away', 0, 7],
  ['designationId', 'designation', 0, 10],
]);

async function nba2k27IsRunning() {
  const tasklist = path.join(process.env.SystemRoot || 'C:\\Windows', 'System32', 'tasklist.exe');
  const { stdout = '' } = await execFileAsync(tasklist, ['/FI', 'IMAGENAME eq NBA2K27.exe', '/FO', 'CSV', '/NH'], {
    windowsHide: true, timeout: 5000, encoding: 'utf8',
  }).catch(() => ({ stdout: '' }));
  return stdout.toLowerCase().includes('"nba2k27.exe"');
}

function normalizePlayerName(value) {
  return String(value || '')
    .normalize('NFKD')
    .replace(/[\u0300-\u036f]/g, '')
    .replace(/[.’]/g, "'")
    .replace(/[^a-z0-9']+/gi, ' ')
    .trim()
    .replace(/\s+/g, ' ')
    .toLowerCase();
}

function joinReferenceFaceIds(report, reference) {
  const byName = new Map();
  for (const entry of reference?.entries || []) {
    if (!Number.isInteger(entry.id)) continue;
    const key = normalizePlayerName(entry.name);
    if (!key) continue;
    const values = byName.get(key) || [];
    if (!values.includes(entry.id)) values.push(entry.id);
    byName.set(key, values);
  }
  let covered = 0;
  let unambiguous = 0;
  let comparable = 0;
  let matchesLive = 0;
  const players = (report?.players || []).map((player) => {
    const ids = byName.get(normalizePlayerName(player.name)) || [];
    if (ids.length) covered += 1;
    if (ids.length === 1) {
      unambiguous += 1;
      if (Number.isInteger(player.cyberfaceId)) {
        comparable += 1;
        if (ids[0] === player.cyberfaceId) matchesLive += 1;
      }
    }
    return {
      ...player,
      referenceFaceIds: ids,
      referenceFaceIdStatus: ids.length === 1
        ? (ids[0] === player.cyberfaceId ? 'matches-live' : 'differs-from-live')
        : ids.length > 1 ? 'ambiguous-name-reference' : 'unavailable',
    };
  });
  return {
    ...report,
    players,
    referenceFaceIdCoverage: covered,
    unambiguousReferenceFaceIds: unambiguous,
    referenceComparableFaceIds: comparable,
    referenceMatchingLiveFaceIds: matchesLive,
    referenceLiveMatchRate: comparable ? matchesLive / comparable : 0,
    referenceFaceIdSource: {
      schema: reference?.schema || null,
      sourceName: reference?.sourceName || null,
      sourceSha256: reference?.sourceSha256 || null,
      tableVersion: reference?.tableVersion || null,
      status: reference?.status || 'reference-only',
    },
  };
}

function baseCyberfaceId(logicalPath) {
  const normalized = String(logicalPath || '').trim().replaceAll('\\', '/').split(',', 1)[0];
  const match = normalized.match(/(?:^|\/)png(\d{4,5})\.iff$/i);
  return match ? Number(match[1]) : null;
}

async function scanManifestAssets(manifestPath) {
  const ids = new Set();
  const uniformBases = new Set();
  const lines = readline.createInterface({
    input: fsSync.createReadStream(manifestPath, { encoding: 'utf8' }),
    crlfDelay: Infinity,
  });
  for await (const line of lines) {
    const id = baseCyberfaceId(line);
    if (Number.isInteger(id)) ids.add(id);
    const logical = String(line).split(',', 1)[0].replaceAll('\\', '/');
    const uniform = logical.match(/^char\/uniforms\/([^/]+)\/([^/]+)\.iff$/i);
    if (uniform && uniform[1].toLowerCase() === uniform[2].toLowerCase()) uniformBases.add(uniform[1].toLowerCase());
  }
  return { cyberfaceIds: ids, uniformBases };
}

async function scanManifestCyberfaces(manifestPath) {
  return (await scanManifestAssets(manifestPath)).cyberfaceIds;
}

async function manifestHasUniformResource(manifestPath, baseFilename) {
  const expected = `char/uniforms/${baseFilename}/${baseFilename}.iff`.toLowerCase();
  const lines = readline.createInterface({
    input: fsSync.createReadStream(manifestPath, { encoding: 'utf8' }),
    crlfDelay: Infinity,
  });
  for await (const line of lines) {
    const logical = String(line).split(',', 1)[0].replaceAll('\\', '/').toLowerCase();
    if (logical === expected) {
      lines.close();
      return true;
    }
  }
  return false;
}

async function scanLooseCyberfaces(root) {
  const ids = new Set();
  const pending = [root];
  let visited = 0;
  while (pending.length) {
    const directory = pending.pop();
    let entries;
    try { entries = await fs.readdir(directory, { withFileTypes: true }); }
    catch (error) {
      if (error.code === 'ENOENT') continue;
      throw error;
    }
    for (const entry of entries) {
      visited += 1;
      if (visited > 100_000) throw new Error('The Mods folder exceeds the bounded player-asset scan limit.');
      const fullPath = path.join(directory, entry.name);
      if (entry.isDirectory()) pending.push(fullPath);
      else if (entry.isFile()) {
        const id = baseCyberfaceId(entry.name);
        if (Number.isInteger(id)) ids.add(id);
      }
    }
  }
  return ids;
}

function rosterRecordKey(player) {
  return [normalizePlayerName(player.name), player.cyberfaceId, player.recordIndex].join('|');
}

function retainedTeam(team) {
  const stored = { ...team, recordAddress: '', source: 'cataloged-team' };
  stored.uniforms = Array.isArray(team?.uniforms) ? team.uniforms.map((uniform) => {
    const retained = { ...uniform };
    delete retained.recordAddress;
    return retained;
  }) : [];
  return stored;
}

function mergeTeamCatalog(liveTeams = [], catalogedTeams = []) {
  const byId = new Map();
  for (const team of liveTeams) {
    if (!Number.isInteger(team.teamId)) continue;
    byId.set(team.teamId, { ...team, source: 'live-team' });
  }
  const liveTeamCount = byId.size;
  for (const team of catalogedTeams) {
    if (!Number.isInteger(team?.teamId) || byId.has(team.teamId)) continue;
    byId.set(team.teamId, retainedTeam(team));
  }
  const teams = [...byId.values()].sort((left, right) => left.teamId - right.teamId);
  return {
    teams,
    liveTeamRecordCount: liveTeamCount,
    catalogedTeamRecordCount: teams.length - liveTeamCount,
    totalTeamCatalogEntries: teams.length,
  };
}

function validateTeamEditInput(value = {}) {
  const integer = (field, minimum, maximum) => {
    const parsed = Number(value[field]);
    if (!Number.isInteger(parsed) || parsed < minimum || parsed > maximum) {
      throw new Error(`${field} must be a whole number from ${minimum} to ${maximum}.`);
    }
    return parsed;
  };
  const text = (field, maximum, label) => {
    const parsed = String(value[field] ?? '').trim();
    if (parsed.length > maximum) throw new Error(`${label} is limited to ${maximum} characters.`);
    return parsed;
  };
  return {
    recordIndex: integer('recordIndex', 0, 1023),
    teamId: integer('teamId', 0, 65535),
    expectedName: text('expectedName', 23, 'Expected team name'),
    name: text('name', 23, 'Team name'),
    cityName: text('cityName', 17, 'City name'),
    cityShort: text('cityShort', 6, 'City abbreviation'),
    stateShort: text('stateShort', 6, 'State abbreviation'),
    mainLogo: text('mainLogo', 35, 'Main logo'),
    rosterTypeId: integer('rosterTypeId', 0, 31),
    historicYearId: integer('historicYearId', 0, 127),
    arenaId: integer('arenaId', 0, 65535),
    ...(value.floorFile!==undefined?{floorFile:text('floorFile',39,'Floor asset stem')}:{}),
  };
}

function validateTeamUniformEditInput(value = {}) {
  const integer = (field, minimum, maximum) => {
    const parsed = Number(value[field]);
    if (!Number.isInteger(parsed) || parsed < minimum || parsed > maximum) {
      throw new Error(`${field} must be a whole number from ${minimum} to ${maximum}.`);
    }
    return parsed;
  };
  const text = (field, maximum, label) => {
    const parsed = String(value[field] ?? '').trim();
    if (!parsed || parsed.length > maximum || /[\u0000-\u001F]/.test(parsed)) {
      throw new Error(`${label} must contain 1–${maximum} valid characters.`);
    }
    return parsed;
  };
  const result = {
    recordIndex: integer('recordIndex', 0, 1023),
    teamId: integer('teamId', 0, 65535),
    expectedTeamName: text('expectedTeamName', 23, 'Expected team name'),
    slot: integer('slot', 0, 31),
    uniformId: integer('uniformId', 0, 65535),
    expectedBaseFilename: text('expectedBaseFilename', 63, 'Uniform filename'),
    expectedHome: integer('expectedHome', 0, 7),
    expectedAway: integer('expectedAway', 0, 7),
    expectedLogo: integer('expectedLogo', 0, 3),
    home: integer('home', 0, 7),
    away: integer('away', 0, 7),
    logo: integer('logo', 0, 3),
  };
  for (const [field] of TEAM_UNIFORM_COLOR_FIELDS) {
    const expected = `expected${field[0].toUpperCase()}${field.slice(1)}`;
    result[expected] = integer(expected, 0, 0xFFFFFFFF);
    result[field] = integer(field, 0, 0xFFFFFFFF);
  }
  for (const suffix of ['160', '164', '168', '16C']) result[`expectedFlags${suffix}`] = integer(`expectedFlags${suffix}`, 0, 0xFFFFFFFF);
  for (const [field, , minimum, maximum] of TEAM_UNIFORM_PACKED_FIELDS) result[field] = integer(field, minimum, maximum);
  return result;
}

function validateTeamUniformLinkInput(value = {}) {
  const integer = (field, minimum, maximum) => {
    const parsed = Number(value[field]);
    if (!Number.isInteger(parsed) || parsed < minimum || parsed > maximum) {
      throw new Error(`${field} must be a whole number from ${minimum} to ${maximum}.`);
    }
    return parsed;
  };
  const text = (field, maximum, label) => {
    const parsed = String(value[field] ?? '').trim();
    if (!parsed || parsed.length > maximum || /[\u0000-\u001F]/.test(parsed)) {
      throw new Error(`${label} must contain 1–${maximum} valid characters.`);
    }
    return parsed;
  };
  return {
    recordIndex: integer('recordIndex', 0, 1023),
    teamId: integer('teamId', 0, 65535),
    expectedTeamName: text('expectedTeamName', 23, 'Expected team name'),
    slot: integer('slot', 0, 31),
    uniformRecordIndex: integer('uniformRecordIndex', 0, 8191),
    uniformId: integer('uniformId', 0, 65535),
    expectedBaseFilename: text('expectedBaseFilename', 63, 'Uniform filename'),
  };
}

function validatePlayerEditInput(value = {}) {
  const numeric = (field) => {
    const raw = value[field];
    if ((typeof raw !== 'number' && typeof raw !== 'string') || (typeof raw === 'string' && !raw.trim())) {
      throw new Error(`${field} needs a numeric value; missing details cannot be treated as zero.`);
    }
    return Number(raw);
  };
  const integer = (field, minimum, maximum) => {
    const parsed = numeric(field);
    if (!Number.isInteger(parsed) || parsed < minimum || parsed > maximum) {
      throw new Error(`${field} must be a whole number from ${minimum} to ${maximum}.`);
    }
    return parsed;
  };
  const decimal = (field, minimum, maximum) => {
    const parsed = numeric(field);
    if (!Number.isFinite(parsed) || parsed < minimum || parsed > maximum) {
      throw new Error(`${field} must be from ${minimum} to ${maximum}.`);
    }
    return parsed;
  };
  const name = (field, label) => {
    if (typeof value[field] !== 'string') throw new Error(`${label} needs text; missing names cannot be cleared implicitly.`);
    const parsed = value[field].trim();
    if (parsed.length > 19 || /[\u0000-\u001F]/.test(parsed)) throw new Error(`${label} is limited to 19 valid characters.`);
    return parsed;
  };
  const result = {
    recordIndex: integer('recordIndex', 0, 8191),
    expectedFirstName: name('expectedFirstName', 'Expected first name'),
    expectedLastName: name('expectedLastName', 'Expected last name'),
    expectedCyberfaceId: integer('expectedCyberfaceId', 0, 65535),
    firstName: name('firstName', 'First name'),
    lastName: name('lastName', 'Last name'),
    cyberfaceId: integer('cyberfaceId', 0, 65535),
    positionId: integer('positionId', 0, 5),
    secondaryPositionId: integer('secondaryPositionId', 0, 5),
    jerseyNumberRaw: integer('jerseyNumberRaw', 0, 100),
    weightLbs: decimal('weightLbs', 70, 500),
    heightCm: decimal('heightCm', 120, 260),
    wingspanCm: decimal('wingspanCm', 120, 320),
    potential: integer('potential', 25, 110),
  };
  if (!result.firstName && !result.lastName) throw new Error('A player needs a first or last name.');
  if(value.fromIndex!==undefined){if(value.fromIndex===null||String(value.fromIndex).trim()==='')throw new Error('Choose a valid From entry.');result.fromIndex=integer('fromIndex',0,4095);}
  if (value.fullNameId !== undefined) {
    if (value.fullNameId === null || String(value.fullNameId).trim() === '') throw new Error('Enter a full-name sound ID or -1 for unassigned.');
    result.fullNameId = integer('fullNameId', -1, 32767);
  }
  result.ratingEdits = require('./player-field-edits').validate(value.ratingEdits);
  if(value.birthDate!==undefined){require('../renderer/player-bio').dateParts(value.birthDate);result.birthDate=value.birthDate;result.expectedBirthYear=integer('expectedBirthYear',0,4095);result.expectedBirthMonth=integer('expectedBirthMonth',0,15);result.expectedBirthDay=integer('expectedBirthDay',0,31);}
  if(value.yearsPro!==undefined){result.yearsPro=integer('yearsPro',0,31);result.expectedYearsPro=integer('expectedYearsPro',0,31);}
  return result;
}

function normalizeArenaStem(value, maximum, label) {
  let stem = String(value ?? '').trim().replaceAll('\\', '/');
  stem = stem.replace(/^levels\//i, '').replace(/\.iff$/i, '');
  if (!stem || stem.length > maximum || !/^[a-z0-9_.-]+$/i.test(stem)) {
    throw new Error(`${label} must be a filename stem of 1–${maximum} letters, numbers, dots, dashes, or underscores.`);
  }
  return stem;
}

function validateArenaCloneInput(value = {}) {
  const team = validateTeamEditInput({
    ...value,
    expectedName: value.expectedTeamName,
    name: value.expectedTeamName,
    cityName: '', cityShort: '', stateShort: '', mainLogo: '',
    rosterTypeId: 0, historicYearId: 0, arenaId: value.targetArenaId,
  });
  const donorArenaId = Number(value.donorArenaId);
  if (!Number.isInteger(donorArenaId) || donorArenaId < 0 || donorArenaId > 65535) throw new Error('Donor arena ID must be a whole number from 0 to 65535.');
  const arenaName = String(value.arenaName ?? '').trim();
  if (!arenaName || arenaName.length > 32) throw new Error('Arena display name must contain 1–32 characters.');
  return {
    recordIndex: team.recordIndex,
    teamId: team.teamId,
    expectedTeamName: team.expectedName,
    targetArenaId: team.arenaId,
    donorArenaId,
    expectedTargetName: String(value.expectedTargetName ?? ''),
    arenaName,
    arenaFile: normalizeArenaStem(value.arenaFile, 37, 'Arena asset'),
    floorFile: normalizeArenaStem(value.floorFile, 39, 'Floor asset'),
  };
}

function mergePlayerAssetCatalog(report, reference, manifestIds = new Set(), modIds = new Set(), catalogedPlayers = []) {
  const namesById = new Map();
  for (const entry of reference?.entries || []) {
    if (!Number.isInteger(entry.id) || !entry.name) continue;
    const names = namesById.get(entry.id) || [];
    if (!names.includes(entry.name)) names.push(entry.name);
    namesById.set(entry.id, names);
  }
  const liveIds = new Set();
  const players = (report.players || []).map((player) => {
    if (Number.isInteger(player.cyberfaceId)) liveIds.add(player.cyberfaceId);
    return {
      ...player,
      source: 'live-roster',
      nameStatus: 'live-exact',
      manifestListed: manifestIds.has(player.cyberfaceId),
      modPresent: modIds.has(player.cyberfaceId),
    };
  });
  const currentKeys = new Set(players.map(rosterRecordKey));
  let catalogedRosterRecordCount = 0;
  for (const player of catalogedPlayers) {
    if (!player?.name || !Number.isInteger(player.cyberfaceId) || currentKeys.has(rosterRecordKey(player))) continue;
    liveIds.add(player.cyberfaceId);
    players.push({
      ...player,
      recordAddress: '',
      source: 'cataloged-roster',
      nameStatus: 'cataloged-exact',
      manifestListed: manifestIds.has(player.cyberfaceId),
      modPresent: modIds.has(player.cyberfaceId),
    });
    currentKeys.add(rosterRecordKey(player));
    catalogedRosterRecordCount += 1;
  }
  const allAssetIds = new Set([...manifestIds, ...modIds]);
  let referenceNamedAssetOnly = 0;
  for (const cyberfaceId of [...allAssetIds].sort((left, right) => left - right)) {
    if (liveIds.has(cyberfaceId)) continue;
    const referenceNames = namesById.get(cyberfaceId) || [];
    if (referenceNames.length) referenceNamedAssetOnly += 1;
    players.push({
      recordIndex: null,
      recordAddress: '',
      firstName: '',
      lastName: '',
      name: referenceNames.length === 1
        ? referenceNames[0]
        : referenceNames.length > 1 ? referenceNames.join(' / ') : `Unknown cyberface ${String(cyberfaceId).padStart(5, '0')}`,
      cyberfaceId,
      teamName: '',
      referenceFaceIds: referenceNames.length ? [cyberfaceId] : [],
      referenceFaceIdStatus: referenceNames.length ? 'id-reference-name' : 'unavailable',
      source: manifestIds.has(cyberfaceId) ? 'stock-manifest' : 'mods-only',
      nameStatus: referenceNames.length ? 'reference-name' : 'unknown',
      manifestListed: manifestIds.has(cyberfaceId),
      modPresent: modIds.has(cyberfaceId),
    });
  }
  return {
    ...report,
    players,
    liveRosterRecordCount: report.players?.length || 0,
    manifestCyberfaceCount: manifestIds.size,
    modsCyberfaceCount: modIds.size,
    catalogedRosterRecordCount,
    assetOnlyCyberfaceCount: players.length - (report.players?.length || 0) - catalogedRosterRecordCount,
    referenceNamedAssetOnly,
    totalCatalogEntries: players.length,
  };
}

class PlayerResearchManager {
  constructor({ helperPath, referenceCatalogPath, seedCatalogPath = null, teamSeedCatalogPath = null, learnedCatalogPath = null, arenaProfilePath = null }) {
    this.helperPath = helperPath;
    this.referenceCatalogPath = referenceCatalogPath;
    this.seedCatalogPath = seedCatalogPath;
    this.teamSeedCatalogPath = teamSeedCatalogPath;
    this.learnedCatalogPath = learnedCatalogPath;
    this.arenaProfilePath = arenaProfilePath;
    this.arenaAutoPromise = null;
    this.scanRevision = 0;
    this.catalogCache = new Map();
    this.arenaAuto = { status: 'idle', message: 'No automatic custom arena has been configured.' };
    this.state = {
      status: 'idle',
      message: 'Start NBA 2K27 and refresh to read the live player directory.',
      players: [],
    };
  }

  view() {
    return { ...this.state, arenaAuto: this.arenaAuto };
  }

  livePlayerForEdit(input, state = this.state) {
    return (state.players || []).find((player) => player.source === 'live-roster'
      && player.recordIndex === Number(input.recordIndex)
      && player.cyberfaceId === Number(input.expectedCyberfaceId)
      && player.firstName === String(input.expectedFirstName ?? '').trim()
      && player.lastName === String(input.expectedLastName ?? '').trim());
  }

  sameGuardedPlayer(left, right) {
    if (!left || !right) return false;
    for (const field of ['recordIndex','cyberfaceId','firstName','lastName','positionId','secondaryPositionId','jerseyNumberRaw','potentialRaw','heightRaw','wingspanRaw','weightLbs','fromIndex','bioLayoutVerified','birthYear','birthMonth','birthDay','yearsPro']) {
      if (left[field] !== right[field]) return false;
    }
    if (left.audio?.fullNameId !== right.audio?.fullNameId) return false;
    const a=left.ratingsRaw,b=right.ratingsRaw;
    return (!Array.isArray(a)&&!Array.isArray(b))||(Array.isArray(a)&&Array.isArray(b)&&a.length===b.length&&a.every((value,index)=>value===b[index]));
  }

  async editPlayerWithRefresh(input, scanOptions) {
    const original=this.livePlayerForEdit(input);
    try{return await this.editPlayer(input);}
    catch(error){
      if(!/unique validated NBA 2K27 database was not found/i.test(String(error?.message||'')))throw error;
      let refreshed;
      try{refreshed=await this.scan(scanOptions);}
      catch{throw new Error('The live roster database was temporarily unavailable. Wait for the roster screen to finish loading and try again.');}
      const current=this.livePlayerForEdit(input,refreshed);
      if(!this.sameGuardedPlayer(original,current))throw new Error('The live roster changed while the editor was open. The player list was refreshed; reopen the player before saving.');
      try{return await this.editPlayer(input);}
      catch(retryError){
        if(/unique validated NBA 2K27 database was not found/i.test(String(retryError?.message||'')))throw new Error('The live roster database is still loading. Wait a moment and try again; the player list has already been refreshed.');
        throw retryError;
      }
    }
  }

  async readArenaProfiles() {
    if (!this.arenaProfilePath) return [];
    try {
      const value = JSON.parse(await fs.readFile(this.arenaProfilePath, 'utf8'));
      return Array.isArray(value.profiles) ? value.profiles : [];
    } catch (error) {
      if (error.code === 'ENOENT') return [];
      throw error;
    }
  }

  async saveArenaProfile(profile) {
    if (!this.arenaProfilePath) return;
    const profiles = (await this.readArenaProfiles()).filter((item) => item.targetArenaId !== profile.targetArenaId);
    profiles.push(profile);
    await fs.mkdir(path.dirname(this.arenaProfilePath), { recursive: true });
    await fs.writeFile(this.arenaProfilePath, `${JSON.stringify({
      schema: 'nba2k27-custom-arena-profiles-v1', updatedAt: new Date().toISOString(), profiles,
    }, null, 2)}\n`, 'utf8');
  }

  async verifyArenaAssets(edit, modsRoot) {
    const levelsRoot = path.resolve(modsRoot || '', 'levels');
    for (const stem of [edit.arenaFile, edit.floorFile]) {
      const asset = path.resolve(levelsRoot, `${stem}.iff`);
      if (!asset.toLowerCase().startsWith(`${levelsRoot.toLowerCase()}${path.sep}`)) throw new Error('The arena asset path escaped the Mods levels folder.');
      const stat = await fs.stat(asset).catch(() => null);
      if (!stat?.isFile()) throw new Error(`Missing Mods asset: levels/${stem}.iff`);
    }
  }

  async executeArenaClone(edit, pid, recordIndex, expectedTargetName, { populateOnly = false } = {}) {
    const args = [
      '--clone-arena', '--pid', String(pid),
      '--target-id', String(edit.targetArenaId), '--donor-id', String(edit.donorArenaId),
      '--expected-target-name', expectedTargetName, '--arena-name', edit.arenaName,
      '--arena-file', edit.arenaFile, '--floor-file', edit.floorFile,
    ];
    if (populateOnly) args.push('--populate-only');
    else args.push('--team-record-index', String(recordIndex), '--team-id', String(edit.teamId), '--expected-team-name', edit.expectedTeamName);
    const { stdout, stderr } = await execFileAsync(this.helperPath, args, {
      windowsHide: true, timeout: 30_000, maxBuffer: 1024 * 1024, encoding: 'utf8',
    });
    let report;
    try { report = JSON.parse(stdout); }
    catch (_error) { throw new Error(stderr?.trim() || 'The custom arena helper returned an invalid report.'); }
    if (!report?.ok || report.schema !== 'nba2k27-arena-clone-v1' || report.gameCodeHooks !== 0
      || report.readBackVerified !== true || report.rollbackOnFailure !== true
      || report.recordsWritten !== (populateOnly ? 1 : 2)
      || (!populateOnly && report.teamId !== edit.teamId) || report.targetArenaId !== edit.targetArenaId) {
      throw new Error('The custom arena helper failed its guarded report checks.');
    }
    return report;
  }

  async readCatalog(catalogPath) {
    if (!catalogPath) return { players: [], teams: [] };
    try {
      const stat = await fs.stat(catalogPath);
      const stamp = `${stat.size}:${stat.mtimeMs}`;
      const cached = this.catalogCache.get(catalogPath);
      if (cached?.stamp === stamp) return cached.value;
      const parsed = JSON.parse(await fs.readFile(catalogPath, 'utf8'));
      const value = {
        players: Array.isArray(parsed.players) ? parsed.players : [],
        teams: Array.isArray(parsed.teams) ? parsed.teams : [],
      };
      this.catalogCache.set(catalogPath, { stamp, value });
      return value;
    } catch (error) {
      if (error.code === 'ENOENT') return { players: [], teams: [] };
      throw error;
    }
  }

  async readCatalogPlayers(catalogPath) {
    return (await this.readCatalog(catalogPath)).players;
  }

  async readCatalogTeams(catalogPath) {
    return (await this.readCatalog(catalogPath)).teams;
  }

  async saveLearnedRoster(players, teams = []) {
    if (!this.learnedCatalogPath) return;
    const byKey = new Map();
    for (const player of players) {
      if (!['live-roster', 'cataloged-roster'].includes(player.source)) continue;
      const stored = { ...player };
      delete stored.source;
      delete stored.nameStatus;
      delete stored.manifestListed;
      delete stored.modPresent;
      byKey.set(rosterRecordKey(stored), stored);
    }
    await fs.mkdir(path.dirname(this.learnedCatalogPath), { recursive: true });
    const document = {
      schema: 'nba2k27-learned-player-catalog-v1',
      updatedAt: new Date().toISOString(),
      access: 'read-only',
      writes: 0,
      players: [...byKey.values()],
      teams: teams.map((team) => {
        const stored = { ...team };
        delete stored.source;
        delete stored.recordAddress;
        stored.uniforms = Array.isArray(team.uniforms) ? team.uniforms.map((uniform) => {
          const retained = { ...uniform };
          delete retained.recordAddress;
          return retained;
        }) : [];
        return stored;
      }),
    };
    // This catalog can contain tens of thousands of retained roster records.
    // Compact JSON avoids tripling its on-disk size; a unique temporary plus
    // fsync prevents interruption from replacing the last known-good catalog.
    const encoded = `${JSON.stringify(document)}\n`;
    const temporary = `${this.learnedCatalogPath}.${crypto.randomUUID()}.tmp`;
    let handle;
    try {
      handle = await fs.open(temporary, 'wx');
      await handle.writeFile(encoded, 'utf8');
      await handle.sync();
      await handle.close(); handle = null;
      await fs.rename(temporary, this.learnedCatalogPath);
    } finally {
      await handle?.close().catch(() => {});
      await fs.unlink(temporary).catch((error) => { if (error.code !== 'ENOENT') throw error; });
    }
    const stat = await fs.stat(this.learnedCatalogPath);
    this.catalogCache.set(this.learnedCatalogPath, {
      stamp: `${stat.size}:${stat.mtimeMs}`,
      value: { players: document.players, teams: document.teams },
    });
  }

  async scan({ gameRoot, modsRoot } = {}) {
    const revision=++this.scanRevision;
    this.state = { ...this.state, status: 'scanning', message: 'Reading the live player directory…' };
    try {
      const [{ stdout, stderr }, rawReference, manifestAssets, modIds, seedPlayers, learnedCatalog, seedTeams] = await Promise.all([
        execFileAsync(this.helperPath, [], {
          windowsHide: true,
          timeout: 30_000,
          maxBuffer: 16 * 1024 * 1024,
          encoding: 'utf8',
        }),
        fs.readFile(this.referenceCatalogPath, 'utf8'),
        scanManifestAssets(path.join(gameRoot || '', 'manifest')),
        scanLooseCyberfaces(modsRoot || path.join(gameRoot || '', 'mods')),
        this.readCatalogPlayers(this.seedCatalogPath),
        this.readCatalog(this.learnedCatalogPath),
        this.readCatalogTeams(this.teamSeedCatalogPath),
      ]);
      const report = JSON.parse(stdout);
      if (!report?.ok || report.schema !== 'nba2k27-live-player-research-v1' || report.writes !== 0) {
        throw new Error('The read-only roster helper returned an invalid safety report.');
      }
      const reference = JSON.parse(rawReference);
      const manifestIds = manifestAssets.cyberfaceIds;
      const joined = mergePlayerAssetCatalog(joinReferenceFaceIds(report, reference), reference, manifestIds, modIds, [...seedPlayers, ...learnedCatalog.players]);
      joined.unreferencedPopulatedUniforms = (joined.unreferencedPopulatedUniforms || []).map((uniform) => ({
        ...uniform,
        stockResourceAvailable: manifestAssets.uniformBases.has(String(uniform.baseFilename || '').toLowerCase()),
      }));
      const teamCatalog = mergeTeamCatalog(report.teams, [...seedTeams, ...learnedCatalog.teams]);
      if(revision!==this.scanRevision)return this.state;
      this.state = {
        ...joined,
        ...teamCatalog,
        origins: Array.isArray(report.origins)?report.origins:[],
        status: 'ready',
        message: `${joined.decodedPlayerCount.toLocaleString()} current live records, ${joined.catalogedRosterRecordCount.toLocaleString()} retained NBA/WNBA records, and ${joined.assetOnlyCyberfaceCount.toLocaleString()} unloaded stock/mod cyberfaces cataloged.`,
      };
      await this.saveLearnedRoster(this.state.players, this.state.teams);
      return this.state;
    } catch (error) {
      if(revision!==this.scanRevision)return this.state;
      const detail = error?.stderr?.trim() || error?.message || String(error);
      this.state = {
        status: 'error',
        message: detail,
        players: [],
      };
      throw new Error(detail);
    }
  }

  async editTeam(input,{backupRoot}={}) {
    if (this.state.status !== 'ready') throw new Error('Refresh live teams before editing a team.');
    const edit = validateTeamEditInput(input);
    const live = (this.state.teams || []).find((team) => team.source === 'live-team'
      && team.recordIndex === edit.recordIndex && team.teamId === edit.teamId);
    if (!live) throw new Error('That team is retained from another roster. Load its roster and refresh before editing it.');
    if (live.name !== edit.expectedName) throw new Error('The selected team changed in the launcher. Refresh and try again.');
    const args = [
      '--write-team', '--pid', String(this.state.pid),
      '--record-index', String(edit.recordIndex), '--team-id', String(edit.teamId),
      '--expected-name', edit.expectedName, '--name', edit.name,
      '--city-name', edit.cityName, '--city-short', edit.cityShort,
      '--state-short', edit.stateShort, '--main-logo', edit.mainLogo,
      '--roster-type', String(edit.rosterTypeId), '--historic-year', String(edit.historicYearId),
      '--arena-id', String(edit.arenaId),
    ];
    if(edit.floorFile!==undefined){
      if(!/^[A-Za-z0-9_ -]+$/.test(edit.floorFile)||!(this.state.arenas||[]).some(a=>!a.emptyCandidate&&a.floorFile===edit.floorFile))throw Error('Choose a floor from the current live arena catalog.');
      if(!backupRoot)throw Error('Team recovery storage is unavailable.');
      args.push('--floor-file',edit.floorFile,'--backup-root',backupRoot);
    }
    const { stdout, stderr } = await execFileAsync(this.helperPath, args, {
      windowsHide: true,
      timeout: 30_000,
      maxBuffer: 1024 * 1024,
      encoding: 'utf8',
    });
    let report;
    try { report = JSON.parse(stdout); }
    catch (_error) { throw new Error(stderr?.trim() || 'The team editor returned an invalid report.'); }
    if (!report?.ok || report.schema !== 'nba2k27-team-edit-v1' || report.gameCodeHooks !== 0
      || report.readBackVerified !== true || report.rollbackOnFailure !== true || ![8,...(edit.floorFile!==undefined?[9]:[])].includes(report.writes)
      || report.recordIndex !== edit.recordIndex || report.teamId !== edit.teamId) {
      throw new Error('The team editor failed its guarded write report checks.');
    }
    return report;
  }

  async editTeamUniform(input) {
    if (this.state.status !== 'ready') throw new Error('Refresh live teams before editing a uniform.');
    const edit = validateTeamUniformEditInput(input);
    const team = (this.state.teams || []).find((entry) => entry.source === 'live-team'
      && entry.recordIndex === edit.recordIndex && entry.teamId === edit.teamId);
    if (!team) throw new Error('That uniform belongs to a retained roster. Load its roster and refresh before editing it.');
    if (team.name !== edit.expectedTeamName) throw new Error('The selected team changed in the launcher. Refresh and try again.');
    const uniform = (team.uniforms || []).find((entry) => entry.slot === edit.slot
      && entry.uniformId === edit.uniformId && entry.baseFilename === edit.expectedBaseFilename);
    if (!uniform) throw new Error('The selected uniform is no longer owned by this team slot. Refresh and try again.');
    if (uniform.headbandColorHome !== edit.expectedHome || uniform.headbandColorAway !== edit.expectedAway
      || uniform.headbandLogoTypeId !== edit.expectedLogo) {
      throw new Error('The selected uniform settings changed in the launcher. Refresh and try again.');
    }
    for (const [field] of TEAM_UNIFORM_COLOR_FIELDS) {
      const expected = `expected${field[0].toUpperCase()}${field.slice(1)}`;
      if (uniform[field] !== edit[expected]) throw new Error('The selected uniform palette changed in the launcher. Refresh and try again.');
    }
    for (const suffix of ['160', '164', '168', '16C']) {
      if (uniform[`flags${suffix}`] !== edit[`expectedFlags${suffix}`]) {
        throw new Error('The selected packed uniform settings changed in the launcher. Refresh and try again.');
      }
    }
    for (const [field] of TEAM_UNIFORM_PACKED_FIELDS) {
      if (Number(uniform[field]) !== edit[field]) throw new Error('The selected mapped uniform settings changed in the launcher. Refresh and try again.');
    }
    const args = [
      '--write-team-uniform', '--pid', String(this.state.pid),
      '--record-index', String(edit.recordIndex), '--team-id', String(edit.teamId),
      '--expected-team-name', edit.expectedTeamName,
      '--slot', String(edit.slot), '--uniform-id', String(edit.uniformId),
      '--expected-base-filename', edit.expectedBaseFilename,
      '--expected-home', String(edit.expectedHome), '--expected-away', String(edit.expectedAway),
      '--expected-logo', String(edit.expectedLogo), '--home', String(edit.home),
      '--away', String(edit.away), '--logo', String(edit.logo),
      '--expected-flags-160', String(edit.expectedFlags160), '--expected-flags-164', String(edit.expectedFlags164),
      '--expected-flags-168', String(edit.expectedFlags168), '--expected-flags-16c', String(edit.expectedFlags16C),
    ];
    for (const [field, argument] of TEAM_UNIFORM_COLOR_FIELDS) {
      const expected = `expected${field[0].toUpperCase()}${field.slice(1)}`;
      args.push(`--expected-${argument}`, String(edit[expected]), `--${argument}`, String(edit[field]));
    }
    for (const [field, argument] of TEAM_UNIFORM_PACKED_FIELDS) args.push(`--${argument}`, String(edit[field]));
    const { stdout, stderr } = await execFileAsync(this.helperPath, args, {
      windowsHide: true, timeout: 30_000, maxBuffer: 1024 * 1024, encoding: 'utf8',
    });
    let report;
    try { report = JSON.parse(stdout); }
    catch (_error) { throw new Error(stderr?.trim() || 'The uniform editor returned an invalid report.'); }
    if (!report?.ok || report.schema !== 'nba2k27-team-uniform-edit-v4' || report.gameCodeHooks !== 0
      || report.readBackVerified !== true || report.rollbackOnFailure !== true || report.writes !== 52
      || !Number.isInteger(report.physicalWrites) || report.physicalWrites < 0 || report.physicalWrites > 25
      || report.recordIndex !== edit.recordIndex
      || report.teamId !== edit.teamId || report.slot !== edit.slot || report.uniformId !== edit.uniformId) {
      throw new Error('The uniform editor failed its guarded write report checks.');
    }
    return report;
  }

  async linkTeamUniform(input, { gameRoot } = {}) {
    if (this.state.status !== 'ready') throw new Error('Refresh live teams before adding a uniform.');
    const edit = validateTeamUniformLinkInput(input);
    const team = (this.state.teams || []).find((entry) => entry.source === 'live-team'
      && entry.recordIndex === edit.recordIndex && entry.teamId === edit.teamId);
    if (!team) throw new Error('That team is retained from another roster. Load its roster and refresh before adding a uniform.');
    if (team.name !== edit.expectedTeamName) throw new Error('The selected team changed in the launcher. Refresh and try again.');
    if (!Array.isArray(team.emptyUniformSlots) || !team.emptyUniformSlots.includes(edit.slot)) {
      throw new Error('The selected team slot is no longer empty. Refresh and try again.');
    }
    const candidate = (this.state.unreferencedPopulatedUniforms || []).find((entry) => entry.recordIndex === edit.uniformRecordIndex
      && entry.uniformId === edit.uniformId && entry.teamId === edit.teamId
      && entry.baseFilename === edit.expectedBaseFilename);
    if (!candidate) throw new Error('That uniform record is no longer an unreferenced same-team candidate. Refresh and try again.');
    const manifestPath = path.join(gameRoot || '', 'manifest');
    if (!await manifestHasUniformResource(manifestPath, edit.expectedBaseFilename)) {
      throw new Error(`The stock art resource for ${edit.expectedBaseFilename} is unavailable; the jersey was not linked.`);
    }
    const args = [
      '--link-team-uniform', '--pid', String(this.state.pid),
      '--record-index', String(edit.recordIndex), '--team-id', String(edit.teamId),
      '--expected-team-name', edit.expectedTeamName, '--slot', String(edit.slot),
      '--uniform-record-index', String(edit.uniformRecordIndex), '--uniform-id', String(edit.uniformId),
      '--expected-base-filename', edit.expectedBaseFilename,
    ];
    const { stdout, stderr } = await execFileAsync(this.helperPath, args, {
      windowsHide: true, timeout: 30_000, maxBuffer: 1024 * 1024, encoding: 'utf8',
    });
    let report;
    try { report = JSON.parse(stdout); }
    catch (_error) { throw new Error(stderr?.trim() || 'The uniform linker returned an invalid report.'); }
    if (!report?.ok || report.schema !== 'nba2k27-team-uniform-link-v1' || report.gameCodeHooks !== 0
      || report.linkReadBackVerified !== true || report.rollbackOnFailure !== true || report.physicalWrites !== 1
      || report.restoredToNull !== false || report.recordIndex !== edit.recordIndex || report.teamId !== edit.teamId
      || report.slot !== edit.slot || report.uniformRecordIndex !== edit.uniformRecordIndex
      || report.uniformId !== edit.uniformId || report.baseFilename !== edit.expectedBaseFilename) {
      throw new Error('The uniform linker failed its guarded write report checks.');
    }
    return report;
  }

  async editPlayer(input) {
    if (this.state.status !== 'ready') throw new Error('Refresh live players before editing a player.');
    const edit = validatePlayerEditInput(input);
    const editingSnapshot=this.state;
    const live = (this.state.players || []).find((player) => player.source === 'live-roster'
      && player.recordIndex === edit.recordIndex
      && player.cyberfaceId === edit.expectedCyberfaceId
      && player.firstName === edit.expectedFirstName
      && player.lastName === edit.expectedLastName);
    if (!live) throw new Error('That player is not the matching live roster record. Load its roster and refresh before editing it.');
    for (const field of ['positionId', 'secondaryPositionId', 'jerseyNumberRaw', 'potentialRaw', 'heightRaw', 'wingspanRaw']) {
      if (!Number.isInteger(live[field])) throw new Error(`The live ${field} value is unavailable; this player was left untouched.`);
    }
    if (!Number.isFinite(live.weightLbs)) throw new Error('The live weight value is unavailable; this player was left untouched.');
    const bio=require('../renderer/player-bio').plan(edit,live);
    const editAudio = edit.fullNameId !== undefined && edit.fullNameId !== live.audio?.fullNameId;
    const editFrom=edit.fromIndex!==undefined&&edit.fromIndex!==live.fromIndex;
    const origin=editFrom?(this.state.origins||[]).find(o=>o.index===edit.fromIndex):null;
    if(editFrom&&(!origin||!Number.isInteger(live.fromIndex)||live.fromIndex < -1))throw new Error('Refresh the live From directory before editing it.');
    if (editAudio && !Number.isInteger(live.audio?.fullNameId)) throw new Error('Refresh the live audio mapping before editing it.');
    const potentialRaw = edit.potential === live.potential
      ? live.potentialRaw
      : edit.potential <= 25 ? 0 : Math.min(255, (edit.potential - 25) * 3 - 1);
    const args = [
      '--write-player', '--pid', String(this.state.pid), '--record-index', String(edit.recordIndex),
      '--expected-first', live.firstName, '--expected-last', live.lastName,
      '--expected-cyberface', String(live.cyberfaceId),
      '--expected-position', String(live.positionId), '--expected-secondary', String(live.secondaryPositionId),
      '--expected-jersey', String(live.jerseyNumberRaw), '--expected-potential-raw', String(live.potentialRaw),
      '--expected-height-raw', String(live.heightRaw), '--expected-wingspan-raw', String(live.wingspanRaw),
      '--first', edit.firstName, '--last', edit.lastName, '--cyberface', String(edit.cyberfaceId),
      '--position', String(edit.positionId), '--secondary', String(edit.secondaryPositionId),
      '--jersey', String(edit.jerseyNumberRaw), '--weight', String(edit.weightLbs),
      '--height-raw', String(Math.round(edit.heightCm * 100)),
      '--wingspan-raw', String(Math.round(edit.wingspanCm * 100)), '--potential-raw', String(potentialRaw),
    ];
    if (editAudio) args.push('--full-name-id', String(edit.fullNameId), '--expected-full-name-id', String(live.audio.fullNameId));
    if(editFrom)args.push('--from-index',String(edit.fromIndex),'--expected-from-index',String(live.fromIndex),'--from-name',origin.name);
    args.push(...bio.args);
    const ratingEdits=require('./player-field-edits').validate(edit.ratingEdits,live.ratingsRaw).filter(c=>c.expected!==c.value);
    if(ratingEdits.length)args.push('--rating-edits',ratingEdits.map(c=>`${c.index}:${c.expected}:${c.value}`).join(';'));
    const { stdout, stderr } = await execFileAsync(this.helperPath, args, {
      windowsHide: true, timeout: 30_000, maxBuffer: 1024 * 1024, encoding: 'utf8',
    }).catch(error=>{
      const detail=String(error.stderr||'').trim();
      if(/selected player changed before Save|player field changed|roster changed|From selection.*changed/i.test(detail))throw new Error('This player or roster changed since it was loaded. Refresh live players and reopen the player before saving again. Your staged edits have not been retried.');
      throw new Error(detail||'Could not save this player. Refresh live players and try again.');
    });
    let report;
    try { report = JSON.parse(stdout); }
    catch (_error) { throw new Error(stderr?.trim() || 'The player editor returned an invalid report.'); }
    if (!report?.ok || report.schema !== 'nba2k27-player-edit-v1' || report.gameCodeHooks !== 0
      || report.readBackVerified !== true || report.rollbackOnFailure !== true || !Number.isInteger(report.ratingWrites) || report.ratingWrites<0 || report.ratingWrites>ratingEdits.length*4 || (report.bioWrites??0)!==bio.writes || report.writes !== (editAudio ? 9 : 8)+report.ratingWrites+(editFrom?1:0)+bio.writes
      || (bio.info.birthDate!==undefined&&report.birthDate!==bio.info.birthDate) || (bio.info.yearsPro!==undefined&&report.yearsPro!==bio.info.yearsPro)
      || (editFrom&&(report.fromIndex!==edit.fromIndex||report.fromName!==origin.name))
      || (editAudio && report.fullNameId !== edit.fullNameId)
      || report.recordIndex !== edit.recordIndex || report.cyberfaceId !== edit.cyberfaceId) {
      throw new Error('The player editor failed its guarded write report checks.');
    }
    // Invalidate pre-save scans; never allow an older asynchronous read to
    // replace the just-verified identity before the post-save refresh completes.
    this.scanRevision++;
    if(this.state.pid===editingSnapshot.pid&&this.state.playerTable===editingSnapshot.playerTable){
      const committed={...live,...edit,name:[edit.firstName,edit.lastName].filter(Boolean).join(' '),potentialRaw,heightRaw:Math.round(edit.heightCm*100),wingspanRaw:Math.round(edit.wingspanCm*100),jerseyNumber:edit.jerseyNumberRaw===100?'00':String(edit.jerseyNumberRaw)};
      delete committed.ratingEdits;delete committed.expectedFirstName;delete committed.expectedLastName;delete committed.expectedCyberfaceId;
      for(const key of ['expectedBirthYear','expectedBirthMonth','expectedBirthDay','expectedYearsPro'])delete committed[key];
      Object.assign(committed,bio.info);
      if(Array.isArray(live.ratingsRaw)){committed.ratingsRaw=[...live.ratingsRaw];for(const c of ratingEdits)committed.ratingsRaw[c.index]=c.value;}
      if(editAudio)committed.audio={...live.audio,fullNameId:edit.fullNameId};
      if(editFrom)committed.collegeName=origin.name;
      this.state={...this.state,status:'ready',players:this.state.players.map(p=>p.source==='live-roster'&&p.recordIndex===live.recordIndex&&p.firstName===live.firstName&&p.lastName===live.lastName&&p.cyberfaceId===live.cyberfaceId?committed:p)};
    }
    return report;
  }

  async cloneArena(input, { modsRoot } = {}) {
    if (this.state.status !== 'ready') throw new Error('Refresh live teams before creating a custom arena.');
    const edit = validateArenaCloneInput(input);
    const team = (this.state.teams || []).find((item) => item.source === 'live-team'
      && item.recordIndex === edit.recordIndex && item.teamId === edit.teamId);
    if (!team || team.name !== edit.expectedTeamName) throw new Error('The selected live team changed. Refresh and try again.');
    const target = (this.state.arenas || []).filter((arena) => arena.arenaId === edit.targetArenaId);
    const donor = (this.state.arenas || []).filter((arena) => arena.arenaId === edit.donorArenaId && !arena.emptyCandidate);
    if (target.length !== 1 || target[0].name !== edit.expectedTargetName) throw new Error('The custom arena slot changed. Refresh and try again.');
    if (donor.length !== 1) throw new Error('Choose one populated donor arena.');
    if (!edit.expectedTargetName && !target[0].emptyCandidate) throw new Error('Choose a verified empty custom arena slot.');
    await this.verifyArenaAssets(edit, modsRoot);
    const report = await this.executeArenaClone(edit, this.state.pid, edit.recordIndex, edit.expectedTargetName);
    await this.saveArenaProfile({
      teamId: edit.teamId, expectedTeamName: edit.expectedTeamName,
      targetArenaId: edit.targetArenaId, donorArenaId: edit.donorArenaId,
      arenaName: edit.arenaName, arenaFile: edit.arenaFile, floorFile: edit.floorFile,
    });
    this.arenaAuto = { status: 'configured', message: `${edit.arenaName} will populate arena ID ${edit.targetArenaId} automatically for every roster.` };
    this.startAutomaticArenas({ modsRoot });
    return report;
  }

  startAutomaticArenas({ modsRoot } = {}) {
    if (this.arenaAutoPromise) return this.arenaAuto;
    this.arenaAutoPromise = this.applyAutomaticArenas({ modsRoot }).finally(() => { this.arenaAutoPromise = null; });
    return this.arenaAuto;
  }

  async applyAutomaticArenas({ modsRoot } = {}) {
    let lastError = null;
    while (await nba2k27IsRunning()) {
      const profiles = await this.readArenaProfiles();
      if (!profiles.length) {
        this.arenaAuto = { status: 'idle', message: 'No automatic custom arena has been configured.' };
        await new Promise((resolve) => setTimeout(resolve, 1000));
        continue;
      }
      try {
        const { stdout } = await execFileAsync(this.helperPath, ['--arena-status'], {
          windowsHide: true, timeout: 30_000, maxBuffer: 1024 * 1024, encoding: 'utf8',
        });
        const live = JSON.parse(stdout);
        if (!live?.ok || live.schema !== 'nba2k27-live-arena-status-v1' || live.writes !== 0) throw new Error('Live arena tables are not ready.');
        let reapplied = 0;
        for (const stored of profiles) {
          const edit = validateArenaCloneInput({ ...stored, recordIndex: 0, expectedTargetName: '' });
          await this.verifyArenaAssets(edit, modsRoot);
          const targets = (live.arenas || []).filter((arena) => arena.arenaId === edit.targetArenaId);
          const donors = (live.arenas || []).filter((arena) => arena.arenaId === edit.donorArenaId && !arena.emptyCandidate);
          if (targets.length !== 1 || donors.length !== 1) throw new Error(`Saved arena ${edit.arenaName} is waiting for its exact arena records.`);
          if (!targets[0].emptyCandidate && targets[0].name !== edit.arenaName) throw new Error(`Reserved arena ID ${edit.targetArenaId} contains a different arena.`);
          const complete = targets[0].name === edit.arenaName && targets[0].arenaFile === edit.arenaFile
            && targets[0].floorFile === edit.floorFile;
          if (!complete) {
            await this.executeArenaClone(edit, live.pid, -1, targets[0].name || '', { populateOnly: true });
            reapplied += 1;
          }
        }
        lastError = null;
        this.arenaAuto = {
          status: 'active',
          message: `${profiles.length} custom arena profile${profiles.length === 1 ? '' : 's'} monitored${reapplied ? ` · ${reapplied} restored after a roster reload` : ''}.`,
        };
      } catch (error) {
        lastError = error;
        this.arenaAuto = { status: 'waiting', message: `Waiting for the arena table after a roster transition: ${error.message}` };
      }
      await new Promise((resolve) => setTimeout(resolve, 750));
    }
    this.arenaAuto = { status: 'stopped', message: lastError?.message || 'Custom arena monitoring stopped when NBA 2K27 closed.' };
    return this.arenaAuto;
  }

  async exportCatalog(destination) {
    if (this.state.status !== 'ready' || !Array.isArray(this.state.players)) {
      throw new Error('Refresh the live player directory before exporting a catalog.');
    }
    const catalog = {
      schema: 'nba2k27-player-asset-catalog-v2',
      generatedAt: new Date().toISOString(),
      source: {
        pid: this.state.pid,
        access: this.state.access,
        writes: this.state.writes,
        playerStride: this.state.playerStride,
        cyberfaceIdOffset: this.state.cyberfaceIdOffset,
        cyberfaceIdWidthBytes: this.state.cyberfaceIdWidthBytes,
        cyberfaceAnchorValidation: this.state.cyberfaceAnchorValidation,
        vitalsOffsets: this.state.vitalsOffsets,
        verifiedFields: this.state.verifiedFields,
        candidateFields: this.state.candidateFields,
      },
      count: this.state.players.length,
      teamCount: this.state.teams?.length || 0,
      teams: this.state.teams || [],
      players: this.state.players.map((player) => ({
        source: player.source,
        rosterGroup: player.rosterGroup || null,
        nameStatus: player.nameStatus,
        manifestListed: player.manifestListed,
        modPresent: player.modPresent,
        recordIndex: player.recordIndex,
        firstName: player.firstName,
        lastName: player.lastName,
        name: player.name,
        cyberfaceId: player.cyberfaceId,
        cyberfaceIdDisplay: String(player.cyberfaceId).padStart(5, '0'),
        teamName: player.teamName || '',
        position: player.position || '',
        secondaryPosition: player.secondaryPosition || '',
        jerseyNumber: player.jerseyNumber ?? '',
        heightCm: player.heightCm ?? null,
        wingspanCm: player.wingspanCm ?? null,
        weightLbs: player.weightLbs ?? null,
        potential: player.potential ?? null,
        potentialRaw: player.potentialRaw ?? null,
        overall: player.overall ?? null,
        cachedOverallRaw: player.cachedOverallRaw ?? null,
        overallStatus: player.overallStatus || null,
      })),
    };
    await fs.writeFile(destination, `${JSON.stringify(catalog, null, 2)}\n`, 'utf8');
    return { path: destination, count: catalog.count };
  }
}

module.exports = {
  PlayerResearchManager,
  baseCyberfaceId,
  joinReferenceFaceIds,
  mergePlayerAssetCatalog,
  mergeTeamCatalog,
  validatePlayerEditInput,
  validateTeamEditInput,
  validateTeamUniformEditInput,
  validateTeamUniformLinkInput,
  TEAM_UNIFORM_COLOR_FIELDS,
  TEAM_UNIFORM_PACKED_FIELDS,
  validateArenaCloneInput,
  normalizePlayerName,
  rosterRecordKey,
  scanManifestCyberfaces,
  scanManifestAssets,
};
