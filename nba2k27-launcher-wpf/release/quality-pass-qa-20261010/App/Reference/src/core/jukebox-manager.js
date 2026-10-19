const crypto = require('node:crypto');
const { execFile } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');
const fsp = require('node:fs/promises');
const { promisify } = require('node:util');
const zlib = require('node:zlib');
const {writePlaylistContainer,openPlaylistContainer,readPlaylistSegment}=require('./playlist-container');
const {FlacAudio}=require('./flac-audio');

const execFileAsync = promisify(execFile);

const MAX_IFF_BYTES = 16 * 1024 * 1024;
const MAX_TRACKS = 4096;
const SCHEMA = 'nba2k27-jukebox-library-v1';
const PLAYLIST_SCHEMA = 'nba2k27-2kplaylist-v1';
const FLAC_PLAYLIST_SCHEMA = 'nba2k27-2kplaylist-v2';
const OPUS_PLAYLIST_SCHEMA='nba2k27-2kplaylist-v3';
const supportedPlaylist = schema => schema===PLAYLIST_SCHEMA || schema===FLAC_PLAYLIST_SCHEMA || schema===OPUS_PLAYLIST_SCHEMA;
const MAX_PLAYLIST_ARCHIVE_BYTES = 384 * 1024 * 1024;
const MAX_PLAYLIST_ENTRY_BYTES = 256 * 1024 * 1024;
const MAX_PLAYLIST_ENTRIES = 512;
const DEFAULT_PLAYLIST_PART_BYTES = 320 * 1024 * 1024;

let crcTable;
function crc32(buffer) {
  if (!crcTable) {
    crcTable = new Uint32Array(256);
    for (let value = 0; value < 256; value += 1) {
      let crc = value;
      for (let bit = 0; bit < 8; bit += 1) crc = (crc >>> 1) ^ ((crc & 1) ? 0xedb88320 : 0);
      crcTable[value] = crc >>> 0;
    }
  }
  let crc = 0xffffffff;
  for (const byte of buffer) crc = (crc >>> 8) ^ crcTable[(crc ^ byte) & 0xff];
  return (crc ^ 0xffffffff) >>> 0;
}

function readMusicJson(archive) {
  if (!Buffer.isBuffer(archive) || archive.length < 22 || archive.length > MAX_IFF_BYTES) {
    throw new Error('music.iff is outside the safe size limit.');
  }
  const eocd = archive.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
  if (eocd < 0 || eocd + 22 > archive.length) throw new Error('music.iff has no valid ZIP end record.');
  const count = archive.readUInt16LE(eocd + 10);
  let cursor = archive.readUInt32LE(eocd + 16);
  for (let index = 0; index < count; index += 1) {
    if (cursor + 46 > archive.length || archive.readUInt32LE(cursor) !== 0x02014b50) throw new Error('music.iff has an invalid central directory.');
    const method = archive.readUInt16LE(cursor + 10);
    const expectedCrc = archive.readUInt32LE(cursor + 16);
    const compressedSize = archive.readUInt32LE(cursor + 20);
    const size = archive.readUInt32LE(cursor + 24);
    const nameLength = archive.readUInt16LE(cursor + 28);
    const extraLength = archive.readUInt16LE(cursor + 30);
    const commentLength = archive.readUInt16LE(cursor + 32);
    const localOffset = archive.readUInt32LE(cursor + 42);
    const name = archive.subarray(cursor + 46, cursor + 46 + nameLength).toString('utf8');
    if (name.toLowerCase() === 'music.json') {
      if (method !== 0 && method !== 8) throw new Error(`music.json uses unsupported ZIP method ${method}.`);
      if (localOffset + 30 > archive.length || archive.readUInt32LE(localOffset) !== 0x04034b50) throw new Error('music.json has an invalid local header.');
      const localNameLength = archive.readUInt16LE(localOffset + 26);
      const localExtraLength = archive.readUInt16LE(localOffset + 28);
      const start = localOffset + 30 + localNameLength + localExtraLength;
      const packed = archive.subarray(start, start + compressedSize);
      const data = method === 0 ? Buffer.from(packed) : zlib.inflateRawSync(packed, { maxOutputLength: size });
      if (data.length !== size || crc32(data) !== expectedCrc) throw new Error('music.json failed size or CRC validation.');
      const parsed = JSON.parse(data.toString('utf8').replace(/^\uFEFF/, ''));
      if (!Array.isArray(parsed?.Tracks) || parsed.Tracks.length > MAX_TRACKS) throw new Error('music.json has an invalid track list.');
      return parsed;
    }
    cursor += 46 + nameLength + extraLength + commentLength;
  }
  throw new Error('music.iff does not contain music.json.');
}

function buildMusicIff(music) {
  const name = Buffer.from('music.json', 'utf8');
  const data = Buffer.from(`${JSON.stringify(music, null, 2)}\n`, 'utf8');
  const packed = zlib.deflateRawSync(data, { level: 9 });
  const checksum = crc32(data);
  const local = Buffer.alloc(30 + name.length);
  local.writeUInt32LE(0x04034b50, 0);
  local.writeUInt16LE(20, 4);
  local.writeUInt16LE(0x0800, 6);
  local.writeUInt16LE(8, 8);
  local.writeUInt32LE(checksum, 14);
  local.writeUInt32LE(packed.length, 18);
  local.writeUInt32LE(data.length, 22);
  local.writeUInt16LE(name.length, 26);
  name.copy(local, 30);

  const central = Buffer.alloc(46 + name.length);
  central.writeUInt32LE(0x02014b50, 0);
  central.writeUInt16LE(20, 4);
  central.writeUInt16LE(20, 6);
  central.writeUInt16LE(0x0800, 8);
  central.writeUInt16LE(8, 10);
  central.writeUInt32LE(checksum, 16);
  central.writeUInt32LE(packed.length, 20);
  central.writeUInt32LE(data.length, 24);
  central.writeUInt16LE(name.length, 28);
  name.copy(central, 46);

  const eocd = Buffer.alloc(22);
  eocd.writeUInt32LE(0x06054b50, 0);
  eocd.writeUInt16LE(1, 8);
  eocd.writeUInt16LE(1, 10);
  eocd.writeUInt32LE(central.length, 12);
  eocd.writeUInt32LE(local.length + packed.length, 16);
  return Buffer.concat([local, packed, central, eocd]);
}

function safeArchiveName(value) {
  const name = String(value || '').replaceAll('\\', '/');
  if (!name || name.startsWith('/') || /^[a-z]:/i.test(name)
    || name.split('/').some((part) => !part || part === '.' || part === '..')) {
    throw new Error('The playlist contains an unsafe archive path.');
  }
  return name;
}

function buildZipArchive(entries, { maxEntries = MAX_PLAYLIST_ENTRIES } = {}) {
  if (!Number.isInteger(maxEntries) || maxEntries < 1 || maxEntries > 10000) throw new Error('Invalid ZIP entry limit.');
  if (!Array.isArray(entries) || !entries.length || entries.length > maxEntries) {
    throw new Error('The playlist bundle has an invalid number of files.');
  }
  const localParts = [];
  const centralParts = [];
  let offset = 0;
  let totalBytes = 0;
  for (const entry of entries) {
    const name = Buffer.from(safeArchiveName(entry.name), 'utf8');
    const data = Buffer.isBuffer(entry.data) ? entry.data : Buffer.from(entry.data || []);
    if (data.length > MAX_PLAYLIST_ENTRY_BYTES || totalBytes + data.length > MAX_PLAYLIST_ARCHIVE_BYTES) {
      const error = new Error('This playlist is too large for one shareable .2kplaylist file (384 MB uncompressed limit).');
      error.code = 'PLAYLIST_BUNDLE_TOO_LARGE';
      throw error;
    }
    totalBytes += data.length;
    const packed = zlib.deflateRawSync(data, { level: 9 });
    const checksum = crc32(data);
    const local = Buffer.alloc(30 + name.length);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt16LE(20, 4);
    local.writeUInt16LE(0x0800, 6);
    local.writeUInt16LE(8, 8);
    local.writeUInt32LE(checksum, 14);
    local.writeUInt32LE(packed.length, 18);
    local.writeUInt32LE(data.length, 22);
    local.writeUInt16LE(name.length, 26);
    name.copy(local, 30);
    const central = Buffer.alloc(46 + name.length);
    central.writeUInt32LE(0x02014b50, 0);
    central.writeUInt16LE(20, 4);
    central.writeUInt16LE(20, 6);
    central.writeUInt16LE(0x0800, 8);
    central.writeUInt16LE(8, 10);
    central.writeUInt32LE(checksum, 16);
    central.writeUInt32LE(packed.length, 20);
    central.writeUInt32LE(data.length, 24);
    central.writeUInt16LE(name.length, 28);
    central.writeUInt32LE(offset, 42);
    name.copy(central, 46);
    localParts.push(local, packed);
    centralParts.push(central);
    offset += local.length + packed.length;
  }
  const centralSize = centralParts.reduce((sum, part) => sum + part.length, 0);
  const eocd = Buffer.alloc(22);
  eocd.writeUInt32LE(0x06054b50, 0);
  eocd.writeUInt16LE(entries.length, 8);
  eocd.writeUInt16LE(entries.length, 10);
  eocd.writeUInt32LE(centralSize, 12);
  eocd.writeUInt32LE(offset, 16);
  return Buffer.concat([...localParts, ...centralParts, eocd]);
}

