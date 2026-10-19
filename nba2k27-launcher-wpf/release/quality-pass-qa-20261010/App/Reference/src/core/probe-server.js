const net = require('node:net');

const PROBE_PIPE_PATH = String.raw`\\.\pipe\NBA2K27-ModLauncher-Lifecycle-v16`;
const LIFECYCLE_MAGIC = 0x394f324e;
const ACK_MAGIC = 0x394b324e;
const PROTOCOL_VERSION = 16;
const LIFECYCLE_BYTES = 312;
const ACK_BYTES = 1456;
const ACK_PROVIDER_PIPE_BYTES = 192;
const ACK_PROVIDER_PATH_BYTES = 260;
const EXPECTED_TIMESTAMP = 0x6a922e5c;
const EXPECTED_IMAGE_SIZE = 0x353f7000;
const EVENT_INITIALIZE = 1;
const EVENT_HEARTBEAT = 2;
const EVENT_SHUTDOWN = 3;
const EVENT_REGISTRATION_RESULT = 4;
const EVENT_LIVE_ROUTE_RESULT = 5;
const EVENT_PATH_DIAGNOSTIC = 6;
const COMMAND_REGISTRATION_PROBE = 1;
const COMMAND_LIVE_ROUTE = 4;
const LIVE_ROUTE_REQUIRED_CHECKS = 0x0000ffff;
const LIVE_ROUTE_FLAG_REGISTERED = 1 << 0;
const LIVE_ROUTE_FLAG_DEFAULT_ACTIVE = 1 << 1;
const LIVE_ROUTE_FLAG_TARGET_SEEN = 1 << 2;
const LIVE_ROUTE_FLAG_DRAINING = 1 << 3;
const LIVE_ROUTE_ALLOWED_FLAGS = LIVE_ROUTE_FLAG_REGISTERED
  | LIVE_ROUTE_FLAG_DEFAULT_ACTIVE | LIVE_ROUTE_FLAG_TARGET_SEEN
  | LIVE_ROUTE_FLAG_DRAINING;
const REGISTRATION_DEVICE_ID = 0xc27d1610;
const STOCK_DEFAULT_DEVICE_ID = 0xc7e442dc;
const REGISTRATION_SUCCESS = 0x504521a8;
const REGISTRATION_REQUIRED_CHECKS = 0xffffffff;
const REGISTRATION_GET_ID_SLOT = 20;
const REGISTRATION_FILE_INFO_SLOT = 33;
const REGISTRATION_OPEN_SLOT = REGISTRATION_FILE_INFO_SLOT;
const REGISTRATION_ACTUAL_OPEN_SLOT = 35;
const REGISTRATION_CLOSE_SLOT = 42;
const REGISTRATION_TRANSFER_SLOT = 43;
const PROCESS_POLL_INTERVAL_MS = 1000;

const EVENT_NAMES = Object.freeze({
  [EVENT_INITIALIZE]: 'initialize',
  [EVENT_HEARTBEAT]: 'heartbeat',
  [EVENT_SHUTDOWN]: 'shutdown',
  [EVENT_REGISTRATION_RESULT]: 'registration-result',
  [EVENT_LIVE_ROUTE_RESULT]: 'live-route-result',
  [EVENT_PATH_DIAGNOSTIC]: 'path-diagnostic',
});

function parseLifecycleEvent(value) {
  if (!Buffer.isBuffer(value) || value.length !== LIFECYCLE_BYTES) {
    throw new Error('Invalid observe-only lifecycle packet.');
  }
  const session = (BigInt(value.readUInt32LE(36)) << 32n)
    | BigInt(value.readUInt32LE(32));
  const eventType = value.readUInt32LE(20);
  const event = {
    magic: value.readUInt32LE(0),
    version: value.readUInt32LE(4),
    pid: value.readUInt32LE(8),
    timestamp: value.readUInt32LE(12),
    imageSize: value.readUInt32LE(16),
    eventType,
    eventName: EVENT_NAMES[eventType] || 'unknown',
    sequence: value.readUInt32LE(24),
    flags: value.readUInt32LE(28),
    sessionId: session.toString(16).padStart(16, '0'),
    callbacksInFlight: value.readUInt32LE(40),
    resourceHandles: value.readUInt32LE(44),
    operationStatus: value.readUInt32LE(48),
    deviceId: value.readUInt32LE(52),
    registrationStatus: value.readUInt32LE(56),
    defaultBefore: value.readUInt32LE(60),
    defaultDuring: value.readUInt32LE(64),
    defaultAfter: value.readUInt32LE(68),
    registrationChecks: value.readUInt32LE(72),
    callbackCalls: value.readUInt32LE(76),
    callbackSlot: value.readUInt32LE(80),
    openCalls: value.readUInt32LE(84),
    openSuccessResults: value.readUInt32LE(88),
    openSlot: value.readUInt32LE(92),
    existsDispatchResult: value.readUInt32LE(96),
    reserved: value.readUInt32LE(100),
    actualOpenCalls: value.readUInt32LE(104),
    actualOpenSuccesses: value.readUInt32LE(108),
    transferCalls: value.readUInt32LE(112),
    closeCalls: value.readUInt32LE(116),
    payloadMatches: value.readUInt32LE(120),
    actualOpenSlot: value.readUInt32LE(124),
    transferSlot: value.readUInt32LE(128),
    closeSlot: value.readUInt32LE(132),
    peakResourceHandles: value.readUInt32LE(136),
    managerOpenStatus: value.readUInt32LE(140),
    readStatus: value.readUInt32LE(144),
    closeStatus: value.readUInt32LE(148),
    providerQueryCalls: value.readUInt32LE(152),
    providerReadCalls: value.readUInt32LE(156),
    providerBytes: (BigInt(value.readUInt32LE(164)) << 32n)
      | BigInt(value.readUInt32LE(160)),
    assetSize: (BigInt(value.readUInt32LE(172)) << 32n)
      | BigInt(value.readUInt32LE(168)),
    unprefixedSelections: value.readUInt32LE(176),
    unprefixedModHits: value.readUInt32LE(180),
    stockFallbackCalls: value.readUInt32LE(184),
    stockFallbackSuccesses: value.readUInt32LE(188),
    stockBaselineOpenStatus: value.readUInt32LE(192),
    stockBaselineReadStatus: value.readUInt32LE(196),
    stockBaselineBytes: value.readUInt32LE(200),
    stockManagerOpenStatus: value.readUInt32LE(204),
    stockReadStatus: value.readUInt32LE(208),
    stockCloseStatus: value.readUInt32LE(212),
    stockActualOpenCalls: value.readUInt32LE(216),
    stockActualOpenSuccesses: value.readUInt32LE(220),
    stockTransferCalls: value.readUInt32LE(224),
    stockCloseCalls: value.readUInt32LE(228),
    stockPayloadMatches: value.readUInt32LE(232),
    stockBytes: value.readUInt32LE(236),
    stockExpectedBytes: value.readUInt32LE(240),
    unprefixedOpenAttempts: value.readUInt32LE(244),
    unprefixedOpenSuccesses: value.readUInt32LE(248),
    unprefixedLocalOpenSuccesses: value.readUInt32LE(252),
    unprefixedStockOpenSuccesses: value.readUInt32LE(256),
    unprefixedOpenRestores: value.readUInt32LE(260),
    openDefaultDuring: value.readUInt32LE(264),
    openDefaultAfter: value.readUInt32LE(268),
    bindingCapacity: value.readUInt32LE(272),
    bindingAllocations: value.readUInt32LE(276),
    bindingReleases: value.readUInt32LE(280),
    bindingPeak: value.readUInt32LE(284),
    bindingActiveAfter: value.readUInt32LE(288),
    interleavedReadRounds: value.readUInt32LE(292),
    secondAssetSize: (BigInt(value.readUInt32LE(300)) << 32n)
      | BigInt(value.readUInt32LE(296)),
    distinctLocalPaths: value.readUInt32LE(304),
    pathBoundProviderHandles: value.readUInt32LE(308),
  };
  if (eventType === EVENT_PATH_DIAGNOSTIC) {
    const payload = value.subarray(48, 308);
    const terminator = payload.indexOf(0);
    event.diagnosticPath = terminator > 0
      ? payload.subarray(0, terminator).toString('ascii')
      : null;
    event.diagnosticPayloadValid = terminator > 0
      && payload.subarray(terminator).every((byte) => byte === 0);
  }
  return event;
}

