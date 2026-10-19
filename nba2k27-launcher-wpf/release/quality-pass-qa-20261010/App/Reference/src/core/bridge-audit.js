const fs = require('node:fs');
const fsp = require('node:fs/promises');
const path = require('node:path');
const crypto = require('node:crypto');
const readline = require('node:readline');
const { evidenceForGame } = require('./bridge-evidence');
const { inspectResourceDeviceBoundary } = require('./resource-device-boundary');
const { inspectDevelopmentManifestBoundary } = require('./development-manifest-boundary');
const { runOfflineDelegationSelfTest } = require('./delegating-device');
const { runNativeAbiHarness } = require('./native-abi-harness');
const { inspectStockDeviceLifetimeBoundary } = require('./stock-device-lifetime-boundary');
const { runCrossModuleHarness } = require('./cross-module-harness');
const { inspectObserveOnlyAddon } = require('./observe-only-addon');

const VERIFIED_BUILDS = Object.freeze([
  Object.freeze({
    name: 'NBA 2K27 Steam 2026-09-02',
    sha256: 'a870c4920985ee9bf9f3dba996d1bf41e5d65748274aadc00861bcf11b3b9ae5',
    peTimestamp: 0x6a922e5c,
    imageSize: 0x353f7000,
  }),
]);

function normalizeLogicalPath(value) {
  const normalized = String(value || '').replaceAll('\\', '/').replace(/^\/+/, '').toLowerCase();
  if (!normalized || normalized.split('/').some((part) => !part || part === '.' || part === '..' || part.includes(':'))) {
    throw new Error('The audit asset path must be a safe path relative to the mods folder.');
  }
  return normalized;
}

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

async function readPeIdentity(filePath) {
  const handle = await fsp.open(filePath, 'r');
  try {
    const dos = Buffer.alloc(64);
    if ((await handle.read(dos, 0, dos.length, 0)).bytesRead !== dos.length || dos.toString('ascii', 0, 2) !== 'MZ') {
      throw new Error('NBA2K27.exe is not a valid PE executable.');
    }
    const peOffset = dos.readUInt32LE(0x3c);
    const header = Buffer.alloc(128);
    if ((await handle.read(header, 0, header.length, peOffset)).bytesRead !== header.length || header.toString('ascii', 0, 4) !== 'PE\0\0') {
      throw new Error('NBA2K27.exe has an invalid PE header.');
    }
    const optionalMagic = header.readUInt16LE(24);
    if (optionalMagic !== 0x20b && optionalMagic !== 0x10b) throw new Error('NBA2K27.exe has an unsupported PE optional header.');
    return {
      peTimestamp: header.readUInt32LE(8),
      imageSize: header.readUInt32LE(24 + 56),
    };
  } finally {
    await handle.close();
  }
}

async function inspectExecutable(filePath, verifiedBuilds = VERIFIED_BUILDS) {
  const stat = await fsp.stat(filePath);
  if (!stat.isFile()) throw new Error('NBA2K27.exe was not found in the selected game folder.');
  const [sha256, pe] = await Promise.all([sha256File(filePath), readPeIdentity(filePath)]);
  const matchedBuild = verifiedBuilds.find((build) => (
    build.sha256 === sha256 && build.peTimestamp === pe.peTimestamp && build.imageSize === pe.imageSize
  )) || null;
  return {
    path: filePath,
    bytes: stat.size,
    sha256,
    peTimestamp: pe.peTimestamp,
    imageSize: pe.imageSize,
    exactVerifiedBuild: Boolean(matchedBuild),
    buildName: matchedBuild?.name || null,
  };
}