function readZipArchive(archive, { maxEntries = MAX_PLAYLIST_ENTRIES } = {}) {
  if (!Number.isInteger(maxEntries) || maxEntries < 1 || maxEntries > 10000) throw new Error('Invalid ZIP entry limit.');
  if (!Buffer.isBuffer(archive) || archive.length < 22 || archive.length > MAX_PLAYLIST_ARCHIVE_BYTES) {
    throw new Error('The .2kplaylist file is outside the safe size limit.');
  }
  const eocd = archive.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
  if (eocd < 0 || eocd + 22 > archive.length) throw new Error('The .2kplaylist file has no valid ZIP end record.');
  const count = archive.readUInt16LE(eocd + 10);
  if (!count || count > maxEntries) throw new Error('The .2kplaylist file has an invalid number of entries.');
  const entries = new Map();
  let totalBytes = 0;
  let cursor = archive.readUInt32LE(eocd + 16);
  for (let index = 0; index < count; index += 1) {
    if (cursor + 46 > archive.length || archive.readUInt32LE(cursor) !== 0x02014b50) throw new Error('The .2kplaylist central directory is invalid.');
    const method = archive.readUInt16LE(cursor + 10);
    const expectedCrc = archive.readUInt32LE(cursor + 16);
    const compressedSize = archive.readUInt32LE(cursor + 20);
    const size = archive.readUInt32LE(cursor + 24);
    const nameLength = archive.readUInt16LE(cursor + 28);
    const extraLength = archive.readUInt16LE(cursor + 30);
    const commentLength = archive.readUInt16LE(cursor + 32);
    const localOffset = archive.readUInt32LE(cursor + 42);
    const name = safeArchiveName(archive.subarray(cursor + 46, cursor + 46 + nameLength).toString('utf8'));
    if (entries.has(name)) throw new Error(`The .2kplaylist contains duplicate entry ${name}.`);
    if ((method !== 0 && method !== 8) || size > MAX_PLAYLIST_ENTRY_BYTES
      || totalBytes + size > MAX_PLAYLIST_ARCHIVE_BYTES) throw new Error('The .2kplaylist contains an unsupported or oversized entry.');
    if (localOffset + 30 > archive.length || archive.readUInt32LE(localOffset) !== 0x04034b50) throw new Error('The .2kplaylist local header is invalid.');
    const localNameLength = archive.readUInt16LE(localOffset + 26);
    const localExtraLength = archive.readUInt16LE(localOffset + 28);
    const start = localOffset + 30 + localNameLength + localExtraLength;
    if (start + compressedSize > archive.length) throw new Error('The .2kplaylist entry is truncated.');
    const packed = archive.subarray(start, start + compressedSize);
    const data = method === 0 ? Buffer.from(packed) : zlib.inflateRawSync(packed, { maxOutputLength: size });
    if (data.length !== size || crc32(data) !== expectedCrc) throw new Error(`The .2kplaylist entry ${name} failed validation.`);
    entries.set(name, data);
    totalBytes += data.length;
    cursor += 46 + nameLength + extraLength + commentLength;
  }
  return entries;
}

async function readZipMemberFromFile(filePath, wantedName, segment = null) {
  const container=await openPlaylistContainer(filePath);
  if(container && segment === null){
    if(wantedName==='playlist.json')return Buffer.from(JSON.stringify(container.manifest));
    for(let i=0;i<container.parts.length;i++){const entries=readZipArchive(await readPlaylistSegment(container,i));if(entries.has(wantedName))return entries.get(wantedName);}
    throw new Error(`The .2kplaylist does not contain ${wantedName}.`);
  }
  const handle = await fsp.open(filePath, 'r');
  try {
    const actual = await handle.stat();
    const part = container && container.parts[segment];
    if(container && !part) throw new Error('Missing playlist segment.');
    const base = part ? part.offset : 0;
    const stat = {size:part ? part.length : actual.size};
    if(container && (actual.size!==container.size || actual.mtimeMs!==container.mtimeMs))throw new Error('Playlist changed during read.');
    if (stat.size < 22 || stat.size > MAX_PLAYLIST_ARCHIVE_BYTES) {
      throw new Error('The .2kplaylist file is outside the safe size limit.');
    }
    const tailLength = Math.min(stat.size, 65557);
    const tail = Buffer.alloc(tailLength);
    await handle.read(tail, 0, tailLength, base + stat.size - tailLength);
    const relativeEocd = tail.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
    if (relativeEocd < 0 || relativeEocd + 22 > tail.length) throw new Error('The .2kplaylist file has no valid ZIP end record.');
    const count = tail.readUInt16LE(relativeEocd + 10);
    const centralSize = tail.readUInt32LE(relativeEocd + 12);
    const centralOffset = tail.readUInt32LE(relativeEocd + 16);
    if (!count || count > MAX_PLAYLIST_ENTRIES || centralOffset + centralSize > stat.size || centralSize > 4 * 1024 * 1024) {
      throw new Error('The .2kplaylist central directory is invalid.');
    }
    const central = Buffer.alloc(centralSize);
    await handle.read(central, 0, centralSize, base + centralOffset);
    let cursor = 0;
    for (let index = 0; index < count; index += 1) {
      if (cursor + 46 > central.length || central.readUInt32LE(cursor) !== 0x02014b50) {
        throw new Error('The .2kplaylist central directory is invalid.');
      }
      const method = central.readUInt16LE(cursor + 10);
      const expectedCrc = central.readUInt32LE(cursor + 16);
      const compressedSize = central.readUInt32LE(cursor + 20);
      const size = central.readUInt32LE(cursor + 24);
      const nameLength = central.readUInt16LE(cursor + 28);
      const extraLength = central.readUInt16LE(cursor + 30);
      const commentLength = central.readUInt16LE(cursor + 32);
      const localOffset = central.readUInt32LE(cursor + 42);
      const name = safeArchiveName(central.subarray(cursor + 46, cursor + 46 + nameLength).toString('utf8'));
      if (name === wantedName) {
        if ((method !== 0 && method !== 8) || size > MAX_PLAYLIST_ENTRY_BYTES || compressedSize > MAX_PLAYLIST_ARCHIVE_BYTES || localOffset + 30 > stat.size) {
          throw new Error(`The .2kplaylist entry ${name} is unsupported or oversized.`);
        }
        const local = Buffer.alloc(30);
        await handle.read(local, 0, 30, base + localOffset);
        if (local.readUInt32LE(0) !== 0x04034b50) throw new Error('The .2kplaylist local header is invalid.');
        const dataOffset = localOffset + 30 + local.readUInt16LE(26) + local.readUInt16LE(28);
        if (dataOffset + compressedSize > stat.size) throw new Error(`The .2kplaylist entry ${name} is truncated.`);
        const packed = Buffer.alloc(compressedSize);
        await handle.read(packed, 0, compressedSize, base + dataOffset);
        const data = method === 0 ? packed : zlib.inflateRawSync(packed, { maxOutputLength: size });
        if (data.length !== size || crc32(data) !== expectedCrc) throw new Error(`The .2kplaylist entry ${name} failed validation.`);
        return data;
      }
      cursor += 46 + nameLength + extraLength + commentLength;
    }
    throw new Error(`The .2kplaylist does not contain ${wantedName}.`);
  } finally {
    await handle.close();
  }
}

function trackKey(track) {
  return `${Number(track.Collection || 0)}:${Number(track['Bank Index'] || 0)}:${Number(track['Unique Id'] || 0)}`;
}

function isCustomTrack(track) {
  return Number(track.Collection || 0) === 1 && Number(track['Bank Index'] || 0) > 71;
}

function audioLogicalPath(bankIndex) {
  return `newaudio/jukeboxmusic/${String(bankIndex).padStart(6, '0')}-000000-000000-000000-000000-000000.wav`;
}

function cleanText(value, label, maximum = 120) {
  const text = String(value ?? '').trim();
  if (!text || text.length > maximum || /[\u0000-\u001f]/.test(text)) throw new Error(`${label} must be 1–${maximum} printable characters.`);
  return text;
}

function validatePcmWave(buffer) {
  if(require('./native-opus').isNativeOpus(buffer)){
    const info=require('./native-opus').parse(buffer);
    return {encoding:'native-opus',channels:2,sampleRate:48000,bitsPerSample:16,dataBytes:info.samples*4};
  }
  if (!Buffer.isBuffer(buffer) || buffer.length < 44 || buffer.subarray(0, 4).toString('ascii') !== 'RIFF'
    || buffer.subarray(8, 12).toString('ascii') !== 'WAVE') throw new Error('Choose a valid WAV file.');
  let cursor = 12;
  let format = null;
  let dataBytes = 0;
  while (cursor + 8 <= buffer.length) {
    const id = buffer.subarray(cursor, cursor + 4).toString('ascii');
    const size = buffer.readUInt32LE(cursor + 4);
    const start = cursor + 8;
    if (start + size > buffer.length) throw new Error('The WAV file is truncated.');
    if (id === 'fmt ' && size >= 16) {
      format = {
        encoding: buffer.readUInt16LE(start),
        channels: buffer.readUInt16LE(start + 2),
        sampleRate: buffer.readUInt32LE(start + 4),
        bitsPerSample: buffer.readUInt16LE(start + 14),
      };
    }
    if (id === 'data') dataBytes += size;
    cursor = start + size + (size % 2);
  }
  if (!format || !dataBytes) throw new Error('The WAV file has no playable PCM data.');
  if (format.encoding !== 1 || format.channels !== 2 || format.sampleRate !== 48000 || format.bitsPerSample !== 16) {
    throw new Error('This first importer accepts PCM WAV: 48 kHz, stereo, 16-bit.');
  }
  return { ...format, dataBytes };
}

async function readJson(filePath) {
  try { return JSON.parse(await fsp.readFile(filePath, 'utf8')); }
  catch (error) {
    if (error.code === 'ENOENT') return null;
    throw error;
  }
}

