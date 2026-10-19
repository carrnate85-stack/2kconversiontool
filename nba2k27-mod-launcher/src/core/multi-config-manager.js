const path = require('node:path');
const fs = require('node:fs');
const fsp = require('node:fs/promises');
const readline = require('node:readline');
const crypto = require('node:crypto');
const { promisify } = require('node:util');
const { execFile } = require('node:child_process');

const { buildTableWithClones, readRecords, replaceConfigLists } = require('./sigconfig-table');
const { readZipArchive, buildZipArchive } = require('./jukebox-manager');

const execFileAsync = promisify(execFile);
const MAX_FILES = 100000;
const MAX_SELECTOR_ARCHIVE_BYTES = 16 * 1024 * 1024;
const TABLE_PATHS = Object.freeze({
  all: 'char/signatureconfigs_all.iff',
  edit: 'char/signatureconfigs_editplayer.iff',
  registry: 'char/fpplayer_global.iff',
});

function digest(value) {
  return crypto.createHash('sha256').update(value).digest('hex');
}

async function sha256File(filePath) {
  const hash = crypto.createHash('sha256');
  const stream = fs.createReadStream(filePath);
  for await (const chunk of stream) hash.update(chunk);
  return hash.digest('hex');
}

function suffixKey(suffixes) {
  return [...suffixes].map((item) => item.toLowerCase()).sort().join('\0');
}

function quotePowerShell(value) {
  return `'${String(value).replaceAll("'", "''")}'`;
}

async function runPowerShell(script) {
  const encoded = Buffer.from(script, 'utf16le').toString('base64');
  return execFileAsync('powershell.exe', [
    '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
    '-EncodedCommand', encoded,
  ], { windowsHide: true, timeout: 120000, maxBuffer: 1024 * 1024 });
}

async function extractZip(archivePath, outputDirectory) {
  await fsp.rm(outputDirectory, { recursive: true, force: true });
  await fsp.mkdir(outputDirectory, { recursive: true });
  await runPowerShell([
    'Add-Type -AssemblyName System.IO.Compression.FileSystem',
    `[IO.Compression.ZipFile]::ExtractToDirectory(${quotePowerShell(archivePath)}, ${quotePowerShell(outputDirectory)})`,
  ].join('; '));
}

async function createZip(inputDirectory, outputPath) {
  await fsp.mkdir(path.dirname(outputPath), { recursive: true });
  await fsp.rm(outputPath, { force: true });
  await runPowerShell([
    'Add-Type -AssemblyName System.IO.Compression.FileSystem',
    `[IO.Compression.ZipFile]::CreateFromDirectory(${quotePowerShell(inputDirectory)}, ${quotePowerShell(outputPath)}, [IO.Compression.CompressionLevel]::Optimal, $false)`,
  ].join('; '));
}

async function archiveContentFingerprint(archivePath) {
  const stat = await fsp.lstat(archivePath);
  if (!stat.isFile() || stat.size < 1 || stat.size > MAX_SELECTOR_ARCHIVE_BYTES) {
    throw new Error(`Selector archive is outside the safe size limit: ${path.basename(archivePath)}`);
  }
  const bytes=await fsp.readFile(archivePath),after=await fsp.lstat(archivePath);
  if(bytes.length!==stat.size||after.size!==stat.size||after.mtimeMs!==stat.mtimeMs||after.ino!==stat.ino||after.isSymbolicLink())throw Error('Selector archive changed during verification.');
  // Bound inflated bytes before the shared CRC-checked ZIP reader allocates.
  const eocd=bytes.lastIndexOf(Buffer.from([0x50,0x4b,0x05,0x06]));
  if(eocd<0||eocd+22>bytes.length)throw Error('Invalid selector archive.');
  const count=bytes.readUInt16LE(eocd+10);let cursor=bytes.readUInt32LE(eocd+16),total=0;
  if(!count||count>10000)throw Error('Invalid selector member count.');
  for(let i=0;i<count;i++){
    if(cursor+46>bytes.length||bytes.readUInt32LE(cursor)!==0x02014b50)throw Error('Invalid selector directory.');
    total+=bytes.readUInt32LE(cursor+24);if(total>128*1024*1024)throw Error('Selector archive content is oversized.');
    cursor+=46+bytes.readUInt16LE(cursor+28)+bytes.readUInt16LE(cursor+30)+bytes.readUInt16LE(cursor+32);
  }
  const entries=readZipArchive(bytes,{maxEntries:10000});
  return digest(JSON.stringify([...entries].sort(([a],[b])=>a<b?-1:a>b?1:0).map(([name,data])=>[name,data.length,digest(data)])));
}