function encodeLifecycleEvent({
  pid = 1234,
  timestamp = EXPECTED_TIMESTAMP,
  imageSize = EXPECTED_IMAGE_SIZE,
  eventType = EVENT_INITIALIZE,
  sequence = 1,
  flags = 0,
  sessionId = 1n,
  callbacksInFlight = 0,
  resourceHandles = 0,
  operationStatus = 0,
  deviceId = 0,
  registrationStatus = 0,
  defaultBefore = 0,
  defaultDuring = 0,
  defaultAfter = 0,
  registrationChecks = 0,
  callbackCalls = 0,
  callbackSlot = 0,
  openCalls = 0,
  openSuccessResults = 0,
  openSlot = 0,
  existsDispatchResult = 0,
  reserved = 0,
  actualOpenCalls = 0,
  actualOpenSuccesses = 0,
  transferCalls = 0,
  closeCalls = 0,
  payloadMatches = 0,
  actualOpenSlot = 0,
  transferSlot = 0,
  closeSlot = 0,
  peakResourceHandles = 0,
  managerOpenStatus = 0,
  readStatus = 0,
  closeStatus = 0,
  providerQueryCalls = 0,
  providerReadCalls = 0,
  providerBytes = 0n,
  assetSize = 0n,
  unprefixedSelections = 0,
  unprefixedModHits = 0,
  stockFallbackCalls = 0,
  stockFallbackSuccesses = 0,
  stockBaselineOpenStatus = 0,
  stockBaselineReadStatus = 0,
  stockBaselineBytes = 0,
  stockManagerOpenStatus = 0,
  stockReadStatus = 0,
  stockCloseStatus = 0,
  stockActualOpenCalls = 0,
  stockActualOpenSuccesses = 0,
  stockTransferCalls = 0,
  stockCloseCalls = 0,
  stockPayloadMatches = 0,
  stockBytes = 0,
  stockExpectedBytes = 0,
  unprefixedOpenAttempts = 0,
  unprefixedOpenSuccesses = 0,
  unprefixedLocalOpenSuccesses = 0,
  unprefixedStockOpenSuccesses = 0,
  unprefixedOpenRestores = 0,
  openDefaultDuring = 0,
  openDefaultAfter = 0,
  bindingCapacity = 0,
  bindingAllocations = 0,
  bindingReleases = 0,
  bindingPeak = 0,
  bindingActiveAfter = 0,
  interleavedReadRounds = 0,
  secondAssetSize = 0n,
  distinctLocalPaths = 0,
  pathBoundProviderHandles = 0,
  diagnosticPath = null,
} = {}) {
  const session = BigInt(sessionId);
  const value = Buffer.alloc(LIFECYCLE_BYTES);
  value.writeUInt32LE(LIFECYCLE_MAGIC, 0);
  value.writeUInt32LE(PROTOCOL_VERSION, 4);
  value.writeUInt32LE(pid >>> 0, 8);
  value.writeUInt32LE(timestamp >>> 0, 12);
  value.writeUInt32LE(imageSize >>> 0, 16);
  value.writeUInt32LE(eventType >>> 0, 20);
  value.writeUInt32LE(sequence >>> 0, 24);
  value.writeUInt32LE(flags >>> 0, 28);
  value.writeUInt32LE(Number(session & 0xffffffffn), 32);
  value.writeUInt32LE(Number((session >> 32n) & 0xffffffffn), 36);
  value.writeUInt32LE(callbacksInFlight >>> 0, 40);
  value.writeUInt32LE(resourceHandles >>> 0, 44);
  value.writeUInt32LE(operationStatus >>> 0, 48);
  value.writeUInt32LE(deviceId >>> 0, 52);
  value.writeUInt32LE(registrationStatus >>> 0, 56);
  value.writeUInt32LE(defaultBefore >>> 0, 60);
  value.writeUInt32LE(defaultDuring >>> 0, 64);
  value.writeUInt32LE(defaultAfter >>> 0, 68);
  value.writeUInt32LE(registrationChecks >>> 0, 72);
  value.writeUInt32LE(callbackCalls >>> 0, 76);
  value.writeUInt32LE(callbackSlot >>> 0, 80);
  value.writeUInt32LE(openCalls >>> 0, 84);
  value.writeUInt32LE(openSuccessResults >>> 0, 88);
  value.writeUInt32LE(openSlot >>> 0, 92);
  value.writeUInt32LE(existsDispatchResult >>> 0, 96);
  value.writeUInt32LE(reserved >>> 0, 100);
  value.writeUInt32LE(actualOpenCalls >>> 0, 104);
  value.writeUInt32LE(actualOpenSuccesses >>> 0, 108);
  value.writeUInt32LE(transferCalls >>> 0, 112);
  value.writeUInt32LE(closeCalls >>> 0, 116);
  value.writeUInt32LE(payloadMatches >>> 0, 120);
  value.writeUInt32LE(actualOpenSlot >>> 0, 124);
  value.writeUInt32LE(transferSlot >>> 0, 128);
  value.writeUInt32LE(closeSlot >>> 0, 132);
  value.writeUInt32LE(peakResourceHandles >>> 0, 136);
  value.writeUInt32LE(managerOpenStatus >>> 0, 140);
  value.writeUInt32LE(readStatus >>> 0, 144);
  value.writeUInt32LE(closeStatus >>> 0, 148);
  const providerByteCount = BigInt(providerBytes);
  const assetByteCount = BigInt(assetSize);
  value.writeUInt32LE(providerQueryCalls >>> 0, 152);
  value.writeUInt32LE(providerReadCalls >>> 0, 156);
  value.writeUInt32LE(Number(providerByteCount & 0xffffffffn), 160);
  value.writeUInt32LE(Number((providerByteCount >> 32n) & 0xffffffffn), 164);
  value.writeUInt32LE(Number(assetByteCount & 0xffffffffn), 168);
  value.writeUInt32LE(Number((assetByteCount >> 32n) & 0xffffffffn), 172);
  value.writeUInt32LE(unprefixedSelections >>> 0, 176);
  value.writeUInt32LE(unprefixedModHits >>> 0, 180);
  value.writeUInt32LE(stockFallbackCalls >>> 0, 184);
  value.writeUInt32LE(stockFallbackSuccesses >>> 0, 188);
  value.writeUInt32LE(stockBaselineOpenStatus >>> 0, 192);
  value.writeUInt32LE(stockBaselineReadStatus >>> 0, 196);
  value.writeUInt32LE(stockBaselineBytes >>> 0, 200);
  value.writeUInt32LE(stockManagerOpenStatus >>> 0, 204);
  value.writeUInt32LE(stockReadStatus >>> 0, 208);
  value.writeUInt32LE(stockCloseStatus >>> 0, 212);
  value.writeUInt32LE(stockActualOpenCalls >>> 0, 216);
  value.writeUInt32LE(stockActualOpenSuccesses >>> 0, 220);
  value.writeUInt32LE(stockTransferCalls >>> 0, 224);
  value.writeUInt32LE(stockCloseCalls >>> 0, 228);
  value.writeUInt32LE(stockPayloadMatches >>> 0, 232);
  value.writeUInt32LE(stockBytes >>> 0, 236);
  value.writeUInt32LE(stockExpectedBytes >>> 0, 240);
  value.writeUInt32LE(unprefixedOpenAttempts >>> 0, 244);
  value.writeUInt32LE(unprefixedOpenSuccesses >>> 0, 248);
  value.writeUInt32LE(unprefixedLocalOpenSuccesses >>> 0, 252);
  value.writeUInt32LE(unprefixedStockOpenSuccesses >>> 0, 256);
  value.writeUInt32LE(unprefixedOpenRestores >>> 0, 260);
  value.writeUInt32LE(openDefaultDuring >>> 0, 264);
  value.writeUInt32LE(openDefaultAfter >>> 0, 268);
  value.writeUInt32LE(bindingCapacity >>> 0, 272);
  value.writeUInt32LE(bindingAllocations >>> 0, 276);
  value.writeUInt32LE(bindingReleases >>> 0, 280);
  value.writeUInt32LE(bindingPeak >>> 0, 284);
  value.writeUInt32LE(bindingActiveAfter >>> 0, 288);
  value.writeUInt32LE(interleavedReadRounds >>> 0, 292);
  const secondAssetByteCount = BigInt(secondAssetSize);
  value.writeUInt32LE(Number(secondAssetByteCount & 0xffffffffn), 296);
  value.writeUInt32LE(Number((secondAssetByteCount >> 32n) & 0xffffffffn), 300);
  value.writeUInt32LE(distinctLocalPaths >>> 0, 304);
  value.writeUInt32LE(pathBoundProviderHandles >>> 0, 308);
  if (eventType === EVENT_PATH_DIAGNOSTIC) {
    const encodedPath = Buffer.from(String(diagnosticPath || ''), 'ascii');
    if (!encodedPath.length || encodedPath.length >= 260) {
      throw new Error('Path diagnostic must contain from 1 to 259 ASCII bytes.');
    }
    value.fill(0, 48, 308);
    encodedPath.copy(value, 48);
  }
  return value;
}

