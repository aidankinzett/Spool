# SteamOS Game-Mode Attached Launch — Design (Sub-project A)

**Date:** 2026-05-29
**Status:** Approved for planning
**Scope:** Spool (Rust/Tauri) only. The companion Decky plugin is a separate sub-project (B) with its own design/plan cycle.

## Problem

Spool runs as a single long-lived, tray-resident process. The library window is a *view* on it: closing the window hides to tray, and `RunEvent::ExitRequested` is blocked unless the tray "Quit Spool" item explicitly called `app.exit(0)` (see `lib.rs::run`).

On a Steam Deck / Bazzite / SteamOS **Game Mode** session, Steam launches a non-Steam shortcut and treats the game as "running" until *that spawned process tree* exits. Spool's Steam shortcut points at `spool --run "Name" "Exe"` (via the stable `spool-launcher.sh` wrapper for AppImage installs). Because no Spool is already running in a fresh Game-Mode session, the launched process becomes the **primary tray app**:

1. It shows the full library window (jarring fullscreen in gamescope).
2. It runs the restore → launch → backup workflow, launching the game as a child.
3. When the game exits, the workflow finishes but **Spool deliberately stays alive** (tray-resident).

Steam never sees the process exit, so the "game" never stops — the user must force-close from the Steam menu, which then risks killing Spool before its post-session backup runs.

## Goal

When launched from a Steam shortcut **in Game Mode**, Spool should:

- Not park itself in the tray with a full library window.
- Show a minimal progress splash during restore → launch.
- Run the existing restore → launch → wait workflow.
- **Exit when the game exits** so Steam correctly registers the game stopping.
- Self-back-up on a normal in-game quit (unchanged behavior, just followed by exit).
- Leave a **session record** on disk so the future Decky plugin can provide a forced-close backup fallback without double-backing-up.

Additionally, expose a **headless `spool --backup "Name"`** one-shot the plugin can spawn.

Everything outside Game Mode (Windows, desktop Linux) keeps today's tray-resident, single-instance-forwarding behavior **unchanged**.

## Non-goals (→ Sub-project B)

- The Decky plugin itself: the stop-event listener, reading the session record, and spawning the fallback backup. Sub-project A only produces the CLI + session record those will consume.
- Changing how Steam shortcuts are created (`steam.rs` continues to point shortcuts at `spool --run`).
- Solving forced-close backup *without* the plugin. A's forced-close story is: "Steam killed us before backup; on the next plugin-equipped run the safety net catches it." Without the plugin, a hard force-close may still miss a backup — accepted.

## Detection: are we in Game Mode?

Game Mode runs under **gamescope**, which exports `GAMESCOPE_WAYLAND_DISPLAY` into the environment of everything it launches. Desktop Mode (X11 / ordinary Wayland desktop) does not. That is the discriminator.

```
gamemode::is_steam_game_mode() -> bool
    // Linux only; always false on Windows/macOS.
    1. If env SPOOL_ATTACHED_LAUNCH is set: "1"/"true" => true, "0"/"false" => false (override/escape hatch).
    2. Otherwise: env GAMESCOPE_WAYLAND_DISPLAY is present and non-empty => true.
    3. Otherwise false.
```

- Consulted **only** on the `--run` startup path.
- The `SPOOL_ATTACHED_LAUNCH` override exists for testing on a desktop and for the rare user running gamescope on a desktop session who wants to opt out.

Sources: gamescope sets `GAMESCOPE_WAYLAND_DISPLAY` for the Game-Mode session compositor (ValveSoftware/gamescope; ArchWiki Gamescope page).

## Startup shapes in `lib.rs::run()`

Parse argv early, then branch **before** building the Tauri app:

### (a) `--backup "Name"` → fully headless
- No Tauri builder, no window, no tray, no single-instance, no background tasks.
- Init tracing, load `Config` + `Library`, resolve ludusavi, run `backup_game_core(...)`, update the matched entry's stats, update the session record (`backed_up = true`), flush, exit with status.
- Runs standalone every time (never forwards to a running instance) so the plugin gets deterministic "spawn process → it backs up → it exits" semantics.

### (b) `--run` AND `is_steam_game_mode()` → attached
- Build the Tauri app **without**: `tauri-plugin-single-instance`, tray mount, LAN discovery, sync health poller, startup sync, accent/size backfills.
- Window: a dedicated **splash** window/route (small or fullscreen, no chrome) instead of `main`.
- In `setup`, write the session record, then spawn `runner::launch_game_inner(&app, id)` directly (resolve the game id by name, as the existing startup `--run` path does).
- Do **not** install the window-close→hide handler or the `ExitRequested` prevention.
- When the workflow future completes (Ok or Err), call `app.exit(0)` (after the splash shows a terminal state briefly on error). This is the behavior that makes Steam see the game stop.

### (c) everything else → unchanged
- Today's path verbatim: single-instance, tray, pollers, library window, `PendingRun` handshake, exit-prevention. Windows and desktop-Linux `--run` land here.

## Minimal splash (frontend)

