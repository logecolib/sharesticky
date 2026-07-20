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
        // The store holds real behaviour - optimistic writes, desktop tagging,
        // cross-window announcements - so it is held near the top.
        "src/store/stickies.ts": {
          lines: 95,
          functions: 80,
          branches: 85,
          statements: 95,
        },
        // Global floor, raised from 5% now that the store and bridge are
        // covered. Still well under the current 37%, because the remainder is
        // React components: the manager's logic already lives in tested pure
        // modules, and TipTap in jsdom is a known tarpit. A floor to stop
        // regression, not a target to chase.
        lines: 30,
        functions: 25,
        branches: 35,
        statements: 30,
      },
    },
  },
});
