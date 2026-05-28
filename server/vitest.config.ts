import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
    // db.ts opens a SQLite database at import time. Point it at an
    // in-memory DB so importing any module under test is side-effect free
    // and route tests can spin up throwaway databases.
    env: { DATABASE_PATH: ":memory:" },
    coverage: {
      provider: "v8",
      reporter: ["text", "html"],
      include: ["src/**/*.ts"],
      exclude: ["src/**/*.test.ts", "src/index.ts"],
    },
  },
});
