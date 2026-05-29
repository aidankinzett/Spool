# Decky Loader Plugin — Forced-Close Backup Safety Net (Sub-project B) — Design

**Date:** 2026-05-30
**Status:** Approved for planning
**Scope:** A Decky Loader plugin (TypeScript/React frontend + Python backend) living in a `decky/` subdir of this repo. Consumes the CLI + session-record contract produced by Sub-project A (merged). No Spool (Rust/Tauri) changes are required.

## Problem

On a SteamOS / Steam Deck **Game Mode** session, when the user closes a game via **Quick Access → Exit Game**, Steam SIGKILLs the tracked process tree. The process Steam tracks is Spool's attached `spool --run` instance (Sub-project A), so Spool can be killed **before** its post-session ludusavi backup runs → that session's saves are not backed up.

Any backup runner that lives *inside* Steam's killed tree races the SIGKILL. The fix is to trigger the backup from a process that **survives** the close. Decky Loader's plugin backend runs in the Steam/Decky service context — **outside** the game's process tree — so a backup it spawns survives the force-close.

Restore is **not** a concern: Spool's attached `--run` restores saves in-process, synchronously, before launching the game (Sub-project A). This plugin only handles the *stop* side.

## Contract from Sub-project A (what we consume)

Sub-project A is merged and provides everything this plugin needs. The plugin reads/spawns these — it does not change them.

1. **Session record** at `~/.local/share/Spool/active-session.json` (Linux `app_data_dir()`):
   ```json
   { "game": "Hades", "steam_appid": 2147483649, "session_id": "2147483649-1717000000000", "started_at": "2026-05-30T12:00:00Z", "backed_up": false }
   ```
   - Written by attached `spool --run` at launch.
   - `steam_appid` is computed with the **same** CRC formula as the Steam shortcut: `steam_shortcuts_util::calculate_app_id("\"<exe>\"", game_name)` over `spool_executable()` (the stable `spool-launcher.sh` for AppImage installs). So it **equals the `unAppID` Steam reports** to the plugin on the lifecycle event. (Verified against `steam.rs::upsert_spool_shortcut` and `session.rs::compute_steam_appid`.)
   - Flipped to `backed_up: true` by Spool's own normal-quit backup (`runner::run_workflow` → `session::mark_backed_up`) **and** by `spool --backup`.

2. **Headless one-shot**: `spool --backup "Game Name"` — loads config + library, runs ludusavi backup for one game, marks the session record `backed_up: true`, exits. No GUI/tray/single-instance; never forwards to a running instance. (`cli.rs::CliMode::Backup`, `lib.rs::run_backup_headless`.)

3. **Stable launcher** for AppImage installs: `~/.local/share/Spool/spool-launcher.sh "$@"` (`exec "$APPIMAGE" "$@"`), so `spool-launcher.sh --backup "Hades"` works. Native installs expose `spool` on PATH / `/usr/bin/spool`.

## Goal

A Decky plugin that, on a game **stop** event for a Spool-managed (non-Steam-shortcut) app whose session was **not** already backed up, spawns a fresh `spool --backup "<game>"` outside the killed tree — unifying the normal-quit and forced-close paths into one race-free post-stop trigger, with no double-backup.

## Non-goals

- Changing Spool (Rust). The A contract is complete. (The single accepted gap: if `spool_executable()` returned `None` at launch, no record is written → plugin no-ops. See *Degraded paths*.)
- Restore on the plugin side (A handles it in-process).
- Backing up arbitrary non-Spool Steam games. The plugin only acts when `active-session.json` exists and its `steam_appid` matches the stopped app.
- A long-lived socket/IPC channel to Spool. **IPC = CLI**: spawn `spool --backup`, read the record from disk. No connection to the (possibly-dead) Spool process.

## Architecture

```
Steam (Game Mode)
   │  game stop  (SteamClient.GameSessions.RegisterForAppLifetimeNotifications)
   ▼
Decky plugin FRONTEND (src/index.tsx, runs in the Steam UI / SP context)
   │  bRunning === false  →  call("on_app_stop", unAppID)
   ▼
Decky plugin BACKEND (main.py, runs in the Decky service as the `deck` user)
   │  read active-session.json
   │  if appid matches && !backed_up:
   ▼
   spawn:  spool --backup "<game>"   (detached, survives force-close)
   │
   ▼
spool --backup  →  ludusavi backup  →  session.backed_up = true  →  exit
```

