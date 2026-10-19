// Worker entry: launcher page <-> WPF fast-clothing-preview-worker protocol.
const category = load('core/clothing-components.js').category;
const accessorySlots = load('shared/accessory-slots.js');
// WPF FastClothingPreview previewNames / previewAssets (core/fast-clothing-preview.js),
// fed by Rust's composition of the outfit instead of streetwear-components.json.
function previewNames(originals) {
  return [...new Set(originals.filter(asset => typeof asset === 'string' && (['top', 'bottom'].includes(category(asset)) || /^acc_sock_/.test(asset))))];
}
function previewAssets(items) {
  if (!Array.isArray(items) || items.length > 7) throw Error('Unsupported outfit preview.');
  return items.filter(asset => ['top', 'bottom'].includes(category(asset)) || /^(?:acc_sock_|shoe_)/.test(asset) || Boolean(accessorySlots.slot(asset)));
}
let started = false;
self.onmessage = event => {
  const message = event.data;
  if (message?.hostReply) {
    const call = hostCalls.get(message.hostReply.id);
    if (!call) return;
    hostCalls.delete(message.hostReply.id);
    if (message.hostReply.error) call.reject(Error(message.hostReply.error));
    else call.resolve(message.hostReply.value);
    return;
  }
  if (message?.init) {
    if (started) return;
    started = true;
    workerShim.workerData = { gameRoot: 'game', cacheRoot: 'indexeddb', decoderPath: 'rust', previewNames: previewNames(message.init.originals || []) };
    load('core/fast-clothing-preview-worker.js');
    return;
  }
  if (!started) return;
  if (Number.isSafeInteger(message?.id) && message.request?.items) {
    try {
      const { items, ...request } = message.request;
      message.request = { ...request, assets: previewAssets(items) };
    } catch (error) {
      self.postMessage({ id: message.id, error: error.message });
      return;
    }
  }
  if (typeof message?.prefetch === 'string' && !workerShim.workerData.previewNames.includes(message.prefetch)) return;
  for (const listener of workerShim.parentPort.listeners) listener(message);
};
