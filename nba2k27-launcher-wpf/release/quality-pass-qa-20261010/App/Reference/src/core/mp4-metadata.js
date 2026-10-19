'use strict';
const fs = require('node:fs/promises');
const path = require('node:path');
const MAX_BYTES = 2 * 1024 ** 3;
const MAX_SECONDS = 600;
const VIDEO_CODECS = Object.freeze({avc1: 'H.264', avc3: 'H.264', av01: 'AV1', vp09: 'VP9'});

function boxes(bytes, start = 0, end = bytes.length) {
  const result = [];
  for (let at = start; at < end;) {
    if (result.length >= 10000 || at + 8 > end) throw Error('Invalid MP4 box table.');
    let size = bytes.readUInt32BE(at), header = 8;
    if (size === 1) {
      if (at + 16 > end) throw Error('Truncated MP4 box.');
      size = Number(bytes.readBigUInt64BE(at + 8)); header = 16;
    }
    if (size === 0) size = end - at;
    if (!Number.isSafeInteger(size) || size < header || at + size > end) throw Error('Invalid MP4 box size.');
    result.push({type: bytes.toString('ascii', at + 4, at + 8), start: at + header, end: at + size});
    at += size;
  }
  return result;
}

function parseMovie(bytes) {
  const root = boxes(bytes), mvhd = root.find(b => b.type === 'mvhd');
  if (!mvhd) throw Error('MP4 duration is missing.');
  const header = bytes.subarray(mvhd.start, mvhd.end), version = header[0];
  if (![0, 1].includes(version) || header.length < (version ? 32 : 20)) throw Error('Invalid MP4 duration.');
  const scale = header.readUInt32BE(version ? 20 : 12);
  const ticks = version ? Number(header.readBigUInt64BE(24)) : header.readUInt32BE(16);
  const durationSeconds = ticks / scale;
  if (!Number.isFinite(durationSeconds) || durationSeconds <= 0 || durationSeconds > MAX_SECONDS) throw Error('Choose an MP4 up to 10 minutes long.');
  const codecs = new Set();
  function children(box, type) { return boxes(bytes, box.start, box.end).filter(b => b.type === type); }
  for (const track of root.filter(b => b.type === 'trak')) {
    for (const mdia of children(track, 'mdia')) {
      const handler = children(mdia, 'hdlr')[0];
      if (!handler || handler.end - handler.start < 12) continue;
      const kind = bytes.toString('ascii', handler.start + 8, handler.start + 12);
      if (!['vide', 'soun'].includes(kind)) continue;
      const minf = children(mdia, 'minf')[0], stbl = minf && children(minf, 'stbl')[0];
      const stsd = stbl && children(stbl, 'stsd')[0];
      if (!stsd || stsd.end - stsd.start < 8) throw Error('MP4 codec metadata is missing.');
      const entries = boxes(bytes, stsd.start + 8, stsd.end);
      if (!entries.length || entries.length !== bytes.readUInt32BE(stsd.start + 4)) throw Error('Invalid MP4 codec table.');
      const unsupported = entries.filter(e => kind === 'vide' ? !Object.hasOwn(VIDEO_CODECS, e.type) : e.type !== 'mp4a');
      if (unsupported.length) throw Error(`Unsupported MP4 ${kind === 'vide' ? 'video' : 'audio'} codec: ${unsupported.map(e => e.type).join(', ')}. Use H.264, AV1 or VP9 video with AAC audio (or no audio).`);
      if (kind === 'vide') for (const entry of entries) codecs.add(VIDEO_CODECS[entry.type]);
    }
  }
  if (!codecs.size) throw Error('This MP4 has no supported video track.');
  return {durationSeconds, format: 'mp4', codec: [...codecs].join(' / '), overlayReady: true};
}

async function inspectMp4(file) {
  if (path.extname(file).toLowerCase() !== '.mp4') throw Error('Choose an .mp4 video. Native game .mov files are no longer supported.');
  const stat = await fs.lstat(file);
  if (!stat.isFile() || stat.isSymbolicLink() || stat.size < 24 || stat.size > MAX_BYTES) throw Error('Choose a regular MP4 file smaller than 2 GB.');
  const handle = await fs.open(file, 'r');
  try {
    let at = 0, count = 0, branded = false, movie = null;
    while (at < stat.size) {
      if (++count > 10000 || stat.size - at < 8) throw Error('Invalid MP4 container.');
      const head = Buffer.alloc(16), {bytesRead} = await handle.read(head, 0, Math.min(16, stat.size - at), at);
      if (bytesRead < 8) throw Error('Truncated MP4 file.');
      let size = head.readUInt32BE(0), header = 8;
      if (size === 1) { if (bytesRead < 16) throw Error('Truncated MP4 box.'); size = Number(head.readBigUInt64BE(8)); header = 16; }
      if (size === 0) size = stat.size - at;
      if (!Number.isSafeInteger(size) || size < header || at + size > stat.size) throw Error('Invalid MP4 container size.');
      const type = head.toString('ascii', 4, 8);
      if (type === 'ftyp' && size >= header + 8) branded = true;
      if (type === 'moov') {
        if (movie || size > 16 * 1024 ** 2) throw Error('Invalid or oversized MP4 metadata.');
        const bytes = Buffer.alloc(size - header);
        if ((await handle.read(bytes, 0, bytes.length, at + header)).bytesRead !== bytes.length) throw Error('Truncated MP4 metadata.');
        movie = parseMovie(bytes);
      }
      at += size;
    }
    if (!branded || !movie) throw Error('This is not a complete MP4 video.');
    return {...movie, bytes: stat.size};
  } finally { await handle.close(); }
}
module.exports = {inspectMp4, parseMovie, MAX_BYTES, MAX_SECONDS};
