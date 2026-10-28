const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const finish = require('./test-watchdog.cjs')('renderer texture transactions');
const source = fs.readFileSync(path.join(__dirname, '../web/renderer/shared/model-preview-core.mjs'), 'utf8');
const section = (start, end) => source.slice(source.indexOf(start), source.indexOf(end, source.indexOf(start)));
const item = (name, extra = {}) => ({ name, bytes: new Uint8Array([1, 2]), ...extra });
function fixture() {
  const made = [], frames = [], applied = [];
  let tick = 0, failRender = false, failApply = false;
  const old = { name: 'old', disposed: 0, dispose() { this.disposed++; } };
  const c = vm.createContext({ old, initial: [item('old')], console,
    performance: { now: () => (tick += 10) },
    nextFrame: () => new Promise(resolve => frames.push(resolve)),
    createDdsTexture(value) {
      if (value.name === 'broken') throw Error('Upload failed');
      const texture = { name: value.name, disposed: 0, dispose() { this.disposed++; } };
      made.push(texture); return texture;
    },
    applyAutomaticTextures() {
      applied.push(vm.runInContext('textureCache.map(texture => texture?.name)', c));
      if (failApply) { failApply = false; throw Error('Material failed'); }
    },
    renderer: { render() { if (failRender) { failRender = false; throw Error('Render failed'); } } }, scene: {}, camera: {},
  });
  vm.runInContext('let loadGeneration = 0, textureBatchGeneration = 0; let textureItems = initial, textureCache = [old]; let currentPayload = {textures: initial, stats: {}, textureDependencies: []};', c);
  for (const [start, end] of [
    ['function cancelLoad()', 'function fitCamera('],
    ['async function loadArenaTextures(', 'function replaceModelColors('],
    ['function replaceModelColors(', 'function dependencyDetails('],
    ['function clearArenaTextures()', 'function pickObject('],
    ['function boundStrandTangent(', 'function boundArenaTexture('],
  ]) vm.runInContext(section(start, end), c);
  return { c, old, made, frames, applied, read: () => vm.runInContext('({items:textureItems, cache:textureCache, payload:currentPayload})', c),
    failRender() { failRender = true; }, failApply() { failApply = true; },
    replace() { c.fresh = { name: 'new-player', dispose() { throw Error('New player disposed by stale request'); } };
      vm.runInContext('loadGeneration++; textureItems = [{name:"new-player"}]; textureCache = [fresh];', c); },
  };
}
(async () => {
  const success = fixture(); let oldAtCommit;
  const result = await success.c.loadArenaTextures([item('new')], () => { oldAtCommit = success.read().cache[0]; });
  assert.equal(oldAtCommit, success.old, 'Batch published before final progress callback');
  assert.equal(result.addedTextures, 1); assert.equal(success.old.disposed, 1);
  assert.equal(success.read().items[0].bytes, undefined); assert.equal(success.made[0].disposed, 0);

  const cancelled = fixture();
  await assert.rejects(cancelled.c.loadArenaTextures([item('new')], () => cancelled.c.cancelLoad()), /cancelled/);
  assert.equal(cancelled.old.disposed, 0); assert.equal(cancelled.made[0].disposed, 1); assert.equal(cancelled.read().cache[0], cancelled.old);

  const callback = fixture();
  await assert.rejects(callback.c.loadArenaTextures([item('new')], () => { throw Error('Callback failed'); }), /Callback failed/);
  assert.equal(callback.old.disposed, 0); assert.equal(callback.made[0].disposed, 1);

  const upload = fixture();
  const uploading = upload.c.loadArenaTextures([item('new'), item('broken')]); upload.frames.shift()();
  await assert.rejects(uploading, /Upload failed/); assert.equal(upload.made[0].disposed, 1); assert.equal(upload.old.disposed, 0);

  for (const method of ['failRender', 'failApply']) {
    const rollback = fixture(); rollback[method]();
    await assert.rejects(rollback.c.loadArenaTextures([item('new')]), /failed/);
    assert.equal(rollback.read().cache[0], rollback.old); assert.equal(rollback.old.disposed, 0); assert.equal(rollback.made[0].disposed, 1);
    assert.equal(rollback.applied.at(-1)[0], 'old');
  }

  const replacement = fixture();
  const stale = replacement.c.loadArenaTextures([item('stale-a'), item('stale-b')]);
  replacement.replace(); replacement.frames.shift()(); await assert.rejects(stale, /cancelled/);
  assert.equal(replacement.read().cache[0].name, 'new-player'); assert.equal(replacement.made.length, 1); assert.equal(replacement.made[0].disposed, 1);

  const overlapping = fixture();
  const first = overlapping.c.loadArenaTextures([item('first-a'), item('first-b')]);
  await overlapping.c.loadArenaTextures([item('latest')]); overlapping.frames.shift()();
  await assert.rejects(first, /cancelled/); assert.equal(overlapping.read().cache[0].name, 'latest');
  assert.equal(overlapping.made.find(x => x.name === 'latest').disposed, 0); assert.equal(overlapping.made[0].disposed, 1);

  const append = fixture();
  const appended = await append.c.loadArenaTextures([item('OLD'), item('new'), item('NEW')], () => {}, { append: true });
  assert.equal(appended.added.length, 1); assert.equal(appended.added[0].index, 1);
  assert.equal(append.read().cache.length, 2); assert.equal(append.old.disposed, 0);

  const cleared = fixture();
  const pending = cleared.c.loadArenaTextures([item('a'), item('b')]); cleared.c.clearArenaTextures(); cleared.frames.shift()();
  await assert.rejects(pending, /cancelled/); assert.equal(cleared.read().cache.length, 0); assert.equal(cleared.made[0].disposed, 1);

  const empty = fixture(); let callbacks = 0;
  await empty.c.loadArenaTextures([], () => callbacks++); assert.equal(callbacks, 1); assert.equal(empty.read().cache.length, 0); assert.equal(empty.old.disposed, 1);

  const tangent = fixture();
  tangent.c.values = [item('staged_tangent', {sourcePriority: 0, binding: {target: 'hair', channel: 'auxiliary'}}),
    item('manifest_assigned', {sourcePriority: 1, assignments: [{material: 'Hair_Shader', channel: 'tangent'}]})];
  vm.runInContext('textureItems = values; textureCache = values;', tangent.c);
  assert.equal(tangent.c.boundStrandTangent('hair_shader', 'hair').name, 'staged_tangent');
  tangent.c.values[1].sourcePriority = 0;
  assert.equal(tangent.c.boundStrandTangent('HAIR_SHADER', 'hair').name, 'manifest_assigned', 'Explicit binding should win within the same source priority');
  tangent.c.values[1].assignments[0].material = 'different';
  assert.equal(tangent.c.boundStrandTangent('hair_shader', 'hair').name, 'staged_tangent');

  const colors = fixture();
  colors.c.deps = [item('old-color', {assignments: [{material: 'Hair_Shader', channel: 'color'}]}), {name: 'normal', assignments: [{material: 'Hair_Shader', channel: 'normal'}]}];
  vm.runInContext('currentPayload.textureDependencies = deps;', colors.c);
  colors.c.replaceModelColors({replaceMaterials: ['hair_shader'], textures: [item('replacement', {data: [9], assignments: [{material: 'Hair_Shader', channel: 'color'}]})]});
  const dependencies = colors.read().payload.textureDependencies;
  assert.equal(dependencies.length, 2); assert.equal(dependencies[0].name, 'normal');
  assert.equal(dependencies[1].name, 'replacement'); assert.equal(dependencies[1].bytes, undefined); assert.equal(dependencies[1].data, undefined);
  console.log('Renderer texture transactions: private preparation, final-callback cancellation, failure rollback, stale/new ownership, append/deduplication, staged tangent priority and dependency refresh passed.');
})().catch(error => { console.error(error); process.exitCode = 1; }).finally(finish);
