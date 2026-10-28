import * as THREE from './vendor/three.module.min.js';
import { captureLayerPlan } from './arena-capture.mjs';

// Capture-only reconstruction of the combined RGB masks. Mesh UVs identify
// seat/padding regions; TEXCOORD2 tiles the individual surface-detail maps.
export function configureArenaLayers(material, definition, lookup, hasDetailUv) {
  if (!definition?.resourceSlots?.includes('CombinedLayerMaskMap')) return null;
  const mask = lookup('layer-mask', 'CombinedLayerMaskMap');
  if (!mask) throw new Error(`Missing combined material mask: ${material.name}`);
  const parameter = definition.parameter || {};
  const plan = captureLayerPlan(parameter, definition.resourceSlots);
  const layers = plan.map(layer => ({ ...layer,
    colorMap: lookup('layer-color', layer.prefix + 'AlbedoMap'),
    roughnessMap: lookup('layer-roughness', layer.prefix + 'NormalAndRoughMap'),
    aoMap: lookup('layer-ao', layer.prefix + 'ConeAngleMap') }));
  for (const layer of layers) {
    if (definition.resourceSlots.includes(layer.prefix + 'AlbedoMap') && !layer.colorMap) {
      throw new Error(`Missing combined material layer: ${material.name}/${layer.prefix}AlbedoMap`);
    }
  }
  const meshAo = lookup('layer-ao', 'MeshMapConeAngleMap');
  const overlay = lookup('layer-color', 'LAYER4_AlbedoMap');
  if (definition.resourceSlots.includes('LAYER4_AlbedoMap') && !overlay) throw new Error(`Missing court graphic: ${material.name}`);
  for (const texture of [mask, meshAo, ...layers.flatMap(layer => [layer.roughnessMap, layer.aoMap])].filter(Boolean)) {
    if (texture.colorSpace !== THREE.NoColorSpace) throw new Error(`Combined surface data must remain linear: ${texture.name}`);
  }
  const maskTransform = new THREE.Vector4(Number(parameter.LayerMaskScaleX ?? 1), Number(parameter.LayerMaskScaleY ?? 1),
    Number(parameter.LayerMaskOffsetX || 0), Number(parameter.LayerMaskOffsetY || 0));
  // The custom composition supplies the complete base color and roughness.
  material.map = null;
  material.color.set(0xffffff);
  material.roughness = 1;
  const previous = material.onBeforeCompile;
  const previousKey = material.customProgramCacheKey();
  material.onBeforeCompile = shader => {
    previous(shader);
    shader.uniforms.arenaLayerMask = { value: mask };
    shader.uniforms.arenaMaskTransform = { value: maskTransform };
    shader.uniforms.arenaLayerEnabled = { value: new THREE.Vector3(...[1, 2, 3].map(index => layers.some(layer => layer.index === index) ? 1 : 0)) };
    let declarations = 'varying vec2 vArenaMeshUv; varying vec2 vArenaDetailUv;\n'
      + 'uniform sampler2D arenaLayerMask; uniform vec4 arenaMaskTransform; uniform vec3 arenaLayerEnabled;\n';
    let composition = `vec3 arenaMasks = clamp(texture2D(arenaLayerMask, vArenaMeshUv * arenaMaskTransform.xy + arenaMaskTransform.zw).rgb, 0.0, 1.0) * arenaLayerEnabled;
      vec4 arenaWeights = vec4((1.0-arenaMasks.r)*(1.0-arenaMasks.g)*(1.0-arenaMasks.b),
        arenaMasks.r*(1.0-arenaMasks.g)*(1.0-arenaMasks.b), arenaMasks.g*(1.0-arenaMasks.b), arenaMasks.b);
      vec3 arenaBlendedColor = vec3(0.0); float arenaBlendedRoughness = 0.0; float arenaBlendedAo = 0.0;\n`;
    for (const layer of layers) {
      const id = layer.index;
      const tint = new THREE.Color().setRGB(...layer.modulate, THREE.SRGBColorSpace);
      const base = new THREE.Color().setRGB(...layer.color, THREE.SRGBColorSpace).multiply(tint);
      shader.uniforms[`arenaColor${id}`] = { value: layer.colorMap ? tint : base };
      shader.uniforms[`arenaRoughness${id}`] = { value: layer.roughness };
      declarations += `uniform vec3 arenaColor${id}; uniform float arenaRoughness${id};\n`;
      composition += `vec3 arenaC${id} = ${!layer.hasColor && id ? 'arenaC0' : `arenaColor${id}`};
        float arenaR${id} = arenaRoughness${id}; float arenaA${id} = 1.0;\n`;
      for (const [channel, texture, transform, variable, component] of [
        ['color', layer.colorMap, layer.colorTransform, `arenaC${id}`, 'rgb'],
        ['rough', layer.roughnessMap, layer.roughnessTransform, `arenaR${id}`, 'g'],
        ['ao', layer.aoMap, layer.aoTransform, `arenaA${id}`, 'r'],
      ]) {
        if (!texture) continue;
        const key = `arena_${channel}${id}`;
        shader.uniforms[key] = { value: texture };
        shader.uniforms[`${key}Transform`] = { value: new THREE.Vector4(...transform) };
        declarations += `uniform sampler2D ${key}; uniform vec4 ${key}Transform;\n`;
        let sample = `texture2D(${key}, vArenaDetailUv * ${key}Transform.xy + ${key}Transform.zw)`;
        if (channel === 'color' && texture.userData.range) {
          const range = texture.userData.range;
          for (const [suffix, array] of [['Min', range.min], ['Max', range.max], ['Mask', range.mask]]) {
            shader.uniforms[key + suffix] = { value: new THREE.Vector4(...array) };
            declarations += `uniform vec4 ${key + suffix};\n`;
          }
          sample = `mix(${sample}, mix(${key}Min, ${key}Max, ${sample}), ${key}Mask)`;
        }
        composition += `${variable} ${channel === 'color' ? '*=' : '='} ${sample}.${component};\n`;
      }
    }
    for (let id = 0; id < 4; id++) {
      const active = layers.some(layer => layer.index === id) ? id : 0;
      composition += `arenaBlendedColor += arenaWeights[${id}] * arenaC${active};
        arenaBlendedRoughness += arenaWeights[${id}] * arenaR${active}; arenaBlendedAo += arenaWeights[${id}] * arenaA${active};\n`;
    }
    if (meshAo) {
      shader.uniforms.arenaMeshAo = { value: meshAo };
      declarations += 'uniform sampler2D arenaMeshAo;\n';
      composition += 'arenaBlendedAo *= texture2D(arenaMeshAo, vArenaMeshUv).r;\n';
    }
    if (overlay) {
      const number = (value, fallback) => value == null || !Number.isFinite(Number(value)) ? fallback : Number(value);
      const angle = number(parameter.LAYER4_AlbedoRotation, 0) * Math.PI / 180;
      shader.uniforms.arenaOverlay = { value: overlay };
      shader.uniforms.arenaOverlayRotation = { value: new THREE.Vector2(Math.cos(angle), Math.sin(angle)) };
      shader.uniforms.arenaOverlayTransform = { value: new THREE.Vector4(
        number(parameter.LAYER4_AlbedoScaleX, 1) * number(parameter.LAYER4_GlobalUScale, 1),
        number(parameter.LAYER4_AlbedoScaleY, 1) * number(parameter.LAYER4_GlobalVScale, 1),
        number(parameter.LAYER4_AlbedoOffsetX, 0), number(parameter.LAYER4_AlbedoOffsetY, 0)) };
      shader.uniforms.arenaOverlayTint = { value: new THREE.Color().setRGB(...(parameter.LAYER4_AlbedoModulate || [1, 1, 1]).slice(0, 3), THREE.SRGBColorSpace) };
      shader.uniforms.arenaOverlayRoughness = { value: number(parameter.LAYER4_DefaultRoughness, 0.5) };
      declarations += 'uniform sampler2D arenaOverlay; uniform vec2 arenaOverlayRotation; uniform vec4 arenaOverlayTransform; uniform vec3 arenaOverlayTint; uniform float arenaOverlayRoughness;\n';
      composition += `vec2 arenaOverlayP = vArenaMeshUv - vec2(0.5);
        vec2 arenaOverlayQ = vec2(arenaOverlayRotation.x*arenaOverlayP.x + arenaOverlayRotation.y*arenaOverlayP.y,
          -arenaOverlayRotation.y*arenaOverlayP.x + arenaOverlayRotation.x*arenaOverlayP.y);
        arenaOverlayQ = arenaOverlayQ * arenaOverlayTransform.xy + arenaOverlayTransform.zw + vec2(0.5);
        if (all(greaterThanEqual(arenaOverlayQ, vec2(0.0))) && all(lessThanEqual(arenaOverlayQ, vec2(1.0)))) {
          vec4 arenaArtwork = texture2D(arenaOverlay, arenaOverlayQ);
          arenaBlendedColor = mix(arenaBlendedColor, arenaArtwork.rgb * arenaOverlayTint, arenaArtwork.a);
          arenaBlendedRoughness = mix(arenaBlendedRoughness, arenaOverlayRoughness, arenaArtwork.a);
        }\n`;
    }
    composition += 'diffuseColor.rgb = arenaBlendedColor;\n';
    shader.vertexShader = 'varying vec2 vArenaMeshUv; varying vec2 vArenaDetailUv;\n'
      + (hasDetailUv ? 'attribute vec2 detailUv;\n' : '') + shader.vertexShader;
    shader.vertexShader = shader.vertexShader.replace('#include <uv_vertex>', `#include <uv_vertex>\nvArenaMeshUv = uv; vArenaDetailUv = ${hasDetailUv ? 'detailUv' : 'uv'};`);
    shader.fragmentShader = declarations + shader.fragmentShader;
    shader.fragmentShader = shader.fragmentShader.replace('#include <map_fragment>', '#include <map_fragment>\n' + composition)
      .replace('#include <roughnessmap_fragment>', '#include <roughnessmap_fragment>\nroughnessFactor = clamp(arenaBlendedRoughness, 0.08, 1.0);')
      .replace('#include <aomap_fragment>', '#include <aomap_fragment>\nreflectedLight.indirectDiffuse *= mix(1.0, arenaBlendedAo, 0.65);');
  };
  const layout = layers.map(layer => `${layer.index}:${layer.hasColor}:${Boolean(layer.colorMap)}:${Boolean(layer.roughnessMap)}:${Boolean(layer.aoMap)}:${Boolean(layer.colorMap?.userData.range)}`).join(',');
  material.customProgramCacheKey = () => `${previousKey}:arena-layers-v2:${hasDetailUv}:${Boolean(meshAo)}:${Boolean(overlay)}:${layout}`;
  material.userData.arenaLayers = { mask: mask.name, meshUv: 'TEXCOORD0', detailUv: hasDetailUv ? 'TEXCOORD2' : 'TEXCOORD0 fallback',
    meshAo: meshAo?.name || '', overlay: overlay ? { texture: overlay.name, uv: 'TEXCOORD0', rotationDegrees: Number(parameter.LAYER4_AlbedoRotation || 0) } : null,
    layers: layers.map(layer => ({ index: layer.index, colorMap: layer.colorMap?.name || '',
      roughnessMap: layer.roughnessMap?.name || '', aoMap: layer.aoMap?.name || '', colorTransform: layer.colorTransform })) };
  return material.userData.arenaLayers;
}
