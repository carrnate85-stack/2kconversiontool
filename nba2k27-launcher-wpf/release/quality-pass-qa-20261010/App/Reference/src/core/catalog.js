const path = require('node:path');
const fsp = require('node:fs/promises');

const MAX_FILES = 100000;

function classifyAsset(relativePath) {
  const name = path.basename(relativePath).toLowerCase();
  if (/^png\d+/.test(name)) return 'Player';
  if (/^(arena|s|floor|stadium)_/.test(name)) return 'Arena';
  if (/^(clothing|uniform|jersey|shorts)_/.test(name)) return 'Uniform';
  if (/^(english|spanish|french|german|italian)\.iff$/.test(name)) return 'Localization';
  if (/\.(iff|iff\.bin)$/i.test(name)) return 'IFF';
  if (/\.(dds|png|jpg|jpeg)$/i.test(name)) return 'Texture';
  if (/\.(wav|mp3|flac|ogg)$/i.test(name)) return 'Audio';
  return 'Other';
}

function assetIdentity(relativePath) {
  return relativePath.replaceAll('\\', '/').toLowerCase();
}

async function scanMods(modsRoot, options = {}) {
  const maximum = Math.min(Math.max(Number(options.maximum || MAX_FILES), 1), MAX_FILES);
  const root = path.resolve(modsRoot);
  const rootStat = await fsp.stat(root).catch(() => null);
  if (!rootStat?.isDirectory()) throw new Error('The selected mods folder does not exist.');

  const pending = [''];
  const files = [];
  const skippedLinks = [];
  let totalBytes = 0;
  while (pending.length) {
    const relativeFolder = pending.pop();
    const folder = path.join(root, relativeFolder);
    const entries = await fsp.readdir(folder, { withFileTypes: true });
    entries.sort((a, b) => a.name.localeCompare(b.name));
    for (const entry of entries) {
      const relative = path.join(relativeFolder, entry.name);
      if (entry.isSymbolicLink()) {
        skippedLinks.push(relative.replaceAll('\\', '/'));
        continue;
      }
      if (entry.isDirectory()) {
        pending.push(relative);
        continue;
      }
      if (!entry.isFile()) continue;
      if (files.length >= maximum) throw new Error(`Mod catalog exceeded its ${maximum.toLocaleString()} file safety limit.`);
      const absolute = path.join(root, relative);
      const stat = await fsp.stat(absolute);
      totalBytes += stat.size;
      files.push({
        id: assetIdentity(relative),
        relativePath: relative.replaceAll('\\', '/'),
        name: entry.name,
        category: classifyAsset(relative),
        size: stat.size,
        modifiedMs: stat.mtimeMs,
      });
    }
  }

  files.sort((a, b) => a.relativePath.localeCompare(b.relativePath));
  const categories = {};
  for (const file of files) categories[file.category] = (categories[file.category] || 0) + 1;
  return { root, files, count: files.length, totalBytes, categories, skippedLinks };
}

module.exports = { MAX_FILES, assetIdentity, classifyAsset, scanMods };
