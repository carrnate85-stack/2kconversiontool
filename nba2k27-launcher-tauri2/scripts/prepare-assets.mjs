// Build-time only. No Node runtime is distributed with the prototype.
import fs from "node:fs";
import path from "node:path";
import zlib from "node:zlib";
import crypto from "node:crypto";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";
const require = createRequire(import.meta.url);
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const source = path.resolve(root, "../nba2k27-mod-launcher/src");
const read = (p) => JSON.parse(fs.readFileSync(path.join(source, p), "utf8"));
const writeAsset = (file, data) => {
  const bytes = Buffer.isBuffer(data) ? data : Buffer.from(data);
  if (!fs.existsSync(file) || !fs.readFileSync(file).equals(bytes))
    fs.writeFileSync(file, bytes);
};
const copyAsset = (from, to) => writeAsset(to, fs.readFileSync(from));
fs.mkdirSync(path.join(root, "assets"), { recursive: true });
const previews = path.join(root, "frontend/public/previews");
fs.mkdirSync(previews, { recursive: true });
copyAsset(
  path.join(source, "assets/app-icon.ico"),
  path.join(root, "app-icon.ico"),
);
copyAsset(
  path.join(source, "assets/app-icon.png"),
  path.join(root, "frontend/public/brand.png"),
);
const court = path.join(source, "assets/blacktop-night-court-v1.png");
if (fs.existsSync(court))
  copyAsset(court, path.join(root, "frontend/public/court.png"));
