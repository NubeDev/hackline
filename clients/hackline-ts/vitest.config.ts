import { defineConfig } from "vitest/config";

// Single shared loopback gateway across the suite (see
// `test/globalSetup.ts`). `pool: "forks"` keeps each test file in
// its own child so a crashed child cannot poison the rest, while
// the gateway itself is a single process for the whole run.
export default defineConfig({
  test: {
    globalSetup: ["./test/globalSetup.ts"],
    pool: "forks",
    poolOptions: {
      forks: {
        singleFork: true,
      },
    },
    testTimeout: 15_000,
    hookTimeout: 30_000,
    include: ["test/**/*.test.ts"],
  },
});
