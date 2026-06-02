/** Mirror of the Rust `UiMode` enum (serde rename_all = "lowercase"). */
export type UiMode = 'auto' | 'desktop' | 'touch';

// Mirror of the Rust `ConfigData` struct in src-tauri/src/config.rs.
// Keep field names in lockstep — serde uses these exact snake_case names.
export type ConfigData = {
  steamgriddb_enabled: boolean;
  steamgriddb_api_key: string;
  spool_exe: string;

  device_id: string;
  device_name: string;

  lan_share_enabled: boolean;
  lan_share_port: number;
  lan_install_dir: string;
  /** Max aggregate LAN download throughput in MB/s. `0` = unlimited. */
  lan_download_max_mbps: number;

  /** Path to `umu-run`. `""` = autodetect. Linux-only. */
  umu_run_path: string;
  /** Default Proton build dir; `""` = auto-pick newest. */
  default_proton_path: string;

  ui_mode: UiMode;

  /** True after the close-to-tray intro toast has been shown once. */
  tray_intro_seen: boolean;

  /** True once the first-run onboarding flow has been finished or dismissed.
   * Pre-existing configs are migrated to true on load (returning users skip
   * the flow); a fresh install starts false so onboarding shows. */
  onboarding_completed: boolean;

  cloud_provider: string;
  cloud_remote: string;
  /** Base folder on the remote. Saves → `<base>/ludusavi-backup`; Spool's
   * cross-device control plane → `<base>/_spool`. */
  cloud_base_path: string;
  /** Legacy: exact ludusavi remote subpath, superseded by `cloud_base_path`.
   * Kept for JSON round-trip with older configs; no longer read. */
  cloud_path: string;
  rclone_args: string;
  cloud_webdav_url: string;
  cloud_webdav_username: string;

  /**
   * Number of full save revisions ludusavi keeps per game
   * (`backup.retention.full`). More = more rollback points, more disk +
   * cloud upload. Clamped to 1–10 by the backend. Default 3.
   */
  save_retention_full: number;
};

// Mirror of the Rust `SaveRevision` struct in src-tauri/src/ludusavi.rs.
// A single restorable save backup, surfaced by the "restore an earlier save"
// picker.
export type SaveRevision = {
  /** ludusavi's backup id (the token passed to `restore --backup`). */
  name: string;
  /** RFC3339 timestamp of when the backup was taken. */
  when: string;
  /** True for the tip — the revision a normal pre-launch restore lands. */
  is_current: boolean;
};

// Mirror of the Rust `GameEntry` struct in src-tauri/src/library.rs.
// Keep field names in lockstep — `serde` on the Rust side serializes with
// these exact snake_case names.

export type GameEntry = {
  id: string;
  /** Sequential shelf number, formatted as SPL-NNNN in the UI. */
  catalog_number: number;
  game_name: string;
  exe_path: string;
  safe_name: string;

  cover_image_path: string | null;
  hero_image_path: string | null;

  added_at: string | null;
  last_played_at: string | null;

  launcher_exe_path: string | null;
  game_folder_path: string | null;

  run_as_admin: boolean;

  /** Launch this Windows exe through Proton (umu-run) on Linux. */
  use_proton: boolean;
  /** Override Proton build dir; null = global default / auto. */
  proton_version_path: string | null;
  /** Override Wine prefix root; null = default prefixes dir / <id>. */
  wine_prefix_path: string | null;
  /** Extra launch args appended after the exe. */
  launch_args: string | null;

  description: string;
  developer: string;
  publisher: string;
  genres: string[];
  release_date: string | null;
  install_size_mb: number;

  playtime_minutes: number;

  lan_shared: boolean;
  lan_share_folder: string | null;

  save_backup_count: number;
  save_last_backed_up_at: string | null;
  save_backup_size_mb: number;

  install_source: string;
  lan_install_source_device_name: string | null;
  lan_install_source_device_id: string | null;

  // Manifest-derived (Tauri rewrite — empty/null for legacy C# entries)
  steam_id: number | null;
  gog_id: number | null;
  lutris_slug: string | null;
  manifest_install_dir: string | null;
  save_paths: string[];

  /** Dominant cover-art colour as `#rrggbb`, or null to use the brand default. */
  accent_color: string | null;

  /**
   * Cross-device save-sync status. Derived from rclone device blobs
   * at startup and updated after each successful backup.
   *
   *   "synced"       this device holds the most recent backup
   *   "cloud-newer"  another device backed up more recently than us
   *   "local-newer"  we backed up locally but the cloud hasn't
   *                  confirmed it yet (offline / sync disabled)
   *
   * `null` means not enough info to badge — cloud not configured or
   * no backup history. The sidebar shows a small coloured dot on
   * the cover when this is set.
   */
  sync_badge: string | null;

  /**
   * Latest ludusavi backup name last reconciled with the cloud on this
   * device — the merge-base for fast-forward vs. true-divergence detection.
   * `null` for legacy/never-synced entries. Backend-managed; the UI doesn't
   * read it directly but it round-trips through `GameEntry`.
   */
  cloud_sync_baseline: string | null;
};

