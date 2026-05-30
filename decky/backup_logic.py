"""Pure, runtime-free logic for the Spool Backup plugin.

Kept free of any `decky` import so it can be unit-tested with plain pytest on
any machine (no Steam Deck / Decky runtime required). `main.py` wires these
helpers to Decky's async `Plugin` methods, logging, and `subprocess`.

The plugin's job: on a game-stop event, decide whether Spool was force-killed
before it backed up this session's saves, and if so produce the command to run
a fallback `spool --backup "<game>"`. See
docs/superpowers/specs/2026-05-30-decky-forced-close-backup-design.md.
"""

from __future__ import annotations

import json
import os
import posixpath
from typing import Callable, Optional


def default_session_path(home: str) -> str:
    """Default location of Spool's active-session record on Linux.

    Mirrors `paths::active_session_file()` in the Rust app:
    `$HOME/.local/share/Spool/active-session.json` (dirs::data_local_dir()).
    Uses posix joins — the plugin only ever runs on SteamOS/Linux, so paths
    must use `/` regardless of where the unit tests happen to run.
    """
    return posixpath.join(home, ".local", "share", "Spool", "active-session.json")


def default_launcher_path(home: str) -> str:
    """Stable AppImage launcher wrapper written by the Rust app
    (`paths::appimage_launcher_script()`): forwards args to the current
    AppImage, so `spool-launcher.sh --backup "<game>"` works across updates."""
    return posixpath.join(home, ".local", "share", "Spool", "spool-launcher.sh")


def read_session(path: str) -> Optional[dict]:
    """Parse the active-session record. Returns None if missing or invalid."""
    try:
        with open(path, "r", encoding="utf-8") as f:
            data = json.load(f)
    except (OSError, ValueError):
        return None
    return data if isinstance(data, dict) else None


def should_backup(rec: Optional[dict], appid: int) -> bool:
    """True when the stopped app is the Spool-managed game from `rec` and that
    session has not been backed up yet (Spool was killed before it could).

    - `rec is None`              -> no Spool session -> no-op
    - appid mismatch             -> a non-Spool game stopped -> no-op
    - `backed_up` already true   -> normal quit already handled it -> no-op

    Spool's shortcut appids set the high bit (`crc32 | 0x80000000`), and Steam
    surfaces those as a signed int32 in some code paths, so the *same* id can
    arrive negative. Compare both sides masked to unsigned 32-bit so the sign
    never causes a false mismatch.
    """
    if not isinstance(rec, dict):
        return False
    rec_appid = rec.get("steam_appid")
    if not isinstance(rec_appid, int) or not isinstance(appid, int):
        return False
    if (rec_appid & 0xFFFFFFFF) != (appid & 0xFFFFFFFF):
        return False
    return not bool(rec.get("backed_up", False))


def resolve_spool_command(
    settings: dict,
    home: str,
    exists: Callable[[str], bool] = os.path.exists,
) -> Optional[str]:
    """Resolve the Spool executable to invoke `--backup` on.

    Order (first that exists wins):
      1. explicit `spool_command` setting,
      2. the AppImage launcher wrapper (`spool-launcher.sh`),
      3. a native install on PATH or `/usr/bin/spool`.
    Returns None if nothing resolves.
    """
    configured = (settings or {}).get("spool_command", "").strip()
    if configured and exists(configured):
        return configured

    launcher = default_launcher_path(home)
    if exists(launcher):
        return launcher

    path_env = os.environ.get("PATH", "")
    for d in path_env.split(os.pathsep):
        if d:
            candidate = os.path.join(d, "spool")
            if exists(candidate):
                return candidate

    if exists("/usr/bin/spool"):
        return "/usr/bin/spool"

    return None


def build_backup_argv(spool_command: str, game: str) -> list[str]:
    """The argv for a headless one-shot backup of a single game."""
    return [spool_command, "--backup", game]


def session_path(settings: dict, home: str) -> str:
    """Session-record path: explicit `session_file` setting, else the default."""
    configured = (settings or {}).get("session_file", "").strip()
    return configured if configured else default_session_path(home)