async function inspectManifest(manifestPath, logicalPath) {
  const wanted = normalizeLogicalPath(logicalPath);
  const wantedLeaf = path.posix.basename(wanted);
  const stat = await fsp.stat(manifestPath);
  if (!stat.isFile()) throw new Error('The stock game manifest was not found.');
  const stream = fs.createReadStream(manifestPath, { encoding: 'utf8' });
  const lines = readline.createInterface({ input: stream, crlfDelay: Infinity });
  let entries = 0;
  let exact = null;
  const leafMatches = [];
  for await (const line of lines) {
    const comma = line.indexOf(',');
    if (comma <= 0) continue;
    entries += 1;
    const name = line.slice(0, comma).replaceAll('\\', '/');
    const lower = name.toLowerCase();
    if (lower === wanted) exact = name;
    if (path.posix.basename(lower) === wantedLeaf && leafMatches.length < 25) leafMatches.push(name);
  }
  return {
    path: manifestPath,
    bytes: stat.size,
    entries,
    requestedPath: wanted,
    exactMatch: exact,
    leafMatches,
    listed: Boolean(exact || leafMatches.length),
  };
}

async function statFile(filePath) {
  try {
    const stat = await fsp.stat(filePath);
    return stat.isFile() ? { exists: true, bytes: stat.size, modifiedAt: stat.mtime.toISOString() } : { exists: false };
  } catch (error) {
    if (error.code === 'ENOENT') return { exists: false };
    throw error;
  }
}

