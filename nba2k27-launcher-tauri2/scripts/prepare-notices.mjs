// License collection for the portable binary; this build script is not shipped as a runtime.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const registry = path.resolve(
  root,
  "../.toolchains/rust-exporter/cargo/registry/src/index.crates.io-1949cf8c6b5b557f",
);
const lock = fs.readFileSync(path.join(root, "Cargo.lock"), "utf8");
let text =
  "2k Launcher Rust / Tauri 2 Prototype — Third-party notices\n\nThis conservative list includes cached runtime and build-time dependencies from Cargo.lock. Build tooling is not shipped. Shared Windows/WebView2 components are not redistributed.\n";
let packages = 0;
function collect(dir, title) {
  if (!fs.existsSync(dir)) return;
  const cargo = path.join(dir, "Cargo.toml");
  const source = fs.existsSync(cargo) ? fs.readFileSync(cargo, "utf8") : "";
  text += `\n\n===== ${title} =====\n${source.match(/^license = (.+)$/m)?.[0] || ""}\n`;
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (
      entry.isFile() &&
      /^(license|licence|notice|copying|copyright)([.-]|$)/i.test(entry.name)
    ) {
      text += `\n--- ${entry.name} ---\n${fs.readFileSync(path.join(dir, entry.name), "utf8")}\n`;
    }
  }
  packages++;
}
for (const block of lock.split("[[package]]").slice(1)) {
  const name = block.match(/^name = "(.+)"/m)?.[1],
    version = block.match(/^version = "(.+)"/m)?.[1];
  if (name && version)
    collect(path.join(registry, `${name}-${version}`), `${name} ${version}`);
}
for (const name of ["react", "react-dom", "@tauri-apps/api"])
  collect(fs.realpathSync(path.join(root, "node_modules", name)), name);
text += `\n\n===== Three.js (unchanged original browser renderer vendor assets) =====\nMIT License\nCopyright 2010-2026 Three.js Authors\n\nPermission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:\n\nThe above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.\n\nTHE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.\n`;
fs.writeFileSync(path.join(root, "THIRD-PARTY-NOTICES.txt"), text);
console.log(
  JSON.stringify({ licensePackages: packages, bytes: Buffer.byteLength(text) }),
);
