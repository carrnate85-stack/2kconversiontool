import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';

const source = readFileSync(new URL('./viewer-controls.js', import.meta.url), 'utf8');
function setup() {
  const button = dataset => ({ dataset, attributes: {}, listeners: {},
    addEventListener(type, listener) { this.listeners[type] = listener; },
    setAttribute(name, value) { this.attributes[name] = value; },
    click() { this.listeners.click(); } });
  const views = ['front', 'back', 'reset'].map(view => button({ view }));
  const backdrops = ['dark', 'light'].map(backdrop => button({ backdrop }));
  const canvas = button({});
  const state = { yaw: 1.2, pitch: 0.6, distance: 4, target: { x: 2, y: 3, z: 1 }, drag: {}, draws: 0, fits: 0, stops: 0,
    canvas, document: { body: { dataset: {} }, querySelectorAll(selector) {
      if(selector === '.studio-controls [data-view]') return views;
      if(selector === '.studio-controls [data-backdrop]') return backdrops;
      throw new Error('Controls must be scoped to the toolbar');
    } },
    stopPan() { state.stops++; }, draw() { state.draws++; }, fit() { state.fits++; }, Math };
  runInNewContext(source, state);
  return { state, views, backdrops, canvas };
}
test('front and back change angle without discarding zoom/pan; reset refits', () => {
  const { state, views } = setup();
  views[0].click();
  assert.equal(state.yaw, 0); assert.equal(state.pitch, 0);
  views[1].click();
  assert.equal(state.yaw, Math.PI); assert.equal(state.distance, 4);
  assert.deepEqual(state.target, { x: 2, y: 3, z: 1 }); assert.equal(state.fits, 0);
  views[2].click();
  assert.equal(state.yaw, 0.45); assert.equal(state.pitch, 0.1);
  assert.equal(state.fits, 1); assert.equal(state.drag, null); assert.equal(state.stops, 3);
});
test('backdrop selection has an accessible selected state and needs no WebGL draw', () => {
  const { state, backdrops } = setup();
  backdrops[1].click();
  assert.equal(state.document.body.dataset.backdrop, 'light');
  assert.equal(backdrops[1].attributes['aria-pressed'], 'true');
  assert.equal(backdrops[0].attributes['aria-pressed'], 'false');
  backdrops[0].click();
  assert.equal(state.document.body.dataset.backdrop, 'dark');
  assert.equal(backdrops[1].attributes['aria-pressed'], 'false');
  assert.equal(state.draws, 0); assert.equal(state.fits, 0);
});
test('double click refits without rotating and ends any in-progress pan or drag', () => {
  const { state, canvas } = setup();
  let prevented = false;
  canvas.listeners.dblclick({ preventDefault() { prevented = true; } });
  assert.equal(prevented, true); assert.equal(state.fits, 1);
  assert.equal(state.yaw, 1.2); assert.equal(state.pitch, 0.6);
  assert.equal(state.drag, null); assert.equal(state.stops, 1);
});
