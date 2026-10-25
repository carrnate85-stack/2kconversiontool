import { unpackPreviewPacket } from './packet.mjs';
let sequence = 0;
const pending = new Map();
let transport = { binaryPackets: 0, binaryBytes: 0, jsonReplies: 0 };
export function resetNativeTransport() { transport = { binaryPackets: 0, binaryBytes: 0, jsonReplies: 0 }; }
export function nativeTransportStats() { return { ...transport }; }
export function nativeBuffer(data) {
  return typeof data === 'string' ? Buffer.from(data, 'base64')
    : Buffer.from(data.buffer, data.byteOffset, data.byteLength);
}
globalThis.addEventListener('message', ({ data }) => {
  if (data.kind !== 'native-reply') return;
  const task = pending.get(data.id);
  if (!task) return;
  pending.delete(data.id);
  if (data.error) task.reject(new Error(data.error));
  else {
    try {
      if (data.value instanceof ArrayBuffer) {
        const value = unpackPreviewPacket(data.value);
        transport.binaryPackets++;
        transport.binaryBytes += data.value.byteLength;
        task.resolve(value);
      } else {
        transport.jsonReplies++;
        task.resolve(data.value);
      }
    } catch (error) { task.reject(error); }
  }
});
export function native(command, payload) {
  return new Promise((resolve, reject) => {
    const id = ++sequence;
    pending.set(id, { resolve, reject });
    globalThis.postMessage({ kind: 'native', id, command, payload });
  });
}
