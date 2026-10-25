export function resultBuffers(value) {
  const buffers = new Set(), seen = new Set();
  const visit = object => {
    if (!object || typeof object !== 'object' || seen.has(object)) return;
    seen.add(object);
    const buffer = ArrayBuffer.isView(object) ? object.buffer : object;
    if (ArrayBuffer.isView(object) || object instanceof ArrayBuffer) {
      if (buffer instanceof ArrayBuffer && buffer.byteLength) buffers.add(buffer);
      return;
    }
    for (const child of Object.values(object)) visit(child);
  };
  visit(value);
  return [...buffers];
}