Standard Decky plugin layout (modern `@decky/*` toolchain, `api_version: 1`):

| Path | Responsibility |
|------|----------------|
| `decky/plugin.json` | Manifest: name, author, `flags`, `api_version`, `publish` metadata |
| `decky/package.json` | `@decky/api`, `@decky/ui`, `@decky/rollup`; pnpm v9; `build`/`watch` scripts |
| `decky/rollup.config.js` | Re-exports `@decky/rollup` config |
| `decky/tsconfig.json` | TS config for the frontend |
| `decky/main.py` | Python backend: `class Plugin` (`_main`/`_unload`/`on_app_stop`/`backup_now`/settings) |
| `decky/src/index.tsx` | Frontend: `definePlugin`, lifecycle hook registration, QAM panel |
| `decky/README.md` | Install/setup (requires Decky Loader) |

### Frontend (src/index.tsx)

- The lifecycle hook is registered **once at plugin load**, in the `definePlugin` factory body — **not** inside the QAM panel `content` component, which unmounts whenever the user closes the panel. The hook must outlive the panel so stops are caught regardless of UI state.
  ```ts
  interface LifetimeNotification { unAppID: number; nInstanceID: number; bRunning: boolean; }

  const onAppStop = callable<[appid: number], void>("on_app_stop");

  export default definePlugin(() => {
    const sub = SteamClient.GameSessions.RegisterForAppLifetimeNotifications(
      (n: LifetimeNotification) => { if (!n.bRunning) onAppStop(n.unAppID); }
    );
    return {
      name: "Spool Backup",
      titleView: <div className={staticClasses.Title}>Spool Backup</div>,
      icon: <FaFloppyDisk/>,           // placeholder; pick a real react-icon
      content: <Content/>,             // QAM panel (status + manual backup + settings)
      onDismount() { sub.unregister(); },
    };
  });
  ```
- The backend does the appid/record matching (it owns the file read), so the frontend can forward **every** stop event; the backend cheaply no-ops on non-matches. This keeps the frontend dumb and the matching logic unit-testable in Python.
- The QAM `Content` panel reads status via backend `callable`s (see UI below).

### Backend (main.py)

```python
import decky, json, os, subprocess

class Plugin:
    async def _main(self):
        decky.logger.info("Spool backup safety-net loaded")

    async def _unload(self):
        pass

    # Called by the frontend on every game stop.
    async def on_app_stop(self, appid: int) -> dict:
        rec = self._read_session()
        if not rec or rec.get("steam_appid") != appid or rec.get("backed_up"):
            return {"acted": False}
        # Optional small grace re-check (see Double-backup avoidance).
        cmd = self._spool_backup_cmd(rec["game"])
        subprocess.Popen(cmd, start_new_session=True,
                         stdout=self._logfile(), stderr=subprocess.STDOUT)
        return {"acted": True, "game": rec["game"]}

    async def backup_now(self) -> dict: ...      # manual button → same spawn
    async def get_status(self) -> dict: ...      # last backup info for the panel
    async def get_settings(self) -> dict: ...
    async def set_spool_command(self, path: str): ...
```

Pure helpers (`_match`, `_read_session`, `_spool_backup_cmd`, path resolution) are factored out of the async methods so they are unit-testable with plain `pytest`/`unittest` (no Decky runtime).

## Key design decisions

### Privilege: omit the `_root` flag → backend runs as the `deck` user

Decky runs a plugin backend **unprivileged (as the `deck` user)** unless `plugin.json.flags` includes `_root`. We **omit `_root`**. Consequences, all desirable:
- `decky.HOME` / the process `$HOME` is the deck user's home → `active-session.json` resolves at the right path with no `sudo`.
- The spawned `spool --backup` inherits the deck user's env (HOME, XDG_DATA_HOME, user dbus), so ludusavi reads/writes the **same** save and backup paths Spool uses interactively, with correct file ownership.

This sidesteps the run-as-root hazard (root-owned backup files, wrong `$HOME`) that would otherwise require `sudo -u deck env HOME=… …`. **Must be confirmed on real hardware** (Decky privilege behavior is the kind of thing that shifts between loader releases) — see Open questions.

### Path resolution (session file + spool command)

Both are **autodetected with a config override** stored in the plugin's settings dir (`decky.DECKY_PLUGIN_SETTINGS_DIR/settings.json`):

