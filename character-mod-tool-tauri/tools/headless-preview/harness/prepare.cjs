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
// Every file under web/renderer/ (the app wrapper, its modules and the vendored
// shared renderer in shared/), published at the same relative path, except the
// explorer entry, which the app serves as explorer/model-preview.js.
const published = (source) => source === 'explorer-model-preview.js' ? 'explorer/model-preview.js' : source;

function vendored(destination) {
  fs.mkdirSync(path.join(destination, 'explorer'), { recursive: true });
  fs.cpSync(path.join(app, 'frontend/explorer/vendor'), path.join(destination, 'explorer/vendor'), { recursive: true });
  for (const name of fs.readdirSync(path.join(app, 'frontend/explorer')).filter((name) => name.endsWith('.mjs'))) {
    fs.copyFileSync(path.join(app, 'frontend/explorer', name), path.join(destination, 'explorer', name));
  }
}
function write(side, source, data) {
  const file = path.join(harness, side, published(source));
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, data);
}
function workingTreeFiles(dir = path.join(app, 'web/renderer'), base = dir) {
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap((entry) => entry.isDirectory()
    ? workingTreeFiles(path.join(dir, entry.name), base) : [path.relative(base, path.join(dir, entry.name)).split(path.sep).join('/')]);
}
const git = (...args) => execFileSync('git', ['-C', app, ...args], { stdio: ['ignore', 'pipe', 'ignore'], maxBuffer: 64 * 1024 * 1024 });

for (const side of ['old', 'new']) fs.rmSync(path.join(harness, side), { recursive: true, force: true });
for (const side of ['old', 'new']) vendored(path.join(harness, side));
for (const source of workingTreeFiles()) write('new', source, fs.readFileSync(path.join(app, 'web/renderer', source)));
const baselineFiles = git('ls-tree', '-r', '--name-only', baseline, '--', 'web/renderer').toString().split(/\r?\n/).filter(Boolean)
  .map((file) => file.replace(/^.*?web\/renderer\//, ''));
for (const source of baselineFiles) write('old', source, git('show', `${baseline}:./web/renderer/${source}`));
if (!fs.existsSync(path.join(harness, 'old/explorer/model-preview.js'))) throw new Error(`No explorer renderer at ${baseline}`);
fs.copyFileSync(path.join(app, 'frontend/skin-preview-fixes.mjs'), path.join(harness, 'skin-preview-fixes.mjs'));
for (const side of ['old', 'new']) fs.copyFileSync(path.join(app, 'frontend/skin-preview-fixes.mjs'), path.join(harness, side, 'skin-preview-fixes.mjs'));
console.log(`Harness renderers: old = ${baseline}, new = working tree.`);
