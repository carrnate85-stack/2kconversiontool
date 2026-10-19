// Build-time import of the exact verified WPF CLOF layouts. No game access.
import fs from "node:fs";
import path from "node:path";
import zlib from "node:zlib";
import crypto from "node:crypto";
import { fileURLToPath } from "node:url";
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const source = fs.readFileSync(path.join(root, "../nba2k27-mod-launcher/src/data/blacktop-preset-layouts.json"));
const groups = JSON.parse(source);
if (!Array.isArray(groups) || groups.length !== 1055 || groups.reduce((n,g)=>n+g.records.length,0) !== 1791) throw Error("Reference outfit layouts changed; review before importing");
for (const g of groups) {
  if (!Number.isInteger(g.length) || g.length < 32 || g.length > 2*1024*1024 || !Number.isInteger(g.count) || g.count < 1 || g.count > 10000 || !Number.isInteger(g.outfitCount) || g.outfitCount < 1 || g.outfitCount > 64 || BigInt(g.hash) < 0n || BigInt(g.hash) > 0xffffffffffffffffn || g.descriptor < 0 || g.descriptor+16 > g.length || !g.records.length || g.records.length > g.outfitCount) throw Error("Unbounded reference outfit group");
  for (const r of g.records) {
    if (r.names.length !== 2 || r.strings.length !== 2 || r.record < 0 || r.record+24 > g.length || r.array < 0 || r.array+16 > g.length || BigInt(r.marker) < 0n || BigInt(r.marker) > 0xffffffffffffffffn) throw Error("Invalid reference outfit record");
    r.names.forEach((name,i)=>{ if (typeof name !== "string" || !name.length || name.length > 255 || name.includes("\0") || r.strings[i] < 0 || r.strings[i]+(name.length+1)*2 > g.length) throw Error("Invalid reference outfit string"); });
  }
}
const data = {schema:"nba2k27-native-outfit-layouts-v1",sourceSha256:crypto.createHash("sha256").update(source).digest("hex"),groups};
fs.writeFileSync(path.join(root, "assets/outfit-layouts.json.gz"), zlib.gzipSync(JSON.stringify(data), {level:9}));
console.log(JSON.stringify({groups:groups.length,records:1791,sourceSha256:data.sourceSha256,gameAccess:false}));