- **Session file**: default `${HOME}/.local/share/Spool/active-session.json`. Override key `session_file`.
- **Spool command**: resolution order —
  1. configured `spool_command` (if set and exists),
  2. `${HOME}/.local/share/Spool/spool-launcher.sh` (AppImage installs — the stable wrapper),
  3. `spool` on `PATH` / `/usr/bin/spool` (native installs).
  Invoked as `[<cmd>, "--backup", "<game>"]`.

### Double-backup avoidance

The record's `backed_up` flag is the guard, re-read at stop time:
- **Normal in-game quit**: attached `spool --run` backs up → sets `backed_up: true` → exits → *then* Steam fires the stop event. By the time `on_app_stop` reads the file it is already `true` → no-op. (Ordering is guaranteed by A: the flag is written before the process exits, which is before Steam observes the stop.)
- **Forced "Exit Game"**: Spool SIGKILLed before flipping the flag → record stays `false` → plugin spawns the fallback. ✅
- **Insurance against a slow disk flush** on normal quit: optionally re-read the record after a short grace delay (≈1–2 s) before spawning; if it flipped to `true` in the interim, skip. Cheap and eliminates the only realistic race. (ludusavi backups are also versioned, so a stray double-backup is harmless, just wasteful.)

### Degraded / no-op paths (defined behavior)

- **No record** (file missing, or `spool_executable()` was `None` at launch so A wrote nothing): `on_app_stop` no-ops. The forced-close backup is simply not provided for that launch — the accepted A-level gap.
- **appid mismatch** (a non-Spool game stopped): no-op. No effect on non-Spool games (acceptance criterion).
- **Stale record** from a prior crashed session of the *same* game (`backed_up:false`, old `started_at`): spawning a backup of the current saves is harmless/desirable. We may additionally ignore records older than a threshold; low priority.
- **spool command not found**: log an error, toast (if enabled), no crash.

### User feedback (optional)

On a fallback backup, optionally surface a Decky `toaster` toast ("Spool: backing up <game>…") so the user knows the safety net fired. Backend signals the frontend via `decky.emit("spool_backup_started", game)`; frontend shows the toast. Gated by a setting (default on).

## Open questions to verify during build (hardware)

- **Lifecycle payload**: confirm `LifetimeNotification` shape (`unAppID`, `nInstanceID`, `bRunning`) and that `unAppID` for a non-Steam shortcut equals our CRC `steam_appid` on current SteamOS. (Prior art `GedasFX/decky-cloud-save` uses exactly this API + fields; confirm appid equality for non-Steam shortcuts specifically.)
- **Privilege**: confirm the backend runs as `deck` (no `_root`) and that `decky.HOME`/env point at the deck user so the spawned backup writes to the right paths with correct ownership.
- **Detached survival**: confirm `subprocess.Popen(..., start_new_session=True)` from the backend genuinely outlives the force-close (it should — the backend is outside the game tree).
- **AppImage vs native**: confirm `spool-launcher.sh --backup` forwards args (it does: `exec "$APPIMAGE" "$@"`) and the native `spool` path resolves.

## Acceptance criteria

- [ ] Plugin detects game stop for Spool-managed (non-Steam-shortcut) apps.
- [ ] On forced "Exit Game", a fresh `spool --backup` runs and saves are backed up (verified on real Deck/Bazzite hardware).
- [ ] No double-backup on a normal in-game quit (respects `backed_up`).
- [ ] No effect on non-Spool games.
- [ ] Documented install/setup (requires Decky Loader).

## References

- A design: `docs/superpowers/specs/2026-05-29-steamos-gamemode-attached-launch-design.md`
- A plan: `docs/superpowers/plans/2026-05-29-steamos-gamemode-attached-launch.md`
- Decky plugin template: `github.com/SteamDeckHomebrew/decky-plugin-template`
- Decky dev wiki: `wiki.deckbrew.xyz/en/plugin-dev/getting-started`
- `decky.pyi`: `github.com/SteamDeckHomebrew/decky-loader` → `backend/decky_loader/plugin/imports/decky.pyi`
- Prior art: `GedasFX/decky-cloud-save` (game start/stop → rclone sync; same lifecycle API), `AkazaRenn/SDH-GameSync`, `popsUlfr/SDH-PauseGames`.
