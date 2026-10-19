// Build-time cross-check only. Uses the unchanged WPF validators; not shipped.
import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
const require = createRequire(import.meta.url);
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const baseline = path.resolve(root, "../nba2k27-mod-launcher/src/core");
const outfit = require(path.join(baseline, "outfit-share.js"));
const dna = require(path.join(baseline, "player-dna.js"));
const team = require(path.join(baseline, "team-dna.js"));
const profiles = process.argv.slice(2);
if (!profiles.length) throw Error("Supply an isolated successful QA profile.");
let outfitFiles = 0,
  playerFiles = 0,
  teamFiles = 0;
for (const profile of profiles) {
  const report = JSON.parse(
    fs.readFileSync(path.join(profile, "Logs/ui-smoke.json"), "utf8"),
  );
  if (!report.result?.pass) throw Error(`QA profile did not pass: ${profile}`);
  for (const [folder, extension, validate] of [
    ["Outfits", ".2koutfit", (text) => outfit.decodeOutfit(text)],
    ["Players", ".2kplayer", (text) => dna.parse(text)],
    ["Teams", ".2kteam", (text) => team.parse(text)],
  ]) {
    const directory = path.join(profile, "Library", folder);
    const files = fs
      .readdirSync(directory)
      .filter((file) => file.endsWith(extension));
    if (!files.length) throw Error(`No exported ${extension} file: ${profile}`);
    for (const file of files) {
      validate(fs.readFileSync(path.join(directory, file), "utf8"));
      if (extension === ".2koutfit") outfitFiles++;
      else if (extension === ".2kteam") teamFiles++;
      else playerFiles++;
    }
  }
}
console.log(
  JSON.stringify({
    profiles: profiles.length,
    outfitFiles,
    playerFiles,
    teamFiles,
    wpfValidatorsAccepted: true,
  }),
);
