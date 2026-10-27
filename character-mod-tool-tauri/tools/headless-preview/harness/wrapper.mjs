// The app's model-preview wrapper (body fit, jersey fit, editor display) on a
// decoded player, old and new side by side.
//   wrapper.html?player=png01013-headphone&jersey=1&yaw=0&pitch=0.05&zoom=0.9
const params = new URLSearchParams(location.search);
const log = (...parts) => { document.getElementById('log').textContent += parts.join(' ') + '\n'; };
const decode = (value) => Array.isArray(value) ? value.map(decode) : !value || typeof value !== 'object' ? value
  : value.__typed ? new globalThis[value.__typed](Uint8Array.from(atob(value.base64), (c) => c.charCodeAt(0)).buffer)
    : Object.fromEntries(Object.entries(value).map(([key, item]) => [key, decode(item)]));

window.wrapperReady = (async () => {
  const response = await fetch(`players/${params.get('player')}.json`);
  if (!response.ok) throw new Error(`players/${params.get('player')}.json not found`);
  const source = await response.json();
  for (const side of ['old', 'new']) {
    await import(`./${side}/model-preview.js?canvas=${side}Canvas&global=viewer_${side}&interactive=0`);
    const viewer = window[`viewer_${side}`];
    const started = performance.now();
    await viewer.load(decode(source));
    viewer.setJerseyVisible(params.get('jersey') !== '0');
    const state = viewer.cameraState();
    viewer.setCameraState({ ...state, yaw: Number(params.get('yaw') ?? 0), pitch: Number(params.get('pitch') ?? 0.05),
      distance: state.fittedDistance * Number(params.get('zoom') ?? 0.9) });
    const fit = viewer.stats().bodyFitJersey;
    log(side, 'loaded in', (performance.now() - started).toFixed(0), 'ms; jersey fit', JSON.stringify(fit));
  }
  log('done');
})().catch((error) => log('error', error.stack || error.message));
