const fsp = require('node:fs/promises');
const path = require('node:path');
const zlib = require('node:zlib');

const EOCD_SIGNATURE = 0x06054b50;
const CENTRAL_SIGNATURE = 0x02014b50;
const LOCAL_SIGNATURE = 0x04034b50;
const MAX_EOCD_SEARCH = 0xffff + 22;

let crcTable;
function getCrcTable() {
  if (crcTable) return crcTable;
  crcTable = new Uint32Array(256);
  for (let value = 0; value < 256; value += 1) {
    let crc = value;
    for (let bit = 0; bit < 8; bit += 1) crc = (crc >>> 1) ^ ((crc & 1) ? 0xedb88320 : 0);
    crcTable[value] = crc >>> 0;
  }
  return crcTable;
}

function crc32(buffer) {
  const table = getCrcTable();
  let crc = 0xffffffff;
  for (const byte of buffer) crc = (crc >>> 8) ^ table[(crc ^ byte) & 0xff];
  return (crc ^ 0xffffffff) >>> 0;
}

function findEocd(buffer) {
  const lowerBound = Math.max(0, buffer.length - MAX_EOCD_SEARCH);
  for (let offset = buffer.length - 22; offset >= lowerBound; offset -= 1) {
    if (buffer.readUInt32LE(offset) === EOCD_SIGNATURE) return offset;
  }
  throw new Error('IFF does not contain a valid ZIP end record.');
}

function parseZip(buffer) {
  const eocd = findEocd(buffer);
  const disk = buffer.readUInt16LE(eocd + 4);
  const centralDisk = buffer.readUInt16LE(eocd + 6);
  const diskEntries = buffer.readUInt16LE(eocd + 8);
  const totalEntries = buffer.readUInt16LE(eocd + 10);
  const centralSize = buffer.readUInt32LE(eocd + 12);
  const centralOffset = buffer.readUInt32LE(eocd + 16);
  if (disk !== 0 || centralDisk !== 0 || diskEntries !== totalEntries) {
    throw new Error('Multi-disk IFF archives are not supported.');
  }
  if (totalEntries === 0xffff || centralOffset === 0xffffffff || centralSize === 0xffffffff) {
    throw new Error('ZIP64 IFF archives are not supported.');
  }
  if (centralOffset + centralSize > eocd || centralOffset < 0) {
    throw new Error('IFF central directory is outside the archive.');
  }

  const entries = [];
  let cursor = centralOffset;
  for (let index = 0; index < totalEntries; index += 1) {
    if (cursor + 46 > buffer.length || buffer.readUInt32LE(cursor) !== CENTRAL_SIGNATURE) {
      throw new Error(`Invalid IFF central-directory record ${index + 1}.`);
    }
    const flags = buffer.readUInt16LE(cursor + 8);
    const method = buffer.readUInt16LE(cursor + 10);
    const expectedCrc = buffer.readUInt32LE(cursor + 16);
    const compressedSize = buffer.readUInt32LE(cursor + 20);
    const size = buffer.readUInt32LE(cursor + 24);
    const nameLength = buffer.readUInt16LE(cursor + 28);
    const extraLength = buffer.readUInt16LE(cursor + 30);
    const commentLength = buffer.readUInt16LE(cursor + 32);
    const localOffset = buffer.readUInt32LE(cursor + 42);
    const next = cursor + 46 + nameLength + extraLength + commentLength;
    if (next > buffer.length) throw new Error('Truncated IFF central directory.');
    if (flags & 1) throw new Error('Encrypted IFF members are not supported.');
    if (![0, 8].includes(method)) throw new Error(`Unsupported ZIP compression method ${method}.`);
    const name = buffer.subarray(cursor + 46, cursor + 46 + nameLength).toString('utf8').replaceAll('\\', '/');
    if (!name || name.includes('\0') || name.startsWith('/') || name.split('/').some((part) => part === '..')) {
      throw new Error(`Unsafe IFF member name: ${name || '(empty)'}.`);
    }
    if (localOffset + 30 > buffer.length || buffer.readUInt32LE(localOffset) !== LOCAL_SIGNATURE) {
      throw new Error(`IFF member ${name} has an invalid local header.`);
    }
    const localNameLength = buffer.readUInt16LE(localOffset + 26);
    const localExtraLength = buffer.readUInt16LE(localOffset + 28);
    const dataOffset = localOffset + 30 + localNameLength + localExtraLength;
    if (dataOffset + compressedSize > buffer.length) throw new Error(`IFF member ${name} is truncated.`);
    entries.push({ name, flags, method, expectedCrc, compressedSize, size, dataOffset });
    cursor = next;
  }
  return { entries, centralOffset, centralSize };
}

