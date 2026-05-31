"""Unit tests for the pure backend logic (no Decky runtime required).

Run: cd decky && python -m pytest tests/   (or: python -m unittest)
"""

import json
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

import backup_logic as logic  # noqa: E402


SAMPLE = {
    "game": "Hades",
    "steam_appid": 2147483649,
    "session_id": "2147483649-1717000000000",
    "started_at": "2026-05-30T12:00:00Z",
    "backed_up": False,
}


class ReadSession(unittest.TestCase):
    def test_reads_valid_record(self):
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "active-session.json")
            with open(p, "w", encoding="utf-8") as f:
                json.dump(SAMPLE, f)
            rec = logic.read_session(p)
            self.assertIsNotNone(rec)
            self.assertEqual(rec["game"], "Hades")

    def test_missing_file_is_none(self):
        self.assertIsNone(logic.read_session("/nope/active-session.json"))

    def test_invalid_json_is_none(self):
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "bad.json")
            with open(p, "w", encoding="utf-8") as f:
                f.write("{not json")
            self.assertIsNone(logic.read_session(p))


class ShouldBackup(unittest.TestCase):
    def test_match_not_backed_up(self):
        self.assertTrue(logic.should_backup(SAMPLE, 2147483649))

    def test_appid_mismatch(self):
        self.assertFalse(logic.should_backup(SAMPLE, 12345))

    def test_matches_signed_int32_appid(self):
        # Steam surfaces high-bit shortcut appids (crc32 | 0x80000000) as a
        # signed int32 in some paths. 2147483649 == -2147483647 as int32 —
        # both encodings must match the unsigned value in the session record.
        self.assertTrue(logic.should_backup(SAMPLE, 2147483649 - (1 << 32)))
        self.assertTrue(logic.should_backup(SAMPLE, 2147483649))

    def test_non_int_appid_is_false(self):
        rec = dict(SAMPLE, steam_appid="2147483649")
        self.assertFalse(logic.should_backup(rec, 2147483649))

    def test_already_backed_up(self):
        rec = dict(SAMPLE, backed_up=True)
        self.assertFalse(logic.should_backup(rec, 2147483649))

    def test_none_record(self):
        self.assertFalse(logic.should_backup(None, 2147483649))


class ResolveSpoolCommand(unittest.TestCase):
    def test_configured_override_wins(self):
        existing = {"/opt/spool/spool"}
        cmd = logic.resolve_spool_command(
            {"spool_command": "/opt/spool/spool"}, "/home/deck",
            exists=lambda p: p in existing,
        )
        self.assertEqual(cmd, "/opt/spool/spool")

    def test_falls_back_to_launcher_script(self):
        launcher = logic.default_launcher_path("/home/deck")
        cmd = logic.resolve_spool_command(
            {}, "/home/deck", exists=lambda p: p == launcher,
        )
        self.assertEqual(cmd, launcher)

    def test_configured_but_missing_falls_through(self):
        launcher = logic.default_launcher_path("/home/deck")
        cmd = logic.resolve_spool_command(
            {"spool_command": "/gone/spool"}, "/home/deck",
            exists=lambda p: p == launcher,
        )
        self.assertEqual(cmd, launcher)

    def test_usr_bin_fallback(self):
        cmd = logic.resolve_spool_command(
            {}, "/home/deck", exists=lambda p: p == "/usr/bin/spool",
        )
        self.assertEqual(cmd, "/usr/bin/spool")

    def test_nothing_resolves(self):
        self.assertIsNone(
            logic.resolve_spool_command({}, "/home/deck", exists=lambda p: False)
        )


class Paths(unittest.TestCase):
    def test_default_session_path(self):
        self.assertEqual(
            logic.default_session_path("/home/deck"),
            "/home/deck/.local/share/Spool/active-session.json",
        )

    def test_session_path_override(self):
        self.assertEqual(
            logic.session_path({"session_file": "/tmp/s.json"}, "/home/deck"),
            "/tmp/s.json",
        )

    def test_build_backup_argv(self):
        self.assertEqual(
            logic.build_backup_argv("/usr/bin/spool", "Hades"),
            ["/usr/bin/spool", "--backup", "Hades"],
        )

    def test_build_release_lock_argv(self):
        self.assertEqual(
            logic.build_release_lock_argv("/usr/bin/spool", "Hades"),
            ["/usr/bin/spool", "--release-lock", "Hades"],
        )


if __name__ == "__main__":
    unittest.main()
