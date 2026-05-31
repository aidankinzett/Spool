# Replace the HTTP sync server with an rclone-remote control plane

## Context

Spool currently runs a self-hosted Hono + SQLite **sync server** (`server/`) that does six
things over HTTP: per-game play **locks** (heartbeat/staleness/suspend), additive **playtime**,
last-write-wins **last-played**, backup/restore **events** (drive the per-game sync badge),
**account registration + auth**, and a **/health** poll. The Rust client is `sync.rs`.

The user's actual need is narrow: when launching a game on device B, warn them if device A has
a session whose saves **aren't in the cloud yet** — so they don't restore a stale save and
appear to lose progress. The lock's mutual-exclusion semantics are incidental; an advisory
warning is enough. Everything else the server stores (playtime, last-played, badge) is
CRDT-friendly and maps cleanly to small per-device JSON blobs.

Since the rclone remote used for cloud saves is already configured and `runner.rs` already
shells out to rclone directly, we can store these blobs in that same remote and **delete the
sync server entirely**.

**Decisions (from the user):**
1. **Full replacement** — move session-warning, playtime, last-played, and backup-badge onto
   rclone blobs; decommission `server/`.
2. **Blocking modal + override** for the unsynced-session warning (reuse the existing
   suspended-session modal + "Play here instead" → re-launch with `steal=true`).
3. **No migration** — introduce a base "Remote folder" (default `Spool`); the user fixes
   Settings manually on first launch.

## Remote layout (under `<base>/_spool`, sibling of `<base>/ludusavi-backup`)

Keep `_spool` a **sibling** of `ludusavi-backup`, never nested inside it — ludusavi's
`--cloud-sync` reconciles that subtree and would delete unrecognized files.

- **Per-device blob** `_spool/devices/<device_id>.json` — each device only ever writes its own
  file, so it's conflict-free.
  ```rust
  struct DeviceBlob {
      device_name: String,
      playtime:    BTreeMap<String, i64>,    // game -> minutes accrued ON THIS DEVICE only
      last_played: BTreeMap<String, String>, // game -> rfc3339 (this device)
      backups:     BTreeMap<String, String>, // game -> rfc3339 of last successful cloud upload
      schema: u32,                            // = 1
  }
  ```
- **Per-game session marker** `_spool/sessions/<blake3(game_name)>.json`:
  ```rust
  struct SessionMarker {
      game_name: String,   // stored plaintext; on read, mismatch == hash collision -> treat absent
      device_id: String, device_name: String,
      started_at: String, updated_at: String, // rfc3339; heartbeat bumps updated_at
      state: SessionState, // Active | PendingBackup  (#[serde(rename_all="kebab-case")])
      suspended: bool,     // logind suspend watcher sets this; suppresses staleness
  }
  ```

**Playtime fold — correctness (important):** do NOT sum-then-max. Each device's blob stores
**only its own** contribution; the cross-device total is `Σ` over all device blobs and the
library's `playtime_minutes` is **set to** that sum (not merged with `max`). On session end this
device does `blob.playtime[game] += session_minutes` on its OWN file only — so repeated startup
folds never re-contribute another device's minutes (the bug the naive max/sum mix would cause).
`last_played` fold = `max`. Badge = device with newest `backups[game]` is the "latest backer".

## New module: `tauri/src-tauri/src/rclone.rs`

Factor the remote resolver out of `runner.rs` (`resolve_rclone_remote` ~795,
`get_rclone_remote_name` ~941) so the runner's existing `rclone cat`/`lsjson` calls and the new
control plane share it.

```rust
struct RcloneRemote { exe: PathBuf, remote: String, base: String } // base = cloud_base_path
fn resolve_remote(app: &AppHandle) -> Option<RcloneRemote>;
fn resolve_remote_from_config(cfg: &ConfigData) -> Option<RcloneRemote>; // for headless paths
fn session_hash(game_name: &str) -> String; // blake3 hex (see lan/server.rs:733)

// async, kill_on_drop, CREATE_NO_WINDOW on Windows, tokio timeout (6-8s; 4-5s for probe):
async fn cat(exe, target, timeout) -> Option<String>;
async fn rcat(exe, target, body, timeout) -> bool;        // write stdin -> object (no temp file)
async fn deletefile(exe, target, timeout) -> bool;
async fn lsjson(exe, target, timeout) -> Option<Vec<Entry>>;
async fn lsd(exe, "<remote>:", timeout) -> bool;          // reachability probe
```

**Read markers with `cat` of the deterministic path, never `lsjson`** — a specific-object read
is read-after-write consistent on far more backends than a listing (Drive/S3 list caches +
`--fast-list` lag). Reserve `lsjson` for the device-file fold, where staleness only delays a
stat sync, not correctness. Use `rcat` (stdin→object, atomic single PUT) for writes.