function encodeAck(accepted, reason = 0, commandFlags = 0, providerConfig = null) {
  const value = Buffer.alloc(ACK_BYTES);
  value.writeUInt32LE(ACK_MAGIC, 0);
  value.writeUInt32LE(PROTOCOL_VERSION, 4);
  value.writeUInt32LE(accepted ? 1 : 0, 8);
  value.writeUInt32LE(reason >>> 0, 12);
  value.writeUInt32LE(commandFlags >>> 0, 16);
  value.writeUInt32LE(0, 20);
  if (providerConfig) {
    const pipe = Buffer.from(providerConfig.endpoint || '', 'utf8');
    const assets = Array.isArray(providerConfig.assets) ? providerConfig.assets : [];
    const expectedAssets = commandFlags === COMMAND_LIVE_ROUTE ? 0 : 2;
    if (assets.length !== expectedAssets) {
      throw new Error(`Exactly ${expectedAssets} native provider asset${expectedAssets === 1 ? '' : 's'} required.`);
    }
    if (expectedAssets === 0) {
      if (!pipe.length || pipe.length > ACK_PROVIDER_PIPE_BYTES) {
        throw new Error('Invalid native provider endpoint.');
      }
      value.writeUInt32LE(0, 24);
      value.writeUInt32LE(pipe.length, 28);
      pipe.copy(value, 80);
      return value;
    }
    const logicalPath = Buffer.from(assets[0].logicalPath || '', 'utf8');
    const sha256 = Buffer.from(assets[0].sha256 || '', 'hex');
    const assetSize = BigInt(assets[0].size);
    const extraAssets = assets.slice(1).map((asset) => ({
      logicalPath: Buffer.from(asset.logicalPath || '', 'utf8'),
      sha256: Buffer.from(asset.sha256 || '', 'hex'),
      size: BigInt(asset.size),
    }));
    if (!pipe.length || pipe.length > ACK_PROVIDER_PIPE_BYTES
      || !logicalPath.length || logicalPath.length > ACK_PROVIDER_PATH_BYTES
      || sha256.length !== 32
      || assetSize <= 0n || assetSize > 0xffffffffffffffffn
      || extraAssets.some((asset) => !asset.logicalPath.length
        || asset.logicalPath.length > ACK_PROVIDER_PATH_BYTES
        || asset.sha256.length !== 32
        || asset.size <= 0n || asset.size > 0xffffffffffffffffn)
      || new Set(assets.map((asset) => String(asset.logicalPath).toLowerCase())).size
        !== expectedAssets) {
      throw new Error('Invalid native provider configuration.');
    }
    value.writeUInt32LE(expectedAssets, 24);
    value.writeUInt32LE(pipe.length, 28);
    value.writeUInt32LE(logicalPath.length, 32);
    value.writeUInt32LE(0, 36);
    value.writeBigUInt64LE(assetSize, 40);
    sha256.copy(value, 48);
    pipe.copy(value, 80);
    logicalPath.copy(value, 272);
    extraAssets.forEach((asset, index) => {
      const offset = 532 + (index * 308);
      value.writeUInt32LE(asset.logicalPath.length, offset);
      value.writeUInt32LE(0, offset + 4);
      value.writeBigUInt64LE(asset.size, offset + 8);
      asset.sha256.copy(value, offset + 16);
      asset.logicalPath.copy(value, offset + 48);
    });
  }
  return value;
}

function hasUnexpectedRegistrationPayload(event) {
  return event.operationStatus !== 0 || event.deviceId !== 0
    || event.registrationStatus !== 0 || event.defaultBefore !== 0
    || event.defaultDuring !== 0 || event.defaultAfter !== 0
    || event.registrationChecks !== 0 || event.callbackCalls !== 0
    || event.callbackSlot !== 0 || event.openCalls !== 0
    || event.openSuccessResults !== 0 || event.openSlot !== 0
    || event.existsDispatchResult !== 0 || event.reserved !== 0
    || event.actualOpenCalls !== 0 || event.actualOpenSuccesses !== 0
    || event.transferCalls !== 0 || event.closeCalls !== 0
    || event.payloadMatches !== 0 || event.actualOpenSlot !== 0
    || event.transferSlot !== 0 || event.closeSlot !== 0
    || event.peakResourceHandles !== 0 || event.managerOpenStatus !== 0
    || event.readStatus !== 0 || event.closeStatus !== 0
    || event.providerQueryCalls !== 0 || event.providerReadCalls !== 0
    || event.providerBytes !== 0n || event.assetSize !== 0n
    || event.unprefixedSelections !== 0 || event.unprefixedModHits !== 0
    || event.stockFallbackCalls !== 0 || event.stockFallbackSuccesses !== 0
    || event.stockBaselineOpenStatus !== 0 || event.stockBaselineReadStatus !== 0
    || event.stockBaselineBytes !== 0 || event.stockManagerOpenStatus !== 0
    || event.stockReadStatus !== 0 || event.stockCloseStatus !== 0
    || event.stockActualOpenCalls !== 0 || event.stockActualOpenSuccesses !== 0
    || event.stockTransferCalls !== 0 || event.stockCloseCalls !== 0
    || event.stockPayloadMatches !== 0 || event.stockBytes !== 0
    || event.stockExpectedBytes !== 0
    || event.unprefixedOpenAttempts !== 0 || event.unprefixedOpenSuccesses !== 0
    || event.unprefixedLocalOpenSuccesses !== 0
    || event.unprefixedStockOpenSuccesses !== 0
    || event.unprefixedOpenRestores !== 0 || event.openDefaultDuring !== 0
    || event.openDefaultAfter !== 0 || event.bindingCapacity !== 0
    || event.bindingAllocations !== 0 || event.bindingReleases !== 0
    || event.bindingPeak !== 0 || event.bindingActiveAfter !== 0
    || event.interleavedReadRounds !== 0 || event.secondAssetSize !== 0n
    || event.distinctLocalPaths !== 0 || event.pathBoundProviderHandles !== 0;
}

