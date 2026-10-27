// Loads a decoded player (decode-player.cjs): players/<name>.json with typed
// arrays as { __typed, offset, length } into players/<name>.bin. Each call fetches
// and builds a fresh payload, since renderers edit geometry arrays in place.
// Older payloads with base64 typed arrays still load.
export async function loadPlayer(name) {
  const response = await fetch(`players/${encodeURIComponent(name)}.json`);
  if (!response.ok) throw new Error(`players/${name}.json not found; run decode-player.cjs first`);
  const metadata = await response.json();
  const sidecar = await fetch(`players/${encodeURIComponent(name)}.bin`);
  const binary = sidecar.ok ? await sidecar.arrayBuffer() : null;
  const decode = (value) => {
    if (Array.isArray(value)) return value.map(decode);
    if (!value || typeof value !== 'object') return value;
    if (value.__typed) {
      const Type = globalThis[value.__typed];
      if (typeof value.base64 === 'string') return new Type(Uint8Array.from(atob(value.base64), (c) => c.charCodeAt(0)).buffer);
      if (!binary) throw new Error(`players/${name}.bin not found; rerun decode-player.cjs`);
      return new Type(binary, value.offset, value.length / Type.BYTES_PER_ELEMENT);
    }
    return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, decode(item)]));
  };
  return decode(metadata);
}
