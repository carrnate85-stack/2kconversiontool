const path = require('node:path');
const crypto = require('node:crypto');
const { spawn } = require('node:child_process');
const { fileKind } = require('./paths');
const { requestMockProvider, DIAGNOSTIC_CHUNK_BYTES } = require('./mock-provider');

const START_TIMEOUT_MS = 30000;
const STOP_TIMEOUT_MS = 5000;

async function waitForExit(child, timeoutMs = STOP_TIMEOUT_MS) {
  const exited=await require('./child-lifecycle').waitForExit(child,timeoutMs);
  // null is reserved for a timeout, not an already exited/signalled worker.
  return exited?(child.exitCode??-1):null;
}

function normalizeAliases(aliases = {}) {
  if (!aliases || typeof aliases !== 'object' || Array.isArray(aliases)) throw new Error('Provider aliases must be a logical-path map.');
  const rows = Object.entries(aliases);
  if (rows.length > 8) throw new Error('At most eight provider aliases are supported.');
  const safe = value => {
    const logical = String(value || '').replaceAll('\\', '/');
    if (!logical || logical.startsWith('/') || logical.split('/').some(part => !part || part === '.' || part === '..' || part.includes(':'))) throw new Error('Provider alias paths must remain relative to the mods folder.');
    return logical;
  };
  return Object.fromEntries(rows.map(([target, source]) => [safe(target), safe(source)]));
}

function normalizeMutableAliases(values = []) {
  if (!Array.isArray(values) || values.length > 4) throw new Error('At most four mutable provider aliases are supported.');
  const aliases = normalizeAliases(Object.fromEntries(values.map(value => [value, value])));
  return [...new Set(Object.keys(aliases))];
}

async function startProductionProvider({ modsRoot, hostExecutable, aliases = {}, mutableAliases = [] }) {
  if ((await fileKind(hostExecutable)) !== 'file') throw new Error('The production provider host is missing.');
  const pipeName = `NBA2K27ModProvider-${process.pid}-${crypto.randomUUID()}`;
  const endpoint = `\\\\.\\pipe\\${pipeName}`;
  const sessionToken = crypto.randomBytes(32).toString('hex');
  const child = spawn(hostExecutable, [], {
    cwd: path.dirname(hostExecutable),
    stdio: ['pipe', 'pipe', 'pipe'],
    windowsHide: true,
  });
  let stderr = '';
  let stdinError = '';
  child.stderr.on('data', (chunk) => { stderr = `${stderr}${chunk}`.slice(-8192); });
  child.stdin.on('error', (error) => { stdinError = error?.message || String(error); });

  const ready = new Promise((resolve, reject) => {
    let stdout = '';
    const timer = setTimeout(() => reject(new Error('Production provider startup timed out.')), START_TIMEOUT_MS);
    const finish = (callback, value) => { clearTimeout(timer); callback(value); };
    child.stdout.on('data', (chunk) => {
      stdout += chunk.toString('utf8');
      const newline = stdout.indexOf('\n');
      if (newline < 0) return;
      try {
        const payload = JSON.parse(stdout.slice(0, newline));
        if (!payload.ready) throw new Error('Provider did not report ready.');
        finish(resolve, payload);
      } catch (error) { finish(reject, error); }
    });
    child.once('error', (error) => finish(reject, error));
    child.once('exit', (code) => finish(reject, new Error(`Provider exited during startup (${code}): ${stderr.trim()}`)));
  });

  child.stdin.write(`${JSON.stringify({ pipeName, token: sessionToken, modsRoot, aliases: normalizeAliases(aliases), mutableAliases: normalizeMutableAliases(mutableAliases) })}\n`);
  let readyState;
  try { readyState = await ready; }
  catch (error) {
    child.kill();
    throw error;
  }

  let stopped = false;
  return {
    endpoint,
    pipeName,
    pid: child.pid,
    ready: readyState,
    request: async (request) => {
      if (stopped) throw new Error('The production provider session is closed.');
      const envelope = await requestMockProvider(endpoint, { ...request, sessionToken });
      return envelope.response;
    },
    rejectsToken: async (wrongToken) => {
      try {
        await requestMockProvider(endpoint, { operation: 'status', sessionToken: wrongToken });
        return false;
      } catch (error) {
        return /session token/i.test(error.message);
      }
    },
    stop: async () => {
      if (stopped && (child.exitCode !== null || child.signalCode)) return true;
      stopped = true;
      if (child.exitCode === null && child.stdin.writable && !child.stdin.destroyed) {
        child.stdin.end('STOP\n');
      }
      const code = await waitForExit(child);
      if (code === null) {
        child.kill();
        await waitForExit(child, 1000);
        return false;
      }
      if (code !== 0) {
        const detail = [stderr.trim(), stdinError].filter(Boolean).join('; ');
        throw new Error(`Provider shutdown failed (${code})${detail ? `: ${detail}` : ''}`);
      }
      return true;
    },
  };
}

