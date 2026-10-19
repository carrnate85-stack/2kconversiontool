import { defineConfig } from "vitest/config";
export default defineConfig({test:{include:["react/**/*.test.ts","react/**/*.test.tsx"],pool:"forks",maxWorkers:1,minWorkers:1}});
