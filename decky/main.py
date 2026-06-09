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
import socket
import subprocess
import sys
import time
from functools import partial
from typing import Optional
from urllib.parse import quote

import decky

# ── Server address (mirrors paths::plugin_http_port_path in Rust) ────────────

def _http_port_path() -> str:
    home = os.environ.get("HOME") or getattr(decky, "HOME", "") or os.path.expanduser("~")
    return os.path.join(home, ".local", "share", "Spool", "plugin-http-port")


def _active_session_path() -> str:
    home = os.environ.get("HOME") or getattr(decky, "HOME", "") or os.path.expanduser("~")
    return os.path.join(home, ".local", "share", "Spool", "active-session.json")


def _read_port() -> Optional[int]:
    """Read the loopback port the headless server published, or None if the
    server isn't running (file absent or unreadable)."""
    try:
        with open(_http_port_path(), "r", encoding="utf-8") as f:
            return int(f.read().strip())
    except (OSError, ValueError):
        return None


def _ping(port: int, timeout: float = 1.0) -> bool:
    """True if a healthy headless server answers GET /status on `port`. Used as
    a liveness probe: the port file can outlive the server (forced close in Game
    Mode, crash), so its presence alone doesn't mean the server is reachable."""
    try:
        conn = http.client.HTTPConnection("127.0.0.1", port, timeout=timeout)
        try:
            conn.request("GET", "/status")
            resp = conn.getresponse()
            resp.read()
            return resp.status == 200
        finally:
            conn.close()
    except (OSError, http.client.HTTPException):
        return False


def _ensure_server(wait_s: float = 12.0) -> Optional[int]:
    """Return a port for a live, reachable headless server — starting it if
    necessary. The plugin starts the server once at load, but it can still be
    coming up (AppImage mount + bind takes a moment) or have died since. Rather
    than report "server unavailable" and give up, (re)start it on demand and
    wait for it to publish a port and answer /status. Returns None only if it
    never becomes reachable within `wait_s`."""
    port = _read_port()
    if port is not None and _ping(port):
        return port

    global _server_proc
    # If a server we launched is still alive, it's likely just starting up —
    # wait for it rather than spawning a second one (which would bind an
    # ephemeral port and orphan the first). Otherwise (re)start.
    starting = _server_proc is not None and _server_proc.poll() is None
    if not starting:
        _start_server(_load_settings())

    deadline = time.monotonic() + wait_s
    while time.monotonic() < deadline:
        port = _read_port()
        if port is not None and _ping(port):
            return port
        time.sleep(0.25)
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

def _do_request(
    port: int,
    method: str,
    path: str,
    body: Optional[dict],
    timeout: float,
) -> Optional[dict]:
    """One HTTP attempt against the loopback server. Raises `socket.timeout`
    when the server accepted the request but didn't respond in time (it's still
    working — NOT safe to retry a non-idempotent POST), and other transport
    errors (refused / dropped) when the server could not have acted (safe to
    retry). Returns the parsed JSON dict on a completed response, or None for an
    empty body."""
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


