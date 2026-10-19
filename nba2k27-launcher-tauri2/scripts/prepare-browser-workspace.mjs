// Build-time reuse of the original WPF browser workspace. No Node is shipped.
import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import zlib from "node:zlib";
import { fileURLToPath } from "node:url";
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const source = path.resolve(root, "../nba2k27-mod-launcher/src");
const assets = [];
function add(relative) {
  const local = path.join(root, "browser-overrides", path.basename(relative));
  const override = relative.startsWith("renderer/") && fs.existsSync(local);
  const bytes = fs.readFileSync(override ? local : path.join(source, relative));
  const mime = relative.endsWith(".html")
    ? "text/html; charset=utf-8"
    : relative.endsWith(".js")
      ? "text/javascript; charset=utf-8"
      : relative.endsWith(".css")
        ? "text/css; charset=utf-8"
        : "image/png";
  assets.push({
    path: "/" + relative,
    mime,
    bytes: Array.from(bytes),
    sha256: crypto.createHash("sha256").update(bytes).digest("hex"),
  });
}
for (const name of fs.readdirSync(path.join(source, "renderer")).sort())
  if (/^[\w][\w.-]*\.(?:html|css|js)$/.test(name)) add("renderer/" + name);
for (const name of fs.readdirSync(path.join(source, "shared")).sort())
  if (/^[\w][\w.-]*\.js$/.test(name)) add("shared/" + name);
for (const directory of [
  "renderer/animation-preview",
  "renderer/animation-preview/vendor",
]) {
  for (const name of fs.readdirSync(path.join(source, directory)).sort())
    if (/^[\w][\w.-]*\.js$/.test(name)) add(directory + "/" + name);
}
for (const name of ["season-import-staging.js", "season-import-adapter.js", "editor-destination-capabilities.js", "team-dna-v2.js", "dna-season-selection.js", "remote-ratings-v2.js", "saved-ratings-ui.js", "team-metadata-v2.js", "player-origin-v2.js", "live-ratings-ui.js", "manual-artwork-v2.js", "custom-arena-v2.js"]) {
  const bytes=fs.readFileSync(path.join(root,"browser-overrides",name));
  assets.push({path:"/renderer/"+name,mime:"text/javascript; charset=utf-8",bytes:Array.from(bytes),sha256:crypto.createHash("sha256").update(bytes).digest("hex")});
}
add("assets/app-icon.png");
const lookup = new Map(assets.map((a) => [a.path, a]));
const page=lookup.get("/renderer/index.html");
const index = Buffer.from(page.bytes).toString()
  .replace('<script src="player-dna.js">', '<script src="editor-destination-capabilities.js"></script>\n<script src="dna-season-selection.js"></script>\n<script src="player-dna.js">')
  .replace("</body>",'<script src="season-import-staging.js"></script>\n<script src="season-import-adapter.js"></script>\n<script src="team-dna-v2.js"></script>\n<script src="team-metadata-v2.js"></script>\n<script src="player-origin-v2.js"></script>\n<script src="remote-ratings-v2.js"></script>\n<script src="saved-ratings-ui.js"></script>\n<script src="live-ratings-ui.js"></script>\n<script src="manual-artwork-v2.js"></script>\n<script src="custom-arena-v2.js"></script>\n</body>');
page.bytes=Array.from(Buffer.from(index));page.sha256=crypto.createHash("sha256").update(index).digest("hex");
if (!index.includes('<script src="theme.js">'))
  throw Error("WPF browser bridge insertion point changed");
// Verify every static script and stylesheet reference before packaging.
for (const match of index.matchAll(/(?:src|href)="([^"]+\.(?:css|js))"/g)) {
  const resolved = path.posix.normalize("/renderer/" + match[1]);
  if (!lookup.has(resolved))
    throw Error("Missing WPF browser asset: " + resolved);
}
const bundle = zlib.gzipSync(Buffer.from(JSON.stringify(assets)), { level: 9 });
fs.writeFileSync(path.join(root, "assets/wpf-browser.json.gz"), bundle);
fs.writeFileSync(
  path.join(root, "assets/wpf-browser-manifest.json"),
  JSON.stringify(
    {
      source: "Original WPF browser renderer with project-local editor parity overrides; WPF source files unmodified",
      files: assets.map(({ path, mime, sha256, bytes }) => ({
        path,
        mime,
        sha256,
        bytes: bytes.length,
      })),
      bytes: bundle.length,
    },
    null,
    2,
  ) + "\n",
);
console.log(
  `Original WPF browser workspace: ${assets.length} assets with local season staging overrides, ${bundle.length} compressed bytes`,
);
