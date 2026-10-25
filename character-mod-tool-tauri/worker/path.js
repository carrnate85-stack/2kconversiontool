import posix from '../build-tools/node_modules/path-browserify/index.js';
const normalize = value => String(value).replaceAll('\\', '/');
export const basename = (value, suffix) => posix.basename(normalize(value), suffix);
export const dirname = value => posix.dirname(normalize(value));
export const extname = value => posix.extname(normalize(value));
export const parse = value => posix.parse(normalize(value));
export const resolve = value => normalize(value);
export const join = (...values) => posix.join(...values.map(normalize));
export { posix };
