"""Spool Backup — Decky Loader plugin backend.

Forced-close save-backup safety net. When a game stops, Steam may have
SIGKILLed Spool's attached `spool --run` instance (Quick Access -> Exit Game)
before it could back up that session's saves. This backend runs in the Decky
service context (outside the game's process tree), so a backup it spawns
survives the close.

Flow: the frontend forwards every game-stop event to `on_app_stop(appid)`; we
read Spool's `active-session.json`, and if its `steam_appid` matches and the
session is not yet `backed_up`, we spawn a detached `spool --backup "<game>"`.
On a normal quit Spool already set `backed_up: true`, so we no-op (no double
backup).

Runs as the `deck` user (no `_root` flag in plugin.json) so $HOME and file
ownership match Spool's interactive paths.
"""

import json
import os
import subprocess

import decky

import backup_logic as logic

SETTINGS_FILE = os.path.join(decky.DECKY_PLUGIN_SETTINGS_DIR, "settings.json")
BACKUP_LOG = os.path.join(decky.DECKY_PLUGIN_LOG_DIR, "spool-backup.log")


def _load_settings() -> dict:
    try:
        with open(SETTINGS_FILE, "r", encoding="utf-8") as f:
            data = json.load(f)
        return data if isinstance(data, dict) else {}
    except (OSError, ValueError):
        return {}


def _save_settings(settings: dict) -> None:
    os.makedirs(decky.DECKY_PLUGIN_SETTINGS_DIR, exist_ok=True)
    with open(SETTINGS_FILE, "w", encoding="utf-8") as f:
        json.dump(settings, f, indent=2)


def _home() -> str:
    # As the deck user, $HOME points at the user's home. Fall back to decky.HOME.
    return os.environ.get("HOME") or getattr(decky, "HOME", "") or os.path.expanduser("~")


def _spawn_backup(game: str, settings: dict) -> bool:
    """Spawn a detached `spool --backup "<game>"`. Returns True if spawned."""
    cmd = logic.resolve_spool_command(settings, _home())
    if not cmd:
        decky.logger.error("Spool Backup: could not locate the spool executable")
        return False
    argv = logic.build_backup_argv(cmd, game)
    os.makedirs(decky.DECKY_PLUGIN_LOG_DIR, exist_ok=True)
    decky.logger.info("Spool Backup: spawning %s", argv)
    with open(BACKUP_LOG, "ab") as log:
        subprocess.Popen(
            argv,
            start_new_session=True,  # detach: survives the game force-close
            stdout=log,
            stderr=subprocess.STDOUT,
            stdin=subprocess.DEVNULL,
        )
    return True


class Plugin:
    async def _main(self):
        decky.logger.info("Spool Backup loaded")

    async def _unload(self):
        decky.logger.info("Spool Backup unloaded")

    async def _uninstall(self):
        pass

    # ── Called by the frontend on every game-stop event ──────────────────
    async def on_app_stop(self, appid: int) -> dict:
        settings = _load_settings()
        rec = logic.read_session(logic.session_path(settings, _home()))
        if not logic.should_backup(rec, appid):
            decky.logger.debug("Spool Backup: no-op for appid %s", appid)
            return {"acted": False}

        game = rec.get("game", "")
        decky.logger.info(
            "Spool Backup: forced-close fallback for '%s' (appid %s)", game, appid
        )
        spawned = _spawn_backup(game, settings)
        if spawned and settings.get("notify", True):
            decky.emit("spool_backup_started", game)
        return {"acted": spawned, "game": game}

    # ── QAM panel: manual backup + status + settings ─────────────────────
    async def backup_now(self) -> dict:
        settings = _load_settings()
        rec = logic.read_session(logic.session_path(settings, _home()))
        if not rec or not rec.get("game"):
            return {"acted": False, "reason": "no active session record"}
        game = rec["game"]
        spawned = _spawn_backup(game, settings)
        return {"acted": spawned, "game": game}

    async def get_status(self) -> dict:
        settings = _load_settings()
        rec = logic.read_session(logic.session_path(settings, _home()))
        if not rec:
            return {"hasSession": False}
        return {
            "hasSession": True,
            "game": rec.get("game", ""),
            "backedUp": bool(rec.get("backed_up", False)),
            "startedAt": rec.get("started_at", ""),
        }

    async def get_settings(self) -> dict:
        s = _load_settings()
        return {
            "spool_command": s.get("spool_command", ""),
            "session_file": s.get("session_file", ""),
            "notify": bool(s.get("notify", True)),
        }

    async def set_settings(self, spool_command: str, session_file: str, notify: bool) -> dict:
        settings = _load_settings()
        settings["spool_command"] = (spool_command or "").strip()
        settings["session_file"] = (session_file or "").strip()
        settings["notify"] = bool(notify)
        _save_settings(settings)
        return settings
