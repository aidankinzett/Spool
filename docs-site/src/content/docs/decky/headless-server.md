---
title: Headless server
description: spool --headless-server — the loopback HTTP server the plugin talks to, and how the backend manages it.
sidebar:
  order: 3
---

Everything the plugin does is backed by `spool --headless-server`, a loopback HTTP server inside the main Spool process. It's defined in `tauri/src-tauri/src/plugin_server.rs` and gated `#[cfg(unix)]`.

## Why a server instead of subprocesses

An earlier version shelled out to one-shot `spool --backup` / `--release-lock` subcommands per operation (since removed). A persistent server instead:

- avoids the cold-start cost of spawning a fresh Spool process per operation,
- gives the plugin access to live in-process state (the LAN peer list, in-flight downloads), and
- lets the React UI talk to it **directly** rather than relaying every byte through the Decky callable bridge.

That last point is why it's **loopback TCP**, not a Unix socket: an `<img>` tag can't load from a socket, but it can from `http://127.0.0.1:<port>`. And because `127.0.0.1` is a [secure origin](https://developer.mozilla.org/en-US/docs/Web/Security/Secure_Contexts), the covers it serves aren't blocked as mixed content from the `https://steamloopback.host` page the Steam UI runs under.

## Binding and the port file

On startup the server binds a loopback TCP port — preferring **47650**, falling back to an ephemeral port if 47650 is taken (e.g. a stale instance) — then writes the resolved port to:

```
~/.local/share/Spool/plugin-http-port
```

(`paths::plugin_http_port_path`). Both halves of the plugin read that file to build the `http://127.0.0.1:<port>` base URL. **An absent file means the server is not running** — the frontend surfaces that as *"Spool isn't running. Launch Spool, then try again."*

Config and the library are intentionally **not** cached in the server — they're reloaded from disk on every request, so games added or paths changed in the desktop Spool GUI are visible to the plugin without a restart.

## Endpoints

| Method & path | Purpose |
|---------------|---------|
| `GET /status` | Liveness probe (`{ "ok": true }`) |
| `GET /session` | Current `active-session.json` (`hasSession`, `game`, `backedUp`, `startedAt`) |
| `POST /session/game-stopped` | Forced-close fallback — see [Forced-close backup](./forced-close-backup) |
| `POST /session/backup-now` | Manual backup of the active session |
| `GET /library` | Library entries, each with a server-computed `shortcut_app_id` injected |
| `POST /fold` | Run a cross-device rclone fold and wait for it (refreshes playtime / last-played) |
| `GET /games/:id/steam-launch-info` | Fields to create a non-Steam shortcut live and launch it |
| `GET /games/:id/steam-art/:kind` | Steam art (`capsule`/`hero`/`logo`/`header`) as base64, WebP transcoded to PNG |
| `GET /lan/peers` | Discovered LAN peers |
| `GET /lan/peers/:addr/:port/games` | A peer's shared games (proxied server-side) |
| `GET /lan/peers/:addr/:port/games/:id/cover` | A peer's cover image (proxied) |
| `POST /lan/install` | Start a LAN install, returns an `install_token` |
| `GET /lan/download` | In-flight download progress snapshot (`null` when idle) |
| `DELETE /lan/download` | Cancel an in-flight install by token |
| `GET /covers/*` | Static cover files straight off disk (`ServeDir`) |

A permissive `CorsLayer` is applied because the React UI's JSON `fetch`es are cross-origin (it runs under `steamloopback.host`). `<img>` covers aren't CORS-gated and load without it.

On boot `serve()` also spawns the LAN discovery **listener** so `/lan/peers` has data. The Deck is a pure consumer here — it listens for announces but doesn't announce itself or run a file server. It reads its own `device_id` from config to self-filter the local machine's announces when the GUI runs on the same box.

## Backend lifecycle (`main.py`)

The Python backend owns the server process:

- **`_main`** (plugin load) resolves the spool command and starts `spool --headless-server` as a detached subprocess (`start_new_session=True`).
- **`_unload` / `_uninstall`** terminate the server (`SIGTERM`, then `SIGKILL` after 5 s) and remove the port file.

### Cleaning the environment

Decky Loader ships as a PyInstaller bundle whose bootloader prepends a `/tmp/_MEI*` directory to `LD_LIBRARY_PATH`. A child process would inherit that and load Decky's bundled libs instead of the host's. `_clean_env()` restores the pre-launch values PyInstaller stashed in `*_ORIG` and strips any leftover `/tmp/_MEI*` entries from `LD_LIBRARY_PATH` / `LD_PRELOAD` before launching the server.

### Resolving the spool command

`_resolve_spool_command` tries, in order:

1. A configured `spool_command` in the plugin's `settings.json` (if set and the path exists).
2. `~/.local/share/Spool/spool-launcher.sh` — the stable AppImage wrapper.
3. `spool` on `PATH`.
4. `/usr/bin/spool` — native installs.

The AppImage wrapper matters: `$APPIMAGE` is volatile (the filename carries the version and AppImageLauncher relocates it on each update), so Spool writes a fixed `spool-launcher.sh` that execs whatever AppImage is current (`paths::refresh_appimage_launcher`). This is the same path `paths::spool_executable` hands out to Steam shortcuts.

### The HTTP client

The backend talks to the server with the standard library `http.client` against `127.0.0.1:<port>` (`_request_sync`). Blocking requests run in a thread executor (`_spool` → `run_in_executor`) so the async handlers don't stall. If the port file is absent (server not running) or a request fails, it returns `None` and the caller degrades gracefully — for a game-stop that surfaces as `{ "acted": false, "reason": "server unavailable" }`, logged as a warning rather than a crash.
