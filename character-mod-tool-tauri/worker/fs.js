import { native, nativeBuffer } from './native.js';
export async function readFile(path) { return nativeBuffer((await native('read-file', { path })).data); }
export async function stat(path) { const value = await native('file-stat', { path }); return { ...value, isFile: () => true }; }
export async function realpath(path) { return (await stat(path)).path.replace(/^\\\\\?\\/, ''); }