async function findManifestEntry(gameRoot, logicalPath) {
  const entries = await findManifestEntries(gameRoot, [logicalPath]);
  return entries.get(logicalPath.replaceAll('\\', '/').toLowerCase());
}

async function findManifestEntries(gameRoot, logicalPaths) {
  const manifestPath = path.join(path.resolve(gameRoot), 'manifest');
  const wanted = new Map(logicalPaths.map((logicalPath) => [logicalPath.replaceAll('\\', '/').toLowerCase(), logicalPath]));
  if (!wanted.size) return new Map();
  const text = await fsp.readFile(manifestPath, 'utf8');
  const found = new Map();
  for (const line of text.split(/\r?\n/)) {
    const [name, archive, offsetText, sizeText] = line.split(',');
    const normalized = name?.replaceAll('\\', '/').toLowerCase();
    if (!wanted.has(normalized)) continue;
    const offset = Number(offsetText);
    const size = Number(sizeText);
    if (!/^[0-9A-Z]{2}$/i.test(archive) || !Number.isSafeInteger(offset) || offset < 0
      || !Number.isSafeInteger(size) || size < 1 || size > MAX_IFF_BYTES) {
      throw new Error(`The stock ${wanted.get(normalized)} manifest entry is invalid.`);
    }
    found.set(normalized, { name, archive, offset, size });
    if (found.size === wanted.size) break;
  }
  for (const [normalized, logicalPath] of wanted) if (!found.has(normalized)) {
    throw new Error(`The stock ${logicalPath} entry was not found in the game manifest.`);
  }
  return found;
}

async function findCoverManifestEntry(gameRoot, coverName) {
  const safeName = String(coverName || '').toLowerCase();
  if (!/^[a-z0-9_]{1,120}$/.test(safeName)) throw new Error('The selected stock cover name is unsafe.');
  const manifestPath = path.join(path.resolve(gameRoot), 'manifest');
  const text = await fsp.readFile(manifestPath, 'utf8');
  const suffix = new RegExp(`^shared/[0-9a-f]{2}/${safeName}\\.[0-9a-f]{16}\\.tld$`, 'i');
  for (const line of text.split(/\r?\n/)) {
    const [name, archive, offsetText, sizeText] = line.split(',');
    if (!suffix.test(name?.replaceAll('\\', '/') || '')) continue;
    return { name: name.replaceAll('\\', '/'), archive, offset: Number(offsetText), size: Number(sizeText) };
  }
  throw new Error(`The stock cover resource for ${coverName} was not found in the game manifest.`);
}

function buildTldFromDds(dds) {
  if (!Buffer.isBuffer(dds) || dds.length < 128 || dds.subarray(0, 4).toString('ascii') !== 'DDS '
    || dds.subarray(84, 88).toString('ascii') !== 'DXT1') {
    throw new Error('The artwork converter did not produce a BC1 DDS texture.');
  }
  const width = dds.readUInt32LE(16);
  const height = dds.readUInt32LE(12);
  const mipCount = Math.max(1, dds.readUInt32LE(28));
  if (width !== 256 || height !== 256 || mipCount !== 1) {
    throw new Error('The artwork converter did not produce the required 256 × 256 single-mip texture.');
  }
  const payload = dds.subarray(128);
  if (payload.length !== 32768) throw new Error('The converted cover payload has an unexpected size.');
  const tld = Buffer.alloc(32 + payload.length);
  tld.write('TLD ', 0, 'ascii');
  tld[8] = 5;
  tld[9] = 33;
  tld[10] = 71;
  tld[11] = 1;
  tld.writeUInt32LE(31, 12);
  tld.writeUInt16LE(256, 16);
  tld.writeUInt16LE(256, 18);
  tld.writeUInt16LE(1, 20);
  tld.writeUInt16LE(1, 22);
  tld.writeUInt32LE(payload.length, 24);
  tld.writeUInt32LE(payload.length, 28);
  payload.copy(tld, 32);
  return tld;
}

function validateTldArtwork(tld) {
  if (!Buffer.isBuffer(tld) || tld.length !== 32800 || tld.subarray(0, 4).toString('ascii') !== 'TLD '
    || tld[10] !== 71 || tld[11] !== 1 || tld.readUInt16LE(16) !== 256 || tld.readUInt16LE(18) !== 256
    || tld.readUInt32LE(24) !== 32768 || tld.readUInt32LE(28) !== 32768) {
    throw new Error('The playlist artwork is not a valid 256 × 256 BC1 TLD texture.');
  }
  return tld;
}

async function atomicCopy(source, destination) {
  await fsp.mkdir(path.dirname(destination), { recursive: true });
  const temporary = `${destination}.${process.pid}.${Date.now()}.tmp`;
  await fsp.copyFile(source, temporary);
  try { await fsp.rename(temporary, destination); }
  catch (error) {
    throw error;
  } finally {
    await fsp.rm(temporary, { force: true });
  }
}

async function atomicWrite(destination, bytes) {
  await fsp.mkdir(path.dirname(destination), { recursive: true });
  const temporary = `${destination}.${process.pid}.${Date.now()}.tmp`;
  await fsp.writeFile(temporary, bytes);
  try { await fsp.rename(temporary, destination); }
  finally { await fsp.rm(temporary, { force: true }); }
}

class JukeboxManager {
  constructor({
    statePath, decoderPath = null, texconvPath = null, decodeEntry = null, convertArtwork = null,
    bundlePartBytes = DEFAULT_PLAYLIST_PART_BYTES, flacPath = null,
  }) {
    this.statePath = path.resolve(statePath);
    this.audioCodec=new FlacAudio(flacPath||undefined);this.flacStorage=Boolean(flacPath);
    this.decoderPath = decoderPath ? path.resolve(decoderPath) : null;
    this.defaultBackupPath = path.join(path.dirname(this.statePath), 'default-music.iff');
    this.playlistFolder = path.join(path.dirname(this.statePath), 'playlists');
    this.artworkPreviewFolder = path.join(path.dirname(this.statePath), 'artwork-previews');
    this.bundlePartBytes = Math.min(
      MAX_PLAYLIST_ARCHIVE_BYTES - (1024 * 1024),
      Math.max(64 * 1024, Number(bundlePartBytes) || DEFAULT_PLAYLIST_PART_BYTES),
    );
    this.texconvPath = texconvPath ? path.resolve(texconvPath) : null;
    this.decodeEntry = decodeEntry || (async ({ archivePath, offset, size, oodlePath, outputPath }) => {
      await execFileAsync(this.decoderPath, [archivePath, String(offset), String(size), oodlePath, outputPath], {
        windowsHide: true,
        timeout: 120000,
        maxBuffer: 1024 * 1024,
      });
    });
    this.convertArtwork = convertArtwork || (async ({ pngPath, outputDirectory }) => {
      if (!this.texconvPath) throw new Error('The artwork texture converter is not configured.');
      await execFileAsync(this.texconvPath, [
        '-nologo', '-y', '-w', '256', '-h', '256', '-f', 'BC1_UNORM', '-m', '1', '-o', outputDirectory, pngPath,
      ], { windowsHide: true, timeout: 120000, maxBuffer: 4 * 1024 * 1024 });
      return path.join(outputDirectory, `${path.basename(pngPath, path.extname(pngPath))}.DDS`);
    });
  }

  async ensurePlaylistFolder() {
    await fsp.mkdir(this.playlistFolder, { recursive: true });
    return this.playlistFolder;
  }

  musicPath(modsRoot) { return path.join(path.resolve(modsRoot), 'music.iff'); }
  audioPath(modsRoot, bankIndex) { return path.join(path.resolve(modsRoot), ...audioLogicalPath(bankIndex).split('/')); }
  archivedAssetAvailable() { return false; }

  playlistBundlePath(playlist) {
    const safeName = String(playlist.name || 'Playlist').replace(/[^a-z0-9 _.-]+/gi, '').trim().replace(/[ .]+$/g, '') || 'Playlist';
    return path.join(this.playlistFolder, `${safeName.slice(0, 80)}-${String(playlist.id).slice(0, 8)}.2kplaylist`);
  }

  playlistBundlePartPath(playlist, part) {
    const first = this.playlistBundlePath(playlist);
    if (part === 1) return first;
    return first.replace(/\.2kplaylist$/i, `.part${String(part).padStart(3, '0')}.2kplaylist`);
  }

  artworkPreviewPath(modsRoot, bankIndex) {
    const profile = crypto.createHash('sha256').update(path.resolve(modsRoot)).digest('hex').slice(0, 16);
    return path.join(this.artworkPreviewFolder, profile, `${Number(bankIndex)}.png`);
  }

  async recoverArtworkRecord({ modsRoot, gameRoot, bankIndex, state }) {
    const root = path.resolve(modsRoot);
    const previewPath = this.artworkPreviewPath(root, bankIndex);
    const previewStat = await fsp.stat(previewPath).catch(() => null);
    if (!previewStat?.isFile()) return null;
    const track = state.music.Tracks.find((item) => Number(item['Bank Index'] || 0) === Number(bankIndex));
    const coverName = String(track?.['Cover Name'] || '');
    if (!coverName) return null;
    const privateTld=previewPath.replace(/\.png$/i,'.tld');
    if((await fsp.stat(privateTld).catch(()=>null))?.isFile()){
      const record={modsRoot:root,bankIndex:Number(bankIndex),coverName,logicalPath:`library-artwork/${Number(bankIndex)}.tld`,absolutePath:privateTld,previewPath,storage:'library-v2'};
      state.artworkOverrides.push(record);return record;
    }
    const entry = await findCoverManifestEntry(gameRoot, coverName).catch(() => null);
    if (!entry) return null;
    const logicalPath = entry.name;
    const absolutePath = path.resolve(root, ...logicalPath.split('/'));
    if (!absolutePath.startsWith(`${root}${path.sep}`)) return null;
    const artworkStat = await fsp.stat(absolutePath).catch(() => null);
    if (!artworkStat?.isFile()) return null;
    const record = { modsRoot: root, bankIndex: Number(bankIndex), coverName, logicalPath, absolutePath, previewPath };
    state.artworkOverrides.push(record);
    return record;
  }