- New SvelteKit route `routes/splash/+page.svelte`, opened as its own `WebviewWindow` (label `splash`) in attached mode; `main` is not created.
- Subscribes to the existing `run:phase` events and renders a compact status: "Restoring saves…", "Launching…", spinner.
- When phase reaches `playing`, hide the splash (game is foreground in gamescope). On `error`, show the message briefly; the process exits on completion regardless.
- No new backend commands required. Reuses `runner.rs` event emissions. `api.ts` needs at most a trivial addition (none expected).

## Session record

- `paths::active_session_file()` → `app_data_dir()/active-session.json`.
- New `session.rs`:
  ```
  struct ActiveSession { game: String, steam_appid: u32, session_id: String, started_at: DateTime<Utc>, backed_up: bool }
  fn write_start(game, steam_appid) -> session_id
  fn mark_backed_up()        // set backed_up = true (idempotent; no-op if file missing)
  fn read() -> Option<ActiveSession>
  ```
- `steam_appid` computed with the same `steam_shortcuts_util::calculate_app_id` Spool uses when creating the shortcut, so it matches the appid Steam reports to the plugin on the stop event. (Quoting must match `steam.rs::upsert_spool_shortcut`: `calculate_app_id("\"<exe>\"", app_name)` using the `spool_executable()` path.)
- `session_id` distinguishes runs (avoids a stale prior record being mistaken for the current session). For A, a monotonic-ish value derived from `started_at` plus the game name is sufficient; `Math.random`/`Date::now` constraints don't apply (this is app runtime, not a workflow script).
- Written at attached `--run` start; marked `backed_up = true` by Spool's own post-session backup and by `spool --backup`.

## Backup-core refactor

- Extract the body of `#[tauri::command] manual_backup` (`runner.rs`) into an AppHandle-free function:
  ```
  async fn backup_game_core(
      config_snapshot, library: &SharedLibrary (or owned snapshot), game_id_or_name
  ) -> AppResult<ManualBackupResult>
  ```
  performing: resolve ludusavi + config dir + wine prefix, run `ludusavi backup`, update entry stats, save library.
- `manual_backup` becomes a thin wrapper that also does the `app.emit("library:changed")` + `sync::record_backup_event`.
- The headless `--backup` path calls `backup_game_core` directly; sync-event recording is best-effort and may be skipped when there is no running app/HTTP client (acceptable — the plugin path is about local save safety, and the next online session reconciles).
- The attached normal-exit path continues to back up via the existing workflow (`run_workflow`), then `session::mark_backed_up()`.

## File touch list

| File | Change |
|------|--------|
| `cli.rs` | add `CliMode::Backup { game_name }` + parsing + tests |
| `gamemode.rs` (new) | `is_steam_game_mode()` + tests (env-driven) |
| `session.rs` (new) | session-record read/write/mark + tests |
| `paths.rs` | `active_session_file()` |
| `lib.rs` | three-way startup branch; conditionally skip single-instance/tray/pollers/exit-prevention; attached `setup` spawns workflow + `app.exit(0)` on completion; create splash window in attached mode |
| `runner.rs` | extract `backup_game_core`; mark session `backed_up` after self-backup |
| `tauri.conf.json` | declare the `splash` window (hidden by default / created programmatically) |
| `routes/splash/+page.svelte` (new) | minimal phase splash |

## Testing

- `cli.rs`: `--backup "Name"` parses; `--run` still parses; bad arities fall back to `Normal`.
- `gamemode.rs`: override env precedence; `GAMESCOPE_WAYLAND_DISPLAY` presence; false on Windows/macOS via `cfg`.
- `session.rs`: round-trip write/read; `mark_backed_up` idempotent and no-op when file absent; appid matches `steam.rs` computation for the same inputs.
- `backup_game_core`: unit-level where feasible (ludusavi invocation is integration-level; mirror existing `runner.rs`/`ludusavi.rs` test patterns).
- Manual/E2E on a Deck/Bazzite Game Mode session: launch via Steam shortcut → splash shows → game runs → quit in-game → Spool exits → Steam shows game stopped; verify backup ran and `active-session.json` marked done. Desktop-mode launch unchanged.

## Risks / open points

- **Detection false positives/negatives.** A desktop user running a one-off gamescope wrapper would be detected as Game Mode. Mitigated by the `SPOOL_ATTACHED_LAUNCH` override. Acceptable.
- **Splash window in gamescope.** Need to confirm a borderless/fullscreen Tauri splash renders correctly under gamescope with the existing `WEBKIT_DISABLE_DMABUF_RENDERER`/`WEBKIT_DISABLE_COMPOSITING_MODE` workarounds (already set in `run()`). Verify during implementation.
- **Headless `--backup` and sync events.** Without a running app, `sync::*` calls that depend on `AppHandle` state are skipped; ensure `backup_game_core` degrades cleanly rather than failing.
- **AppImage `--backup`.** The plugin will invoke the stable `spool-launcher.sh` wrapper with `--backup`; confirm the wrapper forwards args (it does: `exec "$APPIMAGE" "$@"`).
