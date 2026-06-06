# Spool Backup — Decky Loader plugin

A forced-close safety net for [Spool](../README.md)-managed games on
SteamOS / Steam Deck **Game Mode** — it backs up the session's saves *and*
releases the sync-server play lock when Steam kills Spool too early.

## Why

When you close a game from **Quick Access → Exit Game**, Steam SIGKILLs the
tracked process tree. That tree is Spool's attached `spool --run` instance, so
Spool can be killed **before** its post-session work runs. Two things are lost:
the session's save backup, and the release of Spool's sync-server play lock — so
other devices keep seeing the game as "playing on <device>" until the server's
stale window (~5 min) elapses.

This plugin's backend runs in the Decky service context, **outside** the game's
process tree, so it survives the force-close. It talks to a long-lived
`spool --headless-server` over loopback HTTP; on a game stop it asks the server
to flag the session unsynced and back up, if Spool didn't already finish.

On a **normal in-game quit** Spool backs up + releases itself and marks the
session `backed_up: true`, so this plugin no-ops — no double backup, no
redundant release. It also no-ops for non-Spool games.

Requires the Game-Mode attached-launch support in Spool: the
`active-session.json` record and the headless server (`spool --headless-server`).

## How it works

1. Frontend registers `SteamClient.GameSessions.RegisterForAppLifetimeNotifications`
   and forwards every game-**stop** to the backend.
2. Backend ensures `spool --headless-server` is running (starting it if needed)
   and POSTs the stopped app id to its game-stop endpoint over loopback HTTP.
3. The server reads `~/.local/share/Spool/active-session.json`. If its
   `steam_appid` matches the stopped app and `backed_up` is `false`, it flags
   this device's session unsynced (the "release lock" marker — scoped by device
   id, a no-op when no sync server is configured), then runs ludusavi to back up
   the saves, records the play session Spool's killed workflow never got to, and
   flips `backed_up: true`.

The backend runs **as the `deck` user** (no `_root` flag), so `$HOME` and file
ownership match Spool's interactive paths.

## Develop / deploy

```bash
cd decky
bun install
bun run build       # → dist/index.js

# Run the backend unit tests (no Deck required):
python -m pytest tests/

# Deploy to a Deck/Bazzite box with Decky Loader installed:
rsync -a --exclude node_modules ./ deck@<deck-ip>:~/homebrew/plugins/spool-backup/
# then restart Decky (or reboot) and the panel appears in the Quick Access Menu.
```

A distributed plugin zip is laid out as:

```
spool-backup/
  dist/index.js   [required]
  main.py
  backup_logic.py
  plugin.json     [required]
  package.json    [required]
```

## Settings (Quick Access panel)

- **Spool command** — override the autodetected `spool` / `spool-launcher.sh`.
- **Session file** — override the autodetected `active-session.json` path.
- **Notify on fallback backup** — toast when the safety net fires (default on).

## Troubleshooting

- Plugin log: `~/homebrew/logs/spool-backup/` (Decky). The headless server logs
  to Spool's own `debug.log`.
- Confirm `spool --headless-server` starts and writes its port to
  `~/.local/share/Spool/plugin-http-port` (use `spool-launcher.sh` on the
  AppImage). The plugin reads that file to reach the server.
- **Game still shows as "playing" on another device** after exiting: the
  unsynced-session flag didn't reach the sync server. Check the server is
  reachable and the cloud remote is configured in Spool's Settings. The server
  reaps stale locks after ~5 min regardless, so this self-heals either way.