async function archivesEquivalent(leftPath, rightPath) {
  const [left, right] = await Promise.all([
    archiveContentFingerprint(leftPath),
    archiveContentFingerprint(rightPath),
  ]);
  return left === right;
}

async function scanModConfigs(modsRoot) {
  const root = path.resolve(modsRoot);
  const pending = [''];
  const groups = new Map();
  const baseIds = new Set();
  const baseFiles = new Map();
  const inventory = [];
  let seen = 0;
  while (pending.length) {
    const relativeFolder = pending.pop();
    const entries = await fsp.readdir(path.join(root, relativeFolder), { withFileTypes: true });
    entries.sort((a, b) => a.name.localeCompare(b.name));
    for (const entry of entries) {
      if (entry.isSymbolicLink()) continue;
      const relative = path.join(relativeFolder, entry.name);
      if (entry.isDirectory()) {
        pending.push(relative);
        continue;
      }
      if (!entry.isFile()) continue;
      seen += 1;
      if (seen > MAX_FILES) throw new Error(`Multi-config scan exceeded ${MAX_FILES.toLocaleString()} files.`);
      const baseMatch = /^png(\d{4,5})\.iff$/i.exec(entry.name);
      if (baseMatch) {
        const id = Number(baseMatch[1]);
        baseIds.add(id);
        if (!baseFiles.has(id)) baseFiles.set(id, []);
        baseFiles.get(id).push(path.join(root, relative));
      }
      const match = /^png(\d{4,5})_config_([A-Za-z0-9_-]+)\.iff$/i.exec(entry.name);
      if (!match) continue;
      const stat = await fsp.stat(path.join(root, relative));
      const id = Number(match[1]);
      if (!groups.has(id)) groups.set(id, new Set());
      groups.get(id).add(match[2].toLowerCase());
      inventory.push({
        relativePath: relative.replaceAll('\\', '/').toLowerCase(),
        size: stat.size,
        modifiedMs: Math.trunc(stat.mtimeMs),
      });
    }
  }
  inventory.sort((a, b) => a.relativePath.localeCompare(b.relativePath));
  // Hash only base archives that participate in config sets. This detects
  // JSON edits even when an editor preserves file size and timestamps, without
  // inflating every archive on every launch.
  const definitions = [];
  for (const [id, files] of [...baseFiles].sort((a, b) => a[0] - b[0])) {
    if (!groups.has(id)) continue;
    for (const file of files.sort()) definitions.push({ id, path: path.relative(root, file), sha256: await sha256File(file) });
  }
  return { groups, baseIds, baseFiles, inventory, fingerprint: digest(JSON.stringify({inventory, definitions})) };
}

function partitionConfigNames(info) {
  const headbandItems = new Set((info.items || []).filter(item => /headband/i.test(String(item.type || '') + ' ' + String(item.name || ''))).map(item => String(item.name).toLowerCase()));
  const names = info.configurations.map(config => String(config.name).toUpperCase());
  const headband = info.configurations.filter(config => /headband/i.test(config.name) || (config.items || []).some(item => headbandItems.has(String(item).toLowerCase()))).map(config => String(config.name).toUpperCase());
  const ordinary = names.filter(name => !headband.includes(name));
  const base = ordinary.includes(String(info.default_config).toUpperCase()) ? String(info.default_config).toUpperCase() : (ordinary[0] || names[0]);
  // The native Hair UI merges both lists and skips ONLY the first ordinary
  // entry when reading the headband list. Never repeat all ordinary styles.
  return { names, withoutHeadband: [base, ...ordinary.filter(name => name !== base)], withHeadband: [base, ...headband.filter(name => name !== base)] };
}