/**
 * Enriched search result from `search_games` / `search_by_exe`. Mirrors the
 * Rust `SearchCandidate` struct in src-tauri/src/ludusavi.rs.
 *
 * `score` is 0.0–1.0 from ludusavi find; the UI typically hides it when
 * >= 0.95 (a confident match).
 */
export type SearchCandidate = {
  name: string;
  score: number;
  save_path: string | null;
  save_paths: string[];
  steam_id: number | null;
  gog_id: number | null;
  lutris_slug: string | null;
  manifest_install_dir: string | null;
  /** All install-folder names ludusavi lists for this game. */
  manifest_install_dirs: string[];
  /**
   * The picked exe's ancestor directory matching one of `manifest_install_dirs`
   * — the detected install root. Only set by `searchByExe`; null for manual
   * name searches. The Add flow defaults the install folder to this.
   */
  install_root: string | null;
};

/**
 * One LAN peer (another Spool instance on the local network). Mirrors
 * `LanPeer` in lan.rs. `file_server_port == 0` means the peer is in
 * Phase A — discovery only, no transfers yet.
 */
export type LanPeer = {
  device_id: string;
  device_name: string;
  addr: string;
  game_count: number;
  version: number;
  file_server_port: number;
  last_seen_ago_secs: number;
};

/**
 * Game catalogue entry served by a peer's `/games` endpoint. Mirrors the
 * Rust `PeerGame` struct in lan.rs — a curated subset of `GameEntry` with
 * local filesystem paths stripped (no `exe_path`, no image paths).
 */
export type PeerGame = {
  id: string;
  catalog_number: number;
  game_name: string;
  developer: string;
  publisher: string;
  genres: string[];
  install_size_mb: number;
  release_date: string | null;
  steam_id: number | null;
  gog_id: number | null;
  lutris_slug: string | null;
  /** True if the source peer can actually stream this game. False entries
   *  appear in the catalogue for transparency but the Install button is
   *  disabled — usually means the peer hasn't set `game_folder_path`. */
  shareable: boolean;
};

/**
 * One in-flight (or just-finished) LAN install. Emitted as `lan:download`
 * events and also returned by `current_peer_download` for late-mount
 * catch-up. Mirrors the Rust `DownloadProgress` struct in lan.rs.
 *
 * Status values:
 *   starting       → manifest fetched, transfer about to begin
 *   transferring   → bytes flowing; `current_file` + `bytes_done` updating
 *   done           → install complete; `new_game_id` points at the new entry
 *   error          → install aborted; see `message`
 *   canceled       → user pressed Cancel; partial dir already cleaned up
 */
export type DownloadProgress = {
  install_token: string;
  source_device_id: string;
  source_device_name: string;
  source_game_id: string;
  game_name: string;
  bytes_done: number;
  bytes_total: number;
  current_file: string;
  status: 'starting' | 'transferring' | 'done' | 'error' | 'canceled';
  message: string | null;
  new_game_id: string | null;
  /** Average throughput since the install started, in bytes per second.
   *  `0` for the first half-second so the UI doesn't flash silly values. */
  bytes_per_second: number;
  /** Local filesystem path to the peer-supplied cover image, prefetched
   *  in the background once the manifest lands. `null` until it lands
   *  (or if the peer 404s its `/cover` endpoint). Use `assetUrl()` to
   *  turn it into a webview-loadable URL. */
  cover_image_path: string | null;
};

/**
 * One peer currently downloading from this device. Surfaced to the host
 * UI's "Uploads" view; mirrors the Rust `UploadSnapshot` in lan.rs.
 *
 * `last_seen_ago_secs` is the freshness signal: under ~2 s = actively
 * transferring, older = winding down. Sessions get reaped ~8 s after
 * the last touch, so a stale entry only sits here briefly.
 */
export type UploadSnapshot = {
  session_id: string;
  game_id: string;
  game_name: string;
  peer_addr: string;
  last_seen_ago_secs: number;
  cancelled: boolean;
  /** Total bytes in the transfer (from the manifest). 0 until the manifest has been fetched. */
  bytes_total: number;
  /** Bytes served to the peer so far (optimistic — credited at request time). */
  bytes_sent: number;
};

