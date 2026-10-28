const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { patchWorkerBundle } = require('../worker/bundle-patches.cjs');

// The shipped worker bundle carries the patches, and re-applying them is a no-op.
const bundle = fs.readFileSync(path.join(__dirname, '../frontend/preview-worker.bundle.js'), 'utf8');
assert.ok(bundle.includes('const strandCards = '), 'Worker bundle keeps TEXCOORD1 for regular strand meshes');
assert.equal(patchWorkerBundle(bundle), bundle);
assert.throws(() => patchWorkerBundle('function buildMesh() {}'), /Pinned preview worker hook changed/);
assert.ok(bundle.includes('var MAX_TEXTURE_BYTES = 48 * 1024 * 1024;') && bundle.includes('includeArenaTextures ? 128 : 40;'));
assert.ok(bundle.includes('if (!includeArenaTextures && previewTextureUnused(texture)) continue;'));
assert.ok(bundle.includes('bytes: new Uint8Array(bytes2)') && !bundle.includes('bytes: bytes2.toString("base64")'), 'Texture bytes travel as a Uint8Array');
assert.ok(bundle.includes('morph target(s) detected (facial expressions and pose correctives)') && !bundle.includes('math-node morph evaluation is not applied yet'));

// The strand test: every primitive (with inherited materials) is hair or facial hair.
const start = bundle.indexOf('const strandCards = ');
const expression = bundle.slice(start + 'const strandCards = '.length, bundle.indexOf('})();', start) + 4);
const primitiveMaterial = (primitive, inherited = 'Unassigned') => String(primitive?.Material || '').trim() || inherited || 'Unassigned';
const strand = (prims, tangentSpaceAttachment = false, texcoord1 = true) => vm.runInNewContext(expression, {
  tangentSpaceAttachment, primitiveMaterial, model: { Prim: prims, VertexFormat: texcoord1 ? { TEXCOORD1: {} } : {} },
});
assert.equal(strand([{ Material: 'hair_afro_shader' }, {}]), true);
assert.equal(strand([{ Material: 'facialHair_shader' }]), true);
assert.equal(strand([{ Material: 'torso_shader' }, { Material: 'l_armpitHair_shader' }]), false, 'Body mesh keeps one UV set');
assert.equal(strand([{ Material: 'hat_custom_shader' }]), false);
assert.equal(strand([{ Material: 'hair_afro_shader' }], true), false, 'Tangent-space meshes keep their own path');
assert.equal(strand([{ Material: 'hair_afro_shader' }], false, false), false);
assert.equal(strand([]), false);

// Player previews skip maps the renderer never samples.
const unusedStart = bundle.indexOf('function previewTextureUnused(');
const previewTextureUnused = vm.runInNewContext(`(${bundle.slice(unusedStart).match(/^function previewTextureUnused[\s\S]*?\n {6}\}/)[0]})`);
const texture = (name, channel, target = 'face', assignments) => ({ name: `${name}.0123456789abcdef.dds`, source: 'main: png09613.iff', binding: { target, channel }, assignments });
for (const [name, channel, target] of [['face_color_o', 'color'], ['face_normal_o', 'normal'], ['legs_normal', 'normal', 'legs'],
  ['iris_normalheight1', 'normal', 'eye'], ['hair_lightmap_o', 'auxiliary', 'hair'], ['hair_tangent_o', 'auxiliary', 'hair'], ['legs_occlusion', 'auxiliary', 'legs'],
  ['female_headband_thin_color_o', 'color', undefined], ['female_headband_thin_normal_o', 'normal', undefined]]) {
  assert.equal(previewTextureUnused(texture(name, channel, target)), false, `${name} is kept`);
}
for (const [name, channel, target] of [['chest_normal_avg_ripped', 'normal', 'chest'], ['torso_normal_avg_normal', 'normal', 'torso'],
  ['face_wrinklecolor_o', 'auxiliary'], ['chest_region_id', 'auxiliary', 'chest'],
  ['female_headband_thin_rdmo_o', 'auxiliary', undefined], ['female_headband_thin_coneangle_o', 'auxiliary', undefined]]) {
  assert.equal(previewTextureUnused(texture(name, channel, target)), true, `${name} is skipped`);
}
assert.equal(previewTextureUnused({ ...texture('chest_normal_avg_ripped', 'normal', 'chest'), externalPath: 'cmt-preview://shared/1' }), false, 'Native muscle-type body normal is kept');
assert.equal(previewTextureUnused(texture('face_detailnormal_o', 'normal')), false, "The player's face detail normal is kept");
assert.equal(previewTextureUnused({ ...texture('face_detailnormal_o', 'normal'), source: 'body-detail: char/male_skin.iff' }), true, 'The generic skin copy is skipped');
for (const target of ['face', 'chest', 'torso', 'legs']) {
  assert.equal(previewTextureUnused(texture(`${target}_roughcurvedense`, 'auxiliary', target)), false, `${target}_roughcurvedense is kept for skin roughness`);
}
for (const [name, target] of [['face_bentnormal_o', 'face'], ['chest_bentnormal', 'chest'], ['legs_bentnormal', 'legs']]) {
  assert.equal(previewTextureUnused(texture(name, 'auxiliary', target)), false, `${name} is kept for its occlusion alpha`);
}
assert.equal(previewTextureUnused({ ...texture('face_bentnormal', 'auxiliary'), source: 'body-detail: char/male_skin.iff' }), true, "The generic face bent normal gives way to the player's");
assert.equal(previewTextureUnused(texture('hair_strand_o', 'auxiliary', 'hair', [{ channel: 'tangent', material: 'hair_shader' }])), false, 'Assigned tangent maps are kept');
// A selected config's body color outranks the main IFF's.
const rankStart = bundle.indexOf('function bodyColorRank(');
const bodyColorRank = vm.runInNewContext(`(${bundle.slice(rankStart).match(/^function bodyColorRank[\s\S]*?\n {6}\}/)[0]})`, { Number });
assert.ok(bodyColorRank({ source: 'config: png02444_config_headphone.iff', container: 3 }) < bodyColorRank({ source: 'main: png02444.iff', container: 0 }));
assert.ok(bodyColorRank({ source: 'main: png02444.iff', container: 0 }) < bodyColorRank({ source: 'body-color: char/chest_color_06.iff', container: 5 }));
console.log('Worker bundle patches: idempotent, hair/beard-only TEXCOORD1, unused preview maps skipped, config body colors preferred passed.');