function readZipEntry(buffer, entry, verify = true) {
  const packed = buffer.subarray(entry.dataOffset, entry.dataOffset + entry.compressedSize);
  const decoded = entry.method === 0 ? Buffer.from(packed) : zlib.inflateRawSync(packed, { maxOutputLength: entry.size });
  if (decoded.length !== entry.size) throw new Error(`IFF member ${entry.name} decoded to the wrong size.`);
  if (verify && crc32(decoded) !== entry.expectedCrc) throw new Error(`IFF member ${entry.name} failed CRC validation.`);
  return decoded;
}

function entryLookup(entries) {
  const lookup = new Map();
  for (const entry of entries) {
    lookup.set(entry.name.toLowerCase(), entry);
    const leaf = path.posix.basename(entry.name).toLowerCase();
    if (!lookup.has(leaf)) lookup.set(leaf, entry);
  }
  return lookup;
}

function parseScneText(buffer) {
  const text = buffer.toString('utf8').replace(/^\uFEFF/, '').trim();
  if (!text) return {};
  return JSON.parse(text.startsWith('{') ? text : `{${text}}`);
}

function parseBinaryTree(buffer) {
  const RECORD_START = 24;
  const RECORD_SIZE = 40;
  if (!Buffer.isBuffer(buffer) || buffer.length < RECORD_START
    || buffer.readUInt32LE(0) !== 0xE01FF891 || buffer.readUInt32LE(4) !== 1) return null;
  const stringBytes = buffer.readUInt32LE(8);
  const nodeCount = buffer.readUInt32LE(12);
  const stringStart = buffer.length - stringBytes;
  if (!stringBytes || !nodeCount || stringStart < RECORD_START + nodeCount * RECORD_SIZE) return null;

  function relativeString(pointerOffset) {
    const relative = Number(buffer.readBigInt64LE(pointerOffset));
    const start = pointerOffset + relative - 1;
    if (start < stringStart || start >= buffer.length) throw new Error('Binary-tree string pointer is out of bounds.');
    const end = buffer.indexOf(0, start);
    if (end < start) throw new Error('Binary-tree string is unterminated.');
    return buffer.toString('utf8', start, end);
  }

  function recordOffset(index) {
    if (index < 0 || index >= nodeCount) throw new Error('Binary-tree node index is out of bounds.');
    return RECORD_START + index * RECORD_SIZE;
  }

  function nodeName(index) {
    return relativeString(index === 0 ? 16 : recordOffset(index - 1) + 32);
  }

  let cursor = 0;
  function readNode(unnamed = false) {
    const index = cursor;
    cursor += 1;
    const offset = recordOffset(index);
    const fullTag = buffer.readUInt32LE(offset + 4);
    if ((fullTag & 0xFFFFFF00) !== 0x40000000) throw new Error('Unsupported binary-tree value tag.');
    const tag = fullTag & 0xFF;
    const name = unnamed ? '' : nodeName(index);
    let value;
    if (tag === 0) value = null;
    else if (tag === 3) value = relativeString(offset + 16);
    else if (tag === 5) value = buffer.readDoubleLE(offset + 16);
    else if (tag === 6) {
      const integer = buffer.readBigUInt64LE(offset + 16);
      value = integer <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(integer) : integer.toString();
    } else if (tag === 1 || tag === 2) {
      const count = buffer.readUInt32LE(offset + 12);
      if (tag === 2) {
        value = [];
        for (let child = 0; child < count; child += 1) value.push(readNode(true).value);
      } else {
        value = {};
        for (let child = 0; child < count; child += 1) {
          const parsed = readNode(false);
          value[parsed.name] = parsed.value;
        }
      }
    } else throw new Error(`Unsupported binary-tree value tag ${tag}.`);
    return { name, value };
  }

  const root = readNode(false);
  if (cursor !== nodeCount) throw new Error('Binary-tree node count is inconsistent.');
  return { [root.name]: root.value };
}

function scneBinaryReferences(text) {
  const references = [];
  const pattern = /"Binary"\s*:\s*"([^"]+)"/g;
  for (let match = pattern.exec(text); match; match = pattern.exec(text)) references.push(match[1].replaceAll('\\', '/'));
  return references;
}

async function loadIff(filePath) {
  const buffer = await fsp.readFile(filePath);
  const zip = parseZip(buffer);
  return { buffer, ...zip, lookup: entryLookup(zip.entries) };
}

module.exports = {
  crc32,
  entryLookup,
  loadIff,
  parseBinaryTree,
  parseScneText,
  parseZip,
  readZipEntry,
  scneBinaryReferences,
};
