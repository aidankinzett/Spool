---
title: Decky Backup Plugin
description: The Spool Backup Decky Loader plugin — forced-close save backup safety net for SteamOS Game Mode.
sidebar:
  order: 6
---

The Spool Backup Decky Loader plugin provides a forced-close backup safety net for SteamOS / Steam Deck Game Mode. It lives in `decky/` in the repo and is a separate sub-project from the Tauri app.

See also: [SteamOS Game Mode Launch](./game-mode) for the Spool-side session record and `--headless-server` contract this plugin consumes.

## The problem

On SteamOS Game Mode, when the user closes a game via **Quick Access → Exit Game**, Steam SIGKILLs the tracked process tree. The process Steam tracks is Spool's attached `spool --run` instance, so Spool can be killed **before** its post-session ludusavi backup runs — that session's saves are not backed up.

Any backup runner inside Steam's killed tree races the SIGKILL. The fix is to trigger the backup from a process that **survives** the close. Decky Loader's plugin backend runs in the Steam/Decky service context — outside the game's process tree — so a backup it starts survives the force-close.

## Architecture

The plugin is a thin adapter around `spool --headless-server`. Rather than reading `active-session.json` and spawning `spool --backup` directly, the backend starts a persistent headless server subprocess and communicates with it over a Unix socket. This avoids the cold-start cost of a fresh Spool process per operation and gives the server access to live in-process state (library, LAN peers).

```
Steam (Game Mode)
   │  game stop  (SteamClient.GameSessions.RegisterForAppLifetimeNotifications)
   ▼
Decky plugin FRONTEND (src/index.tsx, runs in the Steam UI / SP context)
   │  bRunning === false  →  call("on_app_stop", unAppID)
   ▼
Decky plugin BACKEND (main.py, runs in the Decky service as the `deck` user)
   │  POST /session/game-stopped  { "appid": <unAppID> }
   │  (over Unix socket at ~/.local/share/Spool/plugin.sock)
   ▼
spool --headless-server  (started by plugin at load time, persistent)
   │  checks active-session.json: appid matches && !backed_up?
   ▼
   run ludusavi backup  →  session.backed_up = true
```

On plugin load (`_main`), the backend resolves the spool command and starts `spool --headless-server` as a detached subprocess. On plugin unload (`_unload`), it terminates the server and cleans up the socket file.

The frontend forwards **every** stop event to the backend; the backend forwards to the server, which does the appid/session matching and cheaply no-ops on non-matches.

## Double-backup avoidance

The `backed_up` flag in `active-session.json` is checked by the headless server:

- **Normal in-game quit**: attached `spool --run` backs up → sets `backed_up: true` → exits → *then* Steam fires the stop event. By the time the server processes the game-stopped request it's already `true` → no-op.
- **Forced "Exit Game"**: Spool SIGKILLed before flipping the flag → record stays `false` → server runs the fallback backup.

## Privilege: no `_root` flag

The plugin omits `_root` in `plugin.json`, so the backend runs as the `deck` user. Consequences, all desirable:
- `decky.HOME` / process `$HOME` is the deck user's home → paths resolve correctly with no `sudo`.
- The `spool --headless-server` subprocess inherits the deck user's env (HOME, XDG_DATA_HOME, user dbus), so ludusavi reads/writes the same save and backup paths Spool uses interactively, with correct file ownership.

Decky Loader's PyInstaller bundle prepends a `/tmp/_MEI*` directory to `LD_LIBRARY_PATH`. `main.py::_clean_env()` strips this before launching the server so it sees the host library path instead of Decky's bundled libs.

## Spool command resolution

The backend resolves the spool command to start `--headless-server` in this order:
1. Configured `spool_command` in `decky.DECKY_PLUGIN_SETTINGS_DIR/settings.json` (if set and exists)
2. `${HOME}/.local/share/Spool/spool-launcher.sh` (AppImage installs — the stable wrapper)
3. `spool` on `PATH`
4. `/usr/bin/spool` (native installs)

## Degraded / no-op paths

- **Socket unavailable** (server not running, spool not found): `on_app_stop` returns `{ "acted": false, "reason": "socket unavailable" }` and logs a warning. No crash.
- **appid mismatch** (a non-Spool game stopped): the server no-ops. No effect on non-Spool games.
- **Already backed up** (`backed_up: true` in the session record): server no-ops, no double backup.

## File structure

| File | Role |
|------|------|
| `decky/plugin.json` | Manifest: name, author, `flags: []` (no `_root`), `api_version: 1` |
| `decky/main.py` | Python backend: `class Plugin` + Unix-socket HTTP client + server lifecycle |
| `decky/src/index.tsx` | Frontend: `definePlugin`, lifecycle hook, QAM panel |
| `decky/package.json` | Build config (`@decky/api`, `@decky/rollup`, pnpm) |
| `decky/README.md` | Install/setup docs |
| `.github/workflows/decky.yml` | CI: pnpm build + zip artifact |

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