async function readReplacementDefinition(files, available) {
  let signature = null, definition = null;
  for (const file of files) {
    const stat = await fsp.stat(file);
    if (stat.size > 128 * 1024 * 1024) throw new Error('Player archive exceeds selector inspection limit.');
    const archive = readZipArchive(await fsp.readFile(file));
    const entry = [...archive].find(([name]) => name.toLowerCase() === 'appearance_info.json');
    if (!entry) throw new Error('No appearance_info.json.');
    const info = JSON.parse(entry[1].toString()).accessory_items;
    const current = info?.configurations?.map(config => String(config.name).toUpperCase());
    if (!current?.length || current.length > 9 || new Set(current).size !== current.length
      || current.some(name => !/^[A-Z0-9_-]+$/.test(name) || !available.has(name.toLowerCase()))) {
      throw new Error('Missing, duplicate, or unsupported config references.');
    }
    for (const key of ['default_config', 'default_out_of_uniform_config']) {
      if (info[key] && !current.includes(String(info[key]).toUpperCase())) throw new Error('Default config is not declared.');
    }
    const nextDefinition = { ...partitionConfigNames(info), accessoryItems: info };
    const next = JSON.stringify(nextDefinition);
    if (signature !== null && signature !== next) throw new Error('Conflicting duplicate player definitions.');
    signature = next; definition = nextDefinition;
  }
  return definition;
}

async function readReplacementNames(files, available) { return (await readReplacementDefinition(files, available)).names; }

async function readStockManifest(gameRoot) {
  const manifestPath = path.join(gameRoot, 'manifest');
  const groups = new Map();
  const baseIds = new Set();
  const tableEntries = {};
  const input = fs.createReadStream(manifestPath, { encoding: 'utf8' });
  const lines = readline.createInterface({ input, crlfDelay: Infinity });
  for await (const line of lines) {
    const parts = line.split(',');
    if (parts.length !== 4) continue;
    const [name, archive, offsetText, sizeText] = parts;
    const normalized = name.replaceAll('\\', '/').toLowerCase();
    for (const [key, logicalPath] of Object.entries(TABLE_PATHS)) {
      if (normalized === logicalPath) {
        tableEntries[key] = {
          name,
          archive,
          offset: Number(offsetText),
          size: Number(sizeText),
        };
      }
    }
    const baseMatch = /^char\/sig\/png(\d{5})\.iff$/i.exec(normalized);
    if (baseMatch) baseIds.add(Number(baseMatch[1]));
    const configMatch = /^char\/sig\/png(\d{5})_config_([a-z0-9_-]+)\.iff$/i.exec(normalized);
    if (!configMatch) continue;
    const id = Number(configMatch[1]);
    if (!groups.has(id)) groups.set(id, new Set());
    groups.get(id).add(configMatch[2]);
  }
  if (!tableEntries.all || !tableEntries.edit || !tableEntries.registry) {
    throw new Error('The stock signature-config tables were not found in the game manifest.');
  }
  return { manifestPath, groups, baseIds, tableEntries };
}

async function decodeManifestEntry({ gameRoot, decoderPath, entry, outputPath }) {
  if (!/^[0-9A-Z]{2}$/i.test(entry.archive)) throw new Error(`Unsafe archive ID ${entry.archive}.`);
  const archivePath = path.join(gameRoot, entry.archive);
  const oodlePath = path.join(gameRoot, 'data', 'oodle', 'oo2core_9_win64.dll');
  await fsp.mkdir(path.dirname(outputPath), { recursive: true });
  await execFileAsync(decoderPath, [
    archivePath,
    String(entry.offset),
    String(entry.size),
    oodlePath,
    outputPath,
  ], { windowsHide: true, timeout: 120000, maxBuffer: 1024 * 1024 });
  const stat = await fsp.stat(outputPath);
  if (!stat.isFile() || stat.size < 1) throw new Error(`Decoder produced no data for ${entry.name}.`);
}

