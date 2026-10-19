// Development-only oracle. No Node/.NET runtime is added to the launcher.
import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import zlib from "node:zlib";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), ".."),
  source = path.resolve(
    root,
    "../nba2k27-mod-launcher/src/core/sigconfig-table.js",
  ),
  { buildTableWithClones, replaceConfigLists, relativePointer } = createRequire(
    import.meta.url,
  )(source);
function fixture(seed) {
  const counts = [1, 2 + (seed % 6), 2],
    end = 16 + counts.length * 16;
  const bytes = Buffer.alloc(end + counts.reduce((a, b) => a + b * 32, 0));
  bytes.writeUInt32LE(counts.length, 0);
  bytes.writeUInt32LE(9, 8);
  let cursor = end;
  counts.forEach((count, i) => {
    const offset = 16 + i * 16,
      payload = cursor + count * 16;
    bytes.writeUInt32LE([21, 4090, 9000][i], offset);
    bytes.writeUInt32LE(count, offset + 4);
    bytes.writeBigUInt64LE(relativePointer(offset + 8, cursor), offset + 8);
    for (let j = 0; j < count; j++) {
      const item = cursor + j * 16,
        block = payload + j * 16,
        intra = seed % 8;
      bytes.writeBigUInt64LE(relativePointer(item, block + intra), item);
      bytes.writeBigUInt64LE(BigInt(seed * 1000 + j), item + 8);
      for (let k = 0; k < 16; k++)
        bytes[block + k] = (seed * 29 + j * 19 + k * 7) % 256;
    }
    cursor = payload + count * 16;
  });
  return bytes;
}
const cases = [];
for (let i = 0; i < 20; i++) {
  const data = fixture(i);
  for (const mode of ["clones", "replacements", "both"]) {
    const clones =
      mode === "replacements"
        ? []
        : [
            { targetId: 10001, donorId: 4090 },
            { targetId: 5, donorId: 4090 },
          ];
    const replacements =
      mode === "clones"
        ? []
        : [
            { id: 21, names: ["afro", `HEADBAND_FURY_${i}`] },
            ...(mode === "both"
              ? [{ id: 10001, names: ["CUSTOM_1", "custom-2", "headband"] }]
              : []),
          ];
    const expected = replaceConfigLists(
      buildTableWithClones(data, clones).data,
      replacements,
    );
    cases.push({
      name: `${i}-${mode}`,
      source: [...data],
      clones,
      replacements,
      expected: [...expected],
    });
  }
}
const report = {
  schema: "nba2k27-sigconfig-oracle-v1",
  sourceSha256: crypto
    .createHash("sha256")
    .update(fs.readFileSync(source))
    .digest("hex"),
  cases,
};
fs.writeFileSync(
  path.join(root, "assets/sigconfig-regressions.json.gz"),
  zlib.gzipSync(JSON.stringify(report), { level: 9 }),
);
console.log(
  JSON.stringify({
    cases: cases.length,
    sourceSha256: report.sourceSha256,
    gameAccess: false,
    networkRequests: 0,
  }),
);
