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

### Phase 6 — Turnkey self-hosted save storage ✅ DONE

Goal: fold save storage into the **existing** self-hosted stack so one
`docker compose up` gives you locks **and** save sync — no more hand-rolled
SFTP/`rclone config` per device.

**Why WebDAV (not SFTP/FTP/S3):** ludusavi has no networking of its own — every
cloud remote is an rclone backend. Of the protocols a server can host, only
`WebDav`, `Ftp`, and `Smb` are first-class in ludusavi's CLI (`ludusavi cloud
set …`); SFTP and S3 require the manual `cloud set custom --id <remote>` path
(a hand-configured `rclone.conf` remote on every machine — exactly today's
SFTP setup we're replacing). WebDAV is HTTP, so it reuses the lock server's
port/TLS/reverse-proxy story and is the friendliest through NAT. FTP is
plaintext + passive-port hell; SMB is LAN-only.

**Verified ludusavi 0.31 schema** (from `ludusavi cloud set webdav --url … --username … --password … --provider nextcloud`):

```yaml
# config.yaml
cloud:
  remote:
    WebDav:
      id: ludusavi-<epoch>      # rclone remote name ludusavi auto-creates
      url: "https://host:port"
      username: deck
      provider: Nextcloud       # other | nextcloud | owncloud | sharepoint | sharepoint-ntlm
  path: ludusavi-backup
  synchronize: true
```

The password is **not** stored here — `cloud set` writes an obscured remote into
the user's global `~/.config/rclone/rclone.conf`:

```ini
[ludusavi-<epoch>]
type = webdav
vendor = nextcloud
url = https://host:port
user = deck
pass = <rclone-obscured>
```

#### Server (`server/` — one stack, two services) ✅

- New **`spool-storage`** service in `docker-compose.yml` (built from
  `Dockerfile.rclone` = upstream `rclone/rclone` + `curl` + the auth-proxy
  script) running `rclone serve webdav --auth-proxy …` on `47634`, sharing the
  `./data` volume and `WEBDAV_AUTH_SECRET` with `spool-lock`.
- **Auth via `--auth-proxy`** (`server/rclone-auth-proxy.sh`): rclone pipes the
  proxy `{user, pass}`; the proxy `curl`s it to the lock server's
  **`POST /internal/webdav-auth`** (`server/src/routes/internal.ts`), gated by
  `X-Internal-Secret: WEBDAV_AUTH_SECRET`. That route validates (username =
  account, password = api key) and returns
  `{ "type": "local", "_root": "<SAVES_DIR>/<account_id>" }` — a jailed root per
  account.
- **`GET /storage`** (`server/src/routes/storage.ts`, API-key-gated) →
  `{ webdav_url, username, password, base_path, provider }`; pre-creates the
  account's dir on the shared volume. Returns 404 when `WEBDAV_PUBLIC_URL` is
  unset (storage opt-in per server).
- Tests in `server/src/routes/storage.test.ts` (auth, 404-when-disabled,
  jailed-root, secret/cred rejection).
- Still open: per-account **quota / retention** on the volume (ludusavi handles
  `backup.retention.full`; the server disk itself is uncapped).

#### Client (Spool) ✅

- **`apply_webdav_remote`** (`ludusavi.rs`) shells out to
  `ludusavi cloud set webdav --config <Spool dir> …` (after pointing
  `apps.rclone.path` at the resolved rclone, since `cloud set` shells out to
  rclone to obscure). This is the shared core; it fixes the Phase 4 gap where
  the `webdav` arm wrote only a bare enum string.
- **`set_cloud_webdav`** command → manual WebDAV (Nextcloud/ownCloud) from the
  settings form; **`use_server_save_storage`** command (`sync.rs`) → turnkey:
  `GET /storage` → `apply_webdav_remote`. New `ConfigData` fields
  `cloud_webdav_url` / `cloud_webdav_username` (password never stored).
- Settings → Cloud saves: **"Use my Spool server for save storage"** button (when
  a sync server is configured) + manual WebDAV url/user/pass fields.

#### Gotchas found wiring it up (verified against rclone v1.74)

- **`rclone serve webdav --auth-proxy` takes NO positional remote** — passing a
  path is a CLI error (`needs 0 arguments maximum`). The proxy supplies the
  backend; the `Dockerfile.rclone` CMD omits the path.
- **The auth-proxy `Reveal()`s the incoming basic-auth password** before handing
  it to the proxy. So the on-wire password must already be rclone-*obscured*.
  `use_server_save_storage` therefore passes `obscure_password: true` to
  `apply_webdav_remote`, which runs `rclone obscure` on the api key first;
  ludusavi obscures it a second time for storage; the client reveals one layer on
  the wire and the server reveals the last, netting the real api key at
  `/internal/webdav-auth`. Manual WebDAV remotes pass `false` (a normal server
  wants the plaintext password).

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
