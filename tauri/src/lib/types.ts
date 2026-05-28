// Mirror of the Rust `ConfigData` struct in src-tauri/src/config.rs.
// Keep field names in lockstep — serde uses these exact snake_case names.
export type ConfigData = {
  ludusavi_path: string;
  steamgriddb_enabled: boolean;
  steamgriddb_api_key: string;
  spool_exe: string;
  theme: string;

  device_id: string;
  device_name: string;

  sync_server_enabled: boolean;
  sync_server_url: string;
  sync_server_api_key: string;

  lan_share_enabled: boolean;
  lan_share_port: number;
  lan_install_dir: string;
  /** Max aggregate LAN download throughput in MB/s. `0` = unlimited. */
  lan_download_max_mbps: number;

  torbox_enabled: boolean;
  torbox_api_key: string;
  download_dir: string;
  download_sources: string[];

  touch_mode: string;

  /** True after the close-to-tray intro toast has been shown once. */
  tray_intro_seen: boolean;
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
  has_cloud_save: boolean;
  manifest_install_dir: string | null;
  save_paths: string[];

  /** Dominant cover-art colour as `#rrggbb`, or null to use the brand default. */
  accent_color: string | null;

  /**
   * Cross-device save-sync status. Set by the sync server's
   * `/events/:game/latest-backup` probe; refreshed at startup +
   * after each successful backup.
   *
   *   "synced"       most recent server event came from this device
   *   "cloud-newer"  another device backed up more recently than us
   *   "local-newer"  we backed up locally but the event didn't reach
   *                  the server (offline / sync disabled)
   *
   * `null` means we don't have enough info to badge — sync off,
   * no backup history, or we never queried. The sidebar shows a
   * small coloured dot on the cover when this is set.
   */
  sync_badge: string | null;
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
  has_cloud_save: boolean;
  manifest_install_dir: string | null;
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
  has_cloud_save: boolean;
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
};

/**
 * One Hydra source entry (a downloadable game). Mirrors
 * `HydraEntry` in hydra.rs.
 */
export type HydraEntry = {
  title: string;
  uris: string[];
  upload_date: string;
  file_size: string;
  source_name: string;
  source_url: string;
};

/** Per-feed fetch outcome reported by `hydra_fetch_all`. */
export type FeedStatus = {
  url: string;
  name: string | null;
  entry_count: number;
  error: string | null;
};

/** Top-level result from `hydra_fetch_all`. */
export type BrowseFetchResult = {
  entries: HydraEntry[];
  feeds: FeedStatus[];
};

/**
 * Reachability state for the configured sync server. Mirrors the
 * Rust `SyncReachability` in sync.rs.
 *
 *   unconfigured → no URL / API key set → icon dimmed
 *   online       → /health returned 200 within timeout → green
 *   offline      → network error, non-200, or timeout → red
 */
export type SyncReachability = 'unconfigured' | 'online' | 'offline';

/**
 * Snapshot of the sync-server health poll. Mirrors `SyncStatus` in
 * sync.rs. Emitted as `sync:status-changed` events whenever any
 * field changes, also available via `currentSyncStatus()` for
 * mount-time catch-up.
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
 * Phases emitted by the Run workflow as `run:phase` events. The frontend
 * uses these to update the Play button label / lock the UI for the
 * currently-running game.
 *
 *   restoring  → ludusavi restore is running
 *   launching  → game process is being spawned
 *   playing    → game is running; await its exit
 *   backing-up → ludusavi backup is running after the session
 *   done       → workflow completed successfully
 *   error      → workflow aborted; see `message`
 */
export type RunPhase =
  | 'restoring'
  | 'launching'
  | 'playing'
  | 'backing-up'
  | 'done'
  | 'error';

export type RunPhaseEvent = {
  game_id: string;
  phase: RunPhase;
  message: string | null;
};

/**
 * Payload for `add_game`. id / catalog_number / timestamps are assigned by
 * the backend. Empty / falsy manifest fields are the signal for the "add
 * without save tracking" path.
 */
export type NewGame = {
  game_name: string;
  exe_path: string;
  steam_id?: number | null;
  gog_id?: number | null;
  lutris_slug?: string | null;
  has_cloud_save?: boolean;
  manifest_install_dir?: string | null;
  save_paths?: string[];
};
