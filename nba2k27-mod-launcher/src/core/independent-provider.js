const path = require('node:path');
const fsp = require('node:fs/promises');
const crypto = require('node:crypto');

const OBSERVED_REQUEST_BYTES = 48;
const OBSERVED_RESULT_BYTES = 16;
const MAX_LOGICAL_PATH_CHARS = 259;
const MAX_DIAGNOSTIC_READ = 1024 * 1024;
const MAX_DIAGNOSTIC_DISCOVERY_FILES = 100000;

function assetNotFound(logicalPath) {
  const error = new Error(`Asset not found: ${logicalPath}`);
  error.code = 'ASSET_NOT_FOUND';
  return error;
}

function assertUint64(value, name) {
  const converted = typeof value === 'bigint' ? value : BigInt(value);
  if (converted < 0n || converted > 0xffffffffffffffffn) throw new RangeError(`${name} is outside uint64.`);
  return converted;
}

function encodeObservedReadRequest(request) {
  const values = [
    request.operation,
    request.resourceKey,
    request.readArgumentsPointer,
    request.logicalPathPointer,
    request.reserved0 || 0,
    request.reserved1 || 0,
  ].map((value, index) => assertUint64(value, `request field ${index}`));
  const buffer = Buffer.alloc(OBSERVED_REQUEST_BYTES);
  values.forEach((value, index) => buffer.writeBigUInt64LE(value, index * 8));
  return buffer;
}

function decodeObservedReadRequest(buffer) {
  if (!Buffer.isBuffer(buffer) || buffer.length !== OBSERVED_REQUEST_BYTES) {
    throw new Error(`Observed read requests must be exactly ${OBSERVED_REQUEST_BYTES} bytes.`);
  }
  const values = Array.from({ length: 6 }, (_unused, index) => buffer.readBigUInt64LE(index * 8));
  return {
    operation: values[0],
    resourceKey: values[1],
    readArgumentsPointer: values[2],
    logicalPathPointer: values[3],
    reserved0: values[4],
    reserved1: values[5],
  };
}

function encodeObservedReadResult(result) {
  const buffer = Buffer.alloc(OBSERVED_RESULT_BYTES);
  buffer.writeBigUInt64LE(assertUint64(result.bytesRead, 'bytesRead'), 0);
  buffer.writeBigUInt64LE(assertUint64(result.unknown || 0, 'unknown'), 8);
  return buffer;
}

function decodeObservedReadResult(buffer) {
  if (!Buffer.isBuffer(buffer) || buffer.length !== OBSERVED_RESULT_BYTES) {
    throw new Error(`Observed read results must be exactly ${OBSERVED_RESULT_BYTES} bytes.`);
  }
  return { bytesRead: buffer.readBigUInt64LE(0), unknown: buffer.readBigUInt64LE(8) };
}

function logicalSegments(logicalPath) {
  if (typeof logicalPath !== 'string' || !logicalPath.trim()) throw new Error('Asset path is required.');
  if (logicalPath.includes('\0')) throw new Error('Asset path contains a null character.');
  if (logicalPath.length > MAX_LOGICAL_PATH_CHARS) throw new Error('Asset path exceeds the observed 260-character metadata buffer.');
  const normalized = logicalPath.trim().replaceAll('\\', '/');
  if (normalized.startsWith('/') || /^[a-z]:/i.test(normalized)) throw new Error('Asset path must be relative to the mods folder.');
  const segments = normalized.split('/');
  if (segments.some((part) => !part || part === '.' || part === '..')) throw new Error('Asset path contains an unsafe path segment.');
  return segments;
}

async function resolveAsset(modsRoot, logicalPath) {
  const root = await fsp.realpath(path.resolve(modsRoot));
  const candidate = path.join(root, ...logicalSegments(logicalPath));
  const resolved = await fsp.realpath(candidate).catch((error) => {
    if (error.code === 'ENOENT') throw assetNotFound(logicalPath);
    throw error;
  });
  const relative = path.relative(root, resolved);
  if (!relative || relative.startsWith('..') || path.isAbsolute(relative)) {
    throw new Error('Resolved asset is outside the selected mods folder.');
  }
  const stat = await fsp.stat(resolved);
  if (!stat.isFile()) throw new Error('Resolved asset is not a regular file.');
  return {
    logicalPath: logicalSegments(logicalPath).join('/'),
    absolutePath: resolved,
    size: stat.size,
    modifiedMs: stat.mtimeMs,
  };
}

