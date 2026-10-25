import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import * as THREE from '../../frontend/public/fit-preview/vendor/three.module.min.js';

test('contact cue follows the floor of translated/resized outfits without textures', () => {
  const scene = new THREE.Scene();
  const context = { THREE, scene };
  runInNewContext(readFileSync(new URL('./contact-shadow.js', import.meta.url), 'utf8'), context);
  const mesh = scene.children[0];
  assert.equal(mesh.visible, false);
  assert.equal(mesh.geometry.index.count, 6);
  assert.equal(mesh.material.depthWrite, false);
  assert.equal(mesh.material.transparent, true);
  assert.equal(mesh.castShadow, false);
  assert.equal(mesh.receiveShadow, false);
  assert.deepEqual(Object.keys(mesh.material.uniforms), []);
  for (const scale of [1, 100]) {
    const bounds = new THREE.Box3(new THREE.Vector3(3, 5, -1).multiplyScalar(scale), new THREE.Vector3(4, 7, 0).multiplyScalar(scale));
    context.positionContactShadow(bounds);
    assert.equal(mesh.visible, true);
    assert.ok(Math.abs(mesh.position.x - 3.5 * scale) < 0.001);
    assert.ok(Math.abs(mesh.position.z + 0.5 * scale) < 0.001);
    assert.ok(mesh.position.y < bounds.min.y && bounds.min.y - mesh.position.y < 0.01 * scale);
    assert.ok(mesh.scale.x > 0 && mesh.scale.y > 0);
    const shadowBounds = new THREE.Box3().setFromObject(mesh);
    assert.ok(shadowBounds.max.y - shadowBounds.min.y < 0.001);
  }
  context.positionContactShadow(new THREE.Box3());
  assert.equal(mesh.visible, false);
  mesh.geometry.dispose(); mesh.material.dispose();
});
