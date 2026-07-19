import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test/setup.ts"],
    include: ["src/**/*.{test,spec}.{ts,tsx}"],
    coverage: {
      provider: "v8",
      reporter: ["text", "html", "lcov"],
      include: ["src/**/*.{ts,tsx}"],
      exclude: [
        "src/**/*.{test,spec}.{ts,tsx}",
        "src/test/**",
        "src/main.tsx",
        "src/vite-env.d.ts",
        // Stubs with no behaviour yet - excluded so they don't inflate the denominator
        "src/sync/**",
      ],
      thresholds: {
        // Modules already under test are held to a high bar so they cannot
        // regress. New pure-logic modules get an entry here as they land.
        "src/lib/desktop-visibility.ts": {
          lines: 100,
          functions: 100,
          branches: 100,
          statements: 100,
        },
        // Global floor. Deliberately low: most of the codebase predates the
        // test harness, and blocking CI on legacy coverage would just get the
        // check disabled. Ratchet this up as modules come under test (#4).
        lines: 5,
        functions: 5,
        branches: 10,
        statements: 5,
      },
    },
  },
});
