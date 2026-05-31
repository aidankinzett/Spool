---
title: Ludusavi Config
description: How Spool owns and manages its isolated ludusavi config — backup paths, retention, mapping.yaml structure, redirect rules, and cloud sync.
sidebar:
  order: 4
---

Spool passes `--config ~/.local/share/Spool/ludusavi/` to every ludusavi call. This isolates Spool from the user's personal ludusavi config — Spool fully owns that directory.

## What Spool writes (`ensure_config()`)

`ludusavi_config.rs::ensure_config()` runs at startup and writes a minimal config:

```yaml
manifest:
  enable: true           # needed for game identification in Add Game

backup:
  path: /home/deck/.local/share/Spool/ludusavi-backup
  format:
    chosen: simple       # plain dirs = parseable mapping.yaml for redirect generation
  retention:
    full: 3
    differential: 0

restore:
  path: /home/deck/.local/share/Spool/ludusavi-backup

cloud:
  remote: false          # filled in by Settings → Cloud saves
```

**Fields NOT set by ensure_config:**
- `manifest.secondary` — the secondary manifest URL (not needed for the community manifest)

## Backup folder naming: Windows vs Linux

Windows cannot create folders with colons, so a ludusavi backup for:
- Game name: `"Lego Batman: Legacy of the Dark Knight"`
- Windows folder: `"Lego Batman_ Legacy of the Dark Knight"` (colon → underscore)
- Linux folder: `"Lego Batman: Legacy of the Dark Knight"` (colon allowed)

`redirects.rs` handles this with `windows_safe_name()` — tries the exact name first, then the underscore-escaped version.

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

**Key facts:**
- `os: windows` with `drives: drive-C: "C:"` is the typical Windows backup
- `os: linux` with `drives: drive-0: ""` is the Proton/Linux backup (absolute paths)
- Registry is `hash: ~` (null) in all observed samples — registry saves are uncommon
- Paths use forward slashes throughout (ludusavi normalises)
- Windows username is embedded in the path: `C:/Users/<username>/AppData/...`
- Children (diffs) share the same OS as the parent

## Redirect rules (cross-platform restore)

When restoring a foreign-origin backup (e.g. Windows save onto a Linux/Proton install), `redirects.rs` parses `mapping.yaml`, extracts the Windows username, and generates redirect rules written to `config.yaml` before the second restore. They're cleared after.

```yaml
redirects:
  - kind: restore
    source: C:/Users/akinz
    target: /home/deck/.local/share/Spool/prefixes/88ec0ff3-3196-4c71-9acb-fe10fd19483c/drive_c/users/steamuser
```

One user-prefix rule covers ~93% of real Windows save paths (AppData Local/Roaming/LocalLow, Documents, Saved Games, OneDrive). Additional rules:
- `C:/Users/Public` → `<prefix>/drive_c/users/Public`
- `C:/ProgramData` → `<prefix>/drive_c/ProgramData`
- `G:/Games/<game>` → local `game_folder_path` (install-dir saves)
- Xbox/UWP paths — skipped (don't run under Proton)

## Cloud sync: what Spool controls vs what rclone controls

Spool writes to `config.yaml`:
```yaml
cloud:
  remote: "gdrive:"          # or "b2:bucket:", "webdav:", etc.
  path: Spool/ludusavi-backup
  synchronize: true
apps:
  rclone:
    path: ""                 # resolved to bundled or system rclone
    arguments: "--fast-list --ignore-checksum"
```

Spool does **not** manage rclone authentication — the user needs to run `rclone config` in a terminal, or use the "Open Ludusavi settings" button in Settings → Cloud saves (which opens the ludusavi GUI against Spool's config dir, which can run `rclone config` from within it).

For the self-hosted Spool server's WebDAV storage, `ludusavi.rs::apply_webdav_remote` shells out to `ludusavi cloud set webdav ...` which writes an obscured rclone remote automatically — no manual `rclone config` needed for that path.