async function readJson(filePath) {
  try {
    return JSON.parse(await fsp.readFile(filePath, 'utf8'));
  } catch (error) {
    if (error.code === 'ENOENT' || error.name === 'SyntaxError') return null;
    throw error;
  }
}

async function pruneGeneratedConfigs(managedRoot,currentKey,keep=2){
  const generated=path.join(managedRoot,'generated');let entries;
  try{entries=await fsp.readdir(generated,{withFileTypes:true});}catch(error){if(error.code==='ENOENT')return [];throw error;}
  const candidates=[];
  for(const entry of entries){
    if(!entry.isDirectory()||entry.name===currentKey||!/^[a-f0-9]{24}$/.test(entry.name))continue;
    const target=path.join(generated,entry.name);const stat=await fsp.lstat(target);
    if(stat.isSymbolicLink())continue;candidates.push({target,modified:stat.mtimeMs});
  }
  candidates.sort((a,b)=>b.modified-a.modified);const removed=[];
  for(const item of candidates.slice(Math.max(0,keep-1))){await fsp.rm(item.target,{recursive:true,force:true});removed.push(item.target);}
  return removed;
}

function buildRenderRegistry(source, replacements) {
  if (source.length > MAX_SELECTOR_ARCHIVE_BYTES) throw new Error('Registry archive exceeds safe size.');
  const members = readZipArchive(source, { maxEntries: 10000 });
  const seen = new Set();
  for (const replacement of replacements) {
    const { id, accessoryItems } = replacement;
    if (!Number.isInteger(id) || id < 0 || id > 99999 || seen.has(id)) throw new Error('Invalid registry player ID.');
    seen.add(id);
    const member = `${String(id).padStart(5, '0')}_modified.json`;
    if (!members.has(member)) throw new Error(`Missing stock registry member ${member}.`);
    const names = accessoryItems?.configurations?.map(c => c.name);
    if (!names?.length || names.length > 9 || names.some(n => typeof n !== 'string' || !/^[a-z0-9_-]+$/i.test(n))
      || new Set(names.map(n => n.toUpperCase())).size !== names.length) throw new Error('Invalid registry configurations.');
    for (const key of ['default_config', 'default_out_of_uniform_config']) {
      if (accessoryItems[key] && !names.includes(accessoryItems[key])) throw new Error('Invalid registry default.');
    }
    const items = new Set((accessoryItems.items || []).map(item => item.name));
    for (const config of accessoryItems.configurations) {
      if ((config.items || []).some(item => !items.has(item)) || (config.fallback && !names.includes(config.fallback))) throw new Error('Invalid registry item or fallback.');
    }
    const original = JSON.parse(members.get(member).toString('utf8'));
    members.set(member, Buffer.from(JSON.stringify({ ...original, accessory_items: accessoryItems })));
  }
  const result = buildZipArchive([...members].map(([name, data]) => ({ name, data })), { maxEntries: 10000 });
  if (result.length > MAX_SELECTOR_ARCHIVE_BYTES) throw new Error('Generated registry exceeds safe size.');
  const readback = readZipArchive(result, { maxEntries: 10000 });
  if (readback.size !== members.size || [...members].some(([name, data]) => !readback.get(name)?.equals(data))) throw new Error('Registry archive verification failed.');
  return result;
}

class MultiConfigManager {
  constructor({ managedRoot, decoderPath, onStatus = () => {} }) {
    this.managedRoot = path.resolve(managedRoot);
    this.decoderPath = path.resolve(decoderPath);
    this.onStatus = onStatus;
    this.statePath = path.join(this.managedRoot, 'state.json');
    this.current = { phase: 'idle', message: 'Automatic multi-config unlock is ready.' };
  }

