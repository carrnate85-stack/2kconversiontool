import { native, nativeBuffer } from './native.js';
const original = require('cmt-original-iff');

async function loadIff(filePath) {
  if (!String(filePath).startsWith('cmt-preview://')) return original.loadIff(filePath);
  try {
    const archive = await native('read-archive', { path: filePath });
    const entries = archive.members.map(member => ({
      name: member.name, size: member.size,
      decoded: nativeBuffer(member.data),
    }));
    return { buffer: null, entries, lookup: original.entryLookup(entries),
      geometryKey: archive.geometryKey, contentKey: archive.contentKey, warnings: archive.warnings || [] };
  } catch (error) {
    if (String(filePath).startsWith('cmt-preview://')) throw error;
    return original.loadIff(filePath);
  }
}
function readZipEntry(buffer, entry, verify = true) {
  return entry.decoded || original.readZipEntry(buffer, entry, verify);
}
module.exports = { ...original, loadIff, readZipEntry };