const catalog = fs.readFileSync(
  path.join(source, "assets/player23-catalog/players.2k23catalog"),
);
const schema = read("data/player-field-schema.json");
const dna = require(path.join(source, "core/player-dna.js"));
const { maximum } = require(path.join(source, "core/player-field-edits.js"));
const math = require(path.join(source, "renderer/player-season-math.js"));
const awards = require(path.join(source, "renderer/player-season-awards.js"));
const teamDna = require(path.join(source, "core/team-dna.js"));
const uniformSchema = require(path.join(source, "renderer/team-uniforms.js"));
writeAsset(
  path.join(root, "assets/team-schema.json"),
  JSON.stringify({
    sections: teamDna.sections,
    roles: read("data/staff-field-schema.json").roles,
    staff: read("data/staff-controls.json").map((f) => ({
      key: f.key,
      label: f.label,
      group: f.group,
      text: !!f.text,
      minimum: f.min ?? 0,
      maximum: f.text ? f.bytes / 2 - 1 : (f.max ?? 2 ** f.bits - 1),
      choices: f.choices ?? [],
    })),
    uniforms: [
      ...uniformSchema.PACKED_FIELDS.map((f) => ({
        key: f.key,
        label: f.label,
        group: f.group,
        maximum: f.values.length - 1,
        choices: f.values,
      })),
      ...uniformSchema.COLOR_FIELDS.map((f) => ({
        ...f,
        maximum: 0xffffffff,
        color: true,
        choices: [],
      })),
      ...["home", "away", "logo"].map((key) => ({
        key,
        label: `Headband ${key}`,
        group: "Headband routing",
        maximum: key === "logo" ? 3 : 7,
        choices:
          key === "logo"
            ? uniformSchema.HEADBAND_LOGOS
            : uniformSchema.HEADBAND_COLORS,
      })),
    ],
  }),
);
const oldSockChoices = [
  "White",
  "Black",
  "Uniform 1",
  "Uniform 2",
  "Uniform 3",
  "Uniform Armsleeve",
  "Uniform Legsleeve",
  "Uniform Primary Color",
  "Navy",
  "Blue",
  "Green",
  "Yellow",
  "Red",
  "Gray",
  "Purple",
  "Orange",
  "Tan",
];
const prior = [schema, schema.filter((f) => f.group !== "playtypes")].flatMap(
  (fields) => [
    fields,
    fields.map((f) =>
      /^SockItems(?:Home|Away)Color$/.test(f.key)
        ? { ...f, choices: oldSockChoices }
        : f,
    ),
  ],
);
const compatible = [
  ...new Set([
    ...prior
      .flatMap((fields) => [
        fields,
        fields.filter((f) => !["BestHand", "BestDunkHand"].includes(f.key)),
      ])
      .map((fields) =>
        crypto
          .createHash("sha256")
          .update(JSON.stringify(fields))
          .digest("hex"),
      ),
    ...read("data/player-schema-legacy-ids.json"),
  ]),
];
writeAsset(
  path.join(root, "assets/player-schema.json"),
  JSON.stringify({
    schemaId: dna.schemaId,
    compatible,
    groups: dna.groups,
    fields: schema.map((f) => ({
      key: f.key,
      label: f.label ?? f.key,
      group: f.group,
      maximum: maximum(f),
      money: !!f.money,
      choices: f.group === "hotzones" ? ["Cold", "Neutral", "Hot"] :
        f.choices ?? (f.group === "badges" ? ["None", "Bronze", "Silver", "Gold", "Hall of Fame", "Legend"] :
          f.bits === 1 ? ["No", "Yes"] : []),
    })),
    seasonLimits: math.limits,
    awards: awards.fields.map((f) => ({
      key: f.key,
      label: f.label,
      maximum: f.max ?? 1,
      choices:
        f.choices ??
        (f.max
          ? [
              "None",
              ...Array.from({ length: f.max }, (_, i) => `Team ${i + 1}`),
            ]
          : ["No", "Yes"]),
    })),
  }),
);
writeAsset(
  path.join(root, "assets/players.json.gz"),
  zlib.gzipSync(catalog, { level: 9 }),
);
const renderRoot = path.resolve(root, "../blacktop-garment-renderer");
const rendererFolders = {
  "blacktop-top-previews": "top-picker-previews",
  "blacktop-bottom-previews": "bottom-picker-previews",
  "blacktop-shoe-previews": "shoe-picker-previews",
  "blacktop-accessory-previews": "accessory-picker-previews",
  "blacktop-sock-previews": "sock-picker-previews",
};
function image(folder, item) {
  let sourceImage = path.join(source, "assets", folder, item.thumbnail);
  if (!fs.existsSync(sourceImage) && rendererFolders[folder])
    sourceImage = path.join(
      renderRoot,
      rendererFolders[folder],
      item.thumbnail,
    );
  if (!fs.existsSync(sourceImage)) return "";
  const target = `${folder}-${path.basename(item.thumbnail)}`;
  copyAsset(sourceImage, path.join(previews, target));
  return `./previews/${target}`;
}
const photoMap = new Map();
const slotMap = new Map();
const keyMap = new Map();
const poolMap = new Map();
const groupMap = new Map();
for (const folder of [
  "blacktop-top-previews",
  "blacktop-bottom-previews",
  "blacktop-shoe-previews",
  "blacktop-accessory-previews",
  "blacktop-sock-previews",
]) {
  const local = path.join(source, `assets/${folder}/catalog.json`);
  const c = fs.existsSync(local)
    ? read(`assets/${folder}/catalog.json`)
    : JSON.parse(
        fs.readFileSync(
          path.join(renderRoot, rendererFolders[folder], "catalog.json"),
          "utf8",
        ),
      );
  for (const item of c.items) {
    keyMap.set(item.id, `${folder}/${item.thumbnail}`);
    poolMap.set(item.id, item.pool);
    if (item.category) groupMap.set(item.id, item.category);
    if (folder.includes("sock")) slotMap.set(item.id, "socks");
  }
  // Small representative 256 px subset for comparison. Full optional preview packs aren't redistributed here.
  if (folder.includes("accessory")) {
    const counts = new Map();
    for (const item of c.items) {
      slotMap.set(item.id, item.slot);
      const count = counts.get(item.slot) || 0;
      if (count < 4) {
        photoMap.set(item.id, image(folder, item));
        counts.set(item.slot, count + 1);
      }
    }
  } else
    for (const item of c.items.slice(0, 18))
      photoMap.set(item.id, image(folder, item));
}
const wear = read("data/streetwear-components.json");
const clothing = require(path.join(source, "core/clothing-components.js"));
const slots = require(path.join(source, "shared/accessory-slots.js"));
// Exact observed variants and composition metadata, without source addresses or hashes.
const outfitCatalog = {
  presets: wear.presets.map((p) => ({
    id: p.id,
    pool: p.sourceGroup.pool,
    original: p.original,
    top: p.top,
    bottom: p.bottom,
    layers: p.layers,
    shoes: p.shoes,
    accessories: p.accessories,
    separable: p.separable,
    manualOnly: !!p.manualOnly,
  })),
  additionalShoes: (wear.additionalShoes || []).map((s) => ({
    asset: s.asset,
    pools: s.pools,
  })),
  accessories: clothing
    .accessoryChoices(wear)
    .map((a) => ({ asset: a.asset, slot: a.slot })),
  slots: slots.slots.map(([id]) => id),
};
writeAsset(
  path.join(root, "assets/outfits.json.gz"),
  zlib.gzipSync(JSON.stringify(outfitCatalog), { level: 9 }),
);
const shoePools = new Map();
for (const preset of wear.presets)
  for (const asset of preset.shoes || []) {
    if (!["male", "female"].includes(preset.sourceGroup.pool)) continue;
    if (!shoePools.has(asset)) shoePools.set(asset, new Set());
    shoePools.get(asset).add(preset.sourceGroup.pool);
  }
