# Spool documentation site

Developer-facing docs for Spool, built with [Astro](https://astro.build/) +
[Starlight](https://starlight.astro.build/). Deployed to GitHub Pages at
<https://aidankinzett.github.io/Spool/> by `.github/workflows/docs.yml` on every
push to `master` that touches `docs-site/`.

## Local development

```bash
cd docs-site
bun install
bun run dev      # http://localhost:4321/Spool/
```

| Command | Action |
| --- | --- |
| `bun install` | Install dependencies |
| `bun run dev` | Start the local dev server |
| `bun run build` | Build the production site to `./dist/` |
| `bun run preview` | Preview the production build locally |
| `bun run check` | Type-check the project |

## Adding content

- Pages are Markdown / MDX under `src/content/docs/`.
- The sidebar is configured in `astro.config.mjs`.
- The `architecture/` section is auto-generated from the files in that
  directory; ordering is controlled per-page via the `sidebar.order` frontmatter.

The `base` is set to `/Spool/` for GitHub Pages project hosting, so internal
links written by hand must include that prefix (Starlight components handle it
automatically).
