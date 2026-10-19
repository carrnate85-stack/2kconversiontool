import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
const require = createRequire(import.meta.url);
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const source = path.resolve(root, "../nba2k27-mod-launcher/src");
const dna = require(path.join(source, "core/player-dna.js"));
const schema = require(path.join(source, "data/player-field-schema.json"));
const { maximum } = require(path.join(source, "core/player-field-edits.js"));
const math = require(path.join(source, "renderer/player-season-math.js"));
const awards = require(path.join(source, "renderer/player-season-awards.js"));
const base = {
  format: "2kplayer",
  version: 1,
  game: "NBA2K27",
  schemaId: dna.schemaId,
  name: "QA player",
  info: { birthDate: "2000-02-29", yearsPro: 3 },
  fields: {},
};
const cases = [];
function add(input) {
  let expected = null;
  try {
    expected = dna.validate(input);
  } catch {}
  cases.push({ input, expected });
}
add(base);
for (const f of schema)
  for (const value of [-1, 0, maximum(f), maximum(f) + 1, 0.5])
    add({ ...base, fields: { [f.key]: value } });
for (const date of [
  "0001-01-01",
  "4095-12-31",
  "1900-02-29",
  "2000-02-29",
  "2000-13-01",
  "2026-04-31",
  "bad",
])
  add({ ...base, info: { birthDate: date } });
for (const key of [
  "version",
  "game",
  "format",
  "schemaId",
  "name",
  "info",
  "fields",
])
  add({ ...base, [key]: null });
for (const field of [
  "birthDate",
  "yearsPro",
  "weightLbs",
  "potential",
  "firstName",
  "fullNameId",
  "unsupported",
])
  for (const value of [-1, 0, 32, 110, 500, "", null, "QA"])
    add({ ...base, info: { [field]: value } });
const season = {
  year: 2026,
  team: "76ers",
  totals: Object.fromEntries(Object.keys(math.limits).map((k) => [k, 0])),
  awards: Object.fromEntries(awards.fields.map((f) => [f.key, 0])),
};
add({ ...base, version: 2, seasonHistory: [season] });
add({
  ...base,
  version: 2,
  seasonHistory: [{ ...season, address: 12345 }],
  pointer: 42,
});
add({ ...base, seasonHistory: [] });
add({ ...base, version: 2 });
add({ ...base, version: 2, seasonHistory: [season, season] });
for (const [key, max] of Object.entries(math.limits))
  for (const value of [-1, max, max + 1])
    add({
      ...base,
      version: 2,
      seasonHistory: [
        { ...season, totals: { ...season.totals, [key]: value } },
      ],
    });
for (const f of awards.fields)
  for (const value of [0, f.max ?? 1, (f.max ?? 1) + 1])
    add({
      ...base,
      version: 2,
      seasonHistory: [
        { ...season, awards: { ...season.awards, [f.key]: value } },
      ],
    });
const file = path.join(root, "assets/player-dna-regressions.json"),
  bytes = JSON.stringify(cases);
if (!fs.existsSync(file) || fs.readFileSync(file, "utf8") !== bytes)
  fs.writeFileSync(file, bytes);
console.log(JSON.stringify({ playerDNAOracleCases: cases.length }));
