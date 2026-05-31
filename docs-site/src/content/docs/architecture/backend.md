---
title: Rust backend
description: The module map for tauri/src-tauri/src/ — persistence, integrations, and workflow orchestration.
sidebar:
  order: 2
---

The backend lives in `tauri/src-tauri/src/`. It's a single long-lived process
that owns all state and side effects. This page is a map of the modules.

## Foundation

- **`main.rs` / `lib.rs`** — entry point, module wiring, Tauri command
  registration. The `generate_handler!` list is the source of truth for every
  IPC command. Also sets up per-concern `State<T>`, the single-instance plugin,
  the tray icon + menu, lifecycle hooks, and CLI dispatch.
- **`error.rs`** — `AppError` enum + `AppResult` alias, serialisable so errors
  round-trip across the IPC boundary as strings.
- **`paths.rs`** — centralised filesystem path resolution. Every module that
  touches an app file goes through here.
- **`cli.rs`** — argv parsing for headless subcommands (`--run`, `--backup`,
  `--release-lock`) vs a normal launch.

## Persistence

- **`config.rs`** — app-wide settings persisted to `config.json`.
  `#[serde(default)]` on every field means older files load without migration.
- **`library.rs`** — `GameEntry` + `Library` CRUD with atomic JSON saves to
  `library.json`. Emits `library:changed` on every mutation.

## External integrations

- **`ludusavi.rs`** — subprocess invocation of the bundled ludusavi CLI. Owns
  the manifest cache and the search/find/enrich + restore/backup flow.
- **`ludusavi_config.rs`** — Spool owns ludusavi's `config.yaml` so it controls
  backup/restore paths, the cloud remote, retention, and per-restore redirects.
- **`steamgriddb.rs`** — HTTP client for SteamGridDB. Downloads portrait covers
  and extracts a vibrant accent colour.
- **`steam.rs`** — non-Steam shortcut creation (`shortcuts.vdf`) + grid art.
- **`sync.rs`** — sync-server HTTP client (the Hono server in `server/`):
  account registration, per-game play-state locks, save events, playtime sync.
- **`metadata.rs`** — HTTP client for the public Steam Store `appdetails` API. Enriches game entries with description, developer, publisher, genres, and release date (no API key required).
- **`lan/`** — the LAN game-sharing subsystem (`discovery.rs` UDP-broadcast peer
  discovery, `server.rs` in-process axum file server with blake3 manifests,
  `install.rs` the resumable content-addressed receiver).

## Platform-specific

These are `#[cfg]`-gated and degrade gracefully on the other OS.

- **Windows-only** — `launcher.rs` (embedded `launcher_stub.exe` generation),
  `registry.rs` (`RUNASADMIN` probe), `process.rs` (the elevated run-as-admin
  path; the normal spawn path is cross-platform).
- **Linux-only** — `proton.rs` (umu-launcher + per-game Wine prefixes),
  `gamemode.rs` (SteamOS Game Mode detection), `session.rs` (Game-Mode session
  records), `decky_install.rs` (companion Decky plugin installer),
  `redirects.rs` (cross-platform save-path mapping), `plugin_server.rs` (Unix-socket
  HTTP server for Decky companion communication), `suspend.rs` (D-Bus system
  suspend/resume watcher for play locks).

## Cross-platform OS integration

- **`system_open.rs`** — native "Open folder" with the AppImage env stripped.
- **`diagnostics.rs`** — the Settings → Compatibility dependency doctor.

## Workflow orchestration

- **`runner.rs`** — the marquee feature. A five-phase state machine
  (`restoring → launching → playing → backing-up → done`) emitting `run:phase`
  events at each transition. A single-launch RAII guard releases the slot even
  on panic.

## Startup backfills

One-shot tasks at boot for legacy library entries, saved once at the end:

- **`accent_backfill.rs`** — fills missing accent colours from covers on disk.
- **`size_backfill.rs`** — computes install sizes via `walkdir`.
- **`metadata_backfill.rs`** — throttled startup task that enriches library entries having a Steam ID but missing description/developer fields.

