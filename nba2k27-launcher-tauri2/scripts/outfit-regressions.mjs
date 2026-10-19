// Build-time oracle only: compare native composition against the untouched WPF core.
import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
const require = createRequire(import.meta.url);
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const source = path.resolve(root, "../nba2k27-mod-launcher/src");
const c = require(path.join(source, "data/streetwear-components.json"));
const { composeStreetwear, layerChoices, accessoryChoices } = require(
  path.join(source, "core/clothing-components.js"),
);
const cases = [];
function add(selection) {
  const s = selection;
  const result = composeStreetwear(
    c,
    s.streetId,
    s.bottom,
    s.shoe,
    s.sock,
    s.layer,
    s.accessory,
    s.accessories,
  );
  cases.push({ selection, items: result?.items ?? null });
}
const valid = c.presets.filter(
  (p) => p.bottom && ["male", "female"].includes(p.sourceGroup.pool),
);
for (const p of valid
  .filter((p, i) => i % 71 === 0 || p.layers.length || p.manualOnly)
  .slice(0, 750)) {
  const basic = { streetId: p.id, bottom: p.bottom };
  add(basic);
  add({ ...basic, shoe: "roster" });
  add({ ...basic, accessories: [] });
  for (const layer of layerChoices(c, p)) add({ ...basic, layer });
  add({ ...basic, layer: "not-observed" });
  add({ ...basic, shoe: "not-a-shoe" });
  add({ ...basic, bottom: "not-a-bottom" });
  const same = accessoryChoices(c).filter(
    (a) =>
      a.slot &&
      (a.asset.startsWith("female_") ? "female" : "male") ===
        p.sourceGroup.pool,
  );
  const a = same[0]?.asset;
  if (a) {
    add({ ...basic, accessory: a });
    add({ ...basic, accessories: [a] });
    add({ ...basic, accessories: [a, a] });
  }
  const slots = new Set();
  const three = same
    .filter((a) => {
      if (slots.has(a.slot)) return false;
      slots.add(a.slot);
      return true;
    })
    .slice(0, 3)
    .map((a) => a.asset)
    .reverse();
  add({ ...basic, shoe: "roster", accessories: three });
  const other = valid.find(
    (d) =>
      d.sourceGroup.pool === p.sourceGroup.pool &&
      d.separable &&
      d.bottom !== p.bottom,
  );
  if (other) add({ ...basic, bottom: other.bottom });
  const opposite = valid.find((d) => d.sourceGroup.pool !== p.sourceGroup.pool);
  if (opposite) add({ ...basic, bottom: opposite.bottom });
  const sock = valid
    .find(
      (d) =>
        d.sourceGroup.pool === p.sourceGroup.pool &&
        d.accessories.some((a) => /^(female_)?acc_sock_/.test(a)),
    )
    ?.accessories.find((a) => /^(female_)?acc_sock_/.test(a));
  if (sock) add({ ...basic, sock });
}
const file = path.join(root, "assets/outfit-regressions.json"),
  bytes = JSON.stringify(cases);
if (!fs.existsSync(file) || fs.readFileSync(file, "utf8") !== bytes)
  fs.writeFileSync(file, bytes);
console.log(JSON.stringify({ outfitOracleCases: cases.length }));
