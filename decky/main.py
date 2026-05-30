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

import asyncio
import json
import os
import sys

import decky

# Decky's sandboxed plugin loader does not put the plugin's own directory on
# sys.path, so a bare `import backup_logic` fails with ModuleNotFoundError.
# Add the plugin dir explicitly before importing our sibling module.
_PLUGIN_DIR = getattr(decky, "DECKY_PLUGIN_DIR", None) or os.path.dirname(
    os.path.abspath(__file__)
)
sys.path.insert(0, _PLUGIN_DIR)

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


def _spawn_env() -> dict:
    """Build a clean environment for the spawned backup process.

    Decky Loader ships as a PyInstaller one-file bundle, whose bootloader
    prepends its own extracted lib dir (``/tmp/_MEI*``) to ``LD_LIBRARY_PATH``
    and bundles libraries (e.g. an older ``libreadline.so.8``). A subprocess we
    spawn — notably ``/bin/sh`` running ``spool-launcher.sh`` — would inherit
    that path and load Decky's bundled libs instead of the system's, crashing
    with ``undefined symbol`` errors. PyInstaller stashes the pre-launch values
    in ``*_ORIG``; restore those (or drop the var entirely) so the child gets
    the system library path, and strip any stray ``/tmp/_MEI*`` entries.
    """
    env = os.environ.copy()
    for var in ("LD_LIBRARY_PATH", "LD_PRELOAD"):
        orig = env.pop(var + "_ORIG", None)
        if orig is not None:
            env[var] = orig
        elif var in env:
            cleaned = [
                p for p in env[var].split(os.pathsep)
                if p and not p.startswith("/tmp/_MEI")
            ]
            if cleaned:
                env[var] = os.pathsep.join(cleaned)
            else:
                env.pop(var, None)
    return env


async def _run_backup_blocking(game: str, settings: dict) -> dict:
    """Run `spool --backup "<game>"` and wait for it. Returns {ok, [reason]}.

    Spawned with ``start_new_session=True`` so the backup runs in its own
    session, detached from both the game's process tree (which Steam SIGKILLs
    on Exit Game) and ours — if Decky itself is torn down mid-backup the child
    keeps running, it just won't report completion. We still await its exit so
    we can surface success/failure (toast + status), since the Decky service
    survives the game force-close.
    """
    cmd = logic.resolve_spool_command(settings, _home())
    if not cmd:
        decky.logger.error("Spool Backup: could not locate the spool executable")
        return {"ok": False, "reason": "could not locate the spool executable"}
    argv = logic.build_backup_argv(cmd, game)
    os.makedirs(decky.DECKY_PLUGIN_LOG_DIR, exist_ok=True)
    decky.logger.info("Spool Backup: running %s", argv)
    try:
        with open(BACKUP_LOG, "ab") as log:
            proc = await asyncio.create_subprocess_exec(
                *argv,
                start_new_session=True,  # detach: survives a force-close
                stdout=log,
                stderr=asyncio.subprocess.STDOUT,
                stdin=asyncio.subprocess.DEVNULL,
                env=_spawn_env(),
            )
            code = await proc.wait()
    except OSError as exc:
        decky.logger.error("Spool Backup: failed to launch backup: %s", exc)
        return {"ok": False, "reason": str(exc)}
    if code == 0:
        decky.logger.info("Spool Backup: backup of '%s' completed", game)
        return {"ok": True}
    decky.logger.error("Spool Backup: backup of '%s' exited %s", game, code)
    return {"ok": False, "reason": f"backup exited with code {code}"}


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
            # INFO (not debug) so the common "stopped game isn't ours / already
            # backed up" path is visible when diagnosing on-device. Surface both
            # appids since a sign/encoding mismatch is the usual culprit.
            session_appid = rec.get("steam_appid") if isinstance(rec, dict) else None
            backed_up = rec.get("backed_up") if isinstance(rec, dict) else None
            decky.logger.info(
                "Spool Backup: no-op for appid %s (session appid=%s, backed_up=%s)",
                appid, session_appid, backed_up,
            )
            return {"acted": False}

        game = rec.get("game", "")
        decky.logger.info(
            "Spool Backup: forced-close fallback for '%s' (appid %s)", game, appid
        )
        notify = settings.get("notify", True)
        # decky.emit is async — must be awaited or the event is never sent.
        if notify:
            await decky.emit("spool_backup_started", game)
        result = await _run_backup_blocking(game, settings)
        if notify:
            await decky.emit("spool_backup_finished", game, result.get("ok", False),
                             result.get("reason", ""))
        return {"acted": True, "game": game, **result}

    # ── QAM panel: manual backup + status + settings ─────────────────────
    async def backup_now(self) -> dict:
        settings = _load_settings()
        rec = logic.read_session(logic.session_path(settings, _home()))
        if not rec or not rec.get("game"):
            return {"acted": False, "ok": False, "reason": "no active session record"}
        game = rec["game"]
        result = await _run_backup_blocking(game, settings)
        return {"acted": True, "game": game, **result}

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