  emit(patch) {
    this.current = { ...this.current, ...patch, updatedAt: new Date().toISOString() };
    this.onStatus(this.current);
    return this.current;
  }

  async view(enabled = true) {
    const saved = await readJson(this.statePath);
    return {
      ...this.current,
      enabled,
      eligiblePlayers: saved?.eligiblePlayers || [],
      skipped: saved?.skipped || [],
      cached: Boolean(saved),
    };
  }

  async ensureStockTables(gameRoot, stock, stockKey) {
    const cacheRoot = path.join(this.managedRoot, 'stock', stockKey);
    const allMember = path.join(cacheRoot, 'all', 'All.SigConfigTable');
    const withoutMember = path.join(cacheRoot, 'edit', 'EditPlayer_WithoutHeadband.SigConfigTable');
    const withMember = path.join(cacheRoot, 'edit', 'EditPlayer_WithHeadband.SigConfigTable');
    const registryArchive = path.join(cacheRoot, 'fpplayer_global.decoded.iff');
    const members = [allMember, withoutMember, withMember, registryArchive];
    if ((await Promise.all(members.map((item) => fsp.stat(item).catch(() => null))))
      .every((item) => item?.isFile())) {
      return { cacheRoot, allMember, withoutMember, withMember, registryArchive };
    }
    await fsp.rm(cacheRoot, { recursive: true, force: true });
    await fsp.mkdir(cacheRoot, { recursive: true });
    const decodedAll = path.join(cacheRoot, 'signatureconfigs_all.decoded.iff');
    const decodedEdit = path.join(cacheRoot, 'signatureconfigs_editplayer.decoded.iff');
    await decodeManifestEntry({ gameRoot, decoderPath: this.decoderPath, entry: stock.tableEntries.all, outputPath: decodedAll });
    await decodeManifestEntry({ gameRoot, decoderPath: this.decoderPath, entry: stock.tableEntries.edit, outputPath: decodedEdit });
    await decodeManifestEntry({ gameRoot, decoderPath: this.decoderPath, entry: stock.tableEntries.registry, outputPath: registryArchive });
    await extractZip(decodedAll, path.join(cacheRoot, 'all'));
    await extractZip(decodedEdit, path.join(cacheRoot, 'edit'));
    for (const member of members) {
      const stat = await fsp.stat(member).catch(() => null);
      if (!stat?.isFile()) throw new Error(`Decoded table member is missing: ${path.basename(member)}`);
    }
    return { cacheRoot, allMember, withoutMember, withMember, registryArchive };
  }

  async reusePrepared(saved, stockKey, fingerprint, modsRoot) {
    if (saved?.cacheVersion !== 5 || saved.stockKey !== stockKey || saved.inventoryFingerprint !== fingerprint
      || saved.modsRoot !== path.resolve(modsRoot) || !/^[a-f0-9]{24}$/.test(saved.generationKey || '')) return false;
    const pending = [];
    for (const [key, logical] of Object.entries(TABLE_PATHS)) {
      const target = path.join(modsRoot, ...logical.split('/'));
      const source = path.join(this.managedRoot, 'generated', saved.generationKey, path.basename(logical));
      const owned = saved.installed?.[key];
      if (!owned || path.resolve(owned.path) !== path.resolve(target) || !/^[a-f0-9]{64}$/i.test(owned.sha256 || '')) return false;
      if (await sha256File(source).catch(() => null) !== owned.sha256) return false;
      const current = await sha256File(target).catch(() => null);
      if (current === owned.sha256) continue;
      if(current&&await this.matchesPreviousArchive(saved,key,target,modsRoot))continue;
      if (await fsp.stat(target).catch(() => null)) return false;
      pending.push({source, target});
    }
    // Validate all outputs and destinations before restoring any missing file.
    for (const {source, target} of pending) {
      await fsp.mkdir(path.dirname(target), {recursive:true});
      await fsp.copyFile(source, target, fs.constants.COPYFILE_EXCL);
    }
    return true;
  }

