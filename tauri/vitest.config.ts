import { defineConfig } from "vitest/config";
import { svelte, vitePreprocess } from "@sveltejs/vite-plugin-svelte";
import path from "node:path";

// Standalone Vitest config — deliberately does NOT use the `sveltekit()`
// plugin (which boots a dev server). The plain `svelte()` plugin is enough
// to compile components for jsdom-based component tests, and `$lib` is
// aliased by hand since that resolution normally comes from SvelteKit.
import { fileURLToPath } from 'node:url';
import { storybookTest } from '@storybook/addon-vitest/vitest-plugin';
import { playwright } from '@vitest/browser-playwright';
const dirname = typeof __dirname !== 'undefined' ? __dirname : path.dirname(fileURLToPath(import.meta.url));

// More info at: https://storybook.js.org/docs/next/writing-tests/integrations/vitest-addon
export default defineConfig({
  plugins: [svelte({
    preprocess: vitePreprocess()
  })],
  resolve: {
    alias: {
      $lib: path.resolve(__dirname, "./src/lib"),
      // $app/* are SvelteKit virtual modules — stub them for the standalone Vitest env.
      "$app/navigation": path.resolve(__dirname, "./src/__mocks__/app-navigation.ts")
    },
    // Resolve the browser build of Svelte so component rendering works.
    conditions: ["browser"]
  },
  test: {
    coverage: {
      provider: "v8",
      reporter: ["text", "html"],
      include: ["src/lib/**/*.{ts,svelte}"],
      exclude: ["src/**/*.{test,spec}.*"]
    },
    projects: [{
      extends: true,
      test: {
        // Plain jsdom unit/component tests. Browser-free, so `bun run test`
        // (which targets this project) needs no Playwright install. The
        // Storybook story tests live in the separate `storybook` project below
        // and run via `bun run test:storybook` (CI installs Chromium first).
        name: "unit",
        environment: "jsdom",
        include: ["src/**/*.{test,spec}.{js,ts}"],
        setupFiles: ["./vitest-setup.ts"]
      }
    }, {
      extends: true,
      plugins: [
      // The plugin will run tests for the stories defined in your Storybook config
      // See options at: https://storybook.js.org/docs/next/writing-tests/integrations/vitest-addon#storybooktest
      storybookTest({
        configDir: path.join(dirname, '.storybook')
      })],
      test: {
        name: 'storybook',
        browser: {
          enabled: true,
          headless: true,
          provider: playwright({}),
          instances: [{
            browser: 'chromium'
          }]
        }
      }
    }]
  }
});