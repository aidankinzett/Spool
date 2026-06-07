---
title: SteamOS Game Mode Launch
description: How Spool's attached-launch mode works in SteamOS Game Mode — splash window, no tray, exit on game close, session record for the Decky plugin.
sidebar:
  order: 7
---

On a Steam Deck / Bazzite / SteamOS **Game Mode** session, Spool switches into *attached-launch* mode: it shows a minimal splash instead of the library window, runs the game workflow, and **exits when the game exits** so Steam correctly registers the game stopping.

Everything outside Game Mode (Windows, desktop Linux) keeps today's tray-resident, single-instance-forwarding behavior unchanged.

## The problem it solves

Spool runs as a single long-lived, tray-resident process. The library window is a *view* on it — closing the window hides to tray, and `RunEvent::ExitRequested` is blocked unless the tray "Quit Spool" item calls `app.exit(0)`.

In Game Mode, Steam treats a non-Steam shortcut as "running" until the *spawned process tree* exits. Because no Spool is already running in a fresh Game-Mode session, the launched `spool --run` process becomes the primary tray app:

1. It would show the full library window (jarring fullscreen in gamescope).
2. It runs the restore → launch → backup workflow.
3. When the game exits, the workflow finishes but **Spool deliberately stays alive** (tray-resident).

Steam never sees the process exit, so the "game" never stops — the user must force-close from the Steam menu, which risks killing Spool before its post-session backup runs.

## Detection

Game Mode runs under **gamescope**, which exports `GAMESCOPE_WAYLAND_DISPLAY` into the environment of everything it launches. Desktop Mode does not.

```
gamemode::is_steam_game_mode() -> bool
    1. If env SPOOL_ATTACHED_LAUNCH is set: "1"/"true" => true, "0"/"false" => false.
    2. Otherwise: env GAMESCOPE_WAYLAND_DISPLAY present and non-empty => true.
    3. Otherwise false.
```

`SPOOL_ATTACHED_LAUNCH` exists for testing on a desktop (`SPOOL_ATTACHED_LAUNCH=1 spool --run ...`) and for users running gamescope on a desktop session who want to opt out. Detection is only consulted on the `--run` startup path.

## Startup shapes

`lib.rs::run()` parses argv early and branches before building the Tauri app:

### (a) `--headless-server` → fully headless
No Tauri builder, no window, no tray, no single-instance, no background tasks. Starts a loopback HTTP server (`plugin_server.rs`) and runs until killed. The Decky plugin drives game-stop backups, the unsynced "release lock" marker, and "Back up now" through its endpoints — which load `Config` + `Library` fresh per request, run `runner::backup_game_core(...)`, record the play session the killed workflow missed, and flip the session record `backed_up = true`. (This replaced the old per-operation `--backup` / `--release-lock` one-shot subcommands.)

### (b) `--run` AND `is_steam_game_mode()` → attached
Builds the Tauri app **without**: `tauri-plugin-single-instance`, tray mount, LAN discovery, sync health poller, startup sync, accent/size backfills. Opens the `splash` window/route instead of `main`. When the workflow future completes, calls `app.exit(0)`. The existing `RunEvent::ExitRequested` guard (`api.prevent_exit()` when `code.is_none()`) is correct as-is: `app_handle.exit(0)` passes `code = Some(0)` so it's allowed through.

### (c) Everything else → unchanged
Today's path verbatim: single-instance, tray, pollers, library window, `PendingRun` handshake, exit-prevention. Windows and desktop-Linux `--run` land here.

## Session record

`session.rs` writes `~/.local/share/Spool/active-session.json` at attached `--run` start:

```json
{
  "game": "Hades",
  "steam_appid": 2147483649,
  "session_id": "2147483649-1717000000000",
  "started_at": "2026-05-30T12:00:00Z",
  "backed_up": false,
  "suspended_secs": 0
}
```

`suspended_secs` is the running total of time spent suspended this session. The suspend watcher checkpoints it on each resume so a Game-Mode force-kill still subtracts sleep from the recorded playtime (the in-memory tally dies with the SIGKILLed workflow).

`steam_appid` is computed with the same CRC formula as the Steam shortcut (`steam_shortcuts_util::calculate_app_id("\"<exe>\"", game_name)` using `spool_executable()`), so it equals the `unAppID` Steam reports to the Decky plugin on the lifecycle event.

`backed_up` is flipped to `true` by Spool's own post-session backup (`runner::run_workflow` → `session::mark_backed_up`) and by the headless server's game-stop backup. The Decky plugin reads this on game-stop: if still `false`, Spool was force-killed before backup, so the plugin asks the headless server to run the fallback.

## Splash window

`routes/splash/+page.svelte` subscribes to `run:phase` events and shows restore / launch / backup progress with cover art and cloud-sync status. It calls `api.notifySplashReady()` after registering its event listener so the Rust workflow doesn't emit phases before the frontend is listening. When the phase reaches `playing`, the splash transitions to its "exit flow" UI; when the full workflow completes, `app.exit(0)` is called from Rust (`lib.rs`), which terminates the entire process and closes the window.

## Files

| File | Role |
|------|------|
| `tauri/src-tauri/src/gamemode.rs` | `is_steam_game_mode()` + env-override logic |
| `tauri/src-tauri/src/session.rs` | Session-record read/write/mark + appid computation |
| `tauri/src-tauri/src/cli.rs` | `CliMode::Backup { game_name }` parsing |
| `tauri/src-tauri/src/paths.rs` | `active_session_file()` |
| `tauri/src-tauri/src/runner.rs` | `backup_game_core` (AppHandle-free) + `mark_backed_up` call |
| `tauri/src-tauri/src/lib.rs` | Three-way startup branch; attached setup |
| `tauri/src/routes/splash/+page.svelte` | Minimal phase splash |
