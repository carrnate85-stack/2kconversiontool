import { defineConfig } from "vitest/config";
export default defineConfig({
  root: "frontend",
  base: "./",
  build: { outDir: "../dist", emptyOutDir: true },
  test: { include: ["**/*.test.ts", "**/*.test.tsx", "**/*.test.mjs"] },
});
