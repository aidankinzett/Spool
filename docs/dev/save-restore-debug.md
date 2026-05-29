# Save Restore Debugging — Lego Batman: Legacy of the Dark Knight

## Status as of 2026-05-29

Phase 3 (redirect generation) is running correctly — the log confirms it:

```
INFO spool_lib::runner: mapping.yaml read … origin_os=Windows path_count=31
INFO spool_lib::runner: foreign-origin backup — running second restore with redirects … redirects=1
```

The redirect rule being generated is:
```yaml
- kind: restore
  source: C:/Users/akinz
  target: /home/deck/.local/share/Spool/prefixes/88ec0ff3-3196-4c71-9acb-fe10fd19483c/drive_c/users/steamuser
```

---

## What restored correctly

These files are in the prefix after the Phase 3 restore:
```
…/76561197960285355/GlobalSaveSlot_TT.sav      ✓
…/76561197960285355/BackupCopy_GlobalSaveSlot_TT.sav  ✓
```

---

## What's missing from the prefix

```
…/76561197960285355/SaveSlot_0_TT.sav           ✗ missing
…/76561197960285355/BackupCopy_SaveSlot_0_TT.sav ✗ missing
…/76561197960287930/   (entire Steam ID folder)  ✗ missing
```

All of these are present in the backup at `~/.local/share/Spool/ludusavi-backup/Lego Batman_ Legacy of the Dark Knight/drive-C/Users/akinz/AppData/Local/Warner Bros.…`.

---

## Things to investigate

### 1. Check the current mapping.yaml origin

```bash
head -20 ~/.local/share/Spool/ludusavi-backup/"Lego Batman_ Legacy of the Dark Knight"/mapping.yaml
```

**Looking for:** Is `os:` still `windows`? Or did the Proton backup (which ran at 08:16:00 and 08:19:52 UTC) replace the Windows mapping.yaml with a new Linux-origin one that only records the prefix files?

If `os: linux` and the paths are now `…/prefixes/88ec0ff3-…/drive_c/users/steamuser/…` — the Proton backup overwrote the Windows backup. See "Retention issue" below.

### 2. Check backup retention in Spool's config

```bash
cat ~/.local/share/Spool/ludusavi/config.yaml | grep -A5 retention
```

Spool's `ensure_config()` does NOT set `backup.retention`, so ludusavi defaults to `full: 1, differential: 0`. If a Proton backup ran and created a new full backup, it would have replaced the Windows-origin backup (only 1 full kept). This would explain why the redirect worked for some files (those that were in the Proton prefix from earlier runs) but not files that were only in the Windows backup.

**Fix:** Add `backup.retention.full: 2` (or more) to `ensure_config()` so the Windows origin backup is retained alongside the Proton backup. Alternatively, the real fix is to get cloud sync working (Phase 4) so the source of truth is the cloud, not the local copy.

### 3. Check what the backup captured from earlier Proton runs

The timeline before Phase 3 was applied:
- `07:04:28` — backup ran (pre-Phase-2, so no `--wine-prefix`; probably backed up nothing or a different path)
- `07:39:08` — backup ran (Phase 2 was in effect; had `--wine-prefix`; captured prefix files)
- `08:07:01` — backup ran (same)
- `08:16:00` — backup ran (same)

Check timestamps on files in the backup dir:
```bash
ls -la ~/.local/share/Spool/ludusavi-backup/"Lego Batman_ Legacy of the Dark Knight"/
```

If any dirs/files are newer than `18:15` (when we copied from `~/ludusavi-backup/`), those were created by a Proton backup run and likely replaced some of the Windows content.

### 4. Check current config.yaml redirects section

After a launch with Phase 3, the redirects should be cleared (we clear them after the second restore). Verify:

```bash
grep -A10 'redirects' ~/.local/share/Spool/ludusavi/config.yaml
```

Should be `redirects: []` after a session completes.

### 5. Directly verify the redirect works with the right backup

If the mapping.yaml has been overwritten by the Proton backup, you can test by manually restoring from the Windows-origin copy:

```bash
# Make sure mapping.yaml is still Windows-origin
head -5 ~/.local/share/Spool/ludusavi-backup/"Lego Batman_ Legacy of the Dark Knight"/mapping.yaml

# Manually set the redirect in Spool's config.yaml:
# Add under redirects:
#   - kind: restore
#     source: C:/Users/akinz
#     target: /home/deck/.local/share/Spool/prefixes/88ec0ff3-3196-4c71-9acb-fe10fd19483c/drive_c/users/steamuser

# Then run ludusavi directly:
ludusavi --config ~/.local/share/Spool/ludusavi \
  restore --api --force "Lego Batman: Legacy of the Dark Knight"

# Check what landed:
find ~/.local/share/Spool/prefixes/88ec0ff3-3196-4c71-9acb-fe10fd19483c/drive_c/users/steamuser \
  -name "*.sav"
```

---

## Root cause hypotheses (most likely first)

1. **Backup retention replaced the Windows backup.** `full: 1` means each Proton backup session replaced the previous backup. After several Proton sessions before Phase 3 was active, the Windows-origin mapping.yaml + drive-C files were replaced by a Proton-origin backup that only captured whatever was in the prefix at that time (GlobalSaveSlot only, because SaveSlot_0 had never been restored there yet). Fix: set `retention.full: 2+` in `ensure_config()` or just use cloud sync as the authoritative source.

2. **The game is running under a Steam ID that doesn't match either backed-up ID.** Proton/umu uses a synthetic Steam ID for non-Steam games. If the game looks for saves under a third Steam ID (different from `76561197960285355` or `76561197960287930`), it won't find the restored saves even if they're in the right place.

3. **The redirect only partially applied** — maybe a ludusavi version edge case with the `--wine-prefix` + redirects combination. Less likely since the redirect rule and path structure are confirmed correct.

---

## Fix plan

### Immediate (in `ensure_config()`)

Add retention to keep more full backups:

```rust
// In ludusavi_config.rs ensure_config():
set_path(&mut v, &["backup", "retention", "full"], Value::Number(3.into()));
set_path(&mut v, &["backup", "retention", "differential"], Value::Number(0.into()));
```

### Proper fix (Phase 4)

Set up cloud sync so the Windows-origin backup is the canonical source on the cloud remote, not the local disk. The local backup then gets replaced by a cloud pull on each restore, making retention irrelevant.

---

## Useful log patterns

When investigating, tail the log and look for:

```bash
tail -f ~/.local/share/Spool/debug.log | grep -E 'restore|redirect|mapping|foreign|origin|backup|error' -i
```

Key lines to look for:
- `mapping.yaml read … origin_os=Windows` — found a Windows backup, redirect will be generated
- `mapping.yaml read … origin_os=Linux` — found a Proton backup, no redirect (same OS)
- `no mapping.yaml found` — backup dir is empty or game name mismatch
- `same-origin backup — no redirects needed` — both sides Linux, no redirect needed
- `foreign-origin backup — running second restore` — redirect generated and applied
