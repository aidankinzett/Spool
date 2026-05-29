# Proton + Cross-Device Save Sync — Implementation Plan

## Goal

Run Windows games on the Steam Deck via Proton, and sync saves between a Windows desktop and the Deck via ludusavi cloud (rclone). Topology: **Windows desktop ⟷ Linux Steam Deck**.

---

## Architecture decisions

| Decision | Choice | Reason |
|---|---|---|
| Proton runner | umu-run | Purpose-built for non-Steam, supports winetricks, protonfixes |
| Proton version default | UMU-Proton > GE-Proton > stock | UMU-Proton works with `umu-run winetricks`; stock Proton doesn't |
| Per-game prefix | `~/.local/share/Spool/prefixes/<game-id>/` | Isolated, deterministic, easy to target for backup |
| ludusavi config | Spool-owned `~/.local/share/Spool/ludusavi/config.yaml` | Spool controls backup path, cloud remote, redirects without touching user's personal ludusavi config |
| Cross-device restore | ludusavi redirects (generated from mapping.yaml) | Official ludusavi mechanism; handles both directions |
| Save bytes transport | ludusavi cloud sync (rclone) | Reuses ludusavi's dedup/retention/conflict handling |
| AUR packaging | `spool-bin` with `depends=(umu-launcher ludusavi rclone)` | One install pulls all deps |

---

## Phases

### Phase 1 — Proton launch ✅ DONE

- New `proton.rs`: discovers Proton builds, resolves `umu-run`, builds `GAMEID`/`WINEPREFIX`/`PROTONPATH` launch command
- `process.rs`: `LaunchSpec` enum (`Native` | `Proton`); Proton arm blocks to exit so backup fires
- `runner.rs`: `LaunchPlan` built before async workflow; `.exe` on Linux without `use_proton` gives clear error
- New `GameEntry` fields: `use_proton`, `proton_version_path`, `wine_prefix_path`, `launch_args`
- New `ConfigData` fields: `umu_run_path`, `default_proton_path`
- `edit/+page.svelte` Launch tab: Proton toggle, version select, prefix override, launch args, "Install dependencies" row
- `settings/+page.svelte`: Compatibility card (umu-run path + autodetect, default Proton select)
- Per-game `install_proton_deps` command: runs `umu-run winetricks -q <verbs>` against the game's prefix
- AUR `packaging/aur/spool-bin/PKGBUILD` + `.SRCINFO`; `release.yml` publishes raw binary tarball

### Phase 2 — Spool-owned ludusavi config + `--wine-prefix` backup ✅ DONE

- New `ludusavi_config.rs`: owns `config.yaml` via `serde_yaml::Value` read-modify-write; `ensure_config()` called at startup
- Every ludusavi call gets `--config ~/.local/share/Spool/ludusavi/` prepended
- `backup()` gains `wine_prefix: Option<&Path>` → `--wine-prefix <prefix_root>` for Proton games
- `open_ludusavi_gui` opens against Spool's config
- `manual_prep` returns `(name, exe, config_dir, wine_prefix)` for both commands

### Phase 3 — Cross-platform restore redirects ✅ DONE (bug fix outstanding — see debug doc)

- New `redirects.rs`: parses `mapping.yaml` (including differential children), extracts Windows username, classifies paths into 5 cases, derives redirect rules
- `restore_with_redirects()` in runner.rs: first restore (pulls cloud), read mapping.yaml, if foreign-origin write redirects + restore again, clear after
- `windows_safe_name()`: handles `"Lego Batman: Legacy of the Dark Knight"` → folder `"Lego Batman_ Legacy of the Dark Knight"` on Windows
- 15 unit tests, 3 against real backup files on disk

### Phase 4 — Cloud/rclone settings UI + ludusavi-gui button ✅ DONE

- `ConfigData` fields: `cloud_provider`, `cloud_remote`, `cloud_path`, `rclone_path`, `rclone_args` (`config.rs`)
- `update_config` pushes to owned `config.yaml` via `ludusavi_config::set_cloud()` (`config.rs`); the rclone path it writes is itself resolved config → bundled → system
- `open_ludusavi_gui` opens the GUI against Spool's config (`ludusavi.rs`)
- `settings/+page.svelte`: "Cloud saves (rclone)" card — provider dropdown, remote, path, rclone binary path, rclone args
- "Open Ludusavi settings" button → opens GUI against Spool's config
- **Beyond the original plan:** a `cloud_provider` dropdown maps presets (Box, Dropbox, GoogleDrive, OneDrive, Ftp, Smb, WebDav) plus a Custom rclone remote onto ludusavi's `cloud.remote` schema in `set_cloud`. Unit-tested via the pure `apply_cloud` helper in `ludusavi_config.rs`.

### Phase 5 — Bundle pinned binaries ✅ DONE

- `ludusavi` and `rclone` shipped as Tauri `externalBin`/sidecar (`tauri.conf.json` → `binaries/ludusavi`, `binaries/rclone`)
- Resolution order config override → bundled → system in `paths::resolve_ludusavi_path` and the rclone branch of `update_config`; `paths::resolve_sidecar_path` handles both dev (target-triple suffix) and packaged (bare name) layouts
- `apps.rclone.path` set in owned config to the resolved rclone (`set_cloud` → `apps.rclone.path`)
- Sidecars are fetched in CI/release via `tauri/scripts/download-sidecars.js` (wired as `bun run download-sidecars` in `ci.yml` + `release.yml`); `/binaries/` is gitignored so the blobs stay out of the repo

### Phase 6 — Turnkey self-hosted save storage (optional, later)

- Add `rclone/rclone serve webdav /data` container to `server/docker-compose.yml`
- New Hono route `GET /storage` returns `{ webdav_url, username, password, base_path }`
- Spool settings: "Use my Spool server for save storage" → fetches creds → writes rclone webdav remote → sets `cloud.remote` in owned config

---

## Key file locations

| Path | Purpose |
|---|---|
| `~/.local/share/Spool/prefixes/<game-id>/` | Per-game Proton Wine prefix |
| `~/.local/share/Spool/ludusavi/config.yaml` | Spool-owned ludusavi config |
| `~/.local/share/Spool/ludusavi-backup/<game>/` | Ludusavi backup dir |
| `~/.local/share/Spool/ludusavi-backup/<game>/mapping.yaml` | Backup manifest (parsed for redirect generation) |
| `~/.local/share/Spool/debug.log` | App log (tracing output) |
| `tauri/src-tauri/src/proton.rs` | Proton discovery + umu-run launch |
| `tauri/src-tauri/src/ludusavi_config.rs` | Owned config management |
| `tauri/src-tauri/src/redirects.rs` | mapping.yaml parser + redirect derivation |
| `tauri/src-tauri/src/runner.rs` | Run workflow (restore → launch → backup) |
| `packaging/aur/spool-bin/PKGBUILD` | AUR package |

---

## Deferred packaging work

- **(b) Dependency-doctor UI** in Settings → Compatibility: detect umu-run/ludusavi/rclone, show status, print per-distro install command for missing ones
- **(c) Bundle ludusavi + rclone** ✅ DONE in Phase 5 — shipped via Tauri `externalBin` and fetched at build time by `download-sidecars.js`. umu-run stays a system prerequisite (it's a Python app with lib32/vulkan deps — not bundleable)
