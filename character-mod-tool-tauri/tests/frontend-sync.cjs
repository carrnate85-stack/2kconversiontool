const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

// frontend/ is what the app loads. build-frontend.cjs copies the edited renderer
// sources from web/renderer/ into it, but it needs the production tree, so the
// copies are often refreshed by hand: fail when one is missed. The pairs come
// from the build script itself, so new copies are checked automatically.
const root = path.resolve(__dirname, '..');
const build = fs.readFileSync(path.join(root, 'build-frontend.cjs'), 'utf8');
const pairs = [...build.matchAll(/copyFileSync\(path\.join\(root, '(web\/renderer\/[^']+)'\), path\.join\(output, '([^']+)'\)\)/g)]
  .map(([, source, published]) => [source, `frontend/${published}`]);
assert.ok(pairs.length >= 5, `Expected the build's renderer copies, found ${pairs.length}`);
const text = (file) => fs.readFileSync(path.join(root, file), 'utf8').replace(/\r\n/g, '\n');
const stale = pairs.filter(([source, published]) => text(source) !== text(published));
assert.deepEqual(stale, [], `frontend/ copies out of date (copy web/renderer/ over them): ${stale.map(([, published]) => published).join(', ')}`);
// The shared renderer: frontend/shared must equal web/renderer/shared, which
// must match its lock (and the canonical renderer, when that folder exists).
const { execFileSync } = require('node:child_process');
execFileSync(process.execPath, [path.join(root, 'shared-renderer.mjs'), path.join(root, 'web/renderer/shared'), '--verify'], { stdio: 'inherit' });
const files = (dir, base = dir) => fs.readdirSync(dir, { withFileTypes: true }).flatMap((entry) => entry.isDirectory()
  ? files(path.join(dir, entry.name), base) : [path.relative(base, path.join(dir, entry.name)).split(path.sep).join('/')]).sort();
const vendored = files(path.join(root, 'web/renderer/shared'));
assert.deepEqual(files(path.join(root, 'frontend/shared')), vendored, 'frontend/shared file list differs from web/renderer/shared');
const staleShared = vendored.filter((file) => !fs.readFileSync(path.join(root, 'web/renderer/shared', file)).equals(fs.readFileSync(path.join(root, 'frontend/shared', file))));
assert.deepEqual(staleShared, [], `frontend/shared out of date: ${staleShared.join(', ')}`);
console.log(`Frontend sync: ${pairs.length} renderer copies and ${vendored.length} shared renderer files match.`);
