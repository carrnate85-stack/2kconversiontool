// Developer-only oracle: runs WPF core/player-audio-preview.js archiveKey/toOgg
// and writes assets/audio-preview-regressions.json. No game files.
//   node scripts/audio-preview-regressions.mjs <WPF Reference src folder>
import { createRequire } from 'node:module';
import { writeFileSync } from 'node:fs';
import path from 'node:path';
const reference = path.resolve(process.argv[2] ?? '../nba2k27-launcher-wpf/release/quality-pass-qa-20261010/App/Reference/src');
const require = createRequire(path.join(reference, 'main.js'));
const audio = require('./core/player-audio-preview.js');
let seed = 7;
const rnd = n => { seed = (seed * 1103515245 + 12345) >>> 0; return seed % n; };
const keys = ['newaudio/players/directory_listing.adb2', 'newaudio/paplayers/directory_listing.adb2', 'newaudio/players/000001-001234-000000-000000-000002-000000.wav', 'a', ''].map(name => ({ name, key: audio.archiveKey(name).toString(16).padStart(16, '0') }));
function clip(frames, config, trim) {
  const packets = Array.from({ length: frames }, () => { const n = 1 + rnd(200); const p = Buffer.alloc(n); for (let i = 0; i < n; i++) p[i] = rnd(256); p[0] = config; return p; });
  const per = config === 0x78 ? 960 : 480, samples = frames * per - trim;
  const body = Buffer.concat(packets.flatMap(p => { const h = Buffer.alloc(2); h.writeUInt16LE(p.length); return [h, p]; }));
  const head = Buffer.alloc(44); head.writeUInt32LE(0xbbcfa17e, 0); head.writeUInt32LE(36, 4); head.writeUInt32LE(0x758195f9, 8); head.writeUInt32LE(1, 12); head.writeUInt32LE(48000, 16); head.writeUInt32LE(samples, 20); head.writeUInt32LE(body.length, 28);
  return Buffer.concat([head, body]);
}
const clips = [[3, 0x78, 100], [40, 0x70, 0], [1, 0x78, 959], [5, 0x78, 960], [2, 0x71, 0]].map(([f, c, t]) => {
  const raw = clip(f, c, t);
  try { return { raw: raw.toString('hex'), ogg: audio.toOgg(raw).toString('hex') }; } catch (e) { return { raw: raw.toString('hex'), error: e.message }; }
});
writeFileSync(new URL('../assets/audio-preview-regressions.json', import.meta.url), JSON.stringify({ keys, clips }));
console.log(JSON.stringify({ keys: keys.length, clips: clips.map(c => c.error ?? 'ok') }));