The runner's `rclone_cat_tip`/`query_rclone_details` should call the shared `cat`/`lsjson`
helpers; delete their duplicate `Command` plumbing. Move `test_get_rclone_remote_name` here.

## `runner.rs` — session-marker lifecycle & the warning

Replace the four `sync::` call sites in `run_workflow`:

- **Phase 1.5 (~1625):** `cat` the marker, then classify:
  - `None` / ours → write our marker (Active), proceed.
  - other + `state==Active` + fresh (`now-updated_at < 180s` and not suspended) + `!steal` →
    **block** with `"Already playing on {dev}. Close it there before launching here."`
  - other + (`state==PendingBackup` or stale) + `!steal` → **block** with the unsynced-session
    message (reuse the `"Suspended session on {dev}. That device is asleep mid-session …"`
    phrasing so the existing modal+override fires; if we want truer copy, change the Rust string
    and the `library.svelte.ts` regex anchor together).
  - `steal==true` → overwrite marker, proceed.
- **Heartbeat (~1695/abort ~1711):** `spawn_session_heartbeat` rewrites the marker's
  `updated_at` every **60s** (rclone PUTs are heavier than HTTP; 60s write + 180s stale window =
  3 missed-write slack). `.abort()` at the same site.
- **Suspend watcher (`suspend.rs`):** replace `suspend_lock` with rewriting the marker
  `suspended=true`; resume rewrites `suspended=false, updated_at=now` (or warns if not ours).
- **Clean exit (~1713):** instead of `release_lock`, rewrite marker `state=PendingBackup`
  (saves not yet uploaded → peers keep warning).
- **After backup (~1864):** if `cloud_configured && !cloud_upload_failed` → `deletefile` the
  marker AND set `blob.backups[game]=now`; if upload failed → leave the PendingBackup marker and
  don't touch `backups`. Delete `record_restore_event` (~1623) — restore events are unused now.

## Playtime / last-played / badge

- Session end (~1743): replace `push_*` with one `update_device_blob` (cat → mutate → rcat):
  `playtime[game] += session_minutes; last_played[game] = end; device_name = …`. Keep the
  existing local `+= session_minutes` (~1727) for instant feedback; the fold corrects it.
- Startup (`lib.rs` ~517): replace `spawn_startup_sync` with `spawn_startup_fold`: `lsjson`
  `_spool/devices` → `cat` each → fold (`playtime = Σ`, `last_played = max`, badge from latest
  backer) → single `lib.save()` + emit `library:changed`.
- `compute_badge(our_id, latest_backer)`: `Some(ours)→"synced"`, `Some(other)→"cloud-newer"`,
  `None→"synced"` (no backups ⇒ nothing newer). Keep the post-backup `sync_badge="synced"` write.

## Reachability / status

Keep `sync:status-changed`, `SyncStatusState`, `current_sync_status`/`refresh_sync_status`, and
the chrome dot. Swap only the probe: in `poll_once`, resolve via `rclone::resolve_remote` and
probe with `rclone::lsd("<remote>:", 5s)`. Keep `Unconfigured/Online/Offline`; drop/ignore
`server_version`. `spawn_health_poller` (lib.rs ~511) name/wiring unchanged.

## Headless paths (Decky fallback) — keep CLI flag names

- `run_release_lock_headless` (lib.rs ~624): drop the `sync_server_enabled` gate; build a remote
  from `ConfigData` and rewrite the marker to `state=PendingBackup`. No-op when cloud off.
- `run_backup_headless` (lib.rs ~553): after a successful `backup_game_core` with cloud upload
  OK → `delete_session_marker`; on failure leave it. `session::mark_backed_up()` stays.
- `cli.rs` unchanged (flag names preserved for the installed Decky plugin); update the doc
  comment to say "session marker" not "lock".

## Config + Settings UI

