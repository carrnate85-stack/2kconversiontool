// Draws a game headshot DDS (from `player-extract headshot`) as a reference panel.
// Usage from the page console or app.mjs: showHeadshot(context, 'players/headshot-809.dds').
export async function showHeadshot(context, url, { left = 8, top = 440, size = 160 } = {}) {
  const { THREE } = context;
  const { DDSLoader } = await import('./new/explorer/vendor/DDSLoader.js');
  const response = await fetch(url);
  if (!response.ok) return null;
  const parsed = new DDSLoader().parse(await response.arrayBuffer(), true);
  const texture = new THREE.CompressedTexture(parsed.mipmaps, parsed.width, parsed.height, parsed.format);
  texture.colorSpace = THREE.SRGBColorSpace;
  texture.needsUpdate = true;
  const pixelsSize = 256, target = new THREE.WebGLRenderTarget(pixelsSize, pixelsSize);
  const scene = new THREE.Scene(), camera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0, 2);
  camera.position.z = 1;
  scene.add(new THREE.Mesh(new THREE.PlaneGeometry(2, 2), new THREE.MeshBasicMaterial({ map: texture, toneMapped: false })));
  const pixels = new Uint8Array(pixelsSize * pixelsSize * 4);
  context.renderer.setRenderTarget(target);
  context.renderer.render(scene, camera);
  context.renderer.readRenderTargetPixels(target, 0, 0, pixelsSize, pixelsSize, pixels);
  context.renderer.setRenderTarget(null);
  target.dispose(); texture.dispose();
  const canvas = document.createElement('canvas');
  canvas.width = canvas.height = pixelsSize;
  canvas.title = `game headshot ${url}`;
  canvas.style = `position:fixed;left:${left}px;top:${top}px;width:${size}px;height:${size}px;border:1px solid #888;z-index:9`;
  const image = canvas.getContext('2d').createImageData(pixelsSize, pixelsSize);
  image.data.set(pixels);
  canvas.getContext('2d').putImageData(image, 0, 0);
  document.body.appendChild(canvas);
  return canvas;
}
