const path = require('node:path');

function tidy(value) {
  return String(value || '').replaceAll('\0', '').replace(/\s+/g, ' ').trim().slice(0, 120);
}

function decodeUtf16Be(bytes) {
  const swapped = Buffer.alloc(bytes.length - (bytes.length % 2));
  for (let index = 0; index < swapped.length; index += 2) {
    swapped[index] = bytes[index + 1];
    swapped[index + 1] = bytes[index];
  }
  return swapped.toString('utf16le');
}

function decodeId3Text(bytes) {
  if (!bytes?.length) return '';
  const encoding = bytes[0];
  const value = bytes.subarray(1);
  if (encoding === 0) return tidy(value.toString('latin1'));
  if (encoding === 3) return tidy(value.toString('utf8'));
  if (encoding === 2) return tidy(decodeUtf16Be(value));
  if (value[0] === 0xfe && value[1] === 0xff) return tidy(decodeUtf16Be(value.subarray(2)));
  return tidy(value.subarray(value[0] === 0xff && value[1] === 0xfe ? 2 : 0).toString('utf16le'));
}

function synchsafe(bytes, offset) {
  return ((bytes[offset] & 0x7f) << 21) | ((bytes[offset + 1] & 0x7f) << 14)
    | ((bytes[offset + 2] & 0x7f) << 7) | (bytes[offset + 3] & 0x7f);
}

function parseId3v2(head) {
  if (head.length < 10 || head.subarray(0, 3).toString('ascii') !== 'ID3') return {};
  const version = head[3];
  const limit = Math.min(head.length, 10 + synchsafe(head, 6));
  const found = {};
  let cursor = 10;
  while (cursor + (version === 2 ? 6 : 10) <= limit) {
    const idLength = version === 2 ? 3 : 4;
    const id = head.subarray(cursor, cursor + idLength).toString('ascii');
    if (!/^[A-Z0-9]+$/.test(id)) break;
    const size = version === 2
      ? head.readUIntBE(cursor + 3, 3)
      : version === 4 ? synchsafe(head, cursor + 4) : head.readUInt32BE(cursor + 4);
    const headerSize = version === 2 ? 6 : 10;
    const start = cursor + headerSize;
    if (!size || start + size > limit) break;
    if (id === 'TIT2' || id === 'TT2') found.title = decodeId3Text(head.subarray(start, start + size));
    if (id === 'TPE1' || id === 'TP1') found.artist = decodeId3Text(head.subarray(start, start + size));
    cursor = start + size;
  }
  return found;
}

function parseId3v1(tail) {
  if (tail.length < 128) return {};
  const tag = tail.subarray(tail.length - 128);
  if (tag.subarray(0, 3).toString('ascii') !== 'TAG') return {};
  return { title: tidy(tag.subarray(3, 33).toString('latin1')), artist: tidy(tag.subarray(33, 63).toString('latin1')) };
}

function parseRiffInfo(head) {
  if (head.length < 12 || head.subarray(0, 4).toString('ascii') !== 'RIFF') return {};
  const found = {};
  for (const [tag, field] of [['INAM', 'title'], ['IART', 'artist']]) {
    const marker = Buffer.from(tag, 'ascii');
    const offset = head.indexOf(marker, 12);
    if (offset >= 0 && offset + 8 <= head.length) {
      const size = head.readUInt32LE(offset + 4);
      if (size > 0 && size <= 512 && offset + 8 + size <= head.length) {
        found[field] = tidy(head.subarray(offset + 8, offset + 8 + size).toString('utf8'));
      }
    }
  }
  return found;
}

function parseFlacComments(head) {
  if (head.length < 8 || head.subarray(0, 4).toString('ascii') !== 'fLaC') return {};
  let cursor = 4;
  while (cursor + 4 <= head.length) {
    const type = head[cursor] & 0x7f;
    const size = head.readUIntBE(cursor + 1, 3);
    const start = cursor + 4;
    if (start + size > head.length) break;
    if (type === 4 && size >= 8) {
      let position = start;
      const vendorSize = head.readUInt32LE(position); position += 4 + vendorSize;
      if (position + 4 > start + size) return {};
      const count = Math.min(head.readUInt32LE(position), 1024); position += 4;
      const found = {};
      for (let index = 0; index < count && position + 4 <= start + size; index += 1) {
        const length = head.readUInt32LE(position); position += 4;
        if (length > 4096 || position + length > start + size) break;
        const comment = head.subarray(position, position + length).toString('utf8'); position += length;
        const split = comment.indexOf('=');
        if (split > 0) {
          const key = comment.slice(0, split).toUpperCase();
          if (key === 'TITLE') found.title = tidy(comment.slice(split + 1));
          if (key === 'ARTIST') found.artist = tidy(comment.slice(split + 1));
        }
      }
      return found;
    }
    cursor = start + size;
    if (head[cursor - size - 4] & 0x80) break;
  }
  return {};
}

function inferFromFilename(fileName) {
  const stem = path.basename(String(fileName || ''), path.extname(String(fileName || '')))
    .replace(/[_]+/g, ' ').replace(/\s+/g, ' ').trim();
  const match = stem.match(/^(.+?)\s+[-–—]\s+(.+)$/);
  return match
    ? { artist: tidy(match[1]), title: tidy(match[2]), structured: true }
    : { artist: 'Custom', title: tidy(stem) || 'Custom Song', structured: false };
}

function inferAudioMetadata(fileName, headBytes, tailBytes = headBytes) {
  const head = Buffer.from(headBytes || []);
  const tail = Buffer.from(tailBytes || []);
  const fallback = inferFromFilename(fileName);
  if (fallback.structured) return { title: fallback.title, artist: fallback.artist, source: 'filename' };
  const embedded = { ...parseId3v1(tail), ...parseRiffInfo(head), ...parseFlacComments(head), ...parseId3v2(head) };
  const hasEmbedded = tidy(embedded.title) || tidy(embedded.artist);
  return {
    title: tidy(embedded.title) || fallback.title,
    artist: tidy(embedded.artist) || fallback.artist,
    source: hasEmbedded ? 'metadata' : 'filename',
  };
}

module.exports = { inferAudioMetadata, inferFromFilename };