function validLiveRouteResult(event) {
  return event.operationStatus === 1
    && event.deviceId === REGISTRATION_DEVICE_ID
    && event.registrationStatus === REGISTRATION_SUCCESS
    && event.defaultBefore === STOCK_DEFAULT_DEVICE_ID
    && event.defaultDuring === REGISTRATION_DEVICE_ID
    && event.defaultAfter === event.defaultBefore
    && event.registrationChecks === LIVE_ROUTE_REQUIRED_CHECKS
    && event.callbackCalls > 0 && event.callbackSlot === REGISTRATION_GET_ID_SLOT
    && event.openCalls > 0 && event.openSlot === REGISTRATION_FILE_INFO_SLOT
    && event.existsDispatchResult > 0
    && event.actualOpenCalls > 0 && event.actualOpenSuccesses > 0
    && event.actualOpenSlot === REGISTRATION_ACTUAL_OPEN_SLOT
    && event.transferCalls > 0 && event.transferSlot === REGISTRATION_TRANSFER_SLOT
    && event.closeCalls > 0 && event.closeSlot === REGISTRATION_CLOSE_SLOT
    && event.payloadMatches > 0
    && event.providerQueryCalls > 0 && event.providerReadCalls > 0
    && event.providerBytes > 0n && event.assetSize > 0n
    && event.unprefixedSelections === 1 && event.unprefixedModHits > 0
    && event.unprefixedLocalOpenSuccesses > 0
    && event.unprefixedOpenRestores === 1
    && event.openDefaultDuring === REGISTRATION_DEVICE_ID
    && event.openDefaultAfter === event.defaultBefore
    && event.bindingCapacity === 256 && event.bindingAllocations > 0
    && event.bindingReleases === event.bindingAllocations
    && event.bindingPeak > 0 && event.bindingPeak <= event.bindingCapacity
    && event.bindingActiveAfter === 0
    && event.pathBoundProviderHandles > 0
    && event.distinctLocalPaths > 0
    && event.reserved === 0;
}

function validPathDiagnostic(event) {
  const leaf = typeof event.diagnosticPath === 'string'
    ? event.diagnosticPath.split(/[\\/]/).pop()
    : '';
  return event.diagnosticPayloadValid === true
    && typeof event.diagnosticPath === 'string'
    && event.diagnosticPath.length > 0
    && event.diagnosticPath.length < 260
    && /^[A-Za-z0-9_./\\-]+$/.test(event.diagnosticPath)
    && /^png\d{5}(?:_config_[A-Za-z0-9_-]+)?\.iff$/i.test(leaf);
}