for (const shoe of wear.additionalShoes || []) {
  if (!shoePools.has(shoe.asset)) shoePools.set(shoe.asset, new Set());
  for (const pool of shoe.pools) shoePools.get(shoe.asset).add(pool);
}
const pieces = wear.pieces.map((p) => ({
  asset: p.asset,
  category:
    p.category === "accessories"
      ? slotMap.get(p.asset) || slots.slot(p.asset) || "other"
      : p.category,
  image: photoMap.get(p.asset) || "",
  previewKey: keyMap.get(p.asset) || "",
  group: groupMap.get(p.asset) || "Other",
  pool:
    poolMap.get(p.asset) || (p.asset.startsWith("female_") ? "female" : "male"),
  ...(p.category === "shoes"
    ? { pools: [...(shoePools.get(p.asset) || [])] }
    : {}),
}));
// Renderer preview coverage is not the selectable wardrobe. The WPF catalog's
// additional accessories include hats, headphones, hands, wrists and jewelry.
const pieceIds = new Set(pieces.map((p) => p.asset));
for (const accessory of clothing.accessoryChoices(wear)) {
  if (!accessory.slot || pieceIds.has(accessory.asset)) continue;
  pieceIds.add(accessory.asset);
  pieces.push({
    asset: accessory.asset,
    category: accessory.slot,
    image: photoMap.get(accessory.asset) || "",
    previewKey: keyMap.get(accessory.asset) || "",
    group: groupMap.get(accessory.asset) || accessory.family || "Other",
    pool: accessory.asset.startsWith("female_") ? "female" : "male",
  });
}
pieces.sort(
  (a, b) =>
    Number(Boolean(b.image)) - Number(Boolean(a.image)) ||
    a.asset.localeCompare(b.asset),
);
for (const shoe of wear.additionalShoes || []) {
  if (shoe.asset && !pieces.some((p) => p.asset === shoe.asset))
    pieces.push({
      asset: shoe.asset,
      category: "shoes",
      image: photoMap.get(shoe.asset) || "",
      previewKey: keyMap.get(shoe.asset) || "",
      pool: poolMap.get(shoe.asset) || "male",
      pools: [...(shoePools.get(shoe.asset) || [])],
      group: groupMap.get(shoe.asset) || "Other",
    });
}
writeAsset(
  path.join(root, "assets/wardrobe.json.gz"),
  zlib.gzipSync(JSON.stringify(pieces), { level: 9 }),
);
const exclusions = new Set(read("data/ball-live-exclusions.json").ids);
// WPF core/ball-selection-manager.js describeBall/title: labels and groups
// come from the logical path; rows sort by group, label, id.
const ballTitle = (value) =>
  String(value || "")
    .split("_")
    .filter(Boolean)
    .map((word) =>
      word.length < 4 && /^nba|wnba$/i.test(word)
        ? word.toUpperCase()
        : word[0].toUpperCase() + word.slice(1),
    )
    .join(" ");
const ballGroups = {
  allstar: "All-Star",
  animal: "Animals",
  camo: "Camo",
  cloth: "Materials",
  country: "Countries",
  emoji: "Emoji",
  event: "Events",
  gem: "Gems",
  holiday: "Holiday",
  nba: "League",
  wnba: "League",
  pattern: "Patterns",
  season: "Seasons",
  solid: "Colors",
  team: "Teams",
};
function describeBall(logicalPath) {
  const stem = path.posix
    .basename(logicalPath, ".iff")
    .replace(/^ball_(?:gameplay_)?(?:official_)?/i, "");
  const parts = stem.split("_").filter(Boolean);
  if (logicalPath === "balls/ball_gameplay_official_nba_nba_official.iff")
    return { label: "NBA Official", group: "League" };
  if (logicalPath === "balls/ball_gameplay_official_default.iff")
    return { label: "2K Default", group: "League" };
  if (/^wnba_/.test(stem))
    return {
      label: ballTitle(parts.slice(1).join("_") || "WNBA Official"),
      group: "League",
    };
  let group = "Other";
  const key = parts[0]?.toLowerCase();
  if (ballGroups[key]) {
    group = ballGroups[key];
    parts.shift();
  }
  return { label: ballTitle(parts.join("_") || stem), group };
}
const balls = read("assets/blacktop-ball-previews/catalog.json")
  .items.filter((b) => !exclusions.has(b.id))
  .map((b) => ({ ...b, ...describeBall(b.id) }))
  .sort(
    (a, b) =>
      a.group.localeCompare(b.group) ||
      a.label.localeCompare(b.label) ||
      a.id.localeCompare(b.id),
  )
  .map((b, i) => ({
    id: b.id,
    name: b.label,
    group: b.group,
    image: i < 24 ? image("blacktop-ball-previews", b) : "",
    previewKey: `blacktop-ball-previews/${b.thumbnail}`,
    // WPF ball-picker.js tile tooltip.
    ...(b.renderNote ? { note: b.renderNote } : {}),
  }));
