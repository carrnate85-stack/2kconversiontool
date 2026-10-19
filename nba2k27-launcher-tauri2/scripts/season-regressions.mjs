// Build-time oracle only; no JavaScript backend is shipped.
import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const require = createRequire(import.meta.url),
  baseline = path.resolve(root, "../nba2k27-mod-launcher/src/renderer");
const transfer = require(path.join(baseline, "player-season-transfer.js"));
const math = require(path.join(baseline, "player-season-math.js"));
const awards = require(path.join(baseline, "player-season-awards.js"));
const row = (year, team = "Warriors") => ({
  year,
  team,
  totals: Object.fromEntries(Object.keys(math.limits).map((k) => [k, 0])),
});
const cases = [];
function check(source, existing = []) {
  let expected = null;
  try {
    expected = transfer.mergeReviewed(source, existing);
  } catch {}
  cases.push({ source, existing, expected });
}
check([]);
check([row(2019)]);
check([row(2019)], [row(2020), row(2018)]);
for (const f of awards.fields) {
  for (const value of [0, f.max ?? 1, (f.max ?? 1) + 1, -1, 0.5]) {
    const a = row(2019);
    a.awards = Object.fromEntries(awards.fields.map((f) => [f.key, 0]));
    a.awards[f.key] = value;
    check([a]);
    const old = row(2019);
    old.awards = { ...a.awards };
    check([row(2019)], [old]);
  }
}
for (const [key, max] of Object.entries(math.limits))
  for (const v of [0, max, max + 1, -1, 0.5, null]) {
    const r = row(2019);
    r.totals[key] = v;
    check([r]);
  }
check([row(2019), row(2019, "76ers")]);
check([row(2019)], [row(2018), row(2018, "76ers")]);
check([row(1899)]);
check([row(2156)]);
check([row(2019, "")]);
check([row(2019, "a\0b")]);
check(Array.from({ length: 26 }, (_, i) => row(2000 + i)));
check(
  [row(2000)],
  Array.from({ length: 25 }, (_, i) => row(2001 + i)),
);
check([
  {
    ...row(2019),
    secondaryTeam: "76ers",
    token: "private",
    slot: 77,
    awards: Object.fromEntries(awards.fields.map((f) => [f.key, 0])),
  },
]);
const a = row(2019);
a.totals.gp = 10;
a.totals.gs = 11;
check([a]);
const b = row(2019);
b.totals.threeMade = 1;
check([b]);
const file = path.join(root, "assets/season-regressions.json"),
  data = JSON.stringify(cases);
if (!fs.existsSync(file) || fs.readFileSync(file, "utf8") !== data)
  fs.writeFileSync(file, data);
console.log(JSON.stringify({ seasonMergeOracleCases: cases.length }));
