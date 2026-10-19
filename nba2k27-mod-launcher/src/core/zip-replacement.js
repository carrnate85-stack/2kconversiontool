const zlib = require('node:zlib');

const LOCAL = 0x04034b50;
const CENTRAL = 0x02014b50;
const EOCD = 0x06054b50;
const MAX_ARCHIVE_BYTES = 64 * 1024 * 1024;
const MAX_ENTRIES = 512;

const crcTable = (() => {
  const table = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let value = n;
    for (let bit = 0; bit < 8; bit++) value = (value & 1) ? (0xedb88320 ^ (value >>> 1)) : (value >>> 1);
    table[n] = value >>> 0;
  }
  return table;
})();

function crc32(bytes) {
  let value = 0xffffffff;
  for (const byte of bytes) value = crcTable[(value ^ byte) & 0xff] ^ (value >>> 8);
  return (value ^ 0xffffffff) >>> 0;
}

function findEocd(buffer) {
  const minimum = Math.max(0, buffer.length - 0xffff - 22);
  for (let offset = buffer.length - 22; offset >= minimum; offset--) {
    if (buffer.readUInt32LE(offset) === EOCD && offset + 22 + buffer.readUInt16LE(offset + 20) === buffer.length) return offset;
  }
  throw new Error('Archive end record was not found.');
}

function parseZip(buffer) {
  if (!Buffer.isBuffer(buffer) || buffer.length < 22 || buffer.length > MAX_ARCHIVE_BYTES) throw new Error('Archive size is invalid.');
  const eocd = findEocd(buffer);
  const disk = buffer.readUInt16LE(eocd + 4);
  const centralDisk = buffer.readUInt16LE(eocd + 6);
  const diskEntries = buffer.readUInt16LE(eocd + 8);
  const entryCount = buffer.readUInt16LE(eocd + 10);
  const centralSize = buffer.readUInt32LE(eocd + 12);
  const centralOffset = buffer.readUInt32LE(eocd + 16);
  if (disk || centralDisk || diskEntries !== entryCount || !entryCount || entryCount > MAX_ENTRIES || centralOffset + centralSize > eocd) throw new Error('Multi-disk, ZIP64 or malformed archives are unsupported.');
  const archiveComment = Buffer.from(buffer.subarray(eocd + 22));
  const entries = [];
  let cursor = centralOffset;
  let totalUncompressed = 0;
  const names = new Set();
  for (let index = 0; index < entryCount; index++) {
    if (cursor + 46 > eocd || buffer.readUInt32LE(cursor) !== CENTRAL) throw new Error('Archive central directory is malformed.');
    const flags = buffer.readUInt16LE(cursor + 8);
    const method = buffer.readUInt16LE(cursor + 10);
    const compressedSize = buffer.readUInt32LE(cursor + 20);
    const uncompressedSize = buffer.readUInt32LE(cursor + 24);
    const nameLength = buffer.readUInt16LE(cursor + 28);
    const extraLength = buffer.readUInt16LE(cursor + 30);
    const commentLength = buffer.readUInt16LE(cursor + 32);
    const localOffset = buffer.readUInt32LE(cursor + 42);
    const end = cursor + 46 + nameLength + extraLength + commentLength;
    if ((flags & 1) || ![0, 8].includes(method) || end > eocd || localOffset + 30 > centralOffset || buffer.readUInt32LE(localOffset) !== LOCAL) throw new Error('Archive entry uses an unsupported layout.');
    const nameBytes = Buffer.from(buffer.subarray(cursor + 46, cursor + 46 + nameLength));
    const name = nameBytes.toString((flags & 0x800) ? 'utf8' : 'latin1');
    if (!name || names.has(name) || name.includes('\0')) throw new Error('Archive contains an invalid or duplicate entry name.');
    names.add(name);
    const localNameLength = buffer.readUInt16LE(localOffset + 26);
    const localExtraLength = buffer.readUInt16LE(localOffset + 28);
    const dataOffset = localOffset + 30 + localNameLength + localExtraLength;
    if (dataOffset + compressedSize > centralOffset) throw new Error('Archive entry data is out of bounds.');
    const compressed = buffer.subarray(dataOffset, dataOffset + compressedSize);
    const data = method === 0 ? Buffer.from(compressed) : zlib.inflateRawSync(compressed, { maxOutputLength: Math.min(MAX_ARCHIVE_BYTES, uncompressedSize + 1) });
    if (data.length !== uncompressedSize || crc32(data) !== buffer.readUInt32LE(cursor + 16)) throw new Error('Archive entry checksum failed.');
    totalUncompressed += data.length;
    if (totalUncompressed > MAX_ARCHIVE_BYTES) throw new Error('Archive expands beyond the safe limit.');
    entries.push({
      name, nameBytes, data, method,
      versionMade: buffer.readUInt16LE(cursor + 4),
      versionNeeded: buffer.readUInt16LE(cursor + 6),
      flags: flags & 0x800,
      time: buffer.readUInt16LE(cursor + 12),
      date: buffer.readUInt16LE(cursor + 14),
      diskStart: buffer.readUInt16LE(cursor + 34),
      internalAttributes: buffer.readUInt16LE(cursor + 36),
      externalAttributes: buffer.readUInt32LE(cursor + 38),
      localExtra: Buffer.from(buffer.subarray(localOffset + 30 + localNameLength, dataOffset)),
      centralExtra: Buffer.from(buffer.subarray(cursor + 46 + nameLength, cursor + 46 + nameLength + extraLength)),
      comment: Buffer.from(buffer.subarray(cursor + 46 + nameLength + extraLength, end)),
    });
    cursor = end;
  }
  if (cursor !== centralOffset + centralSize) throw new Error('Archive central directory length is inconsistent.');
  return { entries, comment: archiveComment };
}