/**
 * Reachability state for the configured cloud remote. Mirrors the
 * Rust `SyncReachability` in rclone.rs.
 *
 *   unconfigured → no cloud remote set → icon dimmed
 *   online       → `rclone lsd` succeeded within timeout → green
 *   offline      → rclone error or timeout → red
 */
export type SyncReachability = 'unconfigured' | 'online' | 'offline';

/**
 * Snapshot of the cloud-remote reachability poll. Mirrors `SyncStatus` in
 * rclone.rs. Emitted as `sync:status-changed` events whenever any field
 * changes, also available via `currentSyncStatus()` for mount-time catch-up.
 * `server_version` is retained for shape compatibility but is always null.
 */
export type SyncStatus = {
  reachability: SyncReachability;
  server_version: string | null;
  error: string | null;
  last_ok_ago_secs: number | null;
};

/** Result returned by `add_to_steam`. Mirrors `AddToSteamResult` in steam.rs. */
export type AddToSteamResult = {
  steam_user_id: string;
  app_id: number;
  shortcuts_path: string;
  portrait_placed: boolean;
  extras_placed: string[];
};

/**
 * A detected Apollo/Sunshine streaming host. Mirrors `StreamingHostInfo` in
 * streaming_host.rs. `kind` is "apollo" | "sunshine" | "".
 */
export type StreamingHostInfo = {
  detected: boolean;
  kind: string;
  apps_path: string;
};

/** Result returned by `add_to_streaming_host`. Mirrors the Rust struct. */
export type AddToStreamingHostResult = {
  host_kind: string;
  apps_path: string;
  app_name: string;
  image_set: boolean;
};

/**
 * Phases emitted by the Run workflow as `run:phase` events. The frontend
 * uses these to update the Play button label / lock the UI for the
 * currently-running game.
 *
 *   restoring  → ludusavi restore is running
 *   launching  → game process is being spawned
 *   playing    → game is running; await its exit
 *   backing-up → ludusavi local backup is running after the session
 *   uploading  → local backup done; mirroring the revision to the cloud remote
 *   done       → workflow completed successfully
 *   error      → workflow aborted; see `message`
 */
export type RunPhase =
  | 'restoring'
  | 'launching'
  | 'playing'
  | 'backing-up'
  | 'uploading'
  | 'done'
  | 'error';

export type RunPhaseEvent = {
  game_id: string;
  phase: RunPhase;
  message: string | null;
  /** True when a cloud remote is configured for this session. */
  cloud_used: boolean;
  /** Play session duration in minutes. Set on backing-up and done phases; null before the game exits. */
  session_minutes: number | null;
  /** True when local backup succeeded but cloud upload failed. Only ever true on the done phase. */
  cloud_upload_failed: boolean;
};

/**
 * Payload for `add_game`. id / catalog_number / timestamps are assigned by
 * the backend. Empty / falsy manifest fields are the signal for the "add
 * without save tracking" path.
 */
/**
 * Resolution source for a dependency, from `check_dependencies`.
 *   bundled  — sidecar shipped alongside the Spool executable
 *   system   — found on the system PATH or a well-known path
 *   missing  — not found anywhere
 */
export type DepSource = 'bundled' | 'system' | 'missing';

/**
 * Status of a single runtime dependency (umu-run, ludusavi, rclone).
 * Mirrors `DepStatus` in src-tauri/src/diagnostics.rs.
 */
export type DepStatus = {
  name: string;
  found: boolean;
  path: string;
  source: DepSource;
  /** Copy-paste install command for the detected distro, or "" if found. */
  install_hint: string;
};

export type NewGame = {
  game_name: string;
  exe_path: string;
  steam_id?: number | null;
  gog_id?: number | null;
  lutris_slug?: string | null;
  manifest_install_dir?: string | null;
  save_paths?: string[];
  game_folder_path?: string | null;
};

/**
 * A discovered Proton build, from `list_proton_versions`. Mirrors the Rust
 * `ProtonVersion` struct in src-tauri/src/proton.rs.
 *
 *   source  "steam" (steamapps/common) | "compat" (compatibilitytools.d)
 */
export type ProtonVersion = {
  name: string;
  path: string;
  source: string;
};

export type RawSaveDetails = {
  modified: string | null;
  size_bytes: number;
};

export type RawConflictDetails = {
  local: RawSaveDetails | null;
  cloud: RawSaveDetails | null;
};
