'use strict';
const fs = require('node:fs/promises');
const path = require('node:path');
const crypto = require('node:crypto');

const AUDIO_INDEX_PATH = 'newaudio/jukeboxmusic/directory_listing.adb2';
const TEST_INDEX_HASH = 'F2CECDB01FDC49A32561092FC2747C6A7C1B9C9E5D570A55BA46C8A5EC9F4EE8';

function verifiedTestIndex(bytes) {
  return bytes.length === 4312 && crypto.createHash('sha256').update(bytes).digest('hex').toUpperCase() === TEST_INDEX_HASH;
}

// Controlled v33 test only. Never override an arbitrary user's stock index.
// Auto-generation for other playlists waits for actual in-game playback validation.
async function selectAudioIndexOverride(modsRoot) {
  let current = path.resolve(modsRoot);
  for (const part of AUDIO_INDEX_PATH.split('/')) {
    current = path.join(current, part);
    let stat;
    try {stat = await fs.lstat(current);}
    catch (error) {if (error.code === 'ENOENT') return null; throw error;}
    if (stat.isSymbolicLink()) return null;
  }
  const stat = await fs.stat(current);
  if (!stat.isFile() || stat.size !== 4312) return null;
  if (!verifiedTestIndex(await fs.readFile(current))) return null;
  return AUDIO_INDEX_PATH;
}

module.exports = {AUDIO_INDEX_PATH, TEST_INDEX_HASH, verifiedTestIndex, selectAudioIndexOverride};