  async selectAvailableArtworkSlot({ modsRoot, gameRoot, state, bankIndex }) {
    const root = path.resolve(modsRoot);
    const used = new Set(state.artworkOverrides
      .filter((item) => path.resolve(item.modsRoot || '') === root)
      .map((item) => String(item.coverName || '').toLowerCase()));
    const stockCovers = state.music.Tracks
      .filter((track) => Number(track.Collection || 0) === 1 && Number(track['Bank Index'] || 0) >= 1
        && Number(track['Bank Index'] || 0) <= 71 && String(track['Cover Name'] || ''))
      .sort((a, b) => Number(a['Bank Index']) - Number(b['Bank Index']));
    for (const donor of stockCovers) {
      const coverName = String(donor['Cover Name']);
      if (used.has(coverName.toLowerCase())) continue;
      const entry = await findCoverManifestEntry(gameRoot, coverName).catch(() => null);
      if (!entry) continue;
      const logicalPath = entry.name;
      const absolutePath = path.resolve(root, ...logicalPath.split('/'));
      if (!absolutePath.startsWith(`${root}${path.sep}`)) continue;
      const managed = state.artworkOverrides.some((item) => path.resolve(item.modsRoot || '') === root
        && String(item.logicalPath || '').toLowerCase() === logicalPath.toLowerCase());
      const occupied = Boolean(await fsp.stat(absolutePath).catch(() => null));
      if (occupied && !managed) continue;
      return { coverName, logicalPath, absolutePath };
    }
    // The library is not limited by the game's simultaneous cover slots.
    // Overflow covers remain private until the active playlist is prepared for Play.
    if(!Number.isInteger(bankIndex)||bankIndex<1)throw new Error('Invalid artwork identity.');
    const absolutePath=this.artworkPreviewPath(root,bankIndex).replace(/\.png$/i,'.tld');
    return {coverName:String(stockCovers[0]?.['Cover Name']||'001_helicopter'),
      logicalPath:`library-artwork/${bankIndex}.tld`,absolutePath,storage:'library-v2'};
  }

