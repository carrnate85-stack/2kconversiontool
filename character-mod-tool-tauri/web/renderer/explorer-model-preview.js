// Compatibility entry for tests and tools that import ./explorer/model-preview.js:
// the renderer itself is the shared one (web/renderer/shared, synced from the
// 2K Exporter's shared/model-preview by shared-renderer.mjs).
export { createModelPreview as createExplorerRenderer } from '../shared/model-preview-core.mjs';
