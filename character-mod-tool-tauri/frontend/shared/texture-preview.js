import * as THREE from './vendor/three.module.min.js';
import { DDSLoader } from './vendor/DDSLoader.js';
import { previewTextureUsesLinearSamples } from './render-profiles.mjs';

const canvas = document.createElement('canvas');
const renderer = new THREE.WebGLRenderer({ canvas, alpha: true, antialias: false, preserveDrawingBuffer: true });
renderer.outputColorSpace = THREE.SRGBColorSpace;
renderer.setPixelRatio(1);
renderer.setSize(420, 420, false);
renderer.setClearColor(0x000000, 0);

const scene = new THREE.Scene();
const camera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0, 2);
camera.position.z = 1;
const geometry = new THREE.PlaneGeometry(2, 2);
const plane = new THREE.Mesh(geometry);
scene.add(plane);
let generation = 0;

function textureBytes(item) {
  const binary = atob(item.bytes);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index);
  return bytes;
}

function createTexture(item, channel, flipY = true) {
  const bytes = textureBytes(item);
  const parsed = new DDSLoader().parse(bytes.buffer, true);
  if (!parsed.format || !parsed.width || !parsed.height || parsed.isCubemap) return null;
  const texture = new THREE.CompressedTexture(parsed.mipmaps, parsed.width, parsed.height, parsed.format);
  texture.colorSpace = previewTextureUsesLinearSamples(item, channel)
    ? THREE.NoColorSpace : THREE.SRGBColorSpace;
  texture.minFilter = parsed.mipmapCount > 1 ? THREE.LinearMipmapLinearFilter : THREE.LinearFilter;
  texture.magFilter = THREE.LinearFilter;
  texture.wrapS = texture.wrapT = THREE.ClampToEdgeWrapping;
  if (flipY) {
    texture.repeat.y = -1;
    texture.offset.y = 1;
  }
  texture.needsUpdate = true;
  return texture;
}

function configureMaterial(material, range, channel) {
  if (!range && channel === 'rgb') return;
  const channelLine = ({
    r: 'sampledDiffuseColor = vec4( vec3( sampledDiffuseColor.r ), 1.0 );',
    g: 'sampledDiffuseColor = vec4( vec3( sampledDiffuseColor.g ), 1.0 );',
    b: 'sampledDiffuseColor = vec4( vec3( sampledDiffuseColor.b ), 1.0 );',
    a: 'sampledDiffuseColor = vec4( vec3( sampledDiffuseColor.a ), 1.0 );',
  })[channel] || '';
  const rangeUniforms = range
    ? 'uniform vec4 mapRangeMin;\nuniform vec4 mapRangeMax;\nuniform vec4 mapRangeMask;'
    : '';
  const rangeDecode = range
    ? `vec4 rangeDecodedColor = mix( mapRangeMin, mapRangeMax, sampledDiffuseColor );
          sampledDiffuseColor = mix( sampledDiffuseColor, rangeDecodedColor, mapRangeMask );`
    : '';
  material.onBeforeCompile = (shader) => {
    if (range) {
      shader.uniforms.mapRangeMin = { value: new THREE.Vector4(...range.min) };
      shader.uniforms.mapRangeMax = { value: new THREE.Vector4(...range.max) };
      shader.uniforms.mapRangeMask = { value: new THREE.Vector4(...range.mask) };
    }
    shader.fragmentShader = shader.fragmentShader
      .replace(
        '#include <map_pars_fragment>',
        `#include <map_pars_fragment>\n${rangeUniforms}`,
      )
      .replace(
        '#include <map_fragment>',
        `#ifdef USE_MAP
          vec4 sampledDiffuseColor = texture2D( map, vMapUv );
          ${rangeDecode}
          ${channelLine}
          diffuseColor *= sampledDiffuseColor;
        #endif`,
      );
  };
  material.customProgramCacheKey = () => `nba2k27-gallery-${range ? 'range' : 'raw'}-${channel}`;
}

function renderItem(item, channel, flipY = true) {
  const texture = createTexture(item, channel, flipY);
  if (!texture) return null;
  const material = new THREE.MeshBasicMaterial({ map: texture, transparent: channel === 'rgb', toneMapped: false });
  configureMaterial(material, item.range, channel);
  plane.material = material;
  const aspect = Math.max(0.0001, Number(item.width || 1) / Number(item.height || 1));
  plane.scale.set(aspect >= 1 ? 1 : aspect, aspect >= 1 ? 1 / aspect : 1, 1);
  renderer.clear();
  renderer.render(scene, camera);
  const image = canvas.toDataURL('image/png');
  material.dispose();
  texture.dispose();
  plane.material = null;
  return image;
}

async function load(items, onTexture = () => {}, options = {}) {
  const currentGeneration = ++generation;
  const channel = ['rgb', 'r', 'g', 'b', 'a'].includes(options.channel) ? options.channel : 'rgb';
  const flipY = options.flipY ?? true;
  const results = [];
  for (let index = 0; index < items.length; index += 1) {
    if (currentGeneration !== generation) break;
    let image = null;
    try { image = renderItem(items[index], channel, flipY); } catch {}
    results.push(image);
    onTexture({ index, image, completed: index + 1, total: items.length });
    if (index % 3 === 2) await new Promise((resolve) => requestAnimationFrame(resolve));
  }
  return results;
}

function cancel() {
  generation += 1;
}

window.texturePreview = { cancel, load };
