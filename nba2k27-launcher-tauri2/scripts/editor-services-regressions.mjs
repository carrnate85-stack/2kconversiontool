// Developer-only oracle generator: runs the frozen WPF Reference editor code
// (core/team-staff.js, core/player-season-stats.js, renderer/player-season-awards.js)
// on deterministic records and writes assets/editor-services-regressions.json.
// No game process, no game files.
//   node scripts/editor-services-regressions.mjs <WPF Reference src folder>
import { createRequire } from 'node:module';
import { writeFileSync } from 'node:fs';
import path from 'node:path';

const reference = path.resolve(process.argv[2] ?? '../nba2k27-launcher-wpf/release/quality-pass-qa-20261010/App/Reference/src');
const require = createRequire(path.join(reference, 'main.js'));
const staff = require('./core/team-staff.js');
const seasons = require('./core/player-season-stats.js');
const awards = require('./renderer/player-season-awards.js');

let seed = 0x2027;
const rnd = () => { seed ^= seed << 13; seed >>>= 0; seed ^= seed >>> 17; seed ^= seed << 5; seed >>>= 0; return seed / 2 ** 32; };
const int = n => Math.floor(rnd() * n);
const bytes = n => Buffer.from(Array.from({ length: n }, () => int(256)));
const attempt = f => { try { return { ok: true, value: f() }; } catch (e) { return { ok: false, error: e.message }; } };

function staffRecord(slot) {
  const b = bytes(456);
  b.fill(0, 0x50, 0xa0);
  const first = ['Becky', 'Steve', 'Ünïcode', 'Q', ''][int(5)], last = ['Hammon', 'Kerr', 'Coach', 'Z'][int(4)];
  b.write(first, 0x50, 38, 'utf16le');
  b.write(last, 0x78, 38, 'utf16le');
  b[0x182] = (b[0x182] & 0xe0) | slot;
  return b;
}
const controls = staff.controls;
const staffCases = [];
for (let i = 0; i < 400; i++) {
  const slot = int(32), raw = staffRecord(slot), index = int(8192);
  const decoded = attempt(() => staff.decode(raw.toString('base64'), slot, index));
  const values = {};
  for (let k = 0, n = 1 + int(4); k < n; k++) {
    const f = controls[int(controls.length)];
    if (f.text) values[f.key] = ['QA', 'Long Name Here', '', 'Bad\u0007', 'x'.repeat(int(25))][int(5)];
    else {
      const max = f.max ?? 2 ** f.bits - 1, min = f.min ?? 0;
      values[f.key] = [min, max, max + 1, min - 1, min + int(max - min + 1), 15, 1.5][int(7)];
    }
  }
  const patched = attempt(() => staff.patch(raw.toString('base64'), values).toString('hex'));
  staffCases.push({ raw: raw.toString('hex'), slot, index, decoded: decoded.ok && i < 40 ? { ...decoded.value, controls: undefined } : null, decodedOk: decoded.ok, decodeError: decoded.ok ? null : decoded.error, values, after: patched.ok ? patched.value : null, patchError: patched.ok ? null : patched.error });
}

const fieldKeys = Object.keys(seasons.fields);
const extra = ['ppg', 'mpg', 'orpg', 'apg', 'spg', 'bpg', 'topg', 'fpg', 'plusMinusAverage', 'fgPercent', 'threePercent', 'ftPercent'];
function seasonRecord() {
  const b = bytes(64);
  // Mostly plausible totals so consistency checks pass and fail in both ways.
  const put = (key, value) => { const [o, s, bits] = seasons.fields[key]; const mask = ((2 ** bits - 1) << s) >>> 0; b.writeUInt32LE(((b.readUInt32LE(o) & ~mask) | ((value & (2 ** bits - 1)) << s)) >>> 0, o); };
  const gp = 1 + int(82), fga = int(1500), fgm = int(fga + 1), tpa = int(Math.min(fga, 600) + 1), tpm = int(Math.min(tpa, fgm) + 1), fta = int(600), ftm = int(fta + 1);
  for (const [k, v] of Object.entries({ gp, gs: int(gp + 1), fga, fgm, threeAttempted: tpa, threeMade: tpm, fta, ftm, points: 2 * fgm + tpm + ftm + (rnd() < 0.2 ? 1 : 0), doubleDoubles: int(gp + 1), tripleDoubles: int(gp + 1), minutes: int(3000) }))
    if (rnd() < 0.95) put(k, v);
  return b;
}
const seasonCases = [];
for (let i = 0; i < 400; i++) {
  const record = seasonRecord(), changes = {}, awardChanges = {};
  for (let k = 0, n = 1 + int(3); k < n; k++) {
    const pick = rnd();
    if (pick < 0.45) { const key = fieldKeys[int(fieldKeys.length)]; changes[key] = [0, int(300), int(20000), -5, 12.5][int(5)]; }
    else if (pick < 0.75) changes[extra[int(extra.length)]] = [0, 1.5, 25.3, 99, 101, -1][int(6)];
    else { const f = awards.fields[int(awards.fields.length)]; awardChanges[f.key] = int((f.max ?? 1) + 2); }
  }
  const result = attempt(() => {
    const r = seasons.plan(record, changes, { linkedPlayers: [{ recordIndex: 1 }], confirmedShared: true });
    return { after: Buffer.from(awards.patch(r.after, awardChanges)).toString('hex'), warnings: r.warnings };
  });
  const request = { ...changes, ...Object.fromEntries(Object.entries(awardChanges).map(([k, v]) => ['award:' + k, v])) };
  seasonCases.push({ record: record.toString('hex'), changes: request, totals: seasons.decode(record), awards: awards.decode(record), after: result.ok ? result.value.after : null, warnings: result.ok ? result.value.warnings : null, error: result.ok ? null : result.error });
}
writeFileSync(new URL('../assets/editor-services-regressions.json', import.meta.url), JSON.stringify({ schema: 'nba2k27-editor-services-regressions-v1', generator: 'scripts/editor-services-regressions.mjs', staff: staffCases, seasons: seasonCases }));
console.log(JSON.stringify({ staff: staffCases.length, staffPatched: staffCases.filter(c => c.after).length, seasons: seasonCases.length, seasonPlanned: seasonCases.filter(c => c.after).length }));