async function runBridgeAudit({
  gameRoot,
  modsRoot,
  logicalPath,
  verifiedBuilds = VERIFIED_BUILDS,
  nativeHarnessPath,
  crossModuleHarnessDirectory,
  observeOnlyAddonPath,
}) {
  const root = path.resolve(gameRoot || '');
  const modRoot = path.resolve(modsRoot || '');
  const requestedPath = normalizeLogicalPath(logicalPath);
  const gameExePath = path.join(root, 'NBA2K27.exe');
  const manifestPath = path.join(root, 'manifest');
  const modExePath = path.join(root, 'mod.exe');
  const looseAssetPath = path.resolve(modRoot, ...requestedPath.split('/'));
  const relativeAsset = path.relative(modRoot, looseAssetPath);
  if (!relativeAsset || relativeAsset.startsWith('..') || path.isAbsolute(relativeAsset)) throw new Error('Resolved audit asset is outside the mods folder.');

  const [executable, manifest, modExecutable, looseAsset] = await Promise.all([
    inspectExecutable(gameExePath, verifiedBuilds),
    inspectManifest(manifestPath, requestedPath),
    statFile(modExePath),
    statFile(looseAssetPath),
  ]);
  const stockManifestKnowsAsset = manifest.listed;
  const liveEvidence = evidenceForGame(executable.sha256);
  const [
    resourceDeviceBoundary,
    stockDeviceLifetimeBoundary,
    developmentManifestBoundary,
    offlineDelegation,
    nativeAbiHarness,
    crossModuleHarness,
    observeOnlyAddon,
  ] = await Promise.all([
    inspectResourceDeviceBoundary(gameExePath, executable.sha256),
    inspectStockDeviceLifetimeBoundary(gameExePath, executable.sha256),
    inspectDevelopmentManifestBoundary(gameExePath, executable.sha256),
    runOfflineDelegationSelfTest(),
    runNativeAbiHarness(nativeHarnessPath),
    runCrossModuleHarness(crossModuleHarnessDirectory),
    inspectObserveOnlyAddon(observeOnlyAddonPath),
  ]);
  return {
    auditedAt: new Date().toISOString(),
    mode: 'offline-read-only',
    requestedPath,
    executable,
    manifest,
    modExecutable,
    looseAsset,
    liveEvidence,
    resourceDeviceBoundary,
    stockDeviceLifetimeBoundary,
    developmentManifestBoundary,
    offlineDelegation,
    nativeAbiHarness,
    crossModuleHarness,
    observeOnlyAddon,
    findings: {
      exactBuildVerified: executable.exactVerifiedBuild,
      looseAssetPresent: looseAsset.exists,
      stockManifestKnowsAsset,
      stockDataOnlyRouteProven: stockManifestKnowsAsset && looseAsset.exists,
      liveObservationStillRequired: !stockManifestKnowsAsset && looseAsset.exists,
      exactBuildHasPriorLiveReceiverEvidence: Boolean(liveEvidence),
      cleanRoomGameBridgeReady: Boolean(liveEvidence?.cleanRoomGameBridgeReady),
      resourceDeviceBoundaryVerified: Boolean(resourceDeviceBoundary.verified),
      managerLifecycleMapped: Boolean(resourceDeviceBoundary.registrationLifecycle?.verified),
      stockVtableFullyMapped: Boolean(stockDeviceLifetimeBoundary.claims?.stockVtableFullyMapped),
      stockAllocationBoundaryMapped: Boolean(stockDeviceLifetimeBoundary.claims?.allocationBoundaryMapped),
      foreignDeviceLifetimeProven: Boolean(stockDeviceLifetimeBoundary.claims?.foreignLifetimeProvenSafe),
      developmentManifestBoundaryVerified: Boolean(developmentManifestBoundary.verified),
      stockDevelopmentLooseModRouteUsable: Boolean(developmentManifestBoundary.verdict?.usableAsSafeLooseModSwitch),
      offlineDelegationPrototypeReady: Boolean(offlineDelegation.passed),
      nativeAbiHarnessReady: Boolean(nativeAbiHarness.passed),
      crossModuleLifetimeReady: Boolean(crossModuleHarness.passed),
      observeOnlyLifecycleAddonVerified: Boolean(observeOnlyAddon.verified),
      reversibleRegistrationProbeAvailable: Boolean(
        observeOnlyAddon.verified
        && observeOnlyAddon.reversibleRegistrationProbeAvailable
        && resourceDeviceBoundary.registrationLifecycle?.verified
      ),
      customGetIdCallbackProbeAvailable: Boolean(
        observeOnlyAddon.verified
        && observeOnlyAddon.customGetIdCallbackProbeAvailable
        && stockDeviceLifetimeBoundary.claims?.stockVtableFullyMapped
      ),
      customExistsCallbackProbeAvailable: Boolean(
        observeOnlyAddon.verified
        && observeOnlyAddon.customExistsCallbackProbeAvailable
        && resourceDeviceBoundary.signatures?.wrapperExistsCall?.matched
        && stockDeviceLifetimeBoundary.claims?.stockVtableFullyMapped
      ),
      customFileInfoCallbackProbeAvailable: Boolean(
        observeOnlyAddon.verified
        && observeOnlyAddon.customFileInfoCallbackProbeAvailable
        && resourceDeviceBoundary.signatures?.baseExistsOpenCall?.matched
        && resourceDeviceBoundary.deviceDispatch?.openResultBytes === 0x130
        && stockDeviceLifetimeBoundary.claims?.stockVtableFullyMapped
      ),
      managerOwnedStreamProbeAvailable: Boolean(
        observeOnlyAddon.verified
        && observeOnlyAddon.managerOwnedStreamProbeAvailable
        && resourceDeviceBoundary.signatures?.managerOpen?.matched
        && resourceDeviceBoundary.signatures?.managerOpenDeviceCall?.matched
        && resourceDeviceBoundary.signatures?.fileHandleConstructor?.matched
        && resourceDeviceBoundary.signatures?.fileHandleInitialize?.matched
        && resourceDeviceBoundary.signatures?.fileHandleRead?.matched
        && resourceDeviceBoundary.signatures?.fileHandleDestructor?.matched
        && resourceDeviceBoundary.deviceDispatch?.managerOwnedStream?.matched
        && stockDeviceLifetimeBoundary.claims?.stockVtableFullyMapped
      ),
      registrationPrototypeReady: false,
    },
    safety: {
      launchedGame: false,
      launchedModium: false,
      attachedToProcess: false,
      wroteGameFolder: false,
      modifiedManifest: false,
    },
  };
}

module.exports = {
  VERIFIED_BUILDS,
  inspectExecutable,
  inspectManifest,
  normalizeLogicalPath,
  readPeIdentity,
  runBridgeAudit,
  sha256File,
};
