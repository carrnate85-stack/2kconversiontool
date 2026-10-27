// The app's model-preview wrapper (body fit, jersey fit, editor display) on a
// decoded player, old and new side by side.
//   wrapper.html?player=png01013-headphone&jersey=1&yaw=0&pitch=0.05&zoom=0.9
import { loadPlayer } from './payload.mjs';

const params = new URLSearchParams(location.search);
const log = (...parts) => { document.getElementById('log').textContent += parts.join(' ') + '\n'; };


window.wrapperReady = (async () => {
  for (const side of ['old', 'new']) {
    await import(`./${side}/model-preview.js?canvas=${side}Canvas&global=viewer_${side}&interactive=0`);
    const viewer = window[`viewer_${side}`];
    // Loading the payload is harness-only work, timed apart from the app's load;
    // each side gets its own copy because renderers edit geometry in place.
    const loadStarted = performance.now();
    const payload = await loadPlayer(params.get('player'));
    log(side, 'read the harness payload in', (performance.now() - loadStarted).toFixed(0), 'ms');
    const started = performance.now();
    await viewer.load(payload);
    viewer.setJerseyVisible(params.get('jersey') !== '0');
    const state = viewer.cameraState();
    viewer.setCameraState({ ...state, yaw: Number(params.get('yaw') ?? 0), pitch: Number(params.get('pitch') ?? 0.05),
      distance: state.fittedDistance * Number(params.get('zoom') ?? 0.9) });
    const stats = viewer.stats();
    log(side, 'loaded in', (performance.now() - started).toFixed(0), 'ms; profile', JSON.stringify(stats.loadProfile), '; jersey fit', JSON.stringify(stats.bodyFitJersey));
  }
  log('done');
})().catch((error) => log('error', error.stack || error.message));
