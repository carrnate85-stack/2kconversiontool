import { inflateSync } from '../build-tools/node_modules/fflate/esm/browser.js';
export function inflateRawSync(input, options = {}) {
  const bytes = inflateSync(input);
  if (bytes.length > (options.maxOutputLength || 512 * 1024 * 1024)) throw new Error('Preview resource exceeds its decoded size limit.');
  return Buffer.from(bytes);
}
