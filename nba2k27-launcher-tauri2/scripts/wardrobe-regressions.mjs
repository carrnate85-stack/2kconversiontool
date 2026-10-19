// Authoritative selectable WPF choices, independent of renderer picture coverage.
import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
const require = createRequire(import.meta.url),
  root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), ".."),
  source = path.resolve(root, "../nba2k27-mod-launcher/src");
const wear = require(path.join(source, "data/streetwear-components.json")),
  { accessoryChoices } = require(
    path.join(source, "core/clothing-components.js"),
  );
const accessories = accessoryChoices(wear)
  .filter((a) => a.slot)
  .map((a) => ({
    asset: a.asset,
    slot: a.slot,
    pool: a.asset.startsWith("female_") ? "female" : "male",
  }));
const shoes = new Map();
for (const preset of wear.presets)
  for (const asset of preset.shoes || [])
    if (["male", "female"].includes(preset.sourceGroup.pool)) {
      if (!shoes.has(asset)) shoes.set(asset, new Set());
      shoes.get(asset).add(preset.sourceGroup.pool);
    }
for (const shoe of wear.additionalShoes || []) {
  if (!shoes.has(shoe.asset)) shoes.set(shoe.asset, new Set());
  for (const pool of shoe.pools) shoes.get(shoe.asset).add(pool);
}
const fixture = {
    accessories,
    shoes: [...shoes].map(([asset, pools]) => ({
      asset,
      pools: [...pools].sort(),
    })),
  },
  file = path.join(root, "assets/wardrobe-regressions.json"),
  bytes = JSON.stringify(fixture);
if (!fs.existsSync(file) || fs.readFileSync(file, "utf8") !== bytes)
  fs.writeFileSync(file, bytes);
console.log(
  JSON.stringify({
    accessoryChoices: accessories.length,
    shoeChoices: shoes.size,
    slotFamilies: new Set(accessories.map((a) => a.slot)).size,
  }),
);
