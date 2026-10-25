import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const runtime = readFileSync(new URL('../frontend/public/fit-preview/fit-runtime.js', import.meta.url), 'utf8');
const start = runtime.indexOf('function colorTextureDependency(item){');
const end = runtime.indexOf('\nfunction queuePrefetch(){', start);
assert(start >= 0 && end > start, 'generated dependency selector is missing');

const resources = new Set([
  'clothing/011no_2002_core_home_s_rdmo_tex.iff',
  'clothing/015lal_1978_core_home_s_classic_rdmo_tex.iff',
  'clothing/basketball_shorts_classic_color_tex.iff',
  'clothing/shorts_artwork_tex.iff',
]);
const chooseDependency = Function(
  'index',
  `${runtime.slice(start, end)}; return colorTextureDependency;`,
)({ has: (name) => resources.has(name) });
const dependency = (name) => ({ kind: 1, name });

assert.equal(
  chooseDependency({ dependencies: [
    dependency('011no_2002_core_home_s_rdmo_tex'),
    dependency('basketball_shorts_classic_normal_tex'),
  ] }).name,
  '011no_2002_core_home_s_rdmo_tex',
  'classic shorts must retain a combined RDMO container as the final candidate',
);
assert.equal(
  chooseDependency({ dependencies: [
    dependency('015lal_1978_core_home_s_classic_rdmo_tex'),
    dependency('basketball_shorts_classic_normal_tex'),
  ] }).name,
  '015lal_1978_core_home_s_classic_rdmo_tex',
  'long classic shorts must retain their combined classic RDMO container',
);
assert.equal(
  chooseDependency({ dependencies: [
    dependency('011no_2002_core_home_s_rdmo_tex'),
    dependency('basketball_shorts_classic_color_tex'),
  ] }).name,
  'basketball_shorts_classic_color_tex',
  'an explicitly named color dependency must remain preferred',
);
assert.equal(
  chooseDependency({ dependencies: [
    dependency('011no_2002_core_home_s_rdmo_tex'),
    dependency('shorts_artwork_tex'),
  ] }).name,
  'shorts_artwork_tex',
  'a neutral generic dependency must remain preferred over the final fallback',
);

const selected = /const selectedTexture=(.+);\n/.exec(runtime);
assert(selected, 'generated embedded texture selector is missing');
const chooseTexture = Function('options', `return (${selected[1]});`);
const normal = { name: 'core_home_s_normal.aaaaaaaaaaaaaaaa.dds' };
const color = { name: 'core_home_s_color.bbbbbbbbbbbbbbbb.dds' };
const rdmo = { name: '011no_2002_core_home_s_rdmo.cccccccccccccccc.dds' };
assert.equal(
  chooseTexture([normal, color, rdmo]),
  color,
  'the embedded *_color DDS must be selected from a combined container',
);

console.log('PASS combined classic/Nike shorts texture-container discovery and embedded color selection');
