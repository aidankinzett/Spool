import type { StorybookConfig } from '@storybook/sveltekit';

/**
 * Storybook config for Spool's SvelteKit frontend.
 *
 * The `@storybook/sveltekit` framework reuses the project's `vite.config.ts`
 * (so Tailwind v4 via `@tailwindcss/vite` and the Svelte preprocessor come
 * through automatically) while stripping the SvelteKit-only compile/guard
 * Vite plugins that don't apply outside a real SvelteKit dev server. It also
 * provides mocks for `$app/*` modules, so components that import navigation/
 * stores/state render without a running Kit server.
 *
 * The remaining Tauri-specific boundary (the `$lib/api` IPC bridge and
 * `assetUrl`) is stubbed in `preview.ts` via `@tauri-apps/api/mocks`.
 */
const config: StorybookConfig = {
  stories: ['../src/**/*.stories.@(js|ts|svelte)'],
  addons: ['@storybook/addon-svelte-csf'],
  framework: {
    name: '@storybook/sveltekit',
    options: {},
  },
};

export default config;