async function runBinaryProviderSelfTest(hostExecutable) {
  if ((await fileKind(hostExecutable)) !== 'file') {
    return { available: false, passed: false, reason: 'The production provider host is missing.' };
  }
  const child = spawn(hostExecutable, ['--binary-self-test'], {
    cwd: path.dirname(hostExecutable),
    stdio: ['ignore', 'pipe', 'pipe'],
    windowsHide: true,
  });
  let stdout = '';
  let stderr = '';
  child.stdout.on('data', (chunk) => { stdout = `${stdout}${chunk}`.slice(-16384); });
  child.stderr.on('data', (chunk) => { stderr = `${stderr}${chunk}`.slice(-8192); });
  const code = await waitForExit(child, START_TIMEOUT_MS);
  if (code === null) {
    child.kill();
    await waitForExit(child, 1000);
    return { available: true, passed: false, reason: 'Binary provider self-test timed out.' };
  }
  if (code !== 0) {
    return { available: true, passed: false, reason: `Binary provider self-test failed (${code}): ${stderr.trim()}` };
  }
  try {
    const report = JSON.parse(stdout.trim());
    const passed = report.schema === 'nba2k27-binary-provider-v1'
      && report.passed === true
      && report.requestHeaderBytes === 24
      && report.responseHeaderBytes === 32
      && report.maxReadBytes === 1024 * 1024
      && report.boundsRejected === true
      && report.payloadRoundTrip === true;
    return { available: true, ...report, passed };
  } catch (error) {
    return { available: true, passed: false, reason: `Invalid binary provider self-test output: ${error.message}` };
  }
}

async function productionProviderDiagnostic({ modsRoot, hostExecutable, logicalPath }) {
  const provider = await startProductionProvider({ modsRoot, hostExecutable });
  let opened = null;
  try {
    const authenticationRejected = await provider.rejectsToken('00'.repeat(32));
    const metadata = await provider.request({ operation: 'query', logicalPath });
    opened = await provider.request({ operation: 'open', logicalPath });
    const hash = crypto.createHash('sha256');
    let fullBytesRead = 0;
    let readCalls = 0;
    while (fullBytesRead < metadata.size) {
      const response = await provider.request({
        operation: 'read', handle: opened.handle, length: DIAGNOSTIC_CHUNK_BYTES,
      });
      const data = Buffer.from(response.data, 'base64');
      if (data.length !== response.bytesRead || data.length === 0) throw new Error('Production provider stopped before the advertised asset size.');
      hash.update(data);
      fullBytesRead += data.length;
      readCalls += 1;
    }
    const eof = await provider.request({ operation: 'read', handle: opened.handle, length: 1 });
    const closed = await provider.request({ operation: 'close', handle: opened.handle });
    opened = null;
    const status = await provider.request({ operation: 'status' });
    return {
      ok: true,
      logicalPath: metadata.logicalPath,
      size: metadata.size,
      fullBytesRead,
      fullSha256: hash.digest('hex'),
      readCalls,
      eofBytesRead: eof.bytesRead,
      handleClosed: closed.closed,
      openHandlesAfterClose: status.openHandles,
      indexedAssets: status.indexedAssets,
      recentActivityCount: status.recentActivity.length,
      security: {
        ...status.security,
        invalidTokenRejected: authenticationRejected,
        internetTransport: false,
      },
      providerPid: provider.pid,
    };
  } finally {
    if (opened) await provider.request({ operation: 'close', handle: opened.handle }).catch(() => {});
    const stoppedCleanly = await provider.stop();
    if (!stoppedCleanly) throw new Error('Production provider required forced shutdown.');
  }
}

module.exports = {
  productionProviderDiagnostic,
  runBinaryProviderSelfTest,
  startProductionProvider,
  normalizeAliases,
  normalizeMutableAliases,
};
