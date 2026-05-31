---
title: Getting Started
description: Clone Spool, install dependencies, and run it in development.
---

Spool is a [Tauri 2](https://v2.tauri.app/) desktop app: a **Rust** backend
(`tauri/src-tauri/`) paired with a **SvelteKit 5** frontend (`tauri/src/`).
Windows and Linux are both primary targets (notably the gaming-handheld
distros — Bazzite, CachyOS, SteamOS).

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (stable toolchain)
- [Bun](https://bun.sh/) for the frontend
- Tauri's platform prerequisites — see the
  [Tauri prerequisites guide](https://v2.tauri.app/start/prerequisites/)
- **Linux only:** `libwebkit2gtk-4.1-dev` and friends (for the WebView), plus
  `umu-launcher` on the host if you want to test the Proton runner

:::note
`ludusavi` and `rclone` ship as bundled Tauri sidecars — you don't install
them separately. `umu-launcher` (`umu-run`) is the one Linux dependency that is
*not* bundled.
:::

## Install & run

All commands run from `tauri/` unless noted.

```bash
cd tauri

# Install frontend dependencies (first time, or after package.json changes)
bun install

# Run the app in development (hot-reload frontend + auto-rebuild backend)
bun run tauri dev
```

## Build a release binary

```bash
cd tauri
bun run tauri build
# Output:
#   tauri/src-tauri/target/release/spool.exe
#   tauri/src-tauri/target/release/bundle/nsis/Spool_<version>_x64-setup.exe
```

On Linux the release build produces an AppImage (`Spool_*_amd64.AppImage`).

## Run the checks locally

CI fails on any clippy warning, so run these before pushing.

```bash
# Backend (from tauri/src-tauri)
cargo check
cargo clippy --all-targets -- -D warnings
cargo test

# Frontend (from tauri/)
bun run check     # svelte-check
bun run lint      # ESLint
bun run test      # Vitest unit tests
```

End-to-end tests drive a real Tauri window via `tauri-driver` + WebdriverIO:

```bash
cd tauri
bun run test:e2e  # builds the app then runs the WebDriver suite
# Headless Linux: xvfb-run -a bun run e2e
```

## Where things live

| Path | What it is |
| --- | --- |
| `tauri/src-tauri/src/` | Rust backend (persistence, subprocess orchestration, OS integration) |
| `tauri/src/` | SvelteKit frontend (routes + `lib/`) |
| `server/` | Self-hostable Hono sync server |
| `decky/` | Companion Decky Loader plugin |
| `docs/` | Internal dev plans & design specs |
| `docs-site/` | This documentation site |

See [Architecture → Overview](/Spool/architecture/overview/) for the full
module map.
