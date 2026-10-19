const fs = require('node:fs');
const fsp = require('node:fs/promises');
const crypto = require('node:crypto');

const EXPECTED_ADDON_SHA256 = '8757410bf329cb59df26c4d9e5fdc014fe9a2d6708186557f8d5be5c83ddae0c';

async function sha256File(filePath) {
  const hash = crypto.createHash('sha256');
  await new Promise((resolve, reject) => {
    const stream = fs.createReadStream(filePath);
    stream.on('data', (chunk) => hash.update(chunk));
    stream.once('error', reject);
    stream.once('end', resolve);
  });
  return hash.digest('hex');
}

async function inspectObserveOnlyAddon(filePath) {
  if (!filePath) {
    return { available: false, verified: false, path: null, reason: 'Observe-only add-on path was not provided.' };
  }
  const stat = await fsp.stat(filePath).catch((error) => {
    if (error.code === 'ENOENT') return null;
    throw error;
  });
  if (!stat?.isFile()) {
    return { available: false, verified: false, path: filePath, reason: 'Observe-only add-on is missing.' };
  }
  const sha256 = await sha256File(filePath);
  const verified = sha256 === EXPECTED_ADDON_SHA256;
  return {
    available: true,
    verified,
    path: filePath,
    bytes: stat.size,
    sha256,
    protocolVersion: 16,
    lifecycleEvents: ['initialize', 'heartbeat', 'registration-result', 'live-route-result', 'path-diagnostic', 'shutdown'],
    hooksInstalled: false,
    deviceRegistrationEnabled: false,
    reversibleRegistrationProbeAvailable: true,
    customGetIdCallbackProbeAvailable: true,
    customExistsCallbackProbeAvailable: true,
    customFileInfoCallbackProbeAvailable: true,
    managerOwnedStreamProbeAvailable: true,
    realAssetStreamProbeAvailable: true,
    temporaryDefaultDelegationProbeAvailable: true,
    sourceBoundStockStreamProbeAvailable: true,
    unprefixedManagerOpenProbeAvailable: true,
    multiHandleRegistryProbeAvailable: true,
    pathAwareBindingsProbeAvailable: true,
    oneShotLiveRouteProbeAvailable: false,
    boundedPlayerPackageRouteAvailable: false,
    stockFirstModsFallbackAvailable: true,
    configPathTraceAvailable: true,
    resourceReplacementEnabled: false,
    reason: verified ? null : 'Observe-only add-on hash does not match this launcher build.',
  };
}

module.exports = {
  EXPECTED_ADDON_SHA256,
  inspectObserveOnlyAddon,
};
