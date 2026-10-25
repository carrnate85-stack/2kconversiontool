import { describe, expect, it } from 'vitest';
import { materialProfile } from '../dev-assets/model-preview/renderer/render-profiles.mjs';

describe('Exporter 0.1.27 player strand shading', () => {
  it('keeps the exact physical hair response', () => {
    expect(materialProfile('hair_shader', '', 'player')).toMatchObject({
      role: 'strand',
      roughness: 0.5,
      metalness: 0,
      normalScale: 0.62,
      alphaTest: 0.035,
      anisotropy: 0.7,
    });
  });
});