function buildZip(document) {
  const localParts = [];
  const centralParts = [];
  let localOffset = 0;
  for (const entry of document.entries) {
    const nameBytes = Buffer.from(entry.nameBytes || Buffer.from(entry.name, 'utf8'));
    const localExtra = Buffer.from(entry.localExtra || []);
    const centralExtra = Buffer.from(entry.centralExtra || []);
    const comment = Buffer.from(entry.comment || []);
    const data = Buffer.from(entry.data);
    const compressed = entry.method === 0 ? data : zlib.deflateRawSync(data, { level: 9 });
    const checksum = crc32(data);
    const local = Buffer.alloc(30);
    local.writeUInt32LE(LOCAL, 0);
    local.writeUInt16LE(entry.versionNeeded || 20, 4);
    local.writeUInt16LE(entry.flags || 0, 6);
    local.writeUInt16LE(entry.method, 8);
    local.writeUInt16LE(entry.time || 0, 10);
    local.writeUInt16LE(entry.date || 0, 12);
    local.writeUInt32LE(checksum, 14);
    local.writeUInt32LE(compressed.length, 18);
    local.writeUInt32LE(data.length, 22);
    local.writeUInt16LE(nameBytes.length, 26);
    local.writeUInt16LE(localExtra.length, 28);
    localParts.push(local, nameBytes, localExtra, compressed);

    const central = Buffer.alloc(46);
    central.writeUInt32LE(CENTRAL, 0);
    central.writeUInt16LE(entry.versionMade || 20, 4);
    central.writeUInt16LE(entry.versionNeeded || 20, 6);
    central.writeUInt16LE(entry.flags || 0, 8);
    central.writeUInt16LE(entry.method, 10);
    central.writeUInt16LE(entry.time || 0, 12);
    central.writeUInt16LE(entry.date || 0, 14);
    central.writeUInt32LE(checksum, 16);
    central.writeUInt32LE(compressed.length, 20);
    central.writeUInt32LE(data.length, 24);
    central.writeUInt16LE(nameBytes.length, 28);
    central.writeUInt16LE(centralExtra.length, 30);
    central.writeUInt16LE(comment.length, 32);
    central.writeUInt16LE(entry.diskStart || 0, 34);
    central.writeUInt16LE(entry.internalAttributes || 0, 36);
    central.writeUInt32LE(entry.externalAttributes || 0, 38);
    central.writeUInt32LE(localOffset, 42);
    centralParts.push(central, nameBytes, centralExtra, comment);
    localOffset += local.length + nameBytes.length + localExtra.length + compressed.length;
  }
  const centralOffset = localOffset;
  const centralSize = centralParts.reduce((sum, part) => sum + part.length, 0);
  const archiveComment = Buffer.from(document.comment || []);
  const eocd = Buffer.alloc(22);
  eocd.writeUInt32LE(EOCD, 0);
  eocd.writeUInt16LE(document.entries.length, 8);
  eocd.writeUInt16LE(document.entries.length, 10);
  eocd.writeUInt32LE(centralSize, 12);
  eocd.writeUInt32LE(centralOffset, 16);
  eocd.writeUInt16LE(archiveComment.length, 20);
  const output = Buffer.concat([...localParts, ...centralParts, eocd, archiveComment]);
  if (output.length > MAX_ARCHIVE_BYTES) throw new Error('Rebuilt archive exceeds the safe limit.');
  return output;
}

function replaceZipEntries(source, replacements) {
  const document = parseZip(source);
  const pending=new Map(replacements);
  if(!pending.size)throw new Error('At least one archive replacement is required.');
  for(const entry of document.entries){if(pending.has(entry.name)){entry.data=Buffer.from(pending.get(entry.name));pending.delete(entry.name);}}
  if(pending.size)throw new Error(`Archive member was not found: ${pending.keys().next().value}`);
  const output = buildZip(document);
  const check = parseZip(output);
  for(const [name,replacement] of replacements){const replaced=check.entries.find(candidate=>candidate.name===name);if(!replaced||!replaced.data.equals(replacement))throw new Error('Rebuilt archive did not retain the replacement.');}
  return output;
}

function replaceZipEntry(source,name,replacement){return replaceZipEntries(source,new Map([[name,replacement]]));}

module.exports = { buildZip, crc32, parseZip, replaceZipEntries, replaceZipEntry };
