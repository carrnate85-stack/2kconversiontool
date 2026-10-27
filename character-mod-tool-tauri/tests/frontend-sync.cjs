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
assert.ok(pairs.length >= 6, `Expected the build's renderer copies, found ${pairs.length}`);
const text = (file) => fs.readFileSync(path.join(root, file), 'utf8').replace(/\r\n/g, '\n');
const stale = pairs.filter(([source, published]) => text(source) !== text(published));
assert.deepEqual(stale, [], `frontend/ copies out of date (copy web/renderer/ over them): ${stale.map(([, published]) => published).join(', ')}`);
console.log(`Frontend sync: ${pairs.length} renderer copies match web/renderer/.`);
