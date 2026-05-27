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

  torbox_enabled: boolean;
  torbox_api_key: string;
  download_dir: string;
  download_sources: string[];

  touch_mode: string;
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
};
