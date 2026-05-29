# Phase 4 — Cloud/rclone Settings UI

## Goal

Let the user configure the rclone cloud remote inside Spool so `--cloud-sync` in ludusavi's backup/restore calls actually pushes/pulls save bytes to/from a real cloud destination. Until Phase 4 is done, `--cloud-sync` is a no-op (the owned config has `cloud.remote: false`).

---

## What needs to happen

### Backend — new `ConfigData` fields

In `tauri/src-tauri/src/config.rs`:

```rust
pub cloud_remote: String,   // rclone remote name e.g. "gdrive:" — "" = unset
pub cloud_path: String,     // remote subpath, default "Spool/ludusavi-backup"
pub rclone_path: String,    // path to rclone binary; "" = let ludusavi find it
pub rclone_args: String,    // default "--fast-list --ignore-checksum"
```

Mirror in `tauri/src/lib/types.ts` `ConfigData`.

### Backend — push to owned config.yaml on save

In `config.rs` `update_config()` (line ~257), after saving Spool's config JSON, call:

```rust
let _ = crate::ludusavi_config::set_cloud(
    Some(&cfg.data.cloud_remote),
    Some(&cfg.data.cloud_path),
    Some(&cfg.data.rclone_path),
    Some(&cfg.data.rclone_args),
);
```

`set_cloud()` already exists in `ludusavi_config.rs` — just needs to be wired.

### Backend — default values

```rust
cloud_remote: String::new(),
cloud_path: "Spool/ludusavi-backup".to_string(),
rclone_path: String::new(),
rclone_args: "--fast-list --ignore-checksum".to_string(),
```

### Frontend — new settings card

In `tauri/src/routes/settings/+page.svelte`, add a "Cloud saves (rclone)" card in the Library group, after the Compatibility card. Add `{ id: 'cloud-saves', title: 'Cloud saves', sub: 'rclone remote' }` to `NAV_GROUPS`.

Fields:
- **Remote** — `TextField` bound to `config.cloud_remote` (e.g. `gdrive:`, `b2:mybucket:`, `webdav:`)
- **Remote path** — `TextField` bound to `config.cloud_path`
- **rclone binary** — `TextField` + Browse bound to `config.rclone_path`
- **rclone args** — `TextField` bound to `config.rclone_args`
- **"Open Ludusavi settings"** button → `api.openLudusaviGui()` (already opens against Spool's config dir)

Helper text: `"Configure a cloud remote here, then use 'Open Ludusavi settings' to run rclone config / authenticate."`

All fields: `oncommit={persist}`.

---

## Note for existing users

When Phase 2 landed, Spool started using its own blank config instead of the user's personal ludusavi config. Any cloud remote the user had configured in their personal ludusavi config is NOT automatically carried over. Phase 4 is the first opportunity for the user to reconfigure it in Spool's UI.

Suggest surfacing a nudge in the UI when `cloud_remote` is empty: "Cloud sync is not configured — saves are backed up locally only."

---

## Verification

1. Enter an rclone remote name that is already configured in `~/.config/rclone/rclone.conf` (e.g. set up via `rclone config` in a terminal)
2. Save the settings
3. Check `~/.local/share/Spool/ludusavi/config.yaml` — `cloud.remote` should match
4. Launch a Proton game, play briefly, quit
5. Verify the backup uploaded: check the remote with `rclone ls <remote>:Spool/ludusavi-backup/`
6. Delete the local backup dir, relaunch — restore should pull from cloud

---

## After Phase 4

With cloud sync working, the "backup retention replaced the Windows backup" issue (see `save-restore-debug.md`) goes away — the Windows-origin backup lives on the cloud and is authoritative, so retention on the local copy doesn't matter.
