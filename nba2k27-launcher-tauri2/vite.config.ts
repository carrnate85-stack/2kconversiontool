import { defineConfig } from "vitest/config";
import { readFileSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, extname, resolve, sep } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
function installedModelPreviewRoot() {
  const app = resolve(here, "Dev/Portable/App");
  const snapshot = JSON.parse(readFileSync(resolve(app, "model-preview-current.json"), "utf8").replace(/^\uFEFF/, ""));
  if (!/^[0-9a-f]{64}-[0-9a-f]{32}$/i.test(snapshot.directory)) throw new Error("Invalid installed renderer snapshot");
  return resolve(app, "model-preview", snapshot.directory);
}
const modelPreviewTypes: Record<string, string> = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".svg": "image/svg+xml",
};

/** Serve the staged Explorer renderer only from `vite serve`. `vite build`
 * never copies it until the feature is approved for release promotion. */
function devModelPreview() {
  return {
    name: "dev-model-preview",
    apply: "serve" as const,
    configureServer(server: { middlewares: { use: (route: string, handler: (request: { url?: string }, response: { statusCode: number; setHeader: (name: string, value: string) => void; end: (body?: Uint8Array | string) => void }, next: () => void) => void) => void } }) {
      server.middlewares.use("/model-preview-dev", (request, response, next) => {
        try {
          const modelPreviewRoot = installedModelPreviewRoot();
          const requestPath = decodeURIComponent((request.url ?? "/").split("?", 1)[0]);
          let file = resolve(modelPreviewRoot, `.${requestPath}`);
          if (file !== modelPreviewRoot && !file.startsWith(`${modelPreviewRoot}${sep}`)) {
            response.statusCode = 403;
            response.end("Forbidden");
            return;
          }
          if (statSync(file).isDirectory()) file = resolve(file, "index.html");
          const bytes = readFileSync(file);
          response.setHeader("Content-Type", modelPreviewTypes[extname(file).toLowerCase()] ?? "application/octet-stream");
          response.setHeader("Cache-Control", "no-store");
          response.end(bytes);
        } catch {
          next();
        }
      });
    },
  };
}

export default defineConfig({
  root: "frontend",
  base: "./",
  plugins: [devModelPreview()],
  build: { outDir: "../dist", emptyOutDir: true },
  test: { include: ["**/*.test.ts", "**/*.test.tsx", "**/*.test.mjs"] },
});
