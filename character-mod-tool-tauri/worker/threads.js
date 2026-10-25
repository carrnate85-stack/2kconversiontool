import { resultBuffers } from './transfer.js';

export const parentPort = {
  on(event, callback) { if (event === 'message') globalThis.addEventListener('message', ({ data }) => { if (!data.kind) callback(data); }); },
  postMessage(value) {
    const buffers = resultBuffers(value.result);
    if (value.result?.stats) value.result.stats.workerTransport = {
      mode: 'transfer', buffers: buffers.length,
      bytes: buffers.reduce((total, buffer) => total + buffer.byteLength, 0),
    };
    globalThis.postMessage(value, buffers);
  }
};
