const path = require('node:path');
const fsp = require('node:fs/promises');

const MAX_TAGS = 12;
const MAX_TAG_LENGTH = 40;

function canonicalRoot(value) {
  return path.resolve(String(value || '')).replaceAll('\\', '/').toLowerCase();
}

function normalizeTags(value) {
  const source = Array.isArray(value) ? value : String(value || '').split(',');
  const tags = [];
  const seen = new Set();
  for (const item of source) {
    const tag = String(item || '').trim().replace(/\s+/g, ' ');
    if (!tag) continue;
    if (tag.length > MAX_TAG_LENGTH || /[\u0000-\u001f]/.test(tag)) {
      throw new Error(`Each mod tag must contain at most ${MAX_TAG_LENGTH} valid characters.`);
    }
    const key = tag.toLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    tags.push(tag);
    if (tags.length > MAX_TAGS) throw new Error(`A mod file can have at most ${MAX_TAGS} tags.`);
  }
  return tags;
}

class ModTagStore {
  constructor(filePath) {
    this.filePath = filePath;
  }

  async read() {
    try {
      const parsed = JSON.parse(await fsp.readFile(this.filePath, 'utf8'));
      return Array.isArray(parsed.entries) ? parsed.entries : [];
    } catch (error) {
      if (error.code === 'ENOENT') return [];
      throw error;
    }
  }

  async apply(catalog) {
    const root = canonicalRoot(catalog.root);
    const entries = await this.read();
    const tagsByFile = new Map(entries
      .filter((entry) => entry.root === root && typeof entry.fileId === 'string')
      .map((entry) => [entry.fileId, normalizeTags(entry.tags)]));
    return {
      ...catalog,
      files: catalog.files.map((file) => ({ ...file, tags: tagsByFile.get(file.id) || [] })),
    };
  }

  async set(rootValue, fileIdValue, tagsValue) {
    const root = canonicalRoot(rootValue);
    const fileId = String(fileIdValue || '').replaceAll('\\', '/').toLowerCase();
    if (!fileId || fileId.length > 500 || fileId.startsWith('/') || fileId.includes('../')) {
      throw new Error('The mod file identity is invalid.');
    }
    const tags = normalizeTags(tagsValue);
    const entries = (await this.read()).filter((entry) => !(entry.root === root && entry.fileId === fileId));
    if (tags.length) entries.push({ root, fileId, tags });
    entries.sort((left, right) => `${left.root}|${left.fileId}`.localeCompare(`${right.root}|${right.fileId}`));
    await fsp.mkdir(path.dirname(this.filePath), { recursive: true });
    await fsp.writeFile(this.filePath, `${JSON.stringify({
      schema: 'nba2k27-mod-tags-v1', updatedAt: new Date().toISOString(), entries,
    }, null, 2)}\n`, 'utf8');
    return tags;
  }
}

module.exports = { MAX_TAGS, MAX_TAG_LENGTH, ModTagStore, canonicalRoot, normalizeTags };
