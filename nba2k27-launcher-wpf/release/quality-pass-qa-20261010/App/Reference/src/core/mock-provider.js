const net = require('node:net');
const os = require('node:os');
const path = require('node:path');
const crypto = require('node:crypto');
const { AssetProviderService } = require('./asset-service');

const MAX_FRAME_BYTES = 2 * 1024 * 1024;
const DIAGNOSTIC_CHUNK_BYTES = 1024 * 1024;

function endpointName() {
  const id = crypto.randomUUID();
  return process.platform === 'win32'
    ? `\\\\.\\pipe\\NBA2K27ModLauncherMock-${process.pid}-${id}`
    : path.join(os.tmpdir(), `nba2k27-mod-launcher-${process.pid}-${id}.sock`);
}

function encodeFrame(payload) {
  const body = Buffer.from(JSON.stringify(payload), 'utf8');
  if (body.length > MAX_FRAME_BYTES) throw new Error('Mock provider frame exceeds its safety limit.');
  const frame = Buffer.alloc(4 + body.length);
  frame.writeUInt32LE(body.length, 0);
  body.copy(frame, 4);
  return frame;
}

function decodeFrame(buffer) {
  if (buffer.length < 4) return null;
  const length = buffer.readUInt32LE(0);
  if (length > MAX_FRAME_BYTES) throw new Error('Mock provider frame exceeds its safety limit.');
  if (buffer.length < length + 4) return null;
  return { payload: JSON.parse(buffer.subarray(4, length + 4).toString('utf8')), consumed: length + 4 };
}

async function startMockProvider(modsRoot) {
  const endpoint = endpointName();
  const service = new AssetProviderService(modsRoot);
  const server = net.createServer((socket) => {
    let pending = Buffer.alloc(0);
    socket.on('data', async (chunk) => {
      pending = Buffer.concat([pending, chunk]);
      let frame;
      try { frame = decodeFrame(pending); }
      catch (error) { socket.end(encodeFrame({ ok: false, error: error.message })); return; }
      if (!frame) return;
      socket.pause();
      try {
        const result = await service.dispatch(frame.payload || {});
        socket.end(encodeFrame({ ok: true, ...result }));
      } catch (error) {
        socket.end(encodeFrame({ ok: false, error: error.message }));
      }
    });
  });
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(endpoint, resolve);
  });
  return {
    endpoint,
    getOpenHandleCount: () => service.handles.size,
    close: () => {
      service.closeAll();
      return new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
    },
  };
}

async function requestMockProvider(endpoint, request, timeoutMs = 5000) {
  return new Promise((resolve, reject) => {
    const socket = net.createConnection(endpoint);
    let pending = Buffer.alloc(0);
    const timer = setTimeout(() => {
      socket.destroy();
      reject(new Error('Mock provider request timed out.'));
    }, timeoutMs);
    const finish = (callback, value) => { clearTimeout(timer); socket.destroy(); callback(value); };
    socket.once('connect', () => socket.write(encodeFrame(request)));
    socket.on('data', (chunk) => {
      pending = Buffer.concat([pending, chunk]);
      try {
        const frame = decodeFrame(pending);
        if (!frame) return;
        if (!frame.payload.ok) finish(reject, new Error(frame.payload.error || 'Mock provider failed.'));
        else finish(resolve, frame.payload);
      } catch (error) { finish(reject, error); }
    });
    socket.once('error', (error) => finish(reject, error));
  });
}

async function mockProviderDiagnostic(modsRoot, logicalPath) {
  const provider = await startMockProvider(modsRoot);
  try {
    const metadata = await requestMockProvider(provider.endpoint, { operation: 'query', logicalPath });
    const opened = await requestMockProvider(provider.endpoint, { operation: 'open', logicalPath });
    const hash = crypto.createHash('sha256');
    let fullBytesRead = 0;
    let readCalls = 0;
    while (fullBytesRead < metadata.size) {
      const response = await requestMockProvider(provider.endpoint, {
        operation: 'read', handle: opened.handle, length: DIAGNOSTIC_CHUNK_BYTES,
      });
      const data = Buffer.from(response.data, 'base64');
      if (data.length !== response.bytesRead || data.length === 0) throw new Error('Provider stopped before the advertised asset size.');
      hash.update(data);
      fullBytesRead += data.length;
      readCalls += 1;
    }
    const eof = await requestMockProvider(provider.endpoint, {
      operation: 'read', handle: opened.handle, length: 1,
    });
    const closed = await requestMockProvider(provider.endpoint, { operation: 'close', handle: opened.handle });
    return {
      ok: true,
      privateTestEndpoint: provider.endpoint,
      fullBytesRead,
      fullSha256: hash.digest('hex'),
      readCalls,
      eofBytesRead: eof.bytesRead,
      handleClosed: closed.closed,
      openHandlesAfterClose: provider.getOpenHandleCount(),
      size: metadata.size,
      logicalPath: metadata.logicalPath,
      endpointIsModiumAssetPipe: false,
    };
  } finally {
    await provider.close();
  }
}

module.exports = {
  MAX_FRAME_BYTES,
  DIAGNOSTIC_CHUNK_BYTES,
  decodeFrame,
  encodeFrame,
  mockProviderDiagnostic,
  requestMockProvider,
  startMockProvider,
};