def _request_sync(
    method: str,
    path: str,
    body: Optional[dict] = None,
    timeout: float = 30.0,
) -> Optional[dict]:
    """Synchronous HTTP request to the plugin's loopback TCP server.

    Returns the parsed JSON response dict, or None if the server cannot be
    reached even after a (re)start attempt, or the response can't be parsed.
    Intended to be called via `run_in_executor` from async handlers.

    The transport is resilient: if the first attempt fails at the connection
    level (the server is still coming up, has died, or the published port is
    stale), we (re)start the server, wait for it to become reachable, and retry
    once. That failure happens before the server acts, so retrying is safe even
    for POSTs.

    A read timeout is different: on loopback the connection was accepted, so the
    server received the request and is still working (e.g. a backup waiting on
    the machine-wide lock for up to 180s). Retrying would fire a SECOND backup,
    so we do NOT retry — we return None and let the server finish on its own.
    Game-stop/backup callers use a timeout above the server's lock wait so a
    legitimately-slow backup completes within the single attempt.
    """
    port = _read_port()
    if port is not None:
        try:
            return _do_request(port, method, path, body, timeout)
        except socket.timeout:
            decky.logger.warning(
                "Spool: %s timed out; server still working, not retrying", path
            )
            return None
        except (OSError, http.client.HTTPException,
                json.JSONDecodeError, ValueError):
            pass  # connection-level failure — fall through to ensure-server + retry

    port = _ensure_server()
    if port is None:
        return None
    try:
        return _do_request(port, method, path, body, timeout)
    except (OSError, http.client.HTTPException,
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
        # Fast local file check: only proceed if the stopped app matches the active,
        # un-backed-up Spool session. This prevents server startup and UI event churn
        # for unrelated games.
        try:
            with open(_active_session_path(), "r", encoding="utf-8") as f:
                rec = json.load(f)
            if rec.get("backed_up") or rec.get("steam_appid") != appid:
                return {"acted": False}
        except Exception:
            return {"acted": False}

        # Tell the UI a backup may be starting so the game-page badge can show a
        # spinner. The frontend debounces this: the common no-op game-stop
        # (Spool's own runner already backed up) resolves fast enough that the
        # spinner never appears — only a real forced-close backup, which takes
        # seconds, crosses the debounce threshold.
        await decky.emit("spool_backup_started", appid)

        # Timeout exceeds the server's 180s backup-lock wait so a backup blocked
        # behind another Spool process still completes within this one attempt
        # (a read timeout is not retried — see `_request_sync`). (#8)
        result = await _spool("POST", "/session/game-stopped", {"appid": appid}, timeout=240.0)
        if result is None:
            decky.logger.warning(
                "Spool: server unavailable for game-stopped (appid %s)", appid
            )
            await decky.emit("spool_backup_finished", appid, False, False, "", "server unavailable")
            return {"acted": False, "reason": "server unavailable"}

        acted = bool(result.get("acted"))
        ok = bool(result.get("ok"))
        game = result.get("game", "")
        reason = result.get("reason", "")

        # Always signal completion so the badge can drop the spinner and refresh,
        # independent of the notify preference (which only governs the toast).
        await decky.emit("spool_backup_finished", appid, acted, ok, game, reason)

        if _load_settings().get("notify", True) and acted:
            await decky.emit("spool_backup_toast", game, ok, reason)

        return result

    # ── QAM panel ─────────────────────────────────────────────────────────────

    async def backup_now(self) -> dict:
        # Also waits on the server's backup lock — match game-stopped's timeout. (#8)
        result = await _spool("POST", "/session/backup-now", timeout=240.0)
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
        and `<img>`-load `/covers/*` directly. This is the first call the QAM
        panel makes, so ensure the server is up (start/await it if needed)
        rather than handing back a dead URL. `baseUrl` is None only if it can't
        be brought up."""
        loop = asyncio.get_event_loop()
        port = await loop.run_in_executor(None, _ensure_server)
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

    async def uninstall_game(self, game_id: str) -> dict:
        """Delete a game's install folder from disk but KEEP its library entry
        (playtime / art / save backups survive; re-adding reuses the entry).
        Forwards to the headless server's POST /games/<id>/uninstall."""
        path = f"/games/{quote(game_id, safe='')}/uninstall"
        result = await _spool("POST", path, timeout=120.0)
        if result is None:
            return {"ok": False, "reason": "server unavailable"}
        return result

    async def forget_game(self, game_id: str) -> dict:
        """Forget a game's library entry but leave its files on disk. Forwards
        to the headless server's POST /games/<id>/forget."""
        path = f"/games/{quote(game_id, safe='')}/forget"
        result = await _spool("POST", path, timeout=120.0)
        if result is None:
            return {"ok": False, "reason": "server unavailable"}
        return result

    async def pull_cloud_saves(self, game_id: str) -> dict:
        """Pull a game's latest cloud saves down to this device and restore them
        to disk, without launching ("Sync now"). Forwards to the headless
        server's POST /games/<id>/pull, which runs the same pull-only sync as
        the desktop app — never uploads. Can take a few seconds (rclone pull +
        restore), so the timeout is generous."""
        path = f"/games/{quote(game_id, safe='')}/pull"
        result = await _spool("POST", path, timeout=120.0)
        if result is None:
            return {"ok": False, "reason": "server unavailable"}
        return result

    async def install_deps(self, game_id: str, verbs: str) -> dict:
        """Install Windows runtime deps (winetricks verbs, e.g. "vcrun2022")
        into a game's Proton prefix so the user doesn't need desktop mode.
        Forwards to the headless server's POST /games/<id>/install-deps, which
        runs `umu-run winetricks -q <verbs>` (needs UMU/GE Proton). This can
        take minutes — downloads + installs into the prefix — so the timeout is
        generous and the UI shows a blocking spinner meanwhile."""
        path = f"/games/{quote(game_id, safe='')}/install-deps"
        result = await _spool("POST", path, {"verbs": verbs}, timeout=900.0)
        if result is None:
            return {"ok": False, "reason": "server unavailable"}
        return result

    async def list_proton_versions(self) -> list:
        """List the Proton builds installed on this machine (newest-first) for
        the per-game Proton picker. Forwards to GET /proton-versions. Returns an
        empty list if the server is unavailable or no Proton is installed."""
        result = await _spool("GET", "/proton-versions", timeout=10.0)
        return result if isinstance(result, list) else []

    async def set_proton_version(self, game_id: str, proton_version_path: str) -> dict:
        """Pin a game's Proton version (empty string = auto / clear override).
        Forwards to POST /games/<id>/proton, which updates the library entry the
        same way the desktop edit page does."""
        path = f"/games/{quote(game_id, safe='')}/proton"
        body = {"proton_version_path": proton_version_path or ""}
        result = await _spool("POST", path, body, timeout=30.0)
        if result is None:
            return {"ok": False, "reason": "server unavailable"}
        return result

    async def list_save_revisions(self, game_id: str) -> dict:
        """List the save revisions ludusavi retains locally for a game (newest-
        first, tip flagged) for the "restore an earlier save" picker. Forwards
        to GET /games/<id>/revisions."""
        path = f"/games/{quote(game_id, safe='')}/revisions"
        result = await _spool("GET", path, timeout=15.0)
        if result is None:
            return {"ok": False, "reason": "server unavailable"}
        return result

    async def restore_save_revision(self, game_id: str, backup_name: str) -> dict:
        """Roll a game back to an earlier save revision and pin it as the new
        tip (restore + cloud-synced backup). Forwards to POST /games/<id>/restore.
        Runs restore + backup + upload, so the timeout is generous and the UI
        shows a spinner meanwhile."""
        path = f"/games/{quote(game_id, safe='')}/restore"
        body = {"backup_name": backup_name}
        result = await _spool("POST", path, body, timeout=120.0)
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