  async exportPlaylistBundle(modsRoot, state, playlist) {
    const warnings = [];
    const root = path.resolve(modsRoot);
    const tracksByKey = new Map(state.music.Tracks.map((track) => [trackKey(track), track]));
    const descriptors = [];
    for (let recordIndex = 0; recordIndex < playlist.trackKeys.length; recordIndex += 1) {
      const key = playlist.trackKeys[recordIndex];
      const track = tracksByKey.get(key);
      if (!track) { warnings.push(`Missing library track ${key}`); continue; }
      const bankIndex = Number(track['Bank Index'] || 0);
      const audioPath = this.audioPath(root, bankIndex);
      const audioStat = Number(track.Collection || 0) === 1 && bankIndex > 71
        ? await fsp.stat(audioPath).catch(() => null) : null;
      const record = {
        kind: audioStat?.isFile() ? 'custom' : 'stock-reference',
        sourceKey: key,
        metadata: {
          Artist: String(track.Artist || ''), Collection: Number(track.Collection || 0),
          'Cover Name': String(track['Cover Name'] || ''), Name: String(track.Name || ''),
          'Bank Index': bankIndex, 'Unique Id': Number(track['Unique Id'] || 0),
        },
      };
      const files = [];
      if (audioStat?.isFile()) {
        if (audioStat.size > MAX_PLAYLIST_ENTRY_BYTES) throw new Error(`Custom WAV for ${track.Name || key} is too large to share.`);
        record.audioFile = `audio/${String(recordIndex + 1).padStart(4, '0')}.wav`;
        files.push({ name: record.audioFile, path: audioPath, size: audioStat.size, kind: 'audio' });
        const artwork = state.artworkOverrides.find((item) => path.resolve(item.modsRoot || '') === root
          && Number(item.bankIndex) === bankIndex);
        if (artwork) {
          const tldStat = await fsp.stat(artwork.absolutePath).catch(() => null);
          if (tldStat?.isFile()) {
            record.artworkFile = `artwork/${String(recordIndex + 1).padStart(4, '0')}.tld`;
            files.push({ name: record.artworkFile, path: artwork.absolutePath, size: tldStat.size, kind: 'artwork' });
            const previewStat = artwork.previewPath ? await fsp.stat(artwork.previewPath).catch(() => null) : null;
            if (previewStat?.isFile()) {
              record.artworkPreviewFile = `artwork/${String(recordIndex + 1).padStart(4, '0')}.png`;
              files.push({ name: record.artworkPreviewFile, path: artwork.previewPath, size: previewStat.size, kind: 'preview' });
            }
          } else warnings.push(`Custom artwork for ${track.Name || key} is missing`);
        }
      } else if (Number(track.Collection || 0) === 1 && bankIndex > 71) {
        warnings.push(`Custom WAV for ${track.Name || key} is missing; saved as a reference only`);
      }
      descriptors.push({ record, files, bytes: files.reduce((sum, file) => sum + file.size, 0) });
    }

    const parts = [];
    let current = [];
    let currentBytes = 64 * 1024;
    let currentEntryCount = 1;
    for (const descriptor of descriptors) {
      if (current.length && (currentBytes + descriptor.bytes > this.bundlePartBytes
        || currentEntryCount + descriptor.files.length > MAX_PLAYLIST_ENTRIES)) {
        parts.push(current);
        current = [];
        currentBytes = 64 * 1024;
        currentEntryCount = 1;
      }
      current.push(descriptor);
      currentBytes += descriptor.bytes;
      currentEntryCount += descriptor.files.length;
    }
    if (current.length || !parts.length) parts.push(current);

    await this.ensurePlaylistFolder();
    const destinations = [this.playlistBundlePath(playlist)];
    let customFiles = 0;
    const codec=this.audioCodec,flacStorage=this.flacStorage,playlistSchema=this.opusEncoder?OPUS_PLAYLIST_SCHEMA:flacStorage?FLAC_PLAYLIST_SCHEMA:PLAYLIST_SCHEMA;
    async function* compressedSegments(){
    for (let partIndex = 0; partIndex < parts.length; partIndex += 1) {
      const partNumber = partIndex + 1;
      const partDescriptors = parts[partIndex];
      const manifest = {
        schema: playlistSchema,
        name: playlist.name,
        tracks: partDescriptors.map((item) => item.record),
        warnings: partNumber === 1 ? warnings : [],
        multipart: { id: playlist.id, part: partNumber, count: parts.length },
      };
      const entries = [];
      for (const descriptor of partDescriptors) {
        for (const file of descriptor.files) {
          let data = await fsp.readFile(file.path),member=file.name;
          if (file.kind === 'audio') {
            validatePcmWave(data);
            if(require('./native-opus').isNativeOpus(data)){
              manifest.schema=OPUS_PLAYLIST_SCHEMA;
              Object.assign(descriptor.record,{audioCodec:'native-opus',audioNativeSha256:crypto.createHash('sha256').update(data).digest('hex')});
            }else if(flacStorage){const encoded=await codec.encode(data);if(encoded){data=encoded.data;member=file.name.replace(/\.wav$/,'.flac');Object.assign(descriptor.record,encoded.metadata,{audioFile:member});}}
          }
          else if (file.kind === 'artwork') validateTldArtwork(data);
          else if (!data.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]))) {
            throw new Error(`Artwork preview for ${descriptor.record.metadata.Name} is not a valid PNG.`);
          }
          entries.push({ name: member, data });
          customFiles += 1;
        }
      }
      entries.unshift({ name:'playlist.json',data:Buffer.from(`${JSON.stringify(manifest,null,2)}\n`,'utf8') });
      yield buildZipArchive(entries);
    }
    }
    const combinedManifest={schema:playlistSchema,name:playlist.name,tracks:descriptors.map(item=>item.record),warnings};
    // Small playlists remain ordinary ZIPs; large playlists become one bounded-stream container.
    if(parts.length===1){for await(const bytes of compressedSegments())await atomicWrite(destinations[0],bytes);}
    else await writePlaylistContainer(destinations[0],combinedManifest,compressedSegments());
    const previousPaths = [...new Set([
      ...(Array.isArray(playlist.bundlePaths) ? playlist.bundlePaths : []),
      playlist.bundlePath,
    ].filter(Boolean).map((item) => path.resolve(item)))];
    const destinationSet = new Set(destinations.map((item) => path.resolve(item)));
    let retiredDirectory=null;
    for (const previousPath of previousPaths) {
      if (!destinationSet.has(previousPath) && previousPath.startsWith(`${path.resolve(this.playlistFolder)}${path.sep}`)) {
        if(await fsp.stat(previousPath).catch(()=>null)){
          if(!retiredDirectory){await fsp.mkdir(path.join(this.playlistFolder,'retired-parts'),{recursive:true});retiredDirectory=await fsp.mkdtemp(path.join(this.playlistFolder,'retired-parts','export-'));}
          await fsp.rename(previousPath,path.join(retiredDirectory,path.basename(previousPath)));
        }
      }
    }
    playlist.bundlePath = destinations[0];
    playlist.bundlePaths = destinations;
    playlist.bundleWarnings = warnings;
    return { path: destinations[0], paths: destinations, parts: destinations.length, customFiles, warnings };
  }

  async refreshPlaylistBundles(modsRoot, state, predicate = () => true) {
    const reports = [];
    for (const playlist of state.playlists.filter(predicate)) {
      try {
        reports.push(await this.exportPlaylistBundle(modsRoot, state, playlist));
      } catch (error) {
        if (error.code !== 'PLAYLIST_BUNDLE_TOO_LARGE') throw error;
        const previousPath = playlist.bundlePath ? path.resolve(playlist.bundlePath) : null;
        if (previousPath && previousPath.startsWith(`${path.resolve(this.playlistFolder)}${path.sep}`)) {
          await fsp.rm(previousPath, { force: true });
        }
        playlist.bundlePath = null;
        playlist.bundlePaths = [];
        playlist.bundleWarnings = [error.message];
        reports.push({ path: null, paths: [], parts: 0, customFiles: 0, warnings: [error.message], skipped: true });
      }
    }
    await this.writeState(state);
    return reports;
  }

  async recoverPlaylistRecords(state) {
    if (state.playlists.length) return 0;
    const files = await fsp.readdir(this.playlistFolder).catch(() => []);
    const firstParts = files.filter((name) => name.toLowerCase().endsWith('.2kplaylist')
      && !/\.part\d{3}\.2kplaylist$/i.test(name)).sort();
    const tracksByKey = new Map(state.music.Tracks.map((track) => [trackKey(track), track]));
    let recovered = 0;
    for (const file of firstParts) {
      try {
        const firstPath = path.join(this.playlistFolder, file);
        const first = JSON.parse((await readZipMemberFromFile(firstPath, 'playlist.json')).toString('utf8'));
        if (!supportedPlaylist(first?.schema) || !Array.isArray(first.tracks)) continue;
        const multipart = first.multipart;
        const count = multipart ? Number(multipart.count) : 1;
        const id = String(multipart?.id || crypto.randomUUID());
        if (!Number.isInteger(count) || count < 1 || count > MAX_TRACKS || Number(multipart?.part || 1) !== 1) continue;
        const base = firstPath.replace(/\.2kplaylist$/i, '');
        const bundlePaths = [];
        const records = [];
        const warnings = [];
        for (let part = 1; part <= count; part += 1) {
          const partPath = part === 1 ? firstPath : `${base}.part${String(part).padStart(3, '0')}.2kplaylist`;
          const manifest = part === 1
            ? first
            : JSON.parse((await readZipMemberFromFile(partPath, 'playlist.json')).toString('utf8'));
          if (!supportedPlaylist(manifest?.schema) || !Array.isArray(manifest.tracks)
            || (count > 1 && (String(manifest.multipart?.id || '') !== id
              || Number(manifest.multipart?.part) !== part || Number(manifest.multipart?.count) !== count))) {
            throw new Error(`Playlist part ${part} does not match ${file}.`);
          }
          bundlePaths.push(partPath);
          records.push(...manifest.tracks);
          if (part === 1 && Array.isArray(manifest.warnings)) warnings.push(...manifest.warnings.map(String));
        }
        const recoveredKeys = [];
        for (const record of records) {
          const metadata = record?.metadata || {};
          let existing = tracksByKey.get(String(record?.sourceKey || ''));
          if (!existing) existing = tracksByKey.get(trackKey(metadata));
          if (!existing) {
            existing = state.music.Tracks.find((track) => String(track.Name || '') === String(metadata.Name || '')
              && String(track.Artist || '') === String(metadata.Artist || ''));
          }
          if (existing) recoveredKeys.push(trackKey(existing));
          else warnings.push(`Could not relink ${metadata.Artist || 'Unknown'} - ${metadata.Name || 'Unknown'}`);
        }
        const trackKeys = [...new Set(recoveredKeys)];
        if (!trackKeys.length) continue;
        state.playlists.push({
          id,
          name: cleanText(first.name || path.basename(file, '.2kplaylist'), 'Playlist name', 60),
          trackKeys,
          bundlePath: bundlePaths[0],
          bundlePaths,
          bundleWarnings: warnings,
          recoveredFromBundle: true,
        });
        recovered += 1;
      } catch {
        // A damaged or unrelated share file is left untouched and cannot block the library.
      }
    }
    return recovered;
  }

  async loadState(modsRoot) {
    const musicIff = this.musicPath(modsRoot);
    const bytes = await fsp.readFile(musicIff).catch((error) => {
      if (error.code === 'ENOENT') throw new Error('No music.iff is installed in the active profile.');
      throw error;
    });
    const currentMusic = readMusicJson(bytes);
    let state = await readJson(this.statePath);
    let stateChanged = false;
    if (!state || state.schema !== SCHEMA || !state.music || !Array.isArray(state.music.Tracks)) {
      state = {
        schema: SCHEMA, music: currentMusic, playlists: [], activePlaylistId: null, applied: false, artworkOverrides: [],
      };
      stateChanged = true;
    } else if (!state.applied && currentMusic.Tracks.length >= state.music.Tracks.length) {
      state.music = currentMusic;
      stateChanged = true;
    }
    state.playlists = Array.isArray(state.playlists) ? state.playlists : [];
    state.artworkOverrides = Array.isArray(state.artworkOverrides) ? state.artworkOverrides : [];
    if (await this.recoverPlaylistRecords(state)) stateChanged = true;
    if (!state.activePlaylistId && currentMusic.Tracks.some(isCustomTrack)) {
      await this.writeActiveMusic(state, musicIff);
      stateChanged = false;
    }
    if (stateChanged) await this.writeState(state);
    return { state, musicIff };
  }

  async writeState(state) {
    await fsp.mkdir(path.dirname(this.statePath), { recursive: true });
    const temporary = `${this.statePath}.${process.pid}.${crypto.randomUUID()}.tmp`;
    await fsp.writeFile(temporary, `${JSON.stringify(state, null, 2)}\n`, 'utf8');
    try {
      await fsp.rename(temporary, this.statePath);
    } catch (error) {
      if (!['EEXIST', 'EPERM'].includes(error.code)) throw error;
      await fsp.copyFile(temporary, this.statePath);
    } finally {
      await fsp.rm(temporary, { force: true });
    }
  }

  async writeActiveMusic(state, musicIff) {
    const active = state.playlists.find((item) => item.id === state.activePlaylistId);
    const selected = active ? new Set(active.trackKeys) : null;
    const output = {
      ...state.music,
      Tracks: selected
        ? state.music.Tracks.filter((track) => selected.has(trackKey(track)))
        : state.music.Tracks.filter((track) => !isCustomTrack(track)),
    };
    if (!output.Tracks.length) throw new Error('A playlist must contain at least one track.');
    const built = buildMusicIff(output);
    readMusicJson(built);
    const backup = `${musicIff}.nba2k27-launcher.bak`;
    const backupStat = await fsp.stat(backup).catch(() => null);
    if (!backupStat) await fsp.copyFile(musicIff, backup);
    const temporary = `${musicIff}.${process.pid}.tmp`;
    await fsp.writeFile(temporary, built);
    try { await fsp.copyFile(temporary, musicIff); }
    finally { await fsp.rm(temporary, { force: true }); }
    state.applied = Boolean(active);
    await this.writeState(state);
  }

  viewFromState(state, musicIff) {
    const active = state.playlists.find((item) => item.id === state.activePlaylistId) || null;
    const activeKeys = active ? new Set(active.trackKeys) : null;
    const modsRoot = path.dirname(musicIff);
    const tracks = state.music.Tracks.map((track) => {
      const collection = Number(track.Collection || 0);
      const bankIndex = Number(track['Bank Index'] || 0);
      const logicalPath = audioLogicalPath(bankIndex);
      const customAudio = collection === 1 && bankIndex > 71;
      const previewFilePresent = customAudio && (fs.existsSync(this.audioPath(modsRoot, bankIndex)) || this.archivedAssetAvailable(state, modsRoot, bankIndex, 'audio'));
      const previewPath = this.artworkPreviewPath(modsRoot, bankIndex);
      const artworkManaged = state.artworkOverrides.some((item) => path.resolve(item.modsRoot || '') === modsRoot
        && Number(item.bankIndex) === bankIndex);
      const artworkPreviewAvailable = state.artworkOverrides.some((item) => path.resolve(item.modsRoot || '') === modsRoot
        && Number(item.bankIndex) === bankIndex && item.previewPath && fs.existsSync(item.previewPath))
        || fs.existsSync(previewPath) || this.archivedAssetAvailable(state, modsRoot, bankIndex, 'preview');
      return {
        key: trackKey(track),
        title: String(track.Name || ''),
        artist: String(track.Artist || ''),
        collection,
        bankIndex,
        uniqueId: Number(track['Unique Id'] || 0),
        coverName: String(track['Cover Name'] || ''),
        artworkManaged,
        artworkPreviewAvailable,
        selected: activeKeys ? activeKeys.has(trackKey(track)) : !isCustomTrack(track),
        previewAvailable: previewFilePresent,
        previewFilePresent,
        previewMissing: customAudio && !previewFilePresent,
      };
    });
    return {
      available: true,
      musicIff,
      tracks,
      trackCount: tracks.length,
      visibleTrackCount: tracks.filter((track) => track.selected).length,
      playlists: state.playlists.map(({ id, name, trackKeys, bundlePath, bundlePaths, bundleWarnings }) => ({
        id, name, trackCount: trackKeys.length, trackKeys: [...trackKeys],
        bundleName: bundlePath ? path.basename(bundlePath) : null,
        bundleParts: Array.isArray(bundlePaths) && bundlePaths.length ? bundlePaths.length : (bundlePath ? 1 : 0),
        bundleWarnings: Array.isArray(bundleWarnings) ? [...bundleWarnings] : [],
      })),
      activePlaylistId: active?.id || null,
      lastPlaylistId: state.playlists.find(item=>item.id===(state.lastPlaylistId || active?.id))?.id || null,
      activePlaylistName: active?.name || 'Default Playlist (Locked)',
      defaultBackupAvailable: fs.existsSync(this.defaultBackupPath),
      playlistFolder: this.playlistFolder,
    };
  }

  async view(modsRoot) {
    try {
      const { state, musicIff } = await this.loadState(modsRoot);
      return this.viewFromState(state, musicIff);
    } catch (error) {
      return { available: false, message: error.message, tracks: [], playlists: [], trackCount: 0, visibleTrackCount: 0 };
    }
  }

  async saveTrack(modsRoot, edit, options = {}) {
    const { state, musicIff } = await this.loadState(modsRoot);
    const track = state.music.Tracks.find((item) => trackKey(item) === edit?.key);
    if (!track) throw new Error('The selected jukebox track no longer exists.');
    let artwork = null;
    if (options.artworkBytes?.length) {
      artwork = await this.installArtworkOverride({
        modsRoot,
        gameRoot: options.gameRoot,
        bankIndex: Number(track['Bank Index'] || 0),
        artworkBytes: options.artworkBytes,
        state,
      });
    }
    track.Name = cleanText(edit.title, 'Title');
    track.Artist = cleanText(edit.artist, 'Artist');
    track['Cover Name'] = artwork?.coverName || cleanText(edit.coverName, 'Cover name', 160);
    await this.writeActiveMusic(state, musicIff);
    if (!options.deferBundle) {
      await this.refreshPlaylistBundles(modsRoot, state, (playlist) => playlist.trackKeys.includes(edit.key));
    }
    return {
      ...this.viewFromState(state, musicIff),
      editReport: { artwork: artwork ? { coverName: artwork.coverName, logicalPath: artwork.logicalPath } : null },
    };
  }

  async importWav(modsRoot, sourcePath) {
    const source = path.resolve(sourcePath || '');
    const sourceBytes = await fsp.readFile(source);
    return this.importWaveBytes(modsRoot, sourceBytes, path.basename(source));
  }

  async repairMissingCustomTracks(modsRoot) {
    const { state, musicIff } = await this.loadState(modsRoot);
    const root = path.resolve(modsRoot);
    const missingTracks = state.music.Tracks.filter((track) => {
      const bankIndex = Number(track['Bank Index'] || 0);
      return Number(track.Collection || 0) === 1 && bankIndex > 71
        && !fs.existsSync(this.audioPath(root, bankIndex)) && !this.archivedAssetAvailable(state, root, bankIndex, 'audio');
    });
    if (!missingTracks.length) return { ...this.viewFromState(state, musicIff), repairReport: { removed: 0 } };
    const missingKeys = new Set(missingTracks.map(trackKey));
    const missingBanks = new Set(missingTracks.map((track) => Number(track['Bank Index'] || 0)));
    state.music.Tracks = state.music.Tracks.filter((track) => !missingKeys.has(trackKey(track)));
    for (const playlist of state.playlists) playlist.trackKeys = playlist.trackKeys.filter((key) => !missingKeys.has(key));
    const removedArtwork = state.artworkOverrides.filter((item) => path.resolve(item.modsRoot || '') === root
      && missingBanks.has(Number(item.bankIndex)));
    state.artworkOverrides = state.artworkOverrides.filter((item) => !removedArtwork.includes(item));
    for (const artwork of removedArtwork) {
      const target = path.resolve(artwork.absolutePath || '');
      if (target.startsWith(`${root}${path.sep}`)) await fsp.rm(target, { force: true }).catch(() => {});
      const previewRoot = path.resolve(this.artworkPreviewFolder);
      const preview = path.resolve(artwork.previewPath || '');
      if (preview.startsWith(`${previewRoot}${path.sep}`)) await fsp.rm(preview, { force: true }).catch(() => {});
    }
    await this.writeActiveMusic(state, musicIff);
    await this.refreshPlaylistBundles(modsRoot, state);
    return { ...this.viewFromState(state, musicIff), repairReport: { removed: missingTracks.length } };
  }

  async importWaveBytes(modsRoot, inputBytes, sourceName = 'Custom Song.wav', options = {}) {
    const sourceBytes = Buffer.isBuffer(inputBytes) ? inputBytes : Buffer.from(inputBytes || []);
    const wave = validatePcmWave(sourceBytes);
    const { state, musicIff } = await this.loadState(modsRoot);
    const originalState = structuredClone(state);
    const usedBanks = new Set(state.music.Tracks.filter((track) => Number(track.Collection || 0) === 1)
      .map((track) => Number(track['Bank Index'] || 0)));
    let bankIndex = 72;
    while (usedBanks.has(bankIndex) && bankIndex < 999) bankIndex += 1;
    if (bankIndex >= 999) throw new Error('No safe custom soundtrack bank index is available.');
    const usedIds = new Set(state.music.Tracks.map((track) => Number(track['Unique Id'] || 0)));
    let uniqueId = 130;
    while (usedIds.has(uniqueId) && uniqueId < 65535) uniqueId += 1;
    if (uniqueId >= 65535) throw new Error('No safe custom soundtrack unique ID is available.');
    const safeSourceName = path.basename(String(sourceName || 'Custom Song.wav'));
    const fallbackTitle = cleanText(path.basename(safeSourceName, path.extname(safeSourceName)).replace(/[_-]+/g, ' '), 'Title');
    const title = options.title ? cleanText(options.title, 'Title') : fallbackTitle;
    const artist = options.artist ? cleanText(options.artist, 'Artist') : 'Custom';
    const playlistWasSpecified = Object.prototype.hasOwnProperty.call(options, 'playlistId');
    const targetPlaylist = options.playlistId
      ? state.playlists.find((item) => item.id === options.playlistId)
      : playlistWasSpecified ? null : state.playlists.find((item) => item.id === state.activePlaylistId);
    if (options.playlistId && !targetPlaylist) throw new Error('The selected playlist no longer exists.');
    const logicalPath = audioLogicalPath(bankIndex);
    const audioName = path.basename(logicalPath);
    const destination = this.audioPath(modsRoot, bankIndex);
    const audioDirectory = path.dirname(destination);
    if (await fsp.stat(destination).catch(() => null)) throw new Error(`Custom audio bank ${bankIndex} already exists and was left untouched.`);
    let artwork = null;
    if (options.artworkBytes?.length) {
      artwork = await this.installArtworkOverride({
        modsRoot,
        gameRoot: options.gameRoot,
        bankIndex,
        artworkBytes: options.artworkBytes,
        state,
      });
    }
    const track = {
      Artist: artist,
      Collection: 1,
      'Cover Name': artwork?.coverName || '001_helicopter',
      Name: title,
      'Bank Index': bankIndex,
      'Unique Id': uniqueId,
    };
    state.music.Tracks.push(track);
    if (targetPlaylist) targetPlaylist.trackKeys.push(trackKey(track));
    await fsp.mkdir(audioDirectory, { recursive: true });
    await fsp.writeFile(destination, sourceBytes);
    try {
      await this.writeActiveMusic(state, musicIff);
      if (targetPlaylist && !options.deferBundle) {
        await this.refreshPlaylistBundles(modsRoot, state, (playlist) => playlist.id === targetPlaylist.id);
      }
    } catch (error) {
      await fsp.rm(destination, { force: true });
      if (artwork) {
        await fsp.rm(artwork.absolutePath, { force: true }).catch(() => {});
        if (artwork.previewPath) await fsp.rm(artwork.previewPath, { force: true }).catch(() => {});
        state.artworkOverrides = state.artworkOverrides.filter((item) => item.logicalPath !== artwork.logicalPath);
      }
      await this.writeActiveMusic(originalState, musicIff).catch(() => {});
      throw error;
    }
    return {
      ...this.viewFromState(state, musicIff),
      importReport: {
        title,
        bankIndex,
        uniqueId,
        logicalPath,
        bytes: sourceBytes.length,
        format: wave,
        artwork: artwork ? { coverName: artwork.coverName, logicalPath: artwork.logicalPath } : null,
      },
    };
  }

  async installArtworkOverride({ modsRoot, gameRoot, bankIndex, artworkBytes, state }) {
    const bytes = Buffer.from(artworkBytes || []);
    if (bytes.length < 16 || bytes.length > 20 * 1024 * 1024
      || !bytes.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]))) {
      throw new Error('The prepared cover artwork is not a valid PNG.');
    }
    const root = path.resolve(modsRoot);
    const previewPath = this.artworkPreviewPath(root, bankIndex);
    let existing = state.artworkOverrides.find((item) => path.resolve(item.modsRoot || '') === root
      && Number(item.bankIndex) === Number(bankIndex));
    if (!existing) existing = await this.recoverArtworkRecord({ modsRoot: root, gameRoot, bankIndex, state });
    if (existing) {
      const logicalPath = safeArchiveName(existing.logicalPath);
      const absolutePath = existing.storage==='library-v2'
        ? this.artworkPreviewPath(root,bankIndex).replace(/\.png$/i,'.tld') : path.resolve(root, ...logicalPath.split('/'));
      if (existing.storage!=='library-v2'&&!absolutePath.startsWith(`${root}${path.sep}`)) throw new Error('The existing artwork path is outside the active mod profile.');
      const tempDirectory = path.join(path.dirname(this.statePath), `artwork-${process.pid}-${Date.now()}`);
      const pngPath = path.join(tempDirectory, 'cover.png');
      await fsp.mkdir(tempDirectory, { recursive: true });
      try {
        await fsp.writeFile(pngPath, bytes);
        const ddsPath = await this.convertArtwork({ pngPath, outputDirectory: tempDirectory });
        const tld = buildTldFromDds(await fsp.readFile(ddsPath));
        await atomicWrite(absolutePath, tld);
        await atomicWrite(previewPath, bytes);
      } finally {
        await fsp.rm(tempDirectory, { recursive: true, force: true });
      }
      existing.absolutePath = absolutePath;
      existing.previewPath = previewPath;
      return existing;
    }
    const { coverName, logicalPath, absolutePath, storage } = await this.selectAvailableArtworkSlot({
      modsRoot: root, gameRoot, state, bankIndex,
    });
    const tempDirectory = path.join(path.dirname(this.statePath), `artwork-${process.pid}-${Date.now()}`);
    const pngPath = path.join(tempDirectory, 'cover.png');
    await fsp.mkdir(tempDirectory, { recursive: true });
    try {
      await fsp.writeFile(pngPath, bytes);
      const ddsPath = await this.convertArtwork({ pngPath, outputDirectory: tempDirectory });
      const tld = buildTldFromDds(await fsp.readFile(ddsPath));
      await atomicWrite(absolutePath, tld);
      await atomicWrite(previewPath, bytes);
    } finally {
      await fsp.rm(tempDirectory, { recursive: true, force: true });
    }
    const record = { modsRoot: root, bankIndex, coverName, logicalPath, absolutePath, previewPath, ...(storage?{storage}: {}) };
    state.artworkOverrides.push(record);
    return record;
  }

  async installTldArtworkOverride({ modsRoot, gameRoot, bankIndex, tldBytes, previewBytes = null, state }) {
    const tld = validateTldArtwork(Buffer.from(tldBytes || []));
    const root = path.resolve(modsRoot);
    const { coverName, logicalPath, absolutePath, storage } = await this.selectAvailableArtworkSlot({
      modsRoot: root, gameRoot, state, bankIndex,
    });
    let previewPath = null;
    const preview = Buffer.from(previewBytes || []);
    if (preview.length) {
      if (preview.length > 20 * 1024 * 1024
        || !preview.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]))) {
        throw new Error('The playlist artwork preview is not a valid PNG.');
      }
      previewPath = this.artworkPreviewPath(root, bankIndex);
    }
    await atomicWrite(absolutePath, tld);
    if (previewPath) await atomicWrite(previewPath, preview);
    const record = { modsRoot: root, bankIndex, coverName, logicalPath, absolutePath, previewPath, ...(storage?{storage}: {}) };
    state.artworkOverrides.push(record);
    return record;
  }

  async previewArtwork(modsRoot, key) {
    const { state } = await this.loadState(modsRoot);
    const track = state.music.Tracks.find((item) => trackKey(item) === key);
    if (!track) throw new Error('The selected jukebox track no longer exists.');
    const root = path.resolve(modsRoot);
    const artwork = state.artworkOverrides.find((item) => path.resolve(item.modsRoot || '') === root
      && Number(item.bankIndex) === Number(track['Bank Index'] || 0));
    const savedPreviewPath = artwork?.previewPath || this.artworkPreviewPath(root, Number(track['Bank Index'] || 0));
    if (!savedPreviewPath) throw new Error('No saved artwork preview is available for this song.');
    const previewRoot = path.resolve(this.artworkPreviewFolder);
    const previewPath = path.resolve(savedPreviewPath);
    if (!previewPath.startsWith(`${previewRoot}${path.sep}`)) throw new Error('The saved artwork preview path is invalid.');
    const bytes = await fsp.readFile(previewPath);
    if (bytes.length > 20 * 1024 * 1024
      || !bytes.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]))) {
      throw new Error('The saved artwork preview is invalid.');
    }
    return { bytes, name: path.basename(previewPath), coverName: artwork?.coverName || String(track['Cover Name'] || '') };
  }

  async previewTrack(modsRoot, key) {
    const { state } = await this.loadState(modsRoot);
    const track = state.music.Tracks.find((item) => trackKey(item) === key);
    if (!track) throw new Error('The selected jukebox track no longer exists.');
    const collection = Number(track.Collection || 0);
    const bankIndex = Number(track['Bank Index'] || 0);
    if (collection !== 1 || bankIndex <= 71) {
      throw new Error('Stock soundtrack preview is not available because its audio is stored inside the game bank.');
    }
    const logicalPath = audioLogicalPath(bankIndex);
    const audioPath = this.audioPath(modsRoot, bankIndex);
    const bytes = await fsp.readFile(audioPath).catch((error) => {
      if (error.code === 'ENOENT') throw new Error('The custom WAV file is missing from the active profile.');
      throw error;
    });
    validatePcmWave(bytes);
    if(require('./native-opus').isNativeOpus(bytes)){
      const previewPath=audioPath+'.ogg';await atomicWrite(previewPath,require('./native-opus').toOgg(bytes));
      return {path:previewPath,logicalPath,title:String(track.Name||''),artist:String(track.Artist||'')};
    }
    return { path: audioPath, logicalPath, title: String(track.Name || ''), artist: String(track.Artist || '') };
  }

  async restoreDefault(modsRoot, gameRoot) {
    const musicIff = this.musicPath(modsRoot);
    let source = 'saved stock backup';
    let stockMusic = null;
    let extractedPath = null;
    try {
      if (!this.decoderPath) throw new Error('The stock archive decoder is not configured.');
      const entry = await findManifestEntry(gameRoot, 'music.iff');
      extractedPath = `${this.defaultBackupPath}.${process.pid}.decoded`;
      await fsp.mkdir(path.dirname(this.defaultBackupPath), { recursive: true });
      const archivePath = path.join(path.resolve(gameRoot), entry.archive);
      const oodlePath = path.join(path.resolve(gameRoot), 'data', 'oodle', 'oo2core_9_win64.dll');
      await this.decodeEntry({
        archivePath,
        offset: entry.offset,
        size: entry.size,
        oodlePath,
        outputPath: extractedPath,
      });
      const bytes = await fsp.readFile(extractedPath);
      stockMusic = readMusicJson(bytes);
      await atomicCopy(extractedPath, this.defaultBackupPath);
      source = 'NBA 2K27 stock manifest';
    } catch (manifestError) {
      const cached = await fsp.readFile(this.defaultBackupPath).catch(() => null);
      if (!cached) throw manifestError;
      stockMusic = readMusicJson(cached);
    } finally {
      if (extractedPath) await fsp.rm(extractedPath, { force: true }).catch(() => {});
    }
    await atomicCopy(this.defaultBackupPath, musicIff);
    const previous = await readJson(this.statePath);
    const root = path.resolve(modsRoot);
    const artworkOverrides = Array.isArray(previous?.artworkOverrides) ? previous.artworkOverrides : [];
    for (const artwork of artworkOverrides.filter((item) => path.resolve(item.modsRoot || '') === root)) {
      const target = path.resolve(artwork.absolutePath || '');
      if (target.startsWith(`${root}${path.sep}`)) await fsp.rm(target, { force: true });
      const previewRoot = path.resolve(this.artworkPreviewFolder);
      const preview = path.resolve(artwork.previewPath || '');
      if (preview.startsWith(`${previewRoot}${path.sep}`)) await fsp.rm(preview, { force: true });
    }
    const state = {
      schema: SCHEMA,
      music: stockMusic,
      playlists: Array.isArray(previous?.playlists) ? previous.playlists : [],
      activePlaylistId: null,
      applied: false,
      artworkOverrides: artworkOverrides.filter((item) => path.resolve(item.modsRoot || '') !== root),
    };
    await this.writeState(state);
    return {
      ...this.viewFromState(state, musicIff),
      restoreReport: { source, trackCount: stockMusic.Tracks.length, backupPath: this.defaultBackupPath },
    };
  }

  async savePlaylist(modsRoot, payload) {
    const { state, musicIff } = await this.loadState(modsRoot);
    const name = cleanText(payload?.name, 'Playlist name', 60);
    const validKeys = new Set(state.music.Tracks.map(trackKey));
    const trackKeys = [...new Set(Array.isArray(payload?.trackKeys) ? payload.trackKeys : [])].filter((key) => validKeys.has(key));
    const id = String(payload?.id || crypto.randomUUID());
    const existing = state.playlists.find((item) => item.id === id);
    if (existing) Object.assign(existing, { name, trackKeys });
    else state.playlists.push({ id, name, trackKeys });
    if (payload?.deferBundle) {
      await this.writeState(state);
      return { ...this.viewFromState(state, musicIff), savedPlaylistId: id, bundleReport: null };
    }
    const [bundleReport] = await this.refreshPlaylistBundles(modsRoot, state, (item) => item.id === id);
    return { ...this.viewFromState(state, musicIff), savedPlaylistId: id, bundleReport };
  }

  async refreshPlaylistBundle(modsRoot, id) {
    const { state } = await this.loadState(modsRoot);
    const snapshot = state.playlists.find((item) => item.id === id);
    if (!snapshot) throw new Error('The selected playlist no longer exists.');
    const bundleReport = await this.exportPlaylistBundle(modsRoot, state, snapshot);
    const { state: latest, musicIff } = await this.loadState(modsRoot);
    const playlist = latest.playlists.find((item) => item.id === id);
    if (!playlist) throw new Error('The selected playlist was deleted while its share files were updating.');
    playlist.bundlePath = snapshot.bundlePath;
    playlist.bundlePaths = snapshot.bundlePaths;
    playlist.bundleWarnings = snapshot.bundleWarnings;
    await this.writeState(latest);
    return { ...this.viewFromState(latest, musicIff), savedPlaylistId: id, bundleReport };
  }

  async importPlaylistBundle(modsRoot, gameRoot, sourcePath) {
    const source = path.resolve(sourcePath || '');
    if (path.extname(source).toLowerCase() !== '.2kplaylist') throw new Error('Choose a .2kplaylist file.');
    const container=await openPlaylistContainer(source);
    const containerPaths=container?container.parts.map((_,i)=>i===0?source:`${source}#segment-${i}`):null;
    const readPart = async (partPath) => {
      const entries = readZipArchive(container?await readPlaylistSegment(container,containerPaths.indexOf(partPath)):await fsp.readFile(partPath));
      const manifestBytes = entries.get('playlist.json');
      if (!manifestBytes) throw new Error(`Playlist part ${path.basename(partPath)} has no playlist.json manifest.`);
      const manifest = JSON.parse(manifestBytes.toString('utf8'));
      if (!supportedPlaylist(manifest?.schema) || !Array.isArray(manifest.tracks) || manifest.tracks.length > MAX_TRACKS) {
        throw new Error(`Playlist part ${path.basename(partPath)} is not supported.`);
      }
      return { entries, manifest };
    };
    let selectedPart = await readPart(source);
    const selectedMultipart = container?null:selectedPart.manifest.multipart;
    let partPaths = containerPaths||[source];
    if (selectedMultipart) {
      const count = Number(selectedMultipart.count);
      const selectedNumber = Number(selectedMultipart.part);
      if (!Number.isInteger(count) || count < 1 || count > MAX_TRACKS
        || !Number.isInteger(selectedNumber) || selectedNumber < 1 || selectedNumber > count
        || !String(selectedMultipart.id || '')) throw new Error('The multipart playlist header is invalid.');
      const base = source.replace(/(?:\.part\d{3})?\.2kplaylist$/i, '');
      partPaths = Array.from({ length: count }, (_unused, index) => (index === 0
        ? `${base}.2kplaylist`
        : `${base}.part${String(index + 1).padStart(3, '0')}.2kplaylist`));
      for (const partPath of partPaths) {
        const stat = await fsp.stat(partPath).catch(() => null);
        if (!stat?.isFile()) throw new Error(`This shared playlist is incomplete. Keep all ${count} numbered .2kplaylist files together.`);
      }
    }
    const firstPath = partPaths[0];
    if (source !== firstPath) selectedPart = null;
    const firstPart = source === firstPath ? selectedPart : await readPart(firstPath);
    const firstManifest = firstPart.manifest;
    if (partPaths.length > 1 && (!firstManifest.multipart
      || (!container&&String(firstManifest.multipart.id) !== String(selectedMultipart.id))
      || Number(firstManifest.multipart.count) !== partPaths.length
      || Number(firstManifest.multipart.part) !== 1)) {
      throw new Error('The first numbered .2kplaylist file does not match the selected multipart set.');
    }
    const { state, musicIff } = await this.loadState(modsRoot);
    const originalState = structuredClone(state);
    const createdFiles = [];
    const warnings = [...(Array.isArray(firstManifest.warnings) ? firstManifest.warnings : [])];
    const keys = [];
    const names = new Set(state.playlists.map((item) => item.name.toLowerCase()));
    const baseName = cleanText(firstManifest.name || path.basename(firstPath, '.2kplaylist'), 'Playlist name', 60);
    let name = baseName;
    let suffix = 2;
    while (names.has(name.toLowerCase())) name = `${baseName.slice(0, 54)} (${suffix++})`;
    let totalManifestTracks = 0;
    try {
      for (let partIndex = 0; partIndex < partPaths.length; partIndex += 1) {
        const part = partIndex === 0 ? firstPart : await readPart(partPaths[partIndex]);
        const { entries, manifest } = part;
        const multipart = manifest.multipart;
        if (partPaths.length > 1 && (!multipart
          || String(multipart.id) !== String(firstManifest.multipart.id)
          || Number(multipart.count) !== partPaths.length
          || Number(multipart.part) !== partIndex + 1
          || String(manifest.name) !== String(firstManifest.name))) {
          throw new Error(`Playlist part ${partIndex + 1} does not belong to this multipart set.`);
        }
        totalManifestTracks += manifest.tracks.length;
        if (totalManifestTracks > MAX_TRACKS) throw new Error('The multipart playlist contains too many songs.');
        for (const record of manifest.tracks) {
          const metadata = record?.metadata;
          if (!metadata || typeof metadata !== 'object') { warnings.push('Skipped a track with missing metadata'); continue; }
          if (record.kind !== 'custom') {
            const existing = state.music.Tracks.find((track) => trackKey(track) === String(record.sourceKey || ''));
            if (existing) keys.push(trackKey(existing));
            else warnings.push(`Stock track ${metadata.Name || record.sourceKey || 'unknown'} is unavailable in this library`);
            continue;
          }
          const packedAudio = entries.get(safeArchiveName(record.audioFile));
          if (!packedAudio) throw new Error(`Custom audio ${record.audioFile || '(missing)'} is absent from playlist part ${partIndex + 1}.`);
          const audio = await this.audioCodec.decode(packedAudio,record);
          validatePcmWave(audio);
          const usedBanks = new Set(state.music.Tracks.filter((track) => Number(track.Collection || 0) === 1)
            .map((track) => Number(track['Bank Index'] || 0)));
          let bankIndex = 72;
          while (usedBanks.has(bankIndex) && bankIndex < 999) bankIndex += 1;
          if (bankIndex >= 999) throw new Error('No safe custom soundtrack bank index is available.');
          const usedIds = new Set(state.music.Tracks.map((track) => Number(track['Unique Id'] || 0)));
          let uniqueId = 130;
          while (usedIds.has(uniqueId) && uniqueId < 65535) uniqueId += 1;
          if (uniqueId >= 65535) throw new Error('No safe custom soundtrack unique ID is available.');
          let artwork = null;
          if (record.artworkFile) {
            const tld = entries.get(safeArchiveName(record.artworkFile));
            if (!tld) throw new Error(`Custom artwork ${record.artworkFile} is absent from playlist part ${partIndex + 1}.`);
            const preview = record.artworkPreviewFile
              ? entries.get(safeArchiveName(record.artworkPreviewFile)) : null;
            if (record.artworkPreviewFile && !preview) throw new Error(`Artwork preview ${record.artworkPreviewFile} is absent from playlist part ${partIndex + 1}.`);
            artwork = await this.installTldArtworkOverride({
              modsRoot, gameRoot, bankIndex, tldBytes: tld, previewBytes: preview, state,
            });
            createdFiles.push(artwork.absolutePath);
            if (artwork.previewPath) createdFiles.push(artwork.previewPath);
          }
          const track = {
            Artist: cleanText(metadata.Artist || 'Custom', 'Artist'), Collection: 1,
            'Cover Name': artwork?.coverName || cleanText(metadata['Cover Name'] || '001_helicopter', 'Cover name', 160),
            Name: cleanText(metadata.Name || 'Custom Song', 'Title'),
            'Bank Index': bankIndex, 'Unique Id': uniqueId,
          };
          const destination = this.audioPath(modsRoot, bankIndex);
          await atomicWrite(destination, audio);
          createdFiles.push(destination);
          state.music.Tracks.push(track);
          keys.push(trackKey(track));
        }
      }
      const playlist = { id: crypto.randomUUID(), name, trackKeys: [...new Set(keys)] };
      state.playlists.push(playlist);
      await this.writeActiveMusic(state, musicIff);
      const [bundleReport] = await this.refreshPlaylistBundles(modsRoot, state, (item) => item.id === playlist.id);
      return {
        ...this.viewFromState(state, musicIff), savedPlaylistId: playlist.id,
        importPlaylistReport: {
          name, trackCount: playlist.trackKeys.length, warnings, bundleReport, importedParts: container?1:partPaths.length,
        },
      };
    } catch (error) {
      for (const file of createdFiles) await fsp.rm(file, { force: true }).catch(() => {});
      await this.writeState(originalState).catch(() => {});
      throw error;
    }
  }

  async activatePlaylist(modsRoot, id) {
    const { state, musicIff } = await this.loadState(modsRoot);
    if (id && !state.playlists.some((item) => item.id === id)) throw new Error('The selected playlist no longer exists.');
    if (id || state.activePlaylistId) state.lastPlaylistId = id || state.activePlaylistId;
    state.activePlaylistId = id || null;
    await this.writeActiveMusic(state, musicIff);
    return this.viewFromState(state, musicIff);
  }

  async deletePlaylist(modsRoot, id) {
    const { state, musicIff } = await this.loadState(modsRoot);
    const removed = state.playlists.find((item) => item.id === id);
    state.playlists = state.playlists.filter((item) => item.id !== id);
    if (state.activePlaylistId === id) state.activePlaylistId = null;
    await this.writeActiveMusic(state, musicIff);
    const bundlePaths = [...new Set([
      ...(Array.isArray(removed?.bundlePaths) ? removed.bundlePaths : []),
      removed?.bundlePath,
    ].filter(Boolean))];
    for (const bundlePath of bundlePaths) {
      const target = path.resolve(bundlePath);
      if (target.startsWith(`${path.resolve(this.playlistFolder)}${path.sep}`)) await fsp.rm(target, { force: true });
    }
    return this.viewFromState(state, musicIff);
  }
}

module.exports = {
  JukeboxManager,
  audioLogicalPath,
  buildMusicIff,
  buildTldFromDds,
  buildZipArchive,
  crc32,
  findManifestEntry,
  findManifestEntries,
  findCoverManifestEntry,
  readMusicJson,
  readZipArchive,
  readZipMemberFromFile,
  trackKey,
  validatePcmWave,
  validateTldArtwork,
};
