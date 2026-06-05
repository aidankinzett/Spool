import type { StorybookConfig } from "@storybook/react-vite";
import { fileURLToPath } from "node:url";

// Resolve the @decky/* imports to local DOM shims so plugin components render
// outside Steam. react-icons stays real (it's plain SVG). React/ReactDOM stay
// real too — only the Steam-runtime libs are mocked.
const deckyUi = fileURLToPath(new URL("./mocks/decky-ui.tsx", import.meta.url));
const deckyApi = fileURLToPath(new URL("./mocks/decky-api.tsx", import.meta.url));

const config: StorybookConfig = {
  stories: ["../src/**/*.stories.@(ts|tsx)"],
  addons: ["@storybook/addon-docs"],
  framework: {
    name: "@storybook/react-vite",
    options: {},
  },
  async viteFinal(cfg) {
    cfg.resolve ??= {};
    cfg.resolve.alias = {
      ...(cfg.resolve.alias as Record<string, string> | undefined),
      "@decky/ui": deckyUi,
      "@decky/api": deckyApi,
    };
    return cfg;
  },
};

export default config;