- `config.rs`: add `cloud_base_path: String` (default `"Spool"`); remove `sync_server_enabled/
  url/api_key` (old configs still parse — `#[serde(default)]`, no `deny_unknown_fields`). In
  `update_config` (~327) derive ludusavi's path: `format!("{}/ludusavi-backup",
  cloud_base_path.trim_end_matches('/'))`. `cloud_path` may stay as an ignored deserializable
  field.
- `ludusavi_config.rs`: caller passes the derived path; drop the `spool-server` provider arm
  (~160). Existing test (`"Spool/ludusavi-backup"`, ~514) still passes.
- Remove commands `sync_register_account` and `use_server_save_storage`; unregister them from
  `generate_handler!` in `lib.rs`. Keep the `reqwest::Client` state (LAN uses it).
- `settings/+page.svelte`: delete the entire sync-server section (URL/key/registration/
  use-server-storage); rework the "Remote path" row to bind `cloud_base_path`, relabel "Remote
  folder", helper "Saves go to <folder>/ludusavi-backup; Spool metadata to <folder>/_spool",
  placeholder `Spool`. Keep `device_name` editable; drive the status pill off rclone reachability.
- `api.ts`/`types.ts`: drop `sync_server_*` + `syncRegisterAccount`/`useServerSaveStorage`; add
  `cloud_base_path`; keep `currentSyncStatus`/`refreshSyncStatus`.
- `library.svelte.ts` (~300): add a second regex so the "Already playing on X" case ALSO opens
  the override modal (rename `suspendedConflict`→`unsyncedConflict`); override already re-launches
  with `steal=true` → our overwrite branch. Steal plumbing (api.ts:144, launch_game) unchanged.

## Decommission

Delete: `server/` (incl. its vitest suite), `.github/workflows/server-publish.yml`, the
`server:` job + `server/**` path filters in `ci.yml`, and `sync.rs` (replaced by `rclone.rs`;
move `SyncStatus`/`SyncReachability`/`SyncStatusState`/status commands/`spawn_health_poller`
into `rclone.rs` or a small `status.rs`). Update `mod sync;`→`mod rclone;` and all
`crate::sync::` refs in `runner.rs`/`suspend.rs`/`lib.rs`. Docs: scrub `CLAUDE.md`, `README.md`
(sync server / Hono / API key / register / `--release-lock` semantics).

## Edge cases & risks

- **Eventual consistency** widens the race — mitigated by reading markers via `cat` (object read,
  not list). Accept advisory-not-mutex (matches the requirement).
- **Rate limits** — 60s heartbeat writes are fine; don't go below 60s; skip a rewrite if <45s
  since the last successful one.
- **Hash collisions** — blake3 ⇒ ~zero; the in-marker `game_name` check is the guard.
- **Dead device** — 180s staleness reclassifies Active→unsynced (overridable, never a hard
  block); a lingering PendingBackup correctly means "their saves never reached cloud". Optional:
  startup fold `deletefile`s session markers older than ~30 days.
- **Concurrent fold vs own-file write** — single-object PUT is atomic; reader sees old-or-new,
  never corrupt; corrected next fold.
- **Cloud unconfigured** — `resolve_remote`→`None` ⇒ every helper no-ops and Phase 1.5 → proceed
  (same as today's "sync disabled → launch anyway"). Verify every new call site treats `None` as
  proceed/skip.
- **`rcat` on WebDAV/SMB/FTP** — rclone falls back to a temp spool for unknown-size streams; safe.

## Verification

- **Rust unit tests (in `rclone.rs`):** marker + `DeviceBlob` round-trip and default-on-missing;
  `fold_sums_playtime` (and single-device == own counter — no double count across 3 restarts);
  `fold_takes_max_last_played`; `latest_backer`/`compute_badge`; `classify(marker,now,steal)` →
  Absent/Ours/ActivePlaying/Unsynced (suspended-never-stale, PendingBackup-always-unsynced,
  stale-active→unsynced, fresh-active→block, steal→proceed); `session_hash` stable +
  collision-guarded. Run `cargo test`, `cargo clippy --all-targets -- -D warnings`.
- **Frontend:** `bun run check`, `bun run lint`, `bun run test`; update any `sync_server_*` test;
  add `cloud_base_path` default assertion.
- **Manual two-device E2E:** same remote + base `Spool` on both; (2) A launches X → marker
  `active` + `devices/A.json` appear, `updated_at` bumps every 60s; (3) B launches while A plays
  → blocking "Already playing on A" modal, override proceeds (marker→B); (4) A quits but is
  killed pre-upload → marker `pending-backup`, B launch → unsynced modal + override; (5) A clean
  session w/ upload → marker deleted, B launch → no modal; (6) 10 min on A + 5 on B, restart both
  3× → library shows 15 min total on both (no inflation); (7) badge `synced` on last backer,
  `cloud-newer` on the other; (8) unconfigure cloud → launches proceed, no modal, dot
  `Unconfigured`.

### Critical files
- `tauri/src-tauri/src/sync.rs` → delete, replaced by new `tauri/src-tauri/src/rclone.rs`
- `tauri/src-tauri/src/runner.rs` (Phase 1.5 ~1625; heartbeat/release ~1695-1713; playtime/badge
  ~1727-1900; shared resolver ~795)
- `tauri/src-tauri/src/config.rs` (cloud_base_path, remove sync_server_*, path derivation ~327)
- `tauri/src-tauri/src/lib.rs` (headless ~553/624; poller+fold ~511/517; handler registration)
- `tauri/src-tauri/src/suspend.rs` (suspend/resume → marker writes)
- `tauri/src/routes/settings/+page.svelte` + `tauri/src/lib/library.svelte.ts` (~300)
- `tauri/src/lib/api.ts`, `tauri/src/lib/types.ts`
- `server/` + `.github/workflows/server-publish.yml` + `ci.yml` (delete)