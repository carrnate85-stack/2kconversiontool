'use strict';
// Offline research only. Observed BNH( / BNDL layout, NOT a native import validator.
// No packet construction, process access, save writes, or decompressed files.
const crypto = require('node:crypto');
const zlib = require('node:zlib');
const MAX_INPUT = 16 * 1024 * 1024;
const MAX_INFLATED = 64 * 1024 * 1024;
const MAX_STREAM = 32 * 1024 * 1024;
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');

function inspect(bytes) {
  if (!Buffer.isBuffer(bytes) || bytes.length < 4096 || bytes.length > MAX_INPUT)
    throw Error('Bundle must be between 4096 bytes and 16 MiB.');
  if (!bytes.subarray(0, 4).equals(Buffer.from('BNH(')))
    throw Error('Unsupported bundle signature; encrypted EBNH saves are not accepted.');
  if (bytes.readUInt32LE(4) !== bytes.length - 8)
    throw Error('Bundle length does not match its header.');
  const count = bytes.readUInt16LE(20);
  if (!count || count > 4096 || bytes.readUInt16LE(22) !== 4)
    throw Error('Unsupported observed bundle layout.');
  const blocks = [];
  let offset = 24, inflatedBytes = 0, streams = 0;
  for (let index = 0; index < count; index++) {
    if (offset + 60 > bytes.length || bytes.toString('ascii', offset, offset + 4) !== 'BNDL')
      throw Error('Missing or truncated BNDL block.');
    const size = bytes.readUInt32LE(offset + 4), end = offset + 60 + size;
    if (!size || end > bytes.length) throw Error('Block exceeds bundle bounds.');
    // The initial directory block has one stream; subsequent blocks have two.
    const sizes = index === 0 ? [bytes.readUInt32LE(offset + 32)] :
      [bytes.readUInt32LE(offset + 12), bytes.readUInt32LE(offset + 16)];
    if (index > 0 && bytes.readUInt32LE(offset + 32) !== sizes[0])
      throw Error('Repeated compressed size disagrees.');
    let cursor = offset + 60;
    const decoded = [];
    for (const compressedBytes of sizes) {
      if (!compressedBytes || compressedBytes > end - cursor)
        throw Error('Compressed stream exceeds block bounds.');
      const remaining = MAX_INFLATED - inflatedBytes;
      if (remaining <= 0) throw Error('Inflated data exceeds research limit.');
      const result = zlib.inflateSync(bytes.subarray(cursor, cursor + compressedBytes), {
        info: true, maxOutputLength: Math.min(MAX_STREAM, remaining),
      });
      if (result.engine.bytesWritten !== compressedBytes)
        throw Error('Compressed stream contains trailing bytes.');
      inflatedBytes += result.buffer.length;
      decoded.push({ compressedBytes, inflatedBytes: result.buffer.length, sha256: hash(result.buffer) });
      streams++;
      cursor += compressedBytes;
    }
    const padding = end - cursor;
    if (padding > 7 || bytes.subarray(cursor, end).some(value => value !== 0))
      throw Error('Unsupported block padding.');
    blocks.push({ index, offset, bytes: end - offset,
      id: bytes.readUInt32LE(offset + 8).toString(16).padStart(8, '0'), streams: decoded });
    offset = end;
  }
  if (offset !== bytes.length) throw Error('Unaccounted bytes after declared blocks.');
  return { format: 'native-roster-bundle-inspection-v1', observedLayout: 'BNH(-BNDL-v4',
    bytes: bytes.length, sha256: hash(bytes), blockCount: blocks.length, streamCount: streams,
    inflatedBytes, blocks, structuralChecksPassed: true, nativeImportVerified: false,
    createdPlayerDependenciesVerified: false, restoreEnabled: false, gameWrites: 0 };
}

module.exports = { inspect, MAX_INPUT };
