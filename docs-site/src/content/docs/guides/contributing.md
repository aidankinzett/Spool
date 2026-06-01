---
title: Contributing
description: Branching, checks, and what CI expects before you open a PR.
---

Thanks for helping out! This page covers the mechanics of getting a change
merged. For how the code is organised, read the
[Architecture overview](/Spool/architecture/overview/) first.

## Branching

- Branch off `master`.
- Keep branches focused — one logical change per PR is easier to review.
- CI runs on every PR (see below); the Linux release-profile build and the
  server tests run too.

## Before you open a PR

Run the full local check suite. **CI fails on any clippy warning**, so don't
skip the clippy step.

```bash
# Backend (from tauri/src-tauri)
cargo check
cargo clippy --all-targets -- -D warnings
cargo test

# Frontend (from tauri/)
bun run check
bun run lint
bun run test
```

## What CI runs

GitHub Actions workflows live in `.github/workflows/`:

- **`ci.yml`** — `build-windows` (backend build + clippy/check/test + frontend
  checks), `build-linux` (release-profile smoke build, push-only), `e2e-linux`
  (WebDriver suite under Xvfb), and `server` (sync-server tests).
- **`release.yml`** — tag-triggered; builds the Windows NSIS installer and the
  Linux AppImage and publishes a GitHub Release plus a `latest.json` updater
  manifest.
- **`docs.yml`** — builds and deploys this documentation site to GitHub Pages
  on pushes to `master` that touch `docs-site/`.
- **`decky.yml`** — builds, verifies, and packages the Spool Backup Decky Loader plugin into an installable zip on pushes affecting `decky/` or the plugin server backend.
- **`bump-release.yml`** — manual workflow kickoff that increments version components (major, minor, patch), tags the commit, and triggers `release.yml`.
- **`server-publish.yml`** — on version tags, builds and publishes the sync-server Docker image to GHCR (only when `server/` changes).


## Conventions worth knowing

- **JSON shape compatibility:** the `library.json` and `config.json` structs
  carry a container-level `#[serde(default)]`, so missing keys fall back to the
  struct's `Default` and older files load without migration — keep it that way
  when adding fields. Apply the attribute at the struct (container) level, not
  per-field: a per-field `#[serde(default)]` shadows the struct's custom
  `Default` values with the field-type default. Fields the app no longer uses
  are removed, not retained for legacy round-trip.
- **Lock discipline:** never hold a `std::sync::Mutex` guard across `.await`.
  Snapshot what you need, drop the guard, then await.
- **Add a command, add a wrapper:** when you add a Rust `#[tauri::command]`,
  register it in the `generate_handler!` list **and** add its typed wrapper to
  `tauri/src/lib/api.ts`. Keep `tauri/src/lib/types.ts` in sync with the Rust
  serde structs it mirrors.
- **Event names** are colon-namespaced (`library:changed`, `run:phase`) —
  Tauri 2 rejects `.` in event names at runtime.

## Editing these docs

The docs site is a standalone Astro project in `docs-site/`.

```bash
cd docs-site
bun install
bun run dev      # local preview with hot reload
bun run build    # production build into docs-site/dist/
```

Content lives in `docs-site/src/content/docs/` as Markdown / MDX. The sidebar is
configured in `docs-site/astro.config.mjs`. Every page has an "Edit page" link
that points straight at the source file on GitHub.
