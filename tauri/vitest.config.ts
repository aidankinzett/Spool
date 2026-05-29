import { defineConfig } from "vitest/config";
import { svelte, vitePreprocess } from "@sveltejs/vite-plugin-svelte";
import path from "node:path";

// Standalone Vitest config — deliberately does NOT use the `sveltekit()`
// plugin (which boots a dev server). The plain `svelte()` plugin is enough
// to compile components for jsdom-based component tests, and `$lib` is
// aliased by hand since that resolution normally comes from SvelteKit.
export default defineConfig({
  plugins: [svelte({ preprocess: vitePreprocess() })],
  resolve: {
    alias: {
      $lib: path.resolve(__dirname, "./src/lib"),
      // $app/* are SvelteKit virtual modules — stub them for the standalone Vitest env.
      "$app/navigation": path.resolve(__dirname, "./src/__mocks__/app-navigation.ts"),
    },
    // Resolve the browser build of Svelte so component rendering works.
    conditions: ["browser"],
  },
  test: {
    environment: "jsdom",
    include: ["src/**/*.{test,spec}.{js,ts}"],
    setupFiles: ["./vitest-setup.ts"],
    coverage: {
      provider: "v8",
      reporter: ["text", "html"],
      include: ["src/lib/**/*.{ts,svelte}"],
      exclude: ["src/**/*.{test,spec}.*"],
    },
  },
});
