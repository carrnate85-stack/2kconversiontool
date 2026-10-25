import { describe, expect, it } from 'vitest';
import { nearestSkinBoundaryPairs } from '../dev-assets/model-preview/renderer/skin-preview-fixes.mjs';

const target = name => name === 'face_shader' ? 'face' : name === 'arms_shader' ? 'chest' : '';

describe('loose player skin boundary fallback', () => {
  it('pairs nearby face and chest seam vertices when exact positions differ', () => {
    const payload = { meshes: [{
      positions: [0, 0, 0, 0, 0, 1, 0.001, 0, 0, 0.001, 0, 1],
      uvs: [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8],
      indices: [0, 1, 2, 3],
      groups: [
        { material: 'face_shader', start: 0, count: 2 },
        { material: 'arms_shader', start: 2, count: 2 },
      ],
    }] };
    const pairs = nearestSkinBoundaryPairs(payload, 'face', 'chest', target);
    expect(pairs).toHaveLength(2);
    expect(pairs[0].distance).toBeCloseTo(0.001);
    expect(pairs[0].left).toEqual([0.1, 0.2]);
    expect(pairs[0].right).toEqual([0.5, 0.6]);
  });

  it('does not invent unsupported regional pairings', () => {
    expect(nearestSkinBoundaryPairs({ meshes: [] }, 'face', 'hair', target)).toEqual([]);
  });
});
