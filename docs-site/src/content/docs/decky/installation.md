---
title: Installation & build
description: One-click install from Spool, the plugin's settings, and how to build and package it by hand.
sidebar:
  order: 7
---

## One-click install from Spool

Spool can install the plugin for you — **Settings → Decky Backup Plugin** in the desktop app. This is implemented in `tauri/src-tauri/src/decky_install.rs`, Linux-only.

The plugin payload is **embedded** into the Spool binary at compile time via `include_str!` — `decky/dist/index.js`, `main.py`, `plugin.json`, and `package.json`. (The whole `embedded` module is `#[cfg(target_os = "linux")]`, and the `dist/index.js` it references requires the plugin to have been built with `bun run build` before Spool is compiled — CI's Linux build does this.) Embedding means the install works offline and is version-locked to the running Spool build.

`install()` stages the payload into a private dir (`<app-data>/decky-staging/spool-backup`), then runs a single privileged step via `pkexec` because Decky's plugin dir and `plugin_loader` service are root-owned:

```sh
mkdir -p "$PLUGINS"
rm -rf "$PLUGINS/spool-backup"
cp -r "$SRC" "$PLUGINS/spool-backup"
chown -R root:root "$PLUGINS/spool-backup"
systemctl restart plugin_loader
```

If `pkexec` isn't present it returns a clear error pointing at polkit or a manual Desktop-Mode copy. A dismissed auth dialog (exit 126) and a missing polkit agent in Game Mode (exit 127) get specific hints — the latter suggests running the install from Desktop Mode.

The `decky_plugin_status` command backs the Settings UI, reporting a `DeckyPluginInfo`:

| Field | Meaning |
|-------|---------|
| `supported` | This platform can install the plugin (Linux only) |
| `installed` | A copy exists at `~/homebrew/plugins/spool-backup` |
| `installed_version` | `version` from the installed `package.json`, if readable |
| `bundled_version` | `version` from the embedded `package.json` |
| `decky_present` | Decky Loader itself appears installed (`~/homebrew` exists) |

## Plugin settings (QAM panel)

The Quick Access panel (`src/components/content.tsx`) has a **Settings** section, persisted to the plugin's `settings.json` via the `get_settings` / `set_settings` callables:

- **Notify on backup** — show a toast when a backup finishes (default on).
- **Spool command** — override the auto-detected `spool` / `spool-launcher.sh` path used to start the headless server (see [Headless server](./headless-server)).

## Manifest

`decky/plugin.json` declares the plugin to Decky Loader:

- `name: "Spool"`, `author`, `api_version: 1`.
- `flags: []` — no `_root`, so the backend runs as the `deck` user (see [Overview](./overview)).
- a `publish` block (tags, description, image) for the Decky store.

## Build and deploy by hand

```bash
cd decky
bun install
bun run build       # rollup -c → dist/index.js
```

`package.json` also has convenience deploy scripts (`deploy`, `deploy:local`) that `rsync` the built plugin to a Deck and restart `plugin_loader`. A distributed plugin is laid out as Decky Loader expects:

```
spool-backup/
  dist/index.js   [required]
  main.py
  plugin.json     [required]
  package.json    [required]
```

## CI

`.github/workflows/decky.yml` builds and packages the plugin on pushes and PRs that touch `decky/**` or the Rust files the plugin depends on (`plugin_server.rs`, `cli.rs`, `lib.rs`, `paths.rs`, `Cargo.toml`). It runs under bun, installs with `--frozen-lockfile`, runs `bun run build`, verifies `dist/index.js` exists, then zips the `spool-backup/` layout above and uploads it as the `spool-backup-plugin` artifact.