function acceptLifecycleEvent(event, current, at = new Date().toISOString(), {
  registrationProbeArmed = false,
  liveRouteArmed = false,
} = {}) {
  let reason = 0;
  const liveStatusEvent = (event.eventType === EVENT_HEARTBEAT
    || event.eventType === EVENT_PATH_DIAGNOSTIC) && liveRouteArmed;
  const validFlags = event.flags === 0 || (liveStatusEvent
    && (event.flags & ~LIVE_ROUTE_ALLOWED_FLAGS) === 0);
  if (event.magic !== LIFECYCLE_MAGIC || event.version !== PROTOCOL_VERSION
    || !validFlags || !EVENT_NAMES[event.eventType]) reason = 1;
  else if (event.timestamp !== EXPECTED_TIMESTAMP || event.imageSize !== EXPECTED_IMAGE_SIZE) reason = 2;
  else if (event.sessionId === '0000000000000000' || event.pid === 0 || event.sequence === 0) reason = 3;
  else if ((event.callbacksInFlight !== 0 || event.resourceHandles !== 0)
    && !liveStatusEvent) reason = 7;
  else if (liveStatusEvent && (event.callbacksInFlight > 256
    || event.resourceHandles > 256)) reason = 7;
  else if (event.eventType !== EVENT_REGISTRATION_RESULT
    && event.eventType !== EVENT_LIVE_ROUTE_RESULT
    && event.eventType !== EVENT_PATH_DIAGNOSTIC
    && hasUnexpectedRegistrationPayload(event)) reason = 1;
  else if (event.eventType === EVENT_REGISTRATION_RESULT && !registrationProbeArmed) reason = 8;
  else if (event.eventType === EVENT_REGISTRATION_RESULT && (
    event.operationStatus !== 1 || event.deviceId !== REGISTRATION_DEVICE_ID
    || event.registrationStatus !== REGISTRATION_SUCCESS
    || event.defaultBefore === 0 || event.defaultDuring !== REGISTRATION_DEVICE_ID
    || event.defaultAfter !== event.defaultBefore
    || ((event.registrationChecks & REGISTRATION_REQUIRED_CHECKS) >>> 0)
      !== REGISTRATION_REQUIRED_CHECKS
    || event.callbackCalls === 0 || event.callbackSlot !== REGISTRATION_GET_ID_SLOT
    || event.openCalls < 3 || event.openSuccessResults < 3
    || event.openSlot !== REGISTRATION_FILE_INFO_SLOT || event.existsDispatchResult !== 7
    || event.actualOpenCalls !== 4 || event.actualOpenSuccesses !== 4
    || event.transferCalls !== 8 || event.closeCalls !== 4
    || event.payloadMatches !== 2
    || event.actualOpenSlot !== REGISTRATION_ACTUAL_OPEN_SLOT
    || event.transferSlot !== REGISTRATION_TRANSFER_SLOT
    || event.closeSlot !== REGISTRATION_CLOSE_SLOT
    || event.peakResourceHandles !== 4
    || event.managerOpenStatus !== REGISTRATION_SUCCESS
    || event.readStatus !== REGISTRATION_SUCCESS
    || event.closeStatus !== REGISTRATION_SUCCESS
    || event.providerQueryCalls !== 2 || event.providerReadCalls === 0
    || event.providerBytes !== event.assetSize + event.secondAssetSize
    || event.assetSize === 0n || event.secondAssetSize === 0n
    || event.unprefixedSelections !== 3 || event.unprefixedModHits < 2
    || event.stockFallbackCalls < 1 || event.stockFallbackSuccesses < 1
    || event.stockBaselineOpenStatus !== REGISTRATION_SUCCESS
    || event.stockBaselineReadStatus !== REGISTRATION_SUCCESS
    || event.stockBaselineBytes !== event.stockExpectedBytes
    || event.stockManagerOpenStatus !== REGISTRATION_SUCCESS
    || event.stockReadStatus !== REGISTRATION_SUCCESS
    || event.stockCloseStatus !== REGISTRATION_SUCCESS
    || event.stockActualOpenCalls !== 2 || event.stockActualOpenSuccesses !== 2
    || event.stockTransferCalls !== 4 || event.stockCloseCalls !== 2
    || event.stockPayloadMatches !== 2
    || event.stockBytes !== event.stockExpectedBytes * 2
    || event.stockExpectedBytes !== 1604
    || event.unprefixedOpenAttempts !== 4 || event.unprefixedOpenSuccesses !== 4
    || event.unprefixedLocalOpenSuccesses !== 2
    || event.unprefixedStockOpenSuccesses !== 2
    || event.unprefixedOpenRestores !== 4
    || event.openDefaultDuring !== REGISTRATION_DEVICE_ID
    || event.openDefaultAfter !== event.defaultBefore
    || event.bindingCapacity !== 256 || event.bindingAllocations !== 4
    || event.bindingReleases !== 4 || event.bindingPeak !== 4
    || event.bindingActiveAfter !== 0 || event.interleavedReadRounds !== 2
    || event.distinctLocalPaths !== 1 || event.pathBoundProviderHandles !== 2
    || event.reserved !== 0
  )) reason = 9;
  else if (event.eventType === EVENT_LIVE_ROUTE_RESULT && !liveRouteArmed) reason = 8;
  else if (event.eventType === EVENT_LIVE_ROUTE_RESULT
    && !validLiveRouteResult(event)) reason = 10;
  else if (event.eventType === EVENT_PATH_DIAGNOSTIC && !liveRouteArmed) reason = 8;
  else if (event.eventType === EVENT_PATH_DIAGNOSTIC
    && !validPathDiagnostic(event)) reason = 11;
  else if (event.eventType === EVENT_INITIALIZE && event.sequence !== 1) reason = 6;
  else if (event.eventType !== EVENT_INITIALIZE && (
    !current?.active || current.sessionId !== event.sessionId || current.pid !== event.pid
  )) reason = 5;
  else if (event.eventType !== EVENT_INITIALIZE && event.sequence <= current.lastSequence) reason = 6;

  if (reason !== 0) return { accepted: false, reason, lifecycle: current };
  if (event.eventType === EVENT_INITIALIZE) {
    return {
      accepted: true,
      reason: 0,
      lifecycle: {
        active: true,
        orderlyShutdown: false,
        processExitConfirmed: false,
        exitDetectedAt: null,
        pid: event.pid,
        sessionId: event.sessionId,
        initializedAt: at,
        lastHeartbeatAt: null,
        shutdownAt: null,
        heartbeatCount: 0,
        lastSequence: event.sequence,
        callbacksInFlight: 0,
        resourceHandles: 0,
        registrationProbeArmed,
        liveRouteArmed,
        liveRouteState: null,
        pathDiagnostic: null,
        pathDiagnostics: [],
        liveRoute: null,
        registration: null,
        hooksInstalled: false,
        resourceReplacementEnabled: false,
        deviceRegistrationEnabled: false,
      },
    };
  }
  if (event.eventType === EVENT_HEARTBEAT) {
    const routeState = liveRouteArmed ? {
      registrationActive: Boolean(event.flags & LIVE_ROUTE_FLAG_REGISTERED),
      defaultActive: Boolean(event.flags & LIVE_ROUTE_FLAG_DEFAULT_ACTIVE),
      targetSeen: Boolean(event.flags & LIVE_ROUTE_FLAG_TARGET_SEEN),
      draining: Boolean(event.flags & LIVE_ROUTE_FLAG_DRAINING),
      callbacksInFlight: event.callbacksInFlight,
      resourceHandles: event.resourceHandles,
    } : current.liveRouteState;
    return {
      accepted: true,
      reason: 0,
      lifecycle: {
        ...current,
        lastHeartbeatAt: at,
        heartbeatCount: current.heartbeatCount + 1,
        lastSequence: event.sequence,
        callbacksInFlight: event.callbacksInFlight,
        resourceHandles: event.resourceHandles,
        liveRouteArmed,
        liveRouteState: routeState,
        deviceRegistrationEnabled: Boolean(routeState?.registrationActive),
        resourceReplacementEnabled: Boolean(routeState?.targetSeen),
      },
    };
  }
  if (event.eventType === EVENT_PATH_DIAGNOSTIC) {
    const previous = Array.isArray(current.pathDiagnostics)
      ? current.pathDiagnostics
      : [];
    const pathDiagnostics = previous.some((item) =>
      item.requestPath.toLowerCase() === event.diagnosticPath.toLowerCase())
      ? previous
      : [...previous, { at, requestPath: event.diagnosticPath }].slice(-64);
    return {
      accepted: true,
      reason: 0,
      lifecycle: {
        ...current,
        lastSequence: event.sequence,
        callbacksInFlight: event.callbacksInFlight,
        resourceHandles: event.resourceHandles,
        pathDiagnostic: {
          at,
          requestPath: event.diagnosticPath,
        },
        pathDiagnostics,
      },
    };
  }
  if (event.eventType === EVENT_REGISTRATION_RESULT) {
    return {
      accepted: true,
      reason: 0,
      lifecycle: {
        ...current,
        lastSequence: event.sequence,
        registration: {
          passed: true,
          at,
          deviceId: event.deviceId,
          status: event.registrationStatus,
          defaultDeviceId: event.defaultBefore,
          temporaryDefaultDeviceId: event.defaultDuring,
          restoredDefaultDeviceId: event.defaultAfter,
          checks: event.registrationChecks,
          callbackCalls: event.callbackCalls,
          callbackSlot: event.callbackSlot,
          openCalls: event.openCalls,
          openSuccessResults: event.openSuccessResults,
          openSlot: event.openSlot,
          existsDispatchResult: event.existsDispatchResult,
          actualOpenCalls: event.actualOpenCalls,
          actualOpenSuccesses: event.actualOpenSuccesses,
          transferCalls: event.transferCalls,
          closeCalls: event.closeCalls,
          payloadMatches: event.payloadMatches,
          actualOpenSlot: event.actualOpenSlot,
          transferSlot: event.transferSlot,
          closeSlot: event.closeSlot,
          peakResourceHandles: event.peakResourceHandles,
          managerOpenStatus: event.managerOpenStatus,
          readStatus: event.readStatus,
          closeStatus: event.closeStatus,
          providerQueryCalls: event.providerQueryCalls,
          providerReadCalls: event.providerReadCalls,
          providerBytes: event.providerBytes.toString(),
          assetSize: event.assetSize.toString(),
          unprefixedSelections: event.unprefixedSelections,
          unprefixedModHits: event.unprefixedModHits,
          stockFallbackCalls: event.stockFallbackCalls,
          stockFallbackSuccesses: event.stockFallbackSuccesses,
          stockBaselineOpenStatus: event.stockBaselineOpenStatus,
          stockBaselineReadStatus: event.stockBaselineReadStatus,
          stockBaselineBytes: event.stockBaselineBytes,
          stockManagerOpenStatus: event.stockManagerOpenStatus,
          stockReadStatus: event.stockReadStatus,
          stockCloseStatus: event.stockCloseStatus,
          stockActualOpenCalls: event.stockActualOpenCalls,
          stockActualOpenSuccesses: event.stockActualOpenSuccesses,
          stockTransferCalls: event.stockTransferCalls,
          stockCloseCalls: event.stockCloseCalls,
          stockPayloadMatches: event.stockPayloadMatches,
          stockBytes: event.stockBytes,
          stockExpectedBytes: event.stockExpectedBytes,
          unprefixedOpenAttempts: event.unprefixedOpenAttempts,
          unprefixedOpenSuccesses: event.unprefixedOpenSuccesses,
          unprefixedLocalOpenSuccesses: event.unprefixedLocalOpenSuccesses,
          unprefixedStockOpenSuccesses: event.unprefixedStockOpenSuccesses,
          unprefixedOpenRestores: event.unprefixedOpenRestores,
          openDefaultDuring: event.openDefaultDuring,
          openDefaultAfter: event.openDefaultAfter,
          bindingCapacity: event.bindingCapacity,
          bindingAllocations: event.bindingAllocations,
          bindingReleases: event.bindingReleases,
          bindingPeak: event.bindingPeak,
          bindingActiveAfter: event.bindingActiveAfter,
          interleavedReadRounds: event.interleavedReadRounds,
          secondAssetSize: event.secondAssetSize.toString(),
          distinctLocalPaths: event.distinctLocalPaths,
          pathBoundProviderHandles: event.pathBoundProviderHandles,
          activeRegistration: false,
        },
      },
    };
  }
  if (event.eventType === EVENT_LIVE_ROUTE_RESULT) {
    return {
      accepted: true,
      reason: 0,
      lifecycle: {
        ...current,
        lastSequence: event.sequence,
        callbacksInFlight: 0,
        resourceHandles: 0,
        liveRouteArmed: false,
        liveRouteState: {
          registrationActive: false,
          defaultActive: false,
          targetSeen: true,
          draining: false,
          callbacksInFlight: 0,
          resourceHandles: 0,
        },
        liveRoute: {
          passed: true,
          at,
          deviceId: event.deviceId,
          registrationStatus: event.registrationStatus,
          defaultDeviceId: event.defaultBefore,
          temporaryDefaultDeviceId: event.defaultDuring,
          restoredDefaultDeviceId: event.defaultAfter,
          checks: event.registrationChecks,
          callbackCalls: event.callbackCalls,
          targetFileInfoHits: event.existsDispatchResult,
          actualOpenCalls: event.actualOpenCalls,
          actualOpenSuccesses: event.actualOpenSuccesses,
          targetOpenSuccesses: event.unprefixedLocalOpenSuccesses,
          transferCalls: event.transferCalls,
          targetTransferCalls: event.interleavedReadRounds,
          closeCalls: event.closeCalls,
          targetCloseSuccesses: event.payloadMatches,
          providerQueryCalls: event.providerQueryCalls,
          providerReadCalls: event.providerReadCalls,
          providerBytes: event.providerBytes.toString(),
          assetSize: event.assetSize.toString(),
          stockFileInfoCalls: event.stockFallbackCalls,
          stockFileInfoSuccesses: event.stockFallbackSuccesses,
          stockActualOpenCalls: event.stockActualOpenCalls,
          stockActualOpenSuccesses: event.stockActualOpenSuccesses,
          stockTransferCalls: event.stockTransferCalls,
          stockCloseCalls: event.stockCloseCalls,
          bindingCapacity: event.bindingCapacity,
          bindingAllocations: event.bindingAllocations,
          bindingReleases: event.bindingReleases,
          bindingPeak: event.bindingPeak,
          bindingActiveAfter: event.bindingActiveAfter,
          pathBoundProviderHandles: event.pathBoundProviderHandles,
          leafAliasHits: event.distinctLocalPaths,
          activeRegistration: false,
        },
        deviceRegistrationEnabled: false,
        resourceReplacementEnabled: false,
      },
    };
  }
  return {
    accepted: true,
    reason: 0,
    lifecycle: {
      ...current,
      active: false,
      orderlyShutdown: true,
      shutdownAt: at,
      lastSequence: event.sequence,
      callbacksInFlight: event.callbacksInFlight,
      resourceHandles: event.resourceHandles,
    },
  };
}

