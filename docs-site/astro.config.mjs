// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import mermaid from 'astro-mermaid';

// https://astro.build/config
export default defineConfig({
  // Deployed to GitHub Pages on the custom domain spool.kinzett.io
  // (see docs-site/public/CNAME). Served from the domain root, so there's
  // no project-subpath `base` — internal links are root-relative (`/...`).
  site: 'https://spool.kinzett.io',
  base: '/',
  integrations: [
    // Must come before starlight so its rehype plugin processes
    // ```mermaid``` code blocks first. Dark-only to match the app's UI.
    mermaid({ theme: 'dark', autoTheme: false }),
    starlight({
      title: 'Spool',
      description:
        'A cross-platform game library that keeps your game saves in sync between your Steam Deck and your PC.',
      favicon: '/favicon.svg',
      components: {
        // Adds Privacy Policy / Terms of Service links beneath the default
        // page footer on every page (homepage included).
        Footer: './src/components/Footer.astro',
      },
      social: [
        {
          icon: 'github',
          label: 'GitHub',
          href: 'https://github.com/aidankinzett/Spool',
        },
      ],
      // Dark-only, to match the app's UI.
      // User-facing guides come first; developer/architecture docs sit lower.
      sidebar: [
        {
          label: 'Using Spool',
          items: [
            { label: 'Install Spool', slug: 'guides/installing' },
            { label: 'Installing umu-launcher', slug: 'guides/installing-umu' },
            { label: 'Cloud Save Sync', slug: 'guides/cloud-saves' },
            { label: 'Self-hosted SFTP Remote', slug: 'guides/sftp-remote' },
            { label: 'LAN Transfers', slug: 'guides/lan-transfers' },
          ],
        },
        {
          label: 'Develop',
          items: [
            { label: 'Getting Started', slug: 'guides/getting-started' },
            { label: 'Contributing', slug: 'guides/contributing' },
          ],
        },
        {
          label: 'Architecture',
          items: [{ autogenerate: { directory: 'architecture' } }],
        },
        {
          label: 'Steam Deck (Decky Plugin)',
          items: [{ autogenerate: { directory: 'decky' } }],
        },
      ],
      editLink: {
        baseUrl: 'https://github.com/aidankinzett/Spool/edit/master/docs-site/',
      },
    }),
  ],
});
