---
title: Save Path Mapping
description: How Spool moves a single save game between Windows PCs, Linux handhelds, and Proton/Wine prefixes — the format-based redirect strategy, the dual-pass restore, backup canonicalisation, and cloud conflict detection.
sidebar:
  order: 5
---

A save file written on a Windows desktop lives at `C:/Users/akinz/AppData/Local/Deltarune/`.
The *same* save, restored onto a Steam Deck, has to land inside a Proton prefix at
`~/.local/share/Spool/prefixes/<id>/drive_c/users/steamuser/AppData/Local/Deltarune/`.
Different drive letter, different username, different filesystem layout — but it has to be
the byte-for-byte same save, and it has to survive being moved *back* to Windows later,
and back to Linux again after that.

This page explains how Spool makes that work. The core logic lives in
[`redirects.rs`](https://github.com/aidankinzett/Spool/blob/master/tauri/src-tauri/src/redirects.rs),
driven by the [`runner.rs`](https://github.com/aidankinzett/Spool/blob/master/tauri/src-tauri/src/runner.rs)
workflow. For how Spool owns ludusavi's config, see [Ludusavi Config](/Spool/architecture/ludusavi/).

## The problem

Spool delegates the actual save scanning, backup, and restore to
[ludusavi](https://github.com/mtkennerly/ludusavi). Ludusavi records every save file by its
**absolute path** and, on restore, writes each file faithfully back to that recorded path. On a
single machine that's exactly the behaviour you want. Moving a save *between* machines just adds
one more question on top: where should each recorded path land on the new machine?

- The recorded path `C:/Users/akinz/...` doesn't exist on a Linux handheld.
- The Windows username (`akinz`) won't match the Linux user.
- Under Proton, the game expects its save *inside* its own Wine prefix, not at a literal
  `C:/` path on the Linux root.

Ludusavi's escape hatch is a **redirect**: a `{source → target}` rule that says "when you'd
restore to *source*, write to *target* instead." Spool's job is to read what a backup actually
contains, figure out where each path *should* land on the current machine, and synthesise the
right redirects automatically — for any combination of source and destination OS.

## Classify by path *format*, not by OS

Every ludusavi backup is stamped with the OS that authored it (`os: windows` or `os: linux` in
`mapping.yaml`). The obvious approach would be to branch on that field. **Spool deliberately does
not.**

The reason is round-tripping. When Spool restores a Windows save onto Linux, plays it, and backs
it up again, it *canonicalises* the new backup so its paths stay as `C:/...` (more on this below).
The result is a backup that is tagged `os: linux` but contains `C:/...` paths. If redirect logic
keyed off the `os` field, the **second** time you played that game on Linux it would read
`os: linux`, assume the paths are already native, generate no redirect, and dump the save in the
wrong place.

So instead, Spool ignores the `os` stamp for decision-making and classifies each individual
*path* by its literal shape:

| Format | Looks like | Detected by |
|--------|-----------|-------------|
| **Windows** | `C:/Users/...`, `G:/Games/...` | starts with a drive letter + `:` |
| **Wine prefix** | `.../prefixes/<id>/drive_c/users/...` | contains a `/drive_c/` segment |
| **Native Linux** | `/home/deck/.local/share/...` | absolute Linux path, no `/drive_c/` |

```rust
fn classify_format(path: &str) -> PathFormat {
    let p = path.replace('\\', "/");
    // A prefix path is a Linux absolute path that happens to contain /drive_c/,
    // so it's checked before the Windows drive-letter test.
    if let Some(idx) = p.find("/drive_c/") {
        return PathFormat::WinePrefix { drive_c: p[..idx + "/drive_c".len()].to_string() };
    }
    if let Some(stripped) = p.strip_suffix("/drive_c") {
        return PathFormat::WinePrefix { drive_c: format!("{stripped}/drive_c") };
    }
    if is_windows_drive_path(&p) {
        return PathFormat::Windows;
    }
    PathFormat::NativeLinux
}
```

Each format is then reconciled against the **current** machine (Windows or Linux) and, on Linux,
the game's Proton prefix. This stays correct no matter how many times a save hops between OSes.

## The mapping rules

The full matrix of what happens to each path format on each destination OS:

| Save path in backup | Restoring on **Windows** | Restoring on **Linux (Proton)** |
|---------------------|--------------------------|----------------------------------|
| `C:/Users/<user>/...` | native — no redirect | → `<prefix>/drive_c/users/steamuser` |
| `C:/Users/Public/...` | native — no redirect | → `<prefix>/drive_c/users/Public` |
| `C:/ProgramData/...` | native — no redirect | → `<prefix>/drive_c/ProgramData` |
| `G:/Games/<game>/...` (install-dir save) | native — no redirect | → local `game_folder_path` (best-effort) |
| `C:/XboxGames/...`, UWP `Packages/...wgs` | skipped | skipped (don't run under Proton) |
| `<prefix>/drive_c/users/steamuser/...` | → `C:/Users/<local user>` | same machine: no-op; other machine: remap prefix root |
| `<prefix>/drive_c/users/Public/...` | → `C:/Users/Public` | same machine: no-op; other machine: remap prefix root |
| `<prefix>/drive_c/ProgramData/...` | → `C:/ProgramData` | same machine: no-op; other machine: remap prefix root |
| `<prefix>/drive_c/Program Files/...` | skipped (can't safely map) | same machine: no-op; other machine: remap prefix root |
| `/home/.../SomeGame/...` (native Linux) | skipped (no equivalent) | native — no redirect |

A few things worth calling out:

- **One rule covers almost everything.** `C:/Users/<user>` → `drive_c/users/steamuser` catches
  AppData (Local / Roaming / LocalLow), Documents, Saved Games, and OneDrive in a single redirect —
  roughly 93% of real-world Windows save locations. `Public` and `ProgramData` get their own rules
  because Proton maps them to fixed, username-independent locations.

- **The Windows username is read from the backup itself.** Spool scans the backup's paths for the
  first `C:/Users/<name>/...` and uses `<name>` as the source root — it never has to be told what
  the Windows user was called.

  ```rust
  fn windows_username_from_paths(paths: &[String]) -> Option<String> {
      for p in paths {
          if let Some(rest) = p.strip_prefix("C:/Users/") {
              let name = rest.split('/').next()?;
              if !name.is_empty() && name != "Public" && name != "Default" && name != "All Users" {
                  return Some(name.to_string());
              }
          }
      }
      None
  }
  ```

- **Install-dir saves need a hint.** Some games save next to their `.exe` (e.g.
  `G:/Games/ULTRAKILL/Saves/`). Spool can't guess where that game lives on the new machine, so it
  redirects the install root onto the `game_folder_path` you set when adding the game. If that
  isn't set, the redirect is skipped (and logged) rather than guessed wrong.

- **When in doubt, skip.** Xbox/UWP container saves, `Program Files` installs, and native Linux
  paths with no Windows equivalent are deliberately *not* redirected. A skipped redirect leaves
  the save where ludusavi would naturally put it — a wrong redirect could silently corrupt a save
  by writing it somewhere the game never reads.

## The dual-pass restore

Redirects have to be in ludusavi's config *before* the restore that uses them, but Spool can't
know what redirects a backup needs until it has read the backup's `mapping.yaml` — which only
exists on disk after a cloud-syncing restore has pulled it down. The solution is to restore
**twice**:

```
Pass 1  ── restore (pulls the backup + mapping.yaml from cloud)
        │
        ├─ read mapping.yaml → discover every recorded save path
        ├─ classify each path by format, derive redirect rules
        │
        ├─ 0 rules?  → save is already native here. Clear stale redirects, done.
        │
        └─ N rules?  → write them to ludusavi's config.yaml
                       │
Pass 2  ──────────────┴─ restore again — this time the saves land in the right place
        │
        └─ clear the redirects (regenerated fresh on every restore)
```

This is `restore_with_redirects` in `runner.rs`. The second pass only runs when the first pass
revealed a foreign-origin backup; a same-machine restore costs just the one pass. Redirects are
cleared immediately afterward so they never leak into an unrelated backup or restore.

```rust
// ── Pass 1: restore (pulls cloud unless rolling back to a specific id) ──
let first = do_restore!()?;

let Some(origin) = redirects::read_backup_origin(&backup_dir, game_name) else {
    // No backup on disk yet (first-ever session). Nothing to redirect.
    return Ok(first);
};

let n = redirects::apply_redirects_for_restore(&origin, prefix_root, game_folder, local_win_user)?;
if n == 0 {
    let _ = ludusavi_config::set_redirects(&[]); // clear any stale ones
    return Ok(first);
}

// ── Pass 2: restore with redirects in place ──
let second = do_restore!()?;
let _ = ludusavi_config::set_redirects(&[]);
Ok(second)
```

## Backup canonicalisation — keeping the save portable

There's a subtle trap on the way back out. Say you restored a Windows save onto a Proton prefix and
played it. When Spool backs up afterward, ludusavi scans the prefix and would record the *prefix*
path (`.../drive_c/users/steamuser/...`). The backup would silently flip from Windows-format paths to
Linux-prefix paths — and a future restore onto a real Windows PC would have no idea how to place it.

To prevent that drift, the post-session backup runs with **inverted** redirects (`kind: "backup"`,
which maps *scanned* path → *stored* path — the opposite direction of a restore redirect). Spool
re-derives the same restore rules, flips source and target, and re-tags them so the save is *stored*
with its original canonical `C:/...` paths even though it was scanned out of a Linux prefix.

```rust
fn invert_for_backup(restore_rules: Vec<Redirect>) -> Vec<Redirect> {
    restore_rules
        .into_iter()
        // Only the cross-OS rules (restore source is a Windows X:/… path).
        .filter(|r| is_windows_drive_path(&r.source))
        .map(|r| Redirect {
            kind: "backup".to_string(),
            source: r.target, // scanned: the local prefix path
            target: r.source, // stored:  the canonical C:/… path
        })
        .collect()
}
```

Only the **Windows-origin** rules are inverted. Linux↔Linux prefix-root remaps are dropped here on
purpose, so a genuinely native Linux backup keeps its own real paths instead of being forced into a
fake Windows shape. The net effect: a Windows save stays Windows-shaped in every backup, no matter
how many Linux play sessions it goes through — which is exactly what makes the format-based
classification (rather than the `os` field) necessary.

## Cross-device Linux: prefix-root remapping

Two Linux handhelds sharing a save via cloud have *different* home directories, so their prefix roots
differ (`/home/alice/.../prefixes/3f9a…` vs `/home/bob/.../prefixes/7c21…`). A prefix-format path is
remapped only when the authoring prefix root differs from the local one:

```rust
// Both Linux: only remap when the authoring prefix root differs from this
// machine's. Same machine + game_id ⇒ identical root ⇒ no redirect.
if &drive_c != local_drive_c {
    rules.insert((drive_c.clone(), local_drive_c.clone()));
}
```

Because every game gets a deterministic prefix path keyed on its `game_id`
(`~/.local/share/Spool/prefixes/<game_id>/`), the same game on the same machine always resolves to
the same prefix — so the common case (replaying your own save) generates zero redirects.

Note that `game_id` is a random UUID minted locally when the game is added to the library (and a
fresh one is minted again on the receiving device for a LAN install), so it is *not* stable across
devices — alice's and bob's copies of the same game have different ids as well as different home
dirs. That's why the remap swaps the entire `…/drive_c` prefix root in one rule rather than just the
home-directory portion. The cross-device match that decides two saves belong to the same game is by
**game name** (ludusavi's backup folder is named after the game), not the `game_id`.

## How it fits the run workflow

The mapping is part of `runner.rs`'s five-phase state machine
(`restoring → launching → playing → backing-up → done`):

1. **restoring** — `restore_with_redirects` runs the dual-pass restore, steering the save onto this
   machine.
2. **launching / playing** — the game runs (under Proton on Linux), reading the save from the now-correct location.
3. **backing-up** — Spool re-derives the inverted `kind: "backup"` redirects and backs up, canonicalising the paths back out.

## Cloud conflict detection

Mapping decides *where* a save lands; cloud sync decides *which* save to use when more than one
device has been playing. Spool tracks a per-game **baseline** — the backup "tip" (the most recent
backup's unique name) that this device last synced. On each restore it compares three points: the
baseline, the local tip, and the cloud tip.

```rust
enum CloudSyncDecision {
    InSync,              // local == cloud, nothing to do
    FastForwardDownload, // only cloud moved → pull it silently
    FastForwardUpload,   // only local moved → push it silently
    Diverged,            // both moved since the baseline → conflict
}
```

The logic mirrors a three-way merge: if only one side advanced from the baseline, it's a safe
fast-forward in that direction. If **both** sides advanced, the saves have genuinely diverged and
Spool can't pick a winner — it raises a conflict that the frontend's `CloudConflictModal` asks you
to resolve (keep local, or keep cloud). Resolving it mirrors the chosen side, restores it through
the same dual-pass redirect machinery, and advances the baseline so you aren't asked again about the
state you just settled.

The tip name works as a cross-device content fingerprint because ludusavi mirrors `mapping.yaml`
verbatim on every cloud sync — so the same save state carries the same tip name on every device,
independent of OS, drive letters, or prefix paths.

## Worked example: Windows → Deck → Windows

1. **On Windows**, you play *Deltarune*. Backup records `C:/Users/akinz/AppData/Local/DELTARUNE/`,
   tagged `os: windows`, pushed to cloud.
2. **On the Steam Deck**, you launch it. Pass 1 pulls the backup; Spool sees a Windows-format path
   and derives `C:/Users/akinz → <prefix>/drive_c/users/steamuser`. Pass 2 lands the save inside the
   Proton prefix. The game runs under Proton and reads its save normally.
3. **You finish playing on the Deck.** The backup runs with the inverted rule
   `<prefix>/drive_c/users/steamuser → C:/Users/akinz`, so the new backup still stores
   `C:/Users/akinz/...` paths (even though it's tagged `os: linux`). Pushed to cloud.
4. **Back on Windows**, you launch it. The cloud tip is ahead of your baseline → fast-forward
   download. The paths are already `C:/...` and you're on Windows → zero redirects, single-pass
   restore, save lands natively. Your Deck progress is right there.

At no point did anyone tell Spool what OS the backup came from or what the Windows username was — it
read both out of the paths.