function isProcessAlive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return error?.code !== 'ESRCH';
  }
}

function containClientSocketError(socket) {
  socket.on('error', () => {
    if (!socket.destroyed) socket.destroy();
  });
  return socket;
}

class ObserveOnlyProbeServer {
  constructor(onStatus = () => {}, pipePath = PROBE_PIPE_PATH, {
    processAlive = isProcessAlive,
    processPollIntervalMs = PROCESS_POLL_INTERVAL_MS,
  } = {}) {
    this.onStatus = onStatus;
    this.pipePath = pipePath;
    this.processAlive = processAlive;
    this.processPollIntervalMs = processPollIntervalMs;
    this.server = null;
    this.sockets = new Set();
    this.lastHandshake = null;
    this.lifecycle = null;
    this.processMonitor = null;
    this.registrationProbeArmed = false;
    this.providerConfig = null;
    this.liveRouteArmed = false;
    this.liveRouteConfig = null;
  }

  get status() {
    const activeConfig = this.liveRouteConfig || this.providerConfig;
    const providerAssets = Array.isArray(activeConfig?.assets)
      ? activeConfig.assets.map(({ logicalPath, size, sha256 }) => ({ logicalPath, size, sha256 }))
      : [];
    return {
      armed: Boolean(this.server),
      pipePath: this.pipePath,
      lastHandshake: this.lastHandshake,
      lifecycle: this.lifecycle,
      registrationProbeArmed: this.registrationProbeArmed,
      liveRouteArmed: this.liveRouteArmed,
      providerAsset: providerAssets[0] || null,
      providerAssets,
    };
  }

  publish() { this.onStatus(this.status); }

  clearProcessMonitor() {
    if (this.processMonitor) clearInterval(this.processMonitor);
    this.processMonitor = null;
  }

  checkTrackedProcess() {
    const lifecycle = this.lifecycle;
    if (!lifecycle?.active || this.processAlive(lifecycle.pid)) return;
    const at = new Date().toISOString();
    this.lifecycle = {
      ...lifecycle,
      active: false,
      processExitConfirmed: true,
      exitDetectedAt: at,
    };
    this.clearProcessMonitor();
    this.publish();
  }

  startProcessMonitor() {
    this.clearProcessMonitor();
    this.processMonitor = setInterval(() => this.checkTrackedProcess(), this.processPollIntervalMs);
    this.processMonitor.unref?.();
  }

  armLiveRoute(providerConfig) {
    if (!this.server || !this.lifecycle?.active) {
      throw new Error('Start the probe and wait for a LIVE game handshake first.');
    }
    if (this.registrationProbeArmed) {
      throw new Error('The controlled path-aware test and live route cannot run together.');
    }
    if (this.liveRouteArmed || this.lifecycle.liveRoute) {
      throw new Error('The local fallback is already armed or completed.');
    }
    encodeAck(true, 0, COMMAND_LIVE_ROUTE, providerConfig);
    this.liveRouteConfig = providerConfig;
    this.liveRouteArmed = true;
    this.lifecycle = {
      ...this.lifecycle,
      liveRouteArmed: true,
      liveRouteState: {
        registrationActive: false,
        defaultActive: false,
        targetSeen: false,
        draining: false,
        callbacksInFlight: 0,
        resourceHandles: 0,
      },
      pathDiagnostic: null,
      pathDiagnostics: [],
    };
    this.publish();
    return this.status;
  }

