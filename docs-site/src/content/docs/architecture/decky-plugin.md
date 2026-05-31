---
title: Decky Backup Plugin
description: The Spool Backup Decky Loader plugin — forced-close save backup safety net for SteamOS Game Mode.
sidebar:
  order: 6
---

The Spool Backup Decky Loader plugin provides a forced-close backup safety net for SteamOS / Steam Deck Game Mode. It lives in `decky/` in the repo and is a separate sub-project from the Tauri app.

See also: [SteamOS Game Mode Launch](./game-mode) for the Spool-side contract (session record + `spool --backup` CLI) this plugin consumes.

## The problem

On SteamOS Game Mode, when the user closes a game via **Quick Access → Exit Game**, Steam SIGKILLs the tracked process tree. The process Steam tracks is Spool's attached `spool --run` instance, so Spool can be killed **before** its post-session ludusavi backup runs — that session's saves are not backed up.

Any backup runner inside Steam's killed tree races the SIGKILL. The fix is to trigger the backup from a process that **survives** the close. Decky Loader's plugin backend runs in the Steam/Decky service context — outside the game's process tree — so a backup it spawns survives the force-close.

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

The frontend forwards **every** stop event; the backend does the appid/record matching and cheaply no-ops on non-matches. This keeps the frontend dumb and matching logic unit-testable in Python.

## Double-backup avoidance

The `backed_up` flag in the session record is the guard, re-read at stop time:

- **Normal in-game quit**: attached `spool --run` backs up → sets `backed_up: true` → exits → *then* Steam fires the stop event. By the time `on_app_stop` reads the file it's already `true` → no-op. (Guaranteed ordering: the flag is written before the process exits, which is before Steam observes the stop.)
- **Forced "Exit Game"**: Spool SIGKILLed before flipping the flag → record stays `false` → plugin spawns the fallback.
- **Grace re-read**: optionally re-read the record after ≈1–2 s before spawning; if it flipped to `true` in the interim, skip. Eliminates the only realistic race on slow disk flush.

## Privilege: no `_root` flag

The plugin omits `_root` in `plugin.json`, so the backend runs as the `deck` user. Consequences, all desirable:
- `decky.HOME` / process `$HOME` is the deck user's home → `active-session.json` resolves at the right path with no `sudo`.
- The spawned `spool --backup` inherits the deck user's env (HOME, XDG_DATA_HOME, user dbus), so ludusavi reads/writes the same save and backup paths Spool uses interactively, with correct file ownership.

## Path resolution

Both the session file and spool command are **autodetected with a config override** stored in `decky.DECKY_PLUGIN_SETTINGS_DIR/settings.json`:

- **Session file**: default `${HOME}/.local/share/Spool/active-session.json`. Override key `session_file`.
- **Spool command** resolution order:
  1. Configured `spool_command` (if set and exists)
  2. `${HOME}/.local/share/Spool/spool-launcher.sh` (AppImage installs — the stable wrapper)
  3. `spool` on `PATH` / `/usr/bin/spool` (native installs)

## Degraded / no-op paths

- **No record** (file missing, or `spool_executable()` was `None` at launch): `on_app_stop` no-ops.
- **appid mismatch** (a non-Spool game stopped): no-op. No effect on non-Spool games.
- **Stale record** from a prior crashed session of the same game (`backed_up: false`, old `started_at`): spawning a backup of the current saves is harmless/desirable.
- **spool command not found**: logs an error, no crash.

## File structure

| File | Role |
|------|------|
| `decky/plugin.json` | Manifest: name, author, `flags` (no `_root`), `api_version: 1` |
| `decky/main.py` | Python backend: `class Plugin` + pure helpers (testable without Decky runtime) |
| `decky/src/index.tsx` | Frontend: `definePlugin`, lifecycle hook, QAM panel |
| `decky/tests/test_backend.py` | Pure-helper unit tests (`pytest`) |
| `.github/workflows/decky.yml` | CI: pnpm build + python tests + zip artifact |

## Frontend lifecycle hook

The lifecycle hook is registered **once at plugin load**, in the `definePlugin` factory body — not inside the QAM panel `Content` component (which unmounts when the user closes the panel). The hook must outlive the panel so stops are caught regardless of UI state.

```ts
const onAppStop = callable<[appid: number], void>("on_app_stop");

export default definePlugin(() => {
  const sub = SteamClient.GameSessions.RegisterForAppLifetimeNotifications(
    (n: LifetimeNotification) => { if (!n.bRunning) onAppStop(n.unAppID); }
  );
  return {
    name: "Spool Backup",
    content: <Content/>,
    onDismount() { sub.unregister(); },
  };
});
```

## One-click install

`decky_install.rs` embeds the plugin and installs it via `pkexec` to `~/homebrew/plugins/spool-backup`, then restarts the `plugin_loader` service. Settings → Decky Backup Plugin in the Spool app reports `supported`, `installed`, `installed_version`, `bundled_version`, and `decky_present` status.
