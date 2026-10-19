import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
const require = createRequire(import.meta.url);
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const source = path.resolve(root, "../nba2k27-mod-launcher/src");
const original = require(path.join(source, "core/player23-catalog.js"));
const catalog = JSON.parse(
  fs.readFileSync(
    path.join(source, "assets/player23-catalog/players.2k23catalog"),
    "utf8",
  ),
);
const index = original.indexed(catalog),
  cases = [];
for (const query of [
  "",
  "curry",
  "Stephen Curry",
  "Curry Stephen",
  "warriors curry",
  "CURRENT",
  "historic",
  "all_time",
  "76ers",
  "LeBron James",
  "Müller",
  "Nikola Jokić",
  "Doncic",
  "Dončić",
  "\u00a0CURRY\u00a0",
  "\ufeffcurry\ufeff",
  "Ｓｔｅｐｈｅｎ",
  "james lakers",
  "nothing-qa",
]) {
  for (const category of [undefined, "current", "historic"]) {
    const request = {
      query,
      offset: 0,
      limit: 40,
      ...(category ? { category } : {}),
    };
    cases.push({ request, expected: original.search(index, request) });
  }
}
const players = catalog.players.filter((p) => p.history.length).slice(0, 3);
for (const name of ["Stephen Curry", "LeBron James", "Nikola Jokić"]) {
  const p = index.players.find((p) => p.name === name);
  if (p && !players.some((v) => v.id === p.id)) players.push(p);
}
const reviews = players.map((p) => ({
  id: p.id,
  expected: original.playerHistory(index, p.id),
}));
fs.writeFileSync(
  path.join(root, "assets/browser-catalog-regressions.json"),
  JSON.stringify({ cases, reviews }),
);
console.log(
  JSON.stringify({
    originalBrowserSearchCases: cases.length,
    originalBrowserPlayerReviews: reviews.length,
    gameAccess: false,
  }),
);
