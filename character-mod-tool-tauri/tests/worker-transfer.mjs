import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const source = fs.readFileSync(path.join(root, 'worker/transfer.js'), 'utf8').replace('export ', '');
const context = vm.createContext({ ArrayBuffer, Object, Set });
vm.runInContext(source, context);
const buffersFor = value => Array.from(context.resultBuffers(value));

const positions = new Float32Array([1, 2, 3, 4, 5, 6]);
const indices = new Uint32Array([0, 1, 0]);
const view = positions.subarray(3);
const result = { meshes: [{ positions, indices, otherView: view }], textures: [{ bytes: 'unchanged base64' }] };
result.repeated = positions;
result.cycle = result;
const buffers = buffersFor(result);
assert.equal(buffers.length, 2, 'Shared views and cycles must not duplicate a transfer entry');
const moved = structuredClone(result, { transfer: buffers });
assert.equal(positions.byteLength, 0);
assert.equal(indices.byteLength, 0);
assert.deepEqual(Array.from(moved.meshes[0].positions), [1, 2, 3, 4, 5, 6]);
assert.deepEqual(Array.from(moved.meshes[0].otherView), [4, 5, 6]);
assert.equal(moved.meshes[0].positions.buffer, moved.meshes[0].otherView.buffer);
assert.equal(moved.textures[0].bytes, 'unchanged base64');
assert.equal(buffersFor({ shared: new Float32Array(new SharedArrayBuffer(16)) }).length, 0);
assert.equal(buffersFor({ empty: new ArrayBuffer(0), error: 'failed' }).length, 0);
let received;
const wrapper = fs.readFileSync(path.join(root, 'worker/threads.js'), 'utf8')
  .replace(/^import .*;\r?\n/, '').replace('export const parentPort', 'globalThis.parentPort');
const wrapperContext = vm.createContext({ ArrayBuffer, Object, Set,
  postMessage(value, buffers) { received = structuredClone(value, { transfer: Array.from(buffers) }); } });
vm.runInContext(source + '\n' + wrapper, wrapperContext);
const owned = new Float32Array([10, 20, 30]);
wrapperContext.parentPort.postMessage({ id: 7, result: { meshes: [{ positions: owned }], stats: {} } });
assert.equal(owned.byteLength, 0, 'Actual worker wrapper must transfer result ownership');
assert.equal(received.result.stats.workerTransport.bytes, 12);
assert.equal(received.result.stats.workerTransport.buffers, 1);
assert.deepEqual(Array.from(received.result.meshes[0].positions), [10, 20, 30]);

const cacheSource = fs.readFileSync(path.join(root, 'worker/head-cache.js'), 'utf8')
  .replace(/^import .*;\r?\n/gm, '').replace(/export \{.*\};?/, '');
const cacheContext = vm.createContext({ Map, Set, ArrayBuffer, Object, structuredClone });
vm.runInContext(cacheSource, cacheContext);
const cache = vm.runInContext('new HeadPreviewCache()', cacheContext);
const decode = async () => ({ meshes: [{ positions: new Float32Array([7, 8, 9]), indices: new Uint32Array([0]) }] });
const first = await cache.get('cmt-preview://donor', decode);
structuredClone(first.value, { transfer: buffersFor(first.value) });
const second = await cache.get('cmt-preview://donor', () => { throw new Error('Cache was lost'); });
assert.equal(second.cached, true);
assert.deepEqual(Array.from(second.value.meshes[0].positions), [7, 8, 9]);
const borrowed = await cache.get('cmt-preview://donor', decode, { clone: false });
assert.equal(borrowed.value.meshes[0].positions.byteLength, 12);
console.log('Worker transport: exact arrays, unique shared buffers, cycles, shared-memory exclusion and donor-cache isolation passed.');
