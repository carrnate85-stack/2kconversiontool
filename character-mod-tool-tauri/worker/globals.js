import { Buffer } from '../build-tools/node_modules/buffer/index.js';
globalThis.Buffer = Buffer;
globalThis.process = { platform: 'win32' };
export { Buffer };
