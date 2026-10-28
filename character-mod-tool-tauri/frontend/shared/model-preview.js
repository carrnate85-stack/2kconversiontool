// Page entry: one renderer on a canvas, published as a window global. The
// defaults are the Exporter's (#previewCanvas, window.modelPreview); other
// pages pick their own with ?global=&canvas=&transparent=1&interactive=0&shadows=0.
// Tools that wrap the renderer import createModelPreview from the core instead.
import { createModelPreview } from './model-preview-core.mjs';

const params = new URL(import.meta.url).searchParams;
const global = params.get('global') || 'modelPreview';
const preview = createModelPreview({
  canvas: document.getElementById(params.get('canvas') || 'previewCanvas'),
  viewer: params.has('global') ? global : undefined,
  transparent: params.get('transparent') === '1',
  interactive: params.get('interactive') !== '0',
  shadows: params.get('shadows') !== '0',
});
window[global] = preview.api;
