"""Spool — Decky Loader plugin backend.

Communicates with `spool --headless-server` over loopback TCP using plain
HTTP. The headless server owns all backup logic, session state, and library
access; this plugin is a thin adapter that:

  * Manages the headless server subprocess lifetime (start on load, kill on
    unload).
  * Forwards game-stop events to POST /session/game-stopped so the server can
    decide whether a forced-close fallback backup is needed.
  * Exposes the server's endpoints to the QAM frontend, and hands the React UI
    the server's base URL so it can fetch `/library` and `<img>`-load
    `/covers/*` directly.

Why a server instead of subprocesses: a persistent server avoids the cold-
start cost of a fresh Spool process on every operation, gives the plugin
access to live in-process state (library, LAN peers), and returns richer
structured responses. Loopback TCP (rather than a Unix socket) lets the React
UI talk to it directly — `<img>` can't load from a socket, but it can from
`http://127.0.0.1:<port>`.

The server publishes its resolved port to `~/.local/share/Spool/plugin-http-port`
on startup; an absent file means the server is not running.
"""

import asyncio
import http.client
import json
import os
import subprocess
import sys
from functools import partial
from typing import Optional
from urllib.parse import quote

import decky

# ── Server address (mirrors paths::plugin_http_port_path in Rust) ────────────

def _http_port_path() -> str:
    home = os.environ.get("HOME") or getattr(decky, "HOME", "") or os.path.expanduser("~")
    return os.path.join(home, ".local", "share", "Spool", "plugin-http-port")


def _read_port() -> Optional[int]:
    """Read the loopback port the headless server published, or None if the
    server isn't running (file absent or unreadable)."""
    try:
        with open(_http_port_path(), "r", encoding="utf-8") as f:
            return int(f.read().strip())
    except (OSError, ValueError):
        return None


def _launcher_path() -> str:
    """Stable AppImage wrapper written by the Rust app on each launch."""
    home = os.environ.get("HOME") or getattr(decky, "HOME", "") or os.path.expanduser("~")
    return os.path.join(home, ".local", "share", "Spool", "spool-launcher.sh")


# ── Settings persistence ──────────────────────────────────────────────────────

SETTINGS_FILE = os.path.join(decky.DECKY_PLUGIN_SETTINGS_DIR, "settings.json")


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


# ── HTTP-over-loopback-TCP client ─────────────────────────────────────────────

def _request_sync(
    method: str,
    path: str,
    body: Optional[dict] = None,
    timeout: float = 30.0,
) -> Optional[dict]:
    """Synchronous HTTP request to the plugin's loopback TCP server.

    Returns the parsed JSON response dict, or None if the server is
    unavailable (no published port) or the response cannot be parsed.
    Intended to be called via `run_in_executor` from async handlers.
    """
    port = _read_port()
    if port is None:
        return None
    try:
        conn = http.client.HTTPConnection("127.0.0.1", port, timeout=timeout)
        headers: dict = {}
        data: Optional[bytes] = None
        if body is not None:
            data = json.dumps(body).encode()
            headers["Content-Type"] = "application/json"
            headers["Content-Length"] = str(len(data))
        try:
            conn.request(method, path, body=data, headers=headers)
            resp = conn.getresponse()
            raw = resp.read()
            return json.loads(raw) if raw else None
        finally:
            conn.close()
    except (OSError, ConnectionRefusedError, http.client.HTTPException,
            json.JSONDecodeError, ValueError):
        return None


async def _spool(
    method: str,
    path: str,
    body: Optional[dict] = None,
    timeout: float = 30.0,
) -> Optional[dict]:
    """Async wrapper: runs the blocking socket request in a thread executor."""
    loop = asyncio.get_event_loop()
    return await loop.run_in_executor(
        None, partial(_request_sync, method, path, body, timeout)
    )


# ── Headless server lifecycle ─────────────────────────────────────────────────

_server_proc: Optional[subprocess.Popen] = None