  async matchesPreviousArchive(saved,key,target,modsRoot){
    if(saved?.cacheVersion!==5||saved.modsRoot!==path.resolve(modsRoot)||!/^[a-f0-9]{24}$/.test(saved.generationKey||''))return false;
    const owned=saved.installed?.[key],logical=TABLE_PATHS[key];
    if(!logical||!owned||typeof owned.path!=='string'||path.resolve(owned.path)!==path.resolve(target)||!/^[a-f0-9]{64}$/i.test(owned.sha256||''))return false;
    const source=path.join(this.managedRoot,'generated',saved.generationKey,path.basename(logical));
    try{
      for(const folder of [this.managedRoot,path.join(this.managedRoot,'generated'),path.dirname(source)]){
        const stat=await fsp.lstat(folder);if(!stat.isDirectory()||stat.isSymbolicLink())return false;
      }
      const sourceStat=await fsp.lstat(source),targetStat=await fsp.lstat(target);
      if(!sourceStat.isFile()||!targetStat.isFile()||sourceStat.isSymbolicLink()||targetStat.isSymbolicLink())return false;
      if(await sha256File(source)!==owned.sha256.toLowerCase())return false;
      const equivalent=await archivesEquivalent(target,source);
      return equivalent&&await sha256File(source)===owned.sha256.toLowerCase();
    }catch{return false;}
  }