  async waitForLiveRouteActivation(timeoutMs = 5000, pollIntervalMs = 50) {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      const status = this.status;
      const routeState = status.lifecycle?.liveRouteState;
      if (!status.liveRouteArmed
        || (routeState?.registrationActive && routeState?.defaultActive)) {
        return status;
      }
      await new Promise((resolve) => setTimeout(resolve, pollIntervalMs));
    }
    return this.status;
  }

  async start({ registrationProbe = false, providerConfig = null } = {}) {
    if (this.server) return this.status;
    this.clearProcessMonitor();
    this.lastHandshake = null;
    this.lifecycle = null;
    this.registrationProbeArmed = registrationProbe === true;
    this.providerConfig = providerConfig;
    this.liveRouteArmed = false;
    this.liveRouteConfig = null;
    const server = net.createServer((socket) => {
      containClientSocketError(socket);
      this.sockets.add(socket);socket.once('close',()=>this.sockets.delete(socket));
      const chunks = [];
      let bytes = 0;
      socket.setTimeout(1500, () => socket.destroy());
      socket.on('data', (chunk) => {
        if (bytes >= LIFECYCLE_BYTES) return;
        const take = chunk.subarray(0, LIFECYCLE_BYTES - bytes);
        chunks.push(take);
        bytes += take.length;
        if (bytes !== LIFECYCLE_BYTES) return;
        let event;
        let decision = { accepted: false, reason: 3, lifecycle: this.lifecycle };
        const at = new Date().toISOString();
        try {
          event = parseLifecycleEvent(Buffer.concat(chunks, LIFECYCLE_BYTES));
          decision = acceptLifecycleEvent(event, this.lifecycle, at, {
            registrationProbeArmed: this.registrationProbeArmed,
            liveRouteArmed: this.liveRouteArmed,
          });
        } catch {
          decision = { accepted: false, reason: 3, lifecycle: this.lifecycle };
        }
        if (!decision.accepted && event?.eventType === EVENT_REGISTRATION_RESULT
          && this.lifecycle?.active) {
          this.lifecycle = {
            ...this.lifecycle,
            registrationFailure: {
              at,
              reason: decision.reason,
              operationStatus: event.operationStatus,
              deviceId: event.deviceId,
              registrationStatus: event.registrationStatus,
              defaultBefore: event.defaultBefore,
              defaultDuring: event.defaultDuring,
              defaultAfter: event.defaultAfter,
              checks: event.registrationChecks,
              callbackCalls: event.callbackCalls,
              callbackSlot: event.callbackSlot,
              fileInfoCalls: event.openCalls,
              fileInfoSuccesses: event.openSuccessResults,
              fileInfoSlot: event.openSlot,
              existsDispatchResult: event.existsDispatchResult,
              actualOpenCalls: event.actualOpenCalls,
              actualOpenSuccesses: event.actualOpenSuccesses,
              transferCalls: event.transferCalls,
              closeCalls: event.closeCalls,
              payloadMatches: event.payloadMatches,
              actualOpenSlot: event.actualOpenSlot,
              transferSlot: event.transferSlot,
              closeSlot: event.closeSlot,
              peakResourceHandles: event.peakResourceHandles,
              managerOpenStatus: event.managerOpenStatus,
              readStatus: event.readStatus,
              closeStatus: event.closeStatus,
              providerQueryCalls: event.providerQueryCalls,
              providerReadCalls: event.providerReadCalls,
              providerBytes: event.providerBytes.toString(),
              assetSize: event.assetSize.toString(),
              unprefixedSelections: event.unprefixedSelections,
              unprefixedModHits: event.unprefixedModHits,
              stockFallbackCalls: event.stockFallbackCalls,
              stockFallbackSuccesses: event.stockFallbackSuccesses,
              stockBaselineOpenStatus: event.stockBaselineOpenStatus,
              stockBaselineReadStatus: event.stockBaselineReadStatus,
              stockBaselineBytes: event.stockBaselineBytes,
              stockManagerOpenStatus: event.stockManagerOpenStatus,
              stockReadStatus: event.stockReadStatus,
              stockCloseStatus: event.stockCloseStatus,
              stockActualOpenCalls: event.stockActualOpenCalls,
              stockActualOpenSuccesses: event.stockActualOpenSuccesses,
              stockTransferCalls: event.stockTransferCalls,
              stockCloseCalls: event.stockCloseCalls,
              stockPayloadMatches: event.stockPayloadMatches,
              stockBytes: event.stockBytes,
              stockExpectedBytes: event.stockExpectedBytes,
              unprefixedOpenAttempts: event.unprefixedOpenAttempts,
              unprefixedOpenSuccesses: event.unprefixedOpenSuccesses,
              unprefixedLocalOpenSuccesses: event.unprefixedLocalOpenSuccesses,
              unprefixedStockOpenSuccesses: event.unprefixedStockOpenSuccesses,
              unprefixedOpenRestores: event.unprefixedOpenRestores,
              openDefaultDuring: event.openDefaultDuring,
              openDefaultAfter: event.openDefaultAfter,
              bindingCapacity: event.bindingCapacity,
              bindingAllocations: event.bindingAllocations,
              bindingReleases: event.bindingReleases,
              bindingPeak: event.bindingPeak,
              bindingActiveAfter: event.bindingActiveAfter,
              interleavedReadRounds: event.interleavedReadRounds,
              secondAssetSize: event.secondAssetSize.toString(),
              distinctLocalPaths: event.distinctLocalPaths,
              pathBoundProviderHandles: event.pathBoundProviderHandles,
            },
          };
        } else if (!decision.accepted && event?.eventType === EVENT_LIVE_ROUTE_RESULT
          && this.lifecycle?.active) {
          this.lifecycle = {
            ...this.lifecycle,
            liveRouteFailure: {
              at,
              reason: decision.reason,
              operationStatus: event.operationStatus,
              deviceId: event.deviceId,
              registrationStatus: event.registrationStatus,
              defaultBefore: event.defaultBefore,
              defaultDuring: event.defaultDuring,
              defaultAfter: event.defaultAfter,
              checks: event.registrationChecks,
              targetFileInfoHits: event.existsDispatchResult,
              actualOpenCalls: event.actualOpenCalls,
              actualOpenSuccesses: event.actualOpenSuccesses,
              targetOpenSuccesses: event.unprefixedLocalOpenSuccesses,
              transferCalls: event.transferCalls,
              targetTransferCalls: event.interleavedReadRounds,
              closeCalls: event.closeCalls,
              targetCloseSuccesses: event.payloadMatches,
              providerQueryCalls: event.providerQueryCalls,
              providerReadCalls: event.providerReadCalls,
              providerBytes: event.providerBytes.toString(),
              assetSize: event.assetSize.toString(),
              stockFileInfoCalls: event.stockFallbackCalls,
              stockFileInfoSuccesses: event.stockFallbackSuccesses,
              bindingCapacity: event.bindingCapacity,
              bindingAllocations: event.bindingAllocations,
              bindingReleases: event.bindingReleases,
              bindingPeak: event.bindingPeak,
              bindingActiveAfter: event.bindingActiveAfter,
              pathBoundProviderHandles: event.pathBoundProviderHandles,
              leafAliasHits: event.distinctLocalPaths,
            },
            liveRouteArmed: false,
            liveRouteState: null,
            deviceRegistrationEnabled: false,
            resourceReplacementEnabled: false,
          };
          this.liveRouteArmed = false;
        } else {
          this.lifecycle = decision.lifecycle;
        }
        if (decision.accepted && event?.eventType === EVENT_INITIALIZE) this.startProcessMonitor();
        if (decision.accepted && event?.eventType === EVENT_SHUTDOWN) this.clearProcessMonitor();
        this.lastHandshake = {
          at,
          accepted: decision.accepted,
          reason: decision.reason,
          pid: event?.pid || 0,
          timestamp: event?.timestamp || 0,
          imageSize: event?.imageSize || 0,
          eventType: event?.eventType || 0,
          eventName: event?.eventName || 'invalid',
          sequence: event?.sequence || 0,
          sessionId: event?.sessionId || null,
          callbacksInFlight: event?.callbacksInFlight || 0,
          resourceHandles: event?.resourceHandles || 0,
          operationStatus: event?.operationStatus || 0,
          deviceId: event?.deviceId || 0,
          registrationStatus: event?.registrationStatus || 0,
          defaultBefore: event?.defaultBefore || 0,
          defaultDuring: event?.defaultDuring || 0,
          defaultAfter: event?.defaultAfter || 0,
          registrationChecks: event?.registrationChecks || 0,
          callbackCalls: event?.callbackCalls || 0,
          callbackSlot: event?.callbackSlot || 0,
          openCalls: event?.openCalls || 0,
          openSuccessResults: event?.openSuccessResults || 0,
          openSlot: event?.openSlot || 0,
          existsDispatchResult: event?.existsDispatchResult || 0,
          actualOpenCalls: event?.actualOpenCalls || 0,
          actualOpenSuccesses: event?.actualOpenSuccesses || 0,
          transferCalls: event?.transferCalls || 0,
          closeCalls: event?.closeCalls || 0,
          payloadMatches: event?.payloadMatches || 0,
          actualOpenSlot: event?.actualOpenSlot || 0,
          transferSlot: event?.transferSlot || 0,
          closeSlot: event?.closeSlot || 0,
          peakResourceHandles: event?.peakResourceHandles || 0,
          managerOpenStatus: event?.managerOpenStatus || 0,
          readStatus: event?.readStatus || 0,
          closeStatus: event?.closeStatus || 0,
          providerQueryCalls: event?.providerQueryCalls || 0,
          providerReadCalls: event?.providerReadCalls || 0,
          providerBytes: event?.providerBytes?.toString() || '0',
          assetSize: event?.assetSize?.toString() || '0',
          unprefixedSelections: event?.unprefixedSelections || 0,
          unprefixedModHits: event?.unprefixedModHits || 0,
          stockFallbackCalls: event?.stockFallbackCalls || 0,
          stockFallbackSuccesses: event?.stockFallbackSuccesses || 0,
          stockBaselineOpenStatus: event?.stockBaselineOpenStatus || 0,
          stockBaselineReadStatus: event?.stockBaselineReadStatus || 0,
          stockBaselineBytes: event?.stockBaselineBytes || 0,
          stockManagerOpenStatus: event?.stockManagerOpenStatus || 0,
          stockReadStatus: event?.stockReadStatus || 0,
          stockCloseStatus: event?.stockCloseStatus || 0,
          stockActualOpenCalls: event?.stockActualOpenCalls || 0,
          stockActualOpenSuccesses: event?.stockActualOpenSuccesses || 0,
          stockTransferCalls: event?.stockTransferCalls || 0,
          stockCloseCalls: event?.stockCloseCalls || 0,
          stockPayloadMatches: event?.stockPayloadMatches || 0,
          stockBytes: event?.stockBytes || 0,
          stockExpectedBytes: event?.stockExpectedBytes || 0,
          unprefixedOpenAttempts: event?.unprefixedOpenAttempts || 0,
          unprefixedOpenSuccesses: event?.unprefixedOpenSuccesses || 0,
          unprefixedLocalOpenSuccesses: event?.unprefixedLocalOpenSuccesses || 0,
          unprefixedStockOpenSuccesses: event?.unprefixedStockOpenSuccesses || 0,
          unprefixedOpenRestores: event?.unprefixedOpenRestores || 0,
          openDefaultDuring: event?.openDefaultDuring || 0,
          openDefaultAfter: event?.openDefaultAfter || 0,
          bindingCapacity: event?.bindingCapacity || 0,
          bindingAllocations: event?.bindingAllocations || 0,
          bindingReleases: event?.bindingReleases || 0,
          bindingPeak: event?.bindingPeak || 0,
          bindingActiveAfter: event?.bindingActiveAfter || 0,
          interleavedReadRounds: event?.interleavedReadRounds || 0,
          secondAssetSize: event?.secondAssetSize?.toString() || '0',
          distinctLocalPaths: event?.distinctLocalPaths || 0,
          pathBoundProviderHandles: event?.pathBoundProviderHandles || 0,
          diagnosticPath: event?.diagnosticPath || null,
          hooksInstalled: false,
          resourceReplacementEnabled: false,
          deviceRegistrationEnabled: false,
        };
        if (decision.accepted && event?.eventType === EVENT_LIVE_ROUTE_RESULT) {
          this.liveRouteArmed = false;
        }
        let commandFlags = 0;
        let commandProviderConfig = null;
        if (decision.accepted && event?.eventType === EVENT_INITIALIZE
          && this.registrationProbeArmed) {
          commandFlags = COMMAND_REGISTRATION_PROBE;
          commandProviderConfig = this.providerConfig;
        } else if (decision.accepted && event?.eventType === EVENT_HEARTBEAT
          && this.liveRouteArmed
          && !this.lifecycle?.liveRouteState?.registrationActive
          && !this.lifecycle?.liveRouteState?.targetSeen) {
          commandFlags = COMMAND_LIVE_ROUTE;
          commandProviderConfig = this.liveRouteConfig;
        }
        socket.end(encodeAck(
          decision.accepted,
          decision.reason,
          commandFlags,
          commandProviderConfig,
        ));
        this.publish();
      });
    });
    await new Promise((resolve, reject) => {
      const onError = (error) => { server.off('listening', onListening); reject(error); };
      const onListening = () => { server.off('error', onError); resolve(); };
      server.once('error', onError);
      server.once('listening', onListening);
      server.listen({ path: this.pipePath });
    });
    server.on('error', () => {
      if (this.server === server) {
        this.server = null;
        this.clearProcessMonitor();
        this.publish();
      }
    });
    server.unref();
    this.server = server;
    this.publish();
    return this.status;
  }

  async stop() {
    const server = this.server;
    this.server = null;
    this.clearProcessMonitor();
    for(const socket of this.sockets)socket.destroy();
    this.sockets.clear();
    if (server) await new Promise((resolve) => server.close(resolve));
    this.publish();
    return this.status;
  }
}

