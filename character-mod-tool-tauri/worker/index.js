import { parentPort } from './threads.js';
import { catalog } from './catalog.js';
import { selectTangentMeshes } from './staged-projection.mjs';
import { resetNativeTransport, nativeTransportStats } from './native.js';
const { createIffPreview, projectTangentSpaceMeshes } = require('cmt-preview');

async function preview({ filePath, options }) {
  const result = await createIffPreview(filePath, options);
  if (options.projectTangent) {
    const meshes = selectTangentMeshes(result.meshes, options.projectTangent);
    const count = projectTangentSpaceMeshes({ meshes }, result);
    result.stats.facialHairProjectedVertices = count;
    const headKey = result.meshes.filter(mesh => mesh.sourceKind === 'main')
      .map(mesh => mesh.geometryKey || '').join('|');
    meshes.forEach(mesh => { if (mesh.geometryKey) mesh.geometryKey += ':head:' + headKey; });
    if (meshes.length && !count) result.skipped.push('Facial-hair geometry could not attach to the staged face tangent frame.');
  }
  return result;
}

const queue = [];
let running = false;
async function drain() {
  if (running) return;
  running = true;
  try {
    while (queue.length) {
      const { id, type, payload } = queue.shift();
      try {
        if (!['preview', 'catalog'].includes(type)) throw new Error('Unknown preview operation.');
        resetNativeTransport();
        const result = await (type === 'catalog' ? catalog(payload) : preview(payload));
        result.stats.nativeTransport = nativeTransportStats();
        parentPort.postMessage({ id, result });
      } catch (error) {
        parentPort.postMessage({ id, error: { message: error.message, name: error.name } });
      }
    }
  } finally { running = false; }
}
parentPort.on('message', message => {
  if (message.type === 'catalog' && message.latestOnly === true) {
    for (let i = queue.length - 1; i >= 0; i--) {
      if (queue[i].type !== 'catalog' || queue[i].latestOnly !== true) continue;
      const [superseded] = queue.splice(i, 1);
      parentPort.postMessage({ id: superseded.id, error: {
        message: 'Preview request cancelled: newer selection queued.', name: 'AbortError',
      } });
    }
  }
  queue.push(message);
  void drain();
});
