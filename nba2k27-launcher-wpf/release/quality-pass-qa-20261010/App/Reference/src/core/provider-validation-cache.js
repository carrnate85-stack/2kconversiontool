'use strict';

const crypto = require('node:crypto');
const fs = require('node:fs/promises');
const path = require('node:path');
const { productionProviderDiagnostic } = require('./production-provider');

const CACHE_SCHEMA = 'provider-validation-cache-v1';
const DEFAULT_WARM_BYTES = 8 * 1024 * 1024;
const MAX_CACHE_ENTRIES = 32;

function normalizedSha256(value) {
  const result = String(value || '').trim().toLowerCase();
  if (!/^[a-f0-9]{64}$/.test(result)) throw new Error('Expected SHA-256 must contain 64 hexadecimal characters.');
  return result;
}

function safeAssetPath(modsRoot, logicalPath) {
  const parts = String(logicalPath || '').replaceAll('\\', '/').split('/');
  if (!parts.length || parts.some((part) => !part || part === '.' || part === '..' || part.includes(':'))) {
    throw new Error('Provider validation requires a safe logical asset path.');
  }
  const root = path.resolve(modsRoot);
  const assetPath = path.resolve(root, ...parts);
  const relative = path.relative(root, assetPath);
  if (!relative || relative.startsWith(`..${path.sep}`) || path.isAbsolute(relative)) {
    throw new Error('Provider validation asset escaped the mods folder.');
  }
  return assetPath;
}

async function fileIdentity(filePath) {
  const resolved = path.resolve(filePath);
  const stat = await fs.lstat(resolved);
  if (!stat.isFile() || stat.isSymbolicLink()) throw new Error(`Validation input is not a plain file: ${resolved}`);
  return {
    path: process.platform === 'win32' ? resolved.toLowerCase() : resolved,
    size: stat.size,
    mtimeMs: stat.mtimeMs,
    ctimeMs: stat.ctimeMs,
  };
}

function sameIdentity(left, right) {
  return Boolean(left && right
    && left.path === right.path
    && left.size === right.size
    && left.mtimeMs === right.mtimeMs
    && left.ctimeMs === right.ctimeMs);
}

async function readCache(cachePath) {
  try {
    const parsed = JSON.parse(await fs.readFile(cachePath, 'utf8'));
    if (parsed?.schema !== CACHE_SCHEMA || !Array.isArray(parsed.entries)) return { schema: CACHE_SCHEMA, entries: [] };
    return parsed;
  } catch (error) {
    if (error.code === 'ENOENT' || error instanceof SyntaxError) return { schema: CACHE_SCHEMA, entries: [] };
    throw error;
  }
}

async function writeCache(cachePath, entries) {
  await fs.mkdir(path.dirname(cachePath), { recursive: true });
  const temporary = `${cachePath}.${process.pid}.${crypto.randomUUID()}.tmp`;
  const document = `${JSON.stringify({ schema: CACHE_SCHEMA, entries: entries.slice(-MAX_CACHE_ENTRIES) }, null, 2)}\n`;
  await fs.writeFile(temporary, document, { flag: 'wx' });
  try {
    await fs.rename(temporary, cachePath);
  } catch (error) {
    if (error.code !== 'EEXIST' && error.code !== 'EPERM') throw error;
    await fs.rm(cachePath, { force: true });
    await fs.rename(temporary, cachePath);
  } finally {
    await fs.rm(temporary, { force: true });
  }
}

async function warmFilePrefix(filePath, requestedBytes = DEFAULT_WARM_BYTES) {
  if (!Number.isSafeInteger(requestedBytes) || requestedBytes < 0 || requestedBytes > 32 * 1024 * 1024) {
    throw new Error('Provider warm-up must be from 0 to 32 MB.');
  }
  if (requestedBytes === 0) return 0;
  const handle = await fs.open(filePath, 'r');
  try {
    const stat = await handle.stat();
    const length = Math.min(stat.size, requestedBytes);
    const buffer = Buffer.allocUnsafe(length);
    let offset = 0;
    while (offset < length) {
      const { bytesRead } = await handle.read(buffer, offset, length - offset, offset);
      if (!bytesRead) break;
      offset += bytesRead;
    }
    return offset;
  } finally {
    await handle.close();
  }
}

async function cachedProductionProviderDiagnostic({ modsRoot, hostExecutable, logicalPath,
  expectedSha256, cachePath, warmBytes = DEFAULT_WARM_BYTES, force = false,
  diagnose = productionProviderDiagnostic }) {
  const expected = normalizedSha256(expectedSha256);
  const assetPath = safeAssetPath(modsRoot, logicalPath);
  const [asset, host] = await Promise.all([fileIdentity(assetPath), fileIdentity(hostExecutable)]);
  const cache = await readCache(cachePath);
  const key = crypto.createHash('sha256').update([
    asset.path, String(logicalPath).toLowerCase(), host.path, expected, CACHE_SCHEMA,
  ].join('\0')).digest('hex');
  const cached = !force && cache.entries.find((entry) => entry?.key === key
    && entry.expectedSha256 === expected
    && sameIdentity(entry.asset, asset)
    && sameIdentity(entry.host, host));

  let result;
  if (cached) {
    result = {
      ok: true,
      cached: true,
      validationMode: 'cached-exact-file-identity',
      logicalPath,
      size: asset.size,
      fullBytesRead: 0,
      fullSha256: expected,
      readCalls: 0,
      providerTestedThisRun: false,
      validatedAt: cached.validatedAt,
    };
  } else {
    const report = await diagnose({ modsRoot, hostExecutable, logicalPath });
    if (!report?.ok || String(report.fullSha256 || '').toLowerCase() !== expected
      || report.fullBytesRead !== asset.size || report.eofBytesRead !== 0
      || report.openHandlesAfterClose !== 0) {
      throw new Error('Full provider validation did not reproduce the expected asset exactly.');
    }
    const entry = { key, expectedSha256: expected, asset, host, validatedAt: new Date().toISOString() };
    await writeCache(cachePath, [...cache.entries.filter((item) => item?.key !== key), entry]);
    result = {
      ...report,
      cached: false,
      validationMode: 'full-provider-read',
      providerTestedThisRun: true,
      validatedAt: entry.validatedAt,
    };
  }
  result.warmedBytes = await warmFilePrefix(assetPath, warmBytes);
  return result;
}

module.exports = {
  CACHE_SCHEMA,
  DEFAULT_WARM_BYTES,
  cachedProductionProviderDiagnostic,
  fileIdentity,
  safeAssetPath,
  warmFilePrefix,
};