balls.unshift({
  id: "current",
  name: "Currently installed",
  group: "Installed",
  image: "",
  previewKey: "",
});
const sceneSource = fs
  .readFileSync(
    path.join(source, "renderer/scoreboard-preview-catalog.js"),
    "utf8",
  )
  .split("const needsReview")[0];
const sceneMap = Object.fromEntries(
  [...sceneSource.matchAll(/^\s*['"]?([\w-]+)['"]?:\s*'([^']+)'/gm)].map(
    (m) => [m[1], m[2]],
  ),
);
sceneMap["era-1990"] = "1990s_scorebug_alternate";
sceneMap["mycourt-scrimmage"] = sceneMap["w-scrimmage"] =
  "mycourt_scorebug_scrimmage_composite";
// WPF scoreboard-preview-catalog.js captions (third tile line).
const sceneCaptions = require(
  path.join(source, "renderer/scoreboard-preview-catalog.js"),
);
const scoreboards = read("data/scoreboards.json")
  .choices.filter(
    (c) => !["mycareer-story", "flashback-openrun"].includes(c.id),
  )
  .map((c) => {
    const stem = sceneMap[c.id];
    const file = path.join(
      source,
      `renderer/assets/scoreboard-previews/${stem}-asset.png`,
    );
    let image = "";
    if (stem && fs.existsSync(file)) {
      const out = `scoreboard-${stem}.png`;
      copyAsset(file, path.join(previews, out));
      image = `./previews/${out}`;
    }
    return {
      id: c.id,
      name: c.label,
      // WPF scoreboard-picker.js categories.
      group:
        c.tested === false
          ? "Experimental"
          : c.id?.startsWith("era-")
            ? "Eras"
            : c.nativeController === "SCOREBUG_MAIN_GAME"
              ? "Broadcast"
              : "Classic",
      image,
      caption: sceneCaptions[c.id]?.caption ?? "",
      tested: c.tested === true,
    };
  });
const arenaBrowser = require(path.join(source, "shared/arena-browser.js"));
// Native selection uses exact IDs and the original arena/floor/template data,
// not the display-only catalog labels below. Keep disabled entries for rejection.
writeAsset(
  path.join(root, "assets/arena-live.json"),
  JSON.stringify(read("data/blacktop-arenas.json")),
);
// WPF shared/arena-browser.js: the group is derived from the id (the JSON
// group is ignored), rows are sorted default-first then by label, and
// disabled scenes stay listed ("Needs work") but cannot be chosen.
const arenas = arenaBrowser
  .filter(read("data/blacktop-arenas.json").choices)
  .map((c) => ({
    id: c.id,
    name: c.label,
    group: arenaBrowser.group(c),
    image: "",
    tested: c.tested === true,
    file: c.file || "",
    floorFile: c.floorFile || "",
    caution: c.caution || "",
    ...(c.disabled ? { disabled: true } : {}),
  }));
const previewRules = fs
  .readFileSync(
    path.join(source, "renderer/blacktop-overlay-preview-frame.js"),
    "utf8",
  )
  .split("const message")[0];
const numeric = {
  "3Pt Shots": ["Enabled", "Enabled after steal or block", "Disabled"],
  "3 in the Key": ["Off", "3 seconds", "5 seconds"],
  "Def 3 in the Key": ["Off", "3 seconds", "5 seconds"],
  "Foul Out": ["Off", "1", "2", "3", "4", "5", "6"],
};
const rules = [...previewRules.matchAll(/\['([^']+)',([14])\]/g)].map((m) => ({
  name: m[1],
  options: numeric[m[1]] || ["Off", "On"],
}));
if (rules.length !== 39)
  throw Error(
    "The baseline Game Rules inventory changed; inspect before packaging.",
  );
const samples = {
  balls,
  scoreboards,
  arenas,
  rules,
  cameras: [
    "Broadcast Low 2K18",
    "Press 2K17",
    "Press 2K27",
    "Beluba 2K17",
    "Beluba 2K27",
    "Side",
    "Nosebleeds",
  ],
};
writeAsset(
  path.join(root, "frontend/samples.json"),
  JSON.stringify(samples, null, 2),
);
writeAsset(
  path.join(root, "assets/choices.json.gz"),
  zlib.gzipSync(
    JSON.stringify({ ball: balls, scoreboard: scoreboards, arena: arenas }),
    { level: 9 },
  ),
);
console.log(
  JSON.stringify({
    players: JSON.parse(catalog).players.length,
    garments: pieces.length,
    previewImages: fs.readdirSync(previews).length,
  }),
);
