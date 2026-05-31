// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// https://astro.build/config
export default defineConfig({
  // Deployed to GitHub Pages as a project site:
  // https://aidankinzett.github.io/Spool/
  site: 'https://aidankinzett.github.io',
  base: '/Spool/',
  integrations: [
    starlight({
      title: 'Spool',
      description:
        'Cross-platform game library + save-management wrapper built with Tauri 2 and SvelteKit.',
      favicon: '/favicon.svg',
      social: [
        {
          icon: 'github',
          label: 'GitHub',
          href: 'https://github.com/aidankinzett/Spool',
        },
      ],
      // Dark-only, to match the app's UI.
      sidebar: [
        {
          label: 'Start Here',
          items: [
            { label: 'Getting Started', slug: 'guides/getting-started' },
            { label: 'Contributing', slug: 'guides/contributing' },
          ],
        },
        {
          label: 'Architecture',
          items: [{ autogenerate: { directory: 'architecture' } }],
        },
      ],
      editLink: {
        baseUrl: 'https://github.com/aidankinzett/Spool/edit/master/docs-site/',
      },
    }),
  ],
});
