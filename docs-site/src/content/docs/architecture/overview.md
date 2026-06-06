---
title: Overview
description: How Spool is put together — one long-lived Tauri process with a SvelteKit view on top.
sidebar:
  order: 1
---

**Spool** is a cross-platform (Windows + Linux) game library and
save-management wrapper built with [Tauri 2](https://v2.tauri.app/) (Rust
backend) and [SvelteKit 5](https://kit.svelte.dev/) (frontend).

It maintains a persistent game library with cover art (Steam's official CDN, with
SteamGridDB as a fallback), launches
games directly — restoring saves before launch and backing them up on exit —
and handles cloud-save sync and conflict detection via
[ludusavi](https://github.com/mtkennerly/ludusavi) + bundled
[rclone](https://rclone.org/). On Linux it launches Windows `.exe` games through
**Proton** (umu-launcher).

## The core idea

A single long-lived Tauri process owns **all** persistence, subprocess
orchestration, OS integration, HTTP clients, and workflow state. The SvelteKit
frontend is purely a view onto that state — every file IO, subprocess call, and
HTTP request lives in Rust.

### Tray-resident lifecycle

Spool runs as one long-lived process. The library window is a *view* on it —
closing the window hides it to the system tray rather than quitting. Quit is
**only** via the tray menu's "Quit Spool" item. Secondary `spool` invocations
(from Steam shortcuts / Armoury Crate launchers) are caught by
`tauri-plugin-single-instance` and forwarded as argv to the running primary — so
there's no cold-start cost on game launch.

## The two halves

- **[Rust backend](/architecture/backend/)** — `tauri/src-tauri/src/`.
  Persistence, external integrations (ludusavi, SteamGridDB, Steam, sync
  server), LAN sharing, platform-specific OS integration, and the run-workflow
  state machine.
- **[SvelteKit frontend](/architecture/frontend/)** — `tauri/src/`.
  Routes (library, add, edit, splash, settings) plus shared `lib/` code,
  including the single typed `api.ts` IPC wrapper.

## Key patterns

- **Per-concern Tauri `State<T>`** — commands declare their dependencies as
  parameters (`library: State<'_, SharedLibrary>`, etc.). No god object.
- **Atomic JSON saves** — write to a temp file, then `rename` over the target,
  rotating a `.bak`. Survives a crash mid-write.
- **`AppHandle::emit` cross-window broadcast** — events go to all open webviews,
  so a popup mutating the library refreshes the main window for free.
- **RAII run-lock** — `runner.rs` holds a single-launch guard whose `Drop`
  releases the slot, even on panic.

## Data files

State lives under `paths::app_data_dir()` — `%LOCALAPPDATA%\Spool\` on Windows,
`~/.local/share/Spool/` on Linux.

| File | Contents |
| --- | --- |
| `config.json` | App-wide settings (binary paths, Proton, cloud-save/rclone, SteamGridDB, UI mode, LAN, sync server) |
| `library.json` | The game library — a list of `GameEntry` objects |
| `covers/` | Downloaded cover images (Steam CDN first, SteamGridDB fallback) |
| `launchers/` | Generated per-game `.exe` launcher stubs (Windows) |
| `lan-games/` | Default install root for games downloaded from LAN peers |
| `prefixes/` | Per-game Proton/Wine prefixes (Linux) |
| `debug.log` | App log |
