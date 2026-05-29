# Spool-owned ludusavi config — Notes & Gotchas

## Location

`~/.local/share/Spool/ludusavi/config.yaml`

Spool passes `--config ~/.local/share/Spool/ludusavi/` to every ludusavi call. This isolates Spool from the user's personal ludusavi config.

---

## What Spool writes (via `ensure_config()`)

```yaml
manifest:
  enable: true           # needed for game identification in Add Game

backup:
  path: /home/deck/.local/share/Spool/ludusavi-backup
  format:
    chosen: simple       # plain dirs = parseable mapping.yaml for redirect generation

restore:
  path: /home/deck/.local/share/Spool/ludusavi-backup

cloud:
  remote: false          # Phase 4 fills this in
```

**What's NOT set by ensure_config (potential issues):**
- `backup.retention` — defaults to `full: 1, differential: 0`. With multiple Proton sessions, each backup replaces the previous. If a Windows-origin backup is on disk and a Proton session backs up, the Windows backup gets replaced.
- `manifest.secondary` — the secondary manifest URL (if needed for community manifest)

---

## Retention issue

**Problem:** `full: 1` means only one full backup is kept. If the user:
1. Copies Windows backups into Spool's backup dir manually (as a workaround before Phase 4)
2. Plays the game via Proton (which triggers a backup with `--wine-prefix`)

The Proton backup replaces the Windows backup. The next restore no longer has the Windows save to redirect.

**Fix in ensure_config():**
```rust
set_path(&mut v, &["backup", "retention", "full"], Value::Number(3.into()));
```

Or just set `full: 2` to keep Windows + latest Proton.

---

## Backup folder naming: Windows vs Linux

Windows cannot create folders with colons, so a ludusavi backup for:
- Game name: `"Lego Batman: Legacy of the Dark Knight"`
- Windows folder: `"Lego Batman_ Legacy of the Dark Knight"` (colon → underscore)
- Linux folder: `"Lego Batman: Legacy of the Dark Knight"` (colon allowed)

`redirects.rs` handles this with `windows_safe_name()` — tries the exact name first, then the underscore-escaped version.

---

## mapping.yaml structure

```yaml
name: "Game Name"
drives:
  drive-C: "C:"      # Windows. drive-0: "" on Linux.
backups:
  - name: "."        # Base (full) backup
    os: windows      # or linux
    when: "2026-05-23T..."
    files:
      "C:/Users/akinz/AppData/...":
        hash: abc123
        size: 1234
    registry:
      hash: ~        # null in all observed backups — registry saves are rare
    children:        # Differential backups
      - name: "backup-20260524T...Z-diff"
        os: windows
        files:
          "C:/Users/akinz/AppData/...":  # only changed files
            hash: def456
            size: 2345
        registry: ~
```

**Key facts from 23 real backups:**
- `os: windows` with `drives: drive-C: "C:"` is the typical Windows backup
- `os: linux` with `drives: drive-0: ""` is the Proton/Linux backup (absolute paths)
- Registry is `hash: ~` (null) in all observed samples — registry saves are uncommon
- Paths use forward slashes throughout (ludusavi normalises)
- Windows username is embedded in the path: `C:/Users/<username>/AppData/...`
- Children (diffs) share the same OS as the parent

---

## Redirect rules (Phase 3)

Written to `config.yaml` before the second restore, cleared after. Format:

```yaml
redirects:
  - kind: restore
    source: C:/Users/akinz
    target: /home/deck/.local/share/Spool/prefixes/88ec0ff3-3196-4c71-9acb-fe10fd19483c/drive_c/users/steamuser
```

The `source` prefix covers all of AppData (Local, Roaming, LocalLow), Documents, Saved Games, OneDrive under that user — one rule covers ~93% of real Windows save paths.

Additional rules for edge cases:
- `C:/Users/Public` → `<prefix>/drive_c/users/Public`
- `C:/ProgramData` → `<prefix>/drive_c/ProgramData`
- `G:/Games/<game>` → local `game_folder_path` (install-dir saves)
- Xbox/UWP paths (`C:/XboxGames/...`, `Packages/*/wgs`) — skipped (don't run under Proton)

---

## Cloud sync: what Spool controls vs what rclone controls

Spool writes to `config.yaml`:
```yaml
cloud:
  remote: "gdrive:"          # or "b2:bucket:", "webdav:", etc.
  path: Spool/ludusavi-backup
  synchronize: true
apps:
  rclone:
    path: ""                 # "" = ludusavi finds system rclone
    arguments: "--fast-list --ignore-checksum"
```

Spool does NOT manage rclone authentication — the user needs to run `rclone config` in a terminal first to set up the remote, or use the "Open Ludusavi settings" button to access the ludusavi GUI which can run `rclone config` from within it.
