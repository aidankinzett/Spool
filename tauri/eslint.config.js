import js from "@eslint/js";
import svelte from "eslint-plugin-svelte";
import tseslint from "typescript-eslint";
import svelteParser from "svelte-eslint-parser";
import globals from "globals";

export default tseslint.config(
  js.configs.recommended,
  ...tseslint.configs.recommended,
  ...svelte.configs["flat/recommended"],
  {
    languageOptions: {
      globals: {
        ...globals.browser,
        ...globals.node,
      },
    },
  },
  {
    files: ["**/*.svelte"],
    languageOptions: {
      parser: svelteParser,
      parserOptions: {
        parser: tseslint.parser,
      },
    },
  },
  {
    // .svelte.ts files use Svelte 5 runes — parse with svelte-eslint-parser
    files: ["**/*.svelte.ts", "**/*.svelte.js"],
    languageOptions: {
      parser: svelteParser,
      parserOptions: {
        parser: tseslint.parser,
      },
    },
  },
  {
    // Tauri apps have no configurable base path, so resolve() is a no-op
    rules: {
      "svelte/no-navigation-without-resolve": "off",
    },
  },
  {
    // Storybook stories: the root `{#snippet template(args)}` is consumed by
    // the addon-svelte-csf compiler, not referenced in markup, so ESLint sees
    // it as an unused variable. Allow snippet declarations named `template`.
    files: ["**/*.stories.svelte"],
    rules: {
      "@typescript-eslint/no-unused-vars": [
        "error",
        { varsIgnorePattern: "^template$" },
      ],
    },
  },
  {
    ignores: [".svelte-kit/**", "build/**", "node_modules/**"],
  }
);
