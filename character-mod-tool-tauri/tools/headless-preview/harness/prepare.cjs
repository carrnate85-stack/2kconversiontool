// Assembles the harness renderers: new/ from the working tree, old/ from a git ref.
//   node prepare.cjs [--baseline <git ref>]     default ref: HEAD
// Vendored inputs (three.js, arena modules, render profiles) come from frontend/
// for both sides; only the files under web/renderer/ differ between them.
const fs = require('fs');
const path = require('path');
const { execFileSync } = require('child_process');

const harness = __dirname;
const app = path.resolve(harness, '../../..');
const index = process.argv.indexOf('--baseline');
const baseline = index > 0 ? process.argv[index + 1] : 'HEAD';
// Published name in the harness -> source under web/renderer/.
const rendererFiles = {
  'explorer/model-preview.js': 'explorer-model-preview.js',
  'skin-response.mjs': 'skin-response.mjs',
  'strand-preview-fixes.mjs': 'strand-preview-fixes.mjs',
  'studio-shading.mjs': 'studio-shading.mjs',
  // The app wrapper (wrapper.html): body fit, jersey fit, editor display.
  'model-preview.js': 'model-preview.js',
  'body-preview-editor.mjs': 'body-preview-editor.mjs',
};

function vendored(destination) {
  fs.mkdirSync(path.join(destination, 'explorer'), { recursive: true });
  fs.cpSync(path.join(app, 'frontend/explorer/vendor'), path.join(destination, 'explorer/vendor'), { recursive: true });
  for (const name of fs.readdirSync(path.join(app, 'frontend/explorer')).filter((name) => name.endsWith('.mjs'))) {
    fs.copyFileSync(path.join(app, 'frontend/explorer', name), path.join(destination, 'explorer', name));
  }
}
function fromBaseline(source) {
  try {
    return execFileSync('git', ['-C', app, 'show', `${baseline}:./web/renderer/${source}`], { stdio: ['ignore', 'pipe', 'ignore'], maxBuffer: 64 * 1024 * 1024 });
  } catch {
    return null; // The baseline predates this module.
  }
}

for (const side of ['old', 'new']) fs.rmSync(path.join(harness, side), { recursive: true, force: true });
for (const side of ['old', 'new']) vendored(path.join(harness, side));
for (const [published, source] of Object.entries(rendererFiles)) {
  fs.copyFileSync(path.join(app, 'web/renderer', source), path.join(harness, 'new', published));
  const old = fromBaseline(source);
  if (old) fs.writeFileSync(path.join(harness, 'old', published), old);
}
if (!fs.existsSync(path.join(harness, 'old/explorer/model-preview.js'))) throw new Error(`No explorer renderer at ${baseline}`);
fs.copyFileSync(path.join(app, 'frontend/skin-preview-fixes.mjs'), path.join(harness, 'skin-preview-fixes.mjs'));
for (const side of ['old', 'new']) fs.copyFileSync(path.join(app, 'frontend/skin-preview-fixes.mjs'), path.join(harness, side, 'skin-preview-fixes.mjs'));
console.log(`Harness renderers: old = ${baseline}, new = working tree.`);
