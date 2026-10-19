/* Bounded look-ahead: parallel preparation, ordered and strictly serial saves. */
(function (root) {
  async function runArtworkBatch(items, { prepare, save, onProgress = () => {}, concurrency = 3 }) {
    const width = Math.min(3, Math.max(1, Math.floor(concurrency) || 1));
    const pending = new Map();
    const failures = [];
    const updatedKeys = new Set();
    let latest = null;
    const schedule = (index) => {
      if (index >= items.length) return;
      // Attach rejection handling immediately, including while another item saves.
      pending.set(index, Promise.resolve().then(() => prepare(items[index]))
        .then(value => ({ value }), error => ({ error })));
    };
    for (let index = 0; index < Math.min(width, items.length); index += 1) schedule(index);
    for (let index = 0; index < items.length; index += 1) {
      const item = items[index];
      onProgress({ item, completed: index, total: items.length, stage: 'Finding covers' });
      const result = await pending.get(index);
      pending.delete(index);
      try {
        if (result.error) throw result.error;
        onProgress({ item, completed: index, total: items.length, stage: 'Saving artwork' });
        latest = await save(item, result.value);
        updatedKeys.add(item.key);
      } catch (error) {
        failures.push(`${item.artist} - ${item.title}: ${error.message}`);
      }
      onProgress({ item, completed: index + 1, total: items.length, stage: 'Finding covers' });
      schedule(index + width);
    }
    return { latest, failures, updatedKeys };
  }
  if (typeof module === 'object' && module.exports) module.exports = { runArtworkBatch };
  else root.runArtworkBatch = runArtworkBatch;
})(typeof window === 'object' ? window : globalThis);
