// Exact verified WPF mapping oracle; do not modify the original source.
import fs from "node:fs";
import assert from "node:assert/strict";
const original = fs.readFileSync(
  new URL(
    "../../nba2k27-mod-launcher/unlocks-helper/GameRuleData.cs",
    import.meta.url,
  ),
  "utf8",
);
const native = fs.readFileSync(
  new URL("../src/game_rules.rs", import.meta.url),
  "utf8",
);
const baseline = [
  ...original.matchAll(
    /new RuleField\("([^"]+)","([^"]+)",(0x[\dA-Fa-f]+),(\d)\)/g,
  ),
].map((m) => ({
  label: m[1],
  id: m[2],
  offset: Number(m[3]),
  width: Number(m[4]),
}));
const rules = [
  ...native.matchAll(
    /\(\s*"([^"]+)"\s*,\s*"([^"]+)"\s*,\s*(0x[\dA-Fa-f]+)\s*,\s*(\d)\s*,\s*(\d)\s*\)/g,
  ),
].map((m) => ({
  id: m[1],
  label: m[2],
  offset: Number(m[3]),
  width: Number(m[4]),
  max: Number(m[5]),
}));
assert.equal(baseline.length, 39);
assert.equal(rules.length, 39);
for (let n = 0; n < 39; n++) {
  const { max, ...mapped } = rules[n];
  assert.deepEqual(mapped, baseline[n]);
  assert.equal(max, mapped.width === 1 ? 1 : mapped.id === "FoulOut" ? 6 : 2);
}
console.log(
  JSON.stringify({
    verifiedWpfRuleMappings: 39,
    offsetsWidthsLabelsAndRanges: "exact",
  }),
);