module.exports = {
  ACK_BYTES,
  ACK_MAGIC,
  COMMAND_LIVE_ROUTE,
  COMMAND_REGISTRATION_PROBE,
  EVENT_HEARTBEAT,
  EVENT_INITIALIZE,
  EVENT_LIVE_ROUTE_RESULT,
  EVENT_PATH_DIAGNOSTIC,
  EVENT_REGISTRATION_RESULT,
  EVENT_SHUTDOWN,
  EXPECTED_IMAGE_SIZE,
  EXPECTED_TIMESTAMP,
  LIFECYCLE_BYTES,
  LIFECYCLE_MAGIC,
  LIVE_ROUTE_ALLOWED_FLAGS,
  LIVE_ROUTE_FLAG_DEFAULT_ACTIVE,
  LIVE_ROUTE_FLAG_DRAINING,
  LIVE_ROUTE_FLAG_REGISTERED,
  LIVE_ROUTE_FLAG_TARGET_SEEN,
  LIVE_ROUTE_REQUIRED_CHECKS,
  ObserveOnlyProbeServer,
  PROCESS_POLL_INTERVAL_MS,
  PROBE_PIPE_PATH,
  PROTOCOL_VERSION,
  REGISTRATION_DEVICE_ID,
  REGISTRATION_ACTUAL_OPEN_SLOT,
  REGISTRATION_CLOSE_SLOT,
  REGISTRATION_FILE_INFO_SLOT,
  REGISTRATION_GET_ID_SLOT,
  REGISTRATION_OPEN_SLOT,
  REGISTRATION_REQUIRED_CHECKS,
  REGISTRATION_SUCCESS,
  REGISTRATION_TRANSFER_SLOT,
  STOCK_DEFAULT_DEVICE_ID,
  acceptLifecycleEvent,
  encodeAck,
  encodeLifecycleEvent,
  containClientSocketError,
  isProcessAlive,
  parseLifecycleEvent,
};