async function readAssetRange(modsRoot, logicalPath, offset = 0, length = 64) {
  if (!Number.isSafeInteger(offset) || offset < 0) throw new Error('Read offset must be a non-negative safe integer.');
  if (!Number.isSafeInteger(length) || length < 0 || length > MAX_DIAGNOSTIC_READ) {
    throw new Error(`Read length must be from 0 to ${MAX_DIAGNOSTIC_READ} bytes.`);
  }
  const asset = await resolveAsset(modsRoot, logicalPath);
  const available = Math.max(0, asset.size - offset);
  const requested = Math.min(length, available);
  const data = Buffer.alloc(requested);
  if (requested) {
    const handle = await fsp.open(asset.absolutePath, 'r');
    try {
      const result = await handle.read(data, 0, requested, offset);
      if (result.bytesRead !== requested) return { ...asset, offset, data: data.subarray(0, result.bytesRead) };
    } finally {
      await handle.close();
    }
  }
  return { ...asset, offset, data };
}

async function findUniqueAssetByName(modsRoot, fileName) {
  const segments = logicalSegments(fileName);
  if (segments.length !== 1) return segments.join('/');
  const root = await fsp.realpath(path.resolve(modsRoot));
  const pending = [''];
  const matches = [];
  let examined = 0;
  while (pending.length) {
    const relativeFolder = pending.pop();
    const entries = await fsp.readdir(path.join(root, relativeFolder), { withFileTypes: true });
    for (const entry of entries) {
      const relative = path.join(relativeFolder, entry.name);
      if (entry.isSymbolicLink()) continue;
      if (entry.isDirectory()) {
        pending.push(relative);
        continue;
      }
      if (!entry.isFile()) continue;
      examined += 1;
      if (examined > MAX_DIAGNOSTIC_DISCOVERY_FILES) {
        throw new Error(`Filename discovery exceeded its ${MAX_DIAGNOSTIC_DISCOVERY_FILES.toLocaleString()} file safety limit.`);
      }
      if (entry.name.toLowerCase() === segments[0].toLowerCase()) {
        matches.push(relative.replaceAll('\\', '/'));
        if (matches.length > 1) {
          throw new Error(`More than one ${segments[0]} exists. Enter its full path relative to the mods folder.`);
        }
      }
    }
  }
  if (!matches.length) throw new Error(`Asset not found anywhere under mods: ${segments[0]}`);
  return matches[0];
}

async function offlineProviderDiagnostic(modsRoot, logicalPath) {
  let resolvedLogicalPath = logicalPath;
  try {
    await resolveAsset(modsRoot, resolvedLogicalPath);
  } catch (error) {
    if (!/Asset not found:/.test(error.message) || logicalSegments(logicalPath).length !== 1) throw error;
    resolvedLogicalPath = await findUniqueAssetByName(modsRoot, logicalPath);
  }
  const result = await readAssetRange(modsRoot, resolvedLogicalPath, 0, 64);
  const syntheticEnvelope = encodeObservedReadRequest({
    operation: 1n,
    resourceKey: 0x1020304050607080n,
    readArgumentsPointer: 0x1111222233334444n,
    logicalPathPointer: 0x5555666677778888n,
  });
  const decodedEnvelope = decodeObservedReadRequest(syntheticEnvelope);
  const response = decodeObservedReadResult(encodeObservedReadResult({ bytesRead: BigInt(result.data.length) }));
  return {
    ok: true,
    logicalPath: result.logicalPath,
    size: result.size,
    firstReadBytes: result.data.length,
    firstReadSha256: crypto.createHash('sha256').update(result.data).digest('hex'),
    envelope: {
      bytes: syntheticEnvelope.length,
      operation: Number(decodedEnvelope.operation),
      resultBytes: OBSERVED_RESULT_BYTES,
      mockBytesRead: Number(response.bytesRead),
      syntheticPointers: true,
    },
    limitation: 'Offline resolver/codec test only; it does not attach to or write into NBA2K27.exe.',
  };
}

module.exports = {
  MAX_DIAGNOSTIC_READ,
  MAX_DIAGNOSTIC_DISCOVERY_FILES,
  MAX_LOGICAL_PATH_CHARS,
  OBSERVED_REQUEST_BYTES,
  OBSERVED_RESULT_BYTES,
  assetNotFound,
  decodeObservedReadRequest,
  decodeObservedReadResult,
  encodeObservedReadRequest,
  encodeObservedReadResult,
  findUniqueAssetByName,
  logicalSegments,
  offlineProviderDiagnostic,
  readAssetRange,
  resolveAsset,
};
