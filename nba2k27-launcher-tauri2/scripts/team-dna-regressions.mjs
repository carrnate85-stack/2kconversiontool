import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
const require = createRequire(import.meta.url);
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const source = path.resolve(root, "../nba2k27-mod-launcher/src");
const dna = require(path.join(source, "core/team-dna.js"));
const controls = require(path.join(source, "data/staff-controls.json"));
const uniforms = require(path.join(source, "renderer/team-uniforms.js"));
const base = {
  format: "2kteam",
  version: 1,
  game: "NBA2K27",
  name: "QA team",
  info: { name: "Test team" },
  staff: [],
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
add({
  ...base,
  teamId: 99,
  pointer: "0x123",
  pid: 234,
  cachePath: "C:/private",
});
for (const key of ["format", "version", "game", "name", "info", "staff"])
  for (const value of [null, {}, [], false, 1.5])
    add({ ...base, [key]: value });
for (const [key, max] of Object.entries({
  name: 23,
  cityName: 17,
  cityShort: 6,
  stateShort: 6,
  mainLogo: 35,
}))
  for (const value of [
    "",
    "a".repeat(max),
    "a".repeat(max + 1),
    "🧑".repeat(max),
    "bad\u007f",
    9,
  ])
    add({ ...base, info: { [key]: value } });
for (const key of ["rosterTypeId", "historicYearId", "teamId", "unknown"])
  for (const value of [-1, 0, 31, 32, 127, 128, 0.5])
    add({ ...base, info: { [key]: value } });
for (const f of controls) {
  const max = f.text ? f.bytes / 2 - 1 : (f.max ?? 2 ** f.bits - 1);
  for (const value of f.text
    ? ["", "a".repeat(max), "a".repeat(max + 1), "emoji 🧑", "bad\u0000", 123]
    : [-2, f.min ?? 0, 0, max, max + 1, 0.5, "1", 8, 15])
    add({ ...base, staff: [{ slot: 0, values: { [f.key]: value } }] });
}
for (const slot of [-1, 0, 31, 32, 0.5, "0"]) {
  add({ ...base, staff: [{ slot, values: { faceId: 10 } }] });
  add({
    ...base,
    staff: [
      { slot, values: {} },
      { slot, values: {} },
    ],
  });
}
add({ ...base, staff: [{ slot: 0, values: { staffId: 12 } }] });
add({
  ...base,
  staff: [{ slot: 0, values: {}, recordIndex: 123, uniqueId: 12 }],
});
for (const arenaFile of [
  "arena_test",
  "a test-1",
  "../bad",
  "C:\\bad",
  "",
  null,
  12,
  "a".repeat(38),
])
  add({
    ...base,
    arena: { name: "Arena", arenaFile, floorFile: "floor_test" },
  });
add({
  ...base,
  arena: { name: "Arena", arenaFile: "a", floorFile: "f".repeat(40) },
});
const rotation = Array.from({ length: 48 }, () => [
  "Player 1",
  "Player 2",
  "Player 3",
  "Player 4",
  "Player 5",
]);
add({ ...base, rotations: rotation });
add({ ...base, rotations: [] });
add({ ...base, rotations: rotation.slice(1) });
for (const row of [
  ["same", "same", "c", "d", "e"],
  ["a", "b", "c", "d"],
  [0, "b", "c", "d", "e"],
  ["a".repeat(40), "b", "c", "d", "e"],
])
  add({ ...base, rotations: [row, ...rotation.slice(1)] });
const match = { typeId: 0, sideId: 0, designationId: 0 };
const rules = [
  ...uniforms.PACKED_FIELDS.map((f) => [f.key, f.values.length - 1]),
  ...uniforms.COLOR_FIELDS.map((f) => [f.key, 0xffffffff]),
  ["home", 7],
  ["away", 7],
  ["logo", 3],
];
for (const [key, max] of rules)
  for (const value of [-1, 0, max, max + 1, 0.5, "1", null])
    add({ ...base, uniforms: [{ match, values: { [key]: value } }] });
for (const bad of [
  {},
  { typeId: 0, sideId: 0 },
  { ...match, home: 1 },
  { ...match, typeId: 21 },
])
  add({ ...base, uniforms: [{ match: bad, values: {} }] });
add({ ...base, uniforms: [{ match, values: { unknown: 1 } }] });
add({
  ...base,
  uniforms: Array.from({ length: 129 }, () => ({ match, values: {} })),
});
const file = path.join(root, "assets/team-dna-regressions.json"),
  bytes = JSON.stringify(cases);
if (!fs.existsSync(file) || fs.readFileSync(file, "utf8") !== bytes)
  fs.writeFileSync(file, bytes);
console.log(
  JSON.stringify({
    teamDNAOracleCases: cases.length,
    staffControls: controls.length,
    uniformControls: rules.length,
  }),
);
