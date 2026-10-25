const assert = require('assert/strict');
const path = require('path');
const fs = require('fs');
const { spawnSync } = require('child_process');
const { root } = require('./rpc-client.cjs');
const { buildDdsFromPayload } = require('../../character-mod-tool-webview2/dist/Character-Mod-Tool-WebView2-0.1.31/App/Reference/src/core/tld');
const version = JSON.parse(fs.readFileSync(path.join(root, 'dist/BUILD.json'), 'utf8').replace(/^\uFEFF/, '')).version;
const exe = path.join(root, 'dist/Character-Mod-Tool-Tauri-Preview-' + version + '.exe');
let cases = 0;
for (const format of [10, 11, 26, 28, 49, 61, ...Array.from({ length: 15 }, (_, i) => 70 + i), ...Array.from({ length: 6 }, (_, i) => 94 + i)]) {
  for (const layers of [1, 2]) {
    const header = Buffer.alloc(32); header.write('TLD'); header[10] = format; header[11] = 3;
    header.writeUInt16LE(8, 16); header.writeUInt16LE(8, 18); header.writeUInt16LE(1, 20); header.writeUInt16LE(layers, 22);
    const block = [70, 71, 72, 79, 80, 81].includes(format) ? 8 : format >= 70 ? 16 : 0;
    const pixel = [10, 11].includes(format) ? 8 : [26, 28].includes(format) ? 4 : format === 49 ? 2 : 1;
    let size = 0;
    for (const width of [8, 4, 2]) size += block ? Math.ceil(width / 4) ** 2 * block : width ** 2 * pixel;
    size *= layers;
    const payload = Buffer.from(Array.from({ length: size }, (_, i) => i % 251));
    header.writeUInt32LE(size, 24);
    const expected = buildDdsFromPayload(header, payload);
    assert(expected);
    const result = spawnSync(exe, ['--build-dds'], { input: Buffer.concat([header, payload]), windowsHide: true, timeout: 15000 });
    assert.equal(result.status, 0, String(result.stderr));
    assert.deepEqual(result.stdout, expected, 'DDS differs for format ' + format + ', layers ' + layers);
    cases++;
  }
}
const invalid = spawnSync(exe, ['--build-dds'], { input: Buffer.alloc(32), windowsHide: true, timeout: 15000 });
assert.notEqual(invalid.status, 0);
assert.equal(invalid.stdout.length, 0);
console.log(cases + ' native DDS cases match the original builder byte-for-byte; invalid input rejected.');
