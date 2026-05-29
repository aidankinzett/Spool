# Spool Backup — Decky Loader plugin

A forced-close save-backup safety net for [Spool](../README.md)-managed games on
SteamOS / Steam Deck **Game Mode**.

## Why

When you close a game from **Quick Access → Exit Game**, Steam SIGKILLs the
tracked process tree. That tree is Spool's attached `spool --run` instance, so
Spool can be killed **before** its post-session ludusavi backup runs — losing
that session's save backup.

This plugin's backend runs in the Decky service context, **outside** the game's
process tree, so a backup it spawns survives the force-close. On a game stop it
reads Spool's session record and, if Spool didn't already back up, runs a fresh
`spool --backup "<game>"`.

On a **normal in-game quit** Spool backs up itself and marks the session
`backed_up: true`, so this plugin no-ops — no double backup. It also no-ops for
non-Spool games.

Requires the Game-Mode attached-launch support in Spool (Sub-project A): the
`active-session.json` record and the headless `spool --backup` one-shot.

## How it works

1. Frontend registers `SteamClient.GameSessions.RegisterForAppLifetimeNotifications`
   and forwards every game-**stop** to the backend.
2. Backend reads `~/.local/share/Spool/active-session.json`. If its
   `steam_appid` matches the stopped app and `backed_up` is `false`, it spawns a
   detached `spool --backup "<game>"`.
3. `spool --backup` runs ludusavi and flips `backed_up: true`.

The backend runs **as the `deck` user** (no `_root` flag), so `$HOME` and file
ownership match Spool's interactive paths.

## Develop / deploy

```bash
cd decky
pnpm install
pnpm build          # → dist/index.js

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

- Plugin log: `~/homebrew/logs/spool-backup/` (Decky) plus the spawned backup's
  own log at `spool-backup.log` in the plugin log dir.
- Confirm `spool-launcher.sh --backup "<game>"` (AppImage) or `spool --backup
  "<game>"` (native) works from a terminal first.