def _clean_env() -> dict:
    """Return a clean environment for the headless server subprocess.

    Decky Loader ships as a PyInstaller bundle whose bootloader prepends a
    `/tmp/_MEI*` directory to LD_LIBRARY_PATH. Child processes would inherit
    that and load Decky's bundled libs instead of the system's. Restore the
    pre-launch values stashed by PyInstaller (`*_ORIG`) and strip any
    `/tmp/_MEI*` remnants so the server sees the host library path.
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


def _resolve_spool_command(settings: dict) -> Optional[str]:
    configured = (settings or {}).get("spool_command", "").strip()
    if configured and os.path.exists(configured):
        return configured
    launcher = _launcher_path()
    if os.path.exists(launcher):
        return launcher
    for d in os.environ.get("PATH", "").split(os.pathsep):
        if d:
            candidate = os.path.join(d, "spool")
            if os.path.exists(candidate):
                return candidate
    if os.path.exists("/usr/bin/spool"):
        return "/usr/bin/spool"
    return None


def _start_server(settings: dict) -> None:
    global _server_proc
    cmd = _resolve_spool_command(settings)
    if not cmd:
        decky.logger.error("Spool: cannot start --headless-server: spool executable not found")
        return
    try:
        _server_proc = subprocess.Popen(
            [cmd, "--headless-server"],
            start_new_session=True,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            env=_clean_env(),
        )
        decky.logger.info("Spool: started --headless-server (pid %d)", _server_proc.pid)
    except OSError as exc:
        decky.logger.error("Spool: failed to start --headless-server: %s", exc)
        _server_proc = None


def _stop_server() -> None:
    global _server_proc
    if _server_proc is None:
        return
    try:
        _server_proc.terminate()
        _server_proc.wait(timeout=5)
        decky.logger.info("Spool: stopped --headless-server")
    except (ProcessLookupError, subprocess.TimeoutExpired):
        _server_proc.kill()
    finally:
        _server_proc = None
    # Clean up the published port file if the server left it.
    try:
        os.remove(_http_port_path())
    except OSError:
        pass


# ── Plugin class ──────────────────────────────────────────────────────────────

class Plugin:
    async def _main(self):
        decky.logger.info("Spool plugin loaded")
        settings = _load_settings()
        _start_server(settings)

    async def _unload(self):
        decky.logger.info("Spool plugin unloading")
        _stop_server()

    async def _uninstall(self):
        _stop_server()

    # ── Game-stop hook ────────────────────────────────────────────────────────

    async def on_app_stop(self, appid: int) -> dict:
        """Called by the frontend on every game-stop event.

        Forwards the appid to the headless server which applies the same
        should_backup logic previously in backup_logic.py — checks that the
        appid matches the active session and that the session hasn't already
        been backed up, then releases the play lock and runs the backup.
        """
        result = await _spool("POST", "/session/game-stopped", {"appid": appid}, timeout=120.0)
        if result is None:
            decky.logger.warning(
                "Spool: server unavailable for game-stopped (appid %s)", appid
            )
            return {"acted": False, "reason": "server unavailable"}

        notify = _load_settings().get("notify", True)
        if notify and result.get("acted"):
            game = result.get("game", "")
            await decky.emit(
                "spool_backup_finished",
                game,
                bool(result.get("ok")),
                result.get("reason", ""),
            )
        return result

    # ── QAM panel ─────────────────────────────────────────────────────────────

    async def backup_now(self) -> dict:
        result = await _spool("POST", "/session/backup-now", timeout=120.0)
        if result is None:
            return {"acted": False, "ok": False, "reason": "server unavailable"}
        return result

    async def get_status(self) -> dict:
        result = await _spool("GET", "/session", timeout=5.0)
        if result is None:
            return {"hasSession": False}
        return result

    async def get_server_base(self) -> dict:
        """Hand the React UI the loopback base URL so it can fetch `/library`
        and `<img>`-load `/covers/*` directly. `baseUrl` is None when the
        headless server isn't running yet."""
        port = _read_port()
        return {"baseUrl": f"http://127.0.0.1:{port}" if port else None}

    async def delete_game(self, game_id: str) -> dict:
        """Delete a game's install folder from disk and remove its library
        entry. Forwards to the headless server's DELETE /games/<id>, which
        applies the same folder-safety guards as the desktop app."""
        # URL-encode the id segment defensively — ids are UUIDs today, but this
        # avoids any path misrouting should the id ever carry reserved chars.
        path = f"/games/{quote(game_id, safe='')}"
        result = await _spool("DELETE", path, timeout=120.0)
        if result is None:
            return {"ok": False, "reason": "server unavailable"}
        return result

    async def get_settings(self) -> dict:
        s = _load_settings()
        return {
            "spool_command": s.get("spool_command", ""),
            "notify": bool(s.get("notify", True)),
        }

    async def set_settings(self, spool_command: str, notify: bool) -> dict:
        settings = _load_settings()
        settings["spool_command"] = (spool_command or "").strip()
        settings["notify"] = bool(notify)
        _save_settings(settings)
        return settings
