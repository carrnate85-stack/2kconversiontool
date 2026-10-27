const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { patchStrandUvs } = require('../worker/strand-uv-patch.cjs');

// The shipped worker bundle carries the patch, and re-applying it is a no-op.
const bundle = fs.readFileSync(path.join(__dirname, '../frontend/preview-worker.bundle.js'), 'utf8');
assert.ok(bundle.includes('const strandCards = '), 'Worker bundle keeps TEXCOORD1 for regular strand meshes');
assert.equal(patchStrandUvs(bundle), bundle);
assert.throws(() => patchStrandUvs('function buildMesh() {}'), /Pinned preview UV hook changed/);

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
console.log('Strand UV patch: bundle patched, idempotent, hair/beard-only selection passed.');