  async prepare({ gameRoot, modsRoot, enabled = true }) {
    if (!enabled) return this.disable(modsRoot);
    this.emit({ phase: 'scanning', enabled: true, selectorOverride: false, message: 'Scanning unique player config sets…' });
    const [mods, previous, exeStat, manifestStat] = await Promise.all([
      scanModConfigs(modsRoot),
      readJson(this.statePath),
      fsp.stat(path.join(gameRoot, 'NBA2K27.exe')),
      fsp.stat(path.join(gameRoot, 'manifest')),
    ]);
    const stockKey = digest(JSON.stringify({
      exeSize: exeStat.size,
      exeModifiedMs: Math.trunc(exeStat.mtimeMs),
      manifestSize: manifestStat.size,
      manifestModifiedMs: Math.trunc(manifestStat.mtimeMs),
    })).slice(0, 24);
    if (await this.reusePrepared(previous, stockKey, mods.fingerprint, modsRoot)) {
      return this.emit({phase:'ready', enabled:true, selectorOverride:true, cached:true,
        eligiblePlayers:previous.eligiblePlayers || [], skipped:previous.skipped || [],
        message:'Reused verified player config cache.'});
    }
    const stock = await readStockManifest(gameRoot);
    this.emit({ phase: 'extracting', message: 'Preparing local stock selector tables…' });
    const tables = await this.ensureStockTables(gameRoot, stock, stockKey);
    const [allSource, withoutSource, withSource] = await Promise.all([
      fsp.readFile(tables.allMember),
      fsp.readFile(tables.withoutMember),
      fsp.readFile(tables.withMember),
    ]);
    const allRecords = new Map(readRecords(allSource).records.map((record) => [record.id, record]));
    const donorBySet = new Map();
    for (const [id, suffixes] of stock.groups) {
      const record = allRecords.get(id);
      if (!record || suffixes.size < 2 || suffixes.size > 9 || record.configCount !== suffixes.size) continue;
      const key = suffixKey(suffixes);
      if (!donorBySet.has(key)) donorBySet.set(key, id);
    }

    const eligible = [];
    const skipped = [];
    const replacements = [];
    const editRecords = [withoutSource, withSource].map(source => new Set(readRecords(source).records.map(r => r.id)));
    for (const [id, suffixes] of [...mods.groups].sort((a, b) => a[0] - b[0])) {
      const details = { id, configCount: suffixes.size, suffixes: [...suffixes].sort() };
      if (stock.baseIds.has(id) || allRecords.has(id)) {
        if (mods.baseFiles.has(id) && allRecords.has(id) && editRecords.every(records => records.has(id))) {
          try {
            const definition = await readReplacementDefinition(mods.baseFiles.get(id), suffixes);
            replacements.push({ id, ...definition });
          } catch (error) {
            skipped.push({ ...details, reason: 'replacement-layout-unverified', detail: error.message });
          }
        } else skipped.push({ ...details, reason: 'stock-player' });
      } else if (!mods.baseIds.has(id)) {
        skipped.push({ ...details, reason: 'base-png-missing' });
      } else if (suffixes.size < 2) {
        skipped.push({ ...details, reason: 'single-config' });
      } else if (suffixes.size > 9) {
        skipped.push({ ...details, reason: 'over-nine-config-limit' });
      } else {
        const donorId = donorBySet.get(suffixKey(suffixes));
        if (!donorId) skipped.push({ ...details, reason: 'no-exact-stock-style-set' });
        else eligible.push({ ...details, donorId });
      }
    }

    const generationKey = digest(JSON.stringify({ schema: 5, stockKey, inventory: mods.fingerprint, eligible, replacements })).slice(0, 24);
    const generationRoot = path.join(this.managedRoot, 'generated', generationKey);
    const outputAll = path.join(generationRoot, 'signatureconfigs_all.iff');
    const outputEdit = path.join(generationRoot, 'signatureconfigs_editplayer.iff');
    const outputRegistry = path.join(generationRoot, 'fpplayer_global.iff');
    const clones = eligible.map(({ id, donorId }) => ({ targetId: id, donorId }));
    if (!eligible.length && !replacements.length) {
      await this.disable(modsRoot);
      const saved = { schema: 'nba2k27-multi-config-v1', stockKey, generationKey, eligiblePlayers: [], skipped, installed: {} };
      await fsp.mkdir(this.managedRoot, { recursive: true });
      await fsp.writeFile(this.statePath, `${JSON.stringify(saved, null, 2)}\n`, 'utf8');
      await pruneGeneratedConfigs(this.managedRoot,generationKey);
      return this.emit({ phase: 'ready', enabled: true, eligiblePlayers: [], skipped, message: 'No eligible unique multi-config players were found.' });
    }

    let outputsReady = (await Promise.all([outputAll, outputEdit, outputRegistry].map((item) => fsp.stat(item).catch(() => null))))
      .every((item) => item?.isFile());
    if (outputsReady && previous?.generationKey === generationKey) {
      for (const [key, file] of Object.entries({all:outputAll, edit:outputEdit, registry:outputRegistry})) {
        if (await sha256File(file) !== previous.installed?.[key]?.sha256) outputsReady = false;
      }
    }
    if (!outputsReady) {
      this.emit({ phase: 'generating', message: `Building selectors for ${eligible.length} unique player${eligible.length === 1 ? '' : 's'}…` });
      await fsp.rm(generationRoot, { recursive: true, force: true });
      const allDirectory = path.join(generationRoot, 'all');
      const editDirectory = path.join(generationRoot, 'edit');
      await fsp.cp(path.join(tables.cacheRoot, 'all'), allDirectory, { recursive: true });
      await fsp.cp(path.join(tables.cacheRoot, 'edit'), editDirectory, { recursive: true });
      const builtAll = buildTableWithClones(allSource, clones);
      const builtWithout = buildTableWithClones(withoutSource, clones);
      const builtWith = buildTableWithClones(withSource, clones);
      builtAll.data = replaceConfigLists(builtAll.data, replacements);
      builtWithout.data = replaceConfigLists(builtWithout.data, replacements.map(r => ({id:r.id, names:r.withoutHeadband})));
      builtWith.data = replaceConfigLists(builtWith.data, replacements.map(r => ({id:r.id, names:r.withHeadband})));
      await Promise.all([
        fsp.writeFile(path.join(allDirectory, 'All.SigConfigTable'), builtAll.data),
        fsp.writeFile(path.join(editDirectory, 'EditPlayer_WithoutHeadband.SigConfigTable'), builtWithout.data),
        fsp.writeFile(path.join(editDirectory, 'EditPlayer_WithHeadband.SigConfigTable'), builtWith.data),
      ]);
      await createZip(allDirectory, outputAll);
      await createZip(editDirectory, outputEdit);
      await fsp.writeFile(outputRegistry, buildRenderRegistry(await fsp.readFile(tables.registryArchive), replacements));
    }

    const destinations = {
      all: path.join(modsRoot, 'char', 'signatureconfigs_all.iff'),
      edit: path.join(modsRoot, 'char', 'signatureconfigs_editplayer.iff'),
      registry: path.join(modsRoot, 'char', 'fpplayer_global.iff'),
    };
    const generatedHashes = {
      all: await sha256File(outputAll),
      edit: await sha256File(outputEdit),
      registry: await sha256File(outputRegistry),
    };
    for (const key of Object.keys(destinations)) {
      const currentStat = await fsp.stat(destinations[key]).catch(() => null);
      if (!currentStat) continue;
      const currentHash = await sha256File(destinations[key]);
      const previouslyOwned = previous?.installed?.[key]?.sha256 === currentHash
        || await this.matchesPreviousArchive(previous,key,destinations[key],modsRoot);
      const equivalentGeneratedArchive = previouslyOwned || currentHash === generatedHashes[key]
        || await archivesEquivalent(destinations[key], {all: outputAll, edit: outputEdit, registry: outputRegistry}[key]);
      if (!previouslyOwned && !equivalentGeneratedArchive) {
        throw new Error(`A different ${path.basename(destinations[key])} mod is installed; it was left untouched.`);
      }
    }
    await fsp.mkdir(path.join(modsRoot, 'char'), { recursive: true });
    await Promise.all([
      fsp.copyFile(outputAll, destinations.all),
      fsp.copyFile(outputEdit, destinations.edit),
      fsp.copyFile(outputRegistry, destinations.registry),
    ]);
    const installed = {
      all: { path: destinations.all, sha256: generatedHashes.all },
      edit: { path: destinations.edit, sha256: generatedHashes.edit },
      registry: { path: destinations.registry, sha256: generatedHashes.registry },
    };
    const saved = {
      schema: 'nba2k27-multi-config-v1',
      cacheVersion: 5,
      modsRoot: path.resolve(modsRoot),
      stockKey,
      generationKey,
      inventoryFingerprint: mods.fingerprint,
      eligiblePlayers: eligible,
      replacements,
      skipped,
      installed,
    };
    await fsp.mkdir(this.managedRoot, { recursive: true });
    await fsp.writeFile(this.statePath, `${JSON.stringify(saved, null, 2)}\n`, 'utf8');
    await pruneGeneratedConfigs(this.managedRoot,generationKey);
    return this.emit({
      phase: 'ready',
      enabled: true,
      eligiblePlayers: eligible,
      skipped,
      message: `Prepared selectors for ${eligible.length} unique players and synchronized ${replacements.length} replacement players.`,
      selectorOverride: true,
    });
  }

  async disable(modsRoot) {
    const previous = await readJson(this.statePath);
    for (const item of Object.values(previous?.installed || {})) {
      const current = await fsp.stat(item.path).catch(() => null);
      if (!current?.isFile()) continue;
      if (await sha256File(item.path) === item.sha256) await fsp.rm(item.path, { force: true });
    }
    return this.emit({ phase: 'disabled', enabled: false, selectorOverride: false, eligiblePlayers: [], message: 'Automatic multi-config unlock is disabled.' });
  }
}

module.exports = {
  buildRenderRegistry,
  MultiConfigManager,
  archivesEquivalent,
  createZip,
  readStockManifest,
  scanModConfigs,
  suffixKey,
  readReplacementNames,
  partitionConfigNames,
  readReplacementDefinition,
  pruneGeneratedConfigs,
};
