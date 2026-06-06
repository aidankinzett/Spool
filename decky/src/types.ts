// TypeScript mirrors of the Rust serde types the plugin exchanges with Spool's
// headless server and the Decky callable bridge.

export interface Settings {
  spool_command: string;
  notify: boolean;
}

// Mirror of the fields the grid needs from the Rust `GameEntry`.
export interface LibraryGame {
  id: string;
  game_name: string;
  exe_path: string;
  cover_image_path: string | null;
  accent_color: string | null;
  steam_id: number | null;
  playtime_minutes: number;
  shortcut_app_id: number | null;
  last_played_at: string | null;
  sync_badge: string | null;
  game_folder_path: string | null;
  // False after "Remove from disk" (uninstall): files gone, entry kept.
  installed: boolean;
  save_backup_count: number;
  save_last_backed_up_at: string | null;
  save_backup_size_mb: number;
  // Device holding the newest cloud backup + its revision time, when that's
  // another device (paired with sync_badge === "cloud-newer"). Null otherwise.
  save_last_backer_device: string | null;
  save_cloud_revision_at: string | null;
  // Pinned Proton build path, or null for "auto" (umu-run picks its default).
  proton_version_path: string | null;
}

// Mirror of the Rust `SaveRevision` (ludusavi.rs). One retained ludusavi backup
// for a game.
export interface SaveRevision {
  // ludusavi's backup id — the token `restore --backup <name>` accepts. The
  // newest revision (the tip) is reported as ".".
  name: string;
  // ISO timestamp of when the backup was taken.
  when: string;
  // True for the newest revision (the tip a normal restore would land). The
  // picker disables rolling back to it.
  is_current: boolean;
}

// Mirror of the Rust `ProtonVersion` (proton.rs). A discovered Proton build.
export interface ProtonVersion {
  name: string;
  path: string;
  source: string;
}

// Shortcut fields from the backend (mirrors what desktop "Add to Steam" writes).
export interface LaunchInfo {
  appName: string;
  exe: string;
  startDir: string;
  launchOptions: string;
}

// The subset of the live `SteamClient.Apps` API we use to create + launch a
// non-Steam shortcut without restarting Steam. AddShortcut returns the new
// appid; the setters are defensive (some Steam builds ignore AddShortcut's
// extra args).
export interface SteamApps {
  // Order is (appName, exe, startDir, launchOptions) — matches the
  // NonSteamLaunchers plugin's working createShortcut. The exe and startDir
  // must be passed *quoted* (literal surrounding double-quotes), which is also
  // how Spool's server computes `shortcut_app_id` (steam.rs compute_shortcut_app_id
  // CRCs the quoted exe) — so Steam's returned appid matches the server's.
  AddShortcut(appName: string, exe: string, startDir: string, launchOptions: string): Promise<number>;
  SetShortcutName?(appId: number, name: string): void;
  SetShortcutExe?(appId: number, path: string): void;
  SetShortcutStartDir?(appId: number, dir: string): void;
  // NSL uses SetAppLaunchOptions; SetShortcutLaunchOptions appears to be a
  // no-op on current Steam builds (left launch options empty → spool-launcher.sh
  // ran with no `--run` args → nothing launched).
  SetAppLaunchOptions?(appId: number, opts: string): void;
  SetShortcutLaunchOptions?(appId: number, opts: string): void;
  RemoveShortcut?(appId: number): Promise<void> | void;
  // Programmatic launch. gameId is the string gameid (the big number);
  // signature used across Decky plugins: (gameId, "", -1, 100).
  RunGame?(gameId: string, launchSource: string, a: number, b: number): void;
  // SetCustomArtworkForApp(appId, base64, 'png'|'jpg', assetType): the live,
  // no-restart way to set Steam library art. assetType is ELibraryAssetType.
  SetCustomArtworkForApp?(
    appId: number,
    base64: string,
    imageType: string,
    assetType: number,
  ): Promise<void>;
}

// Mirror of the Rust `LanPeer` (lan/discovery.rs).
export interface LanPeer {
  device_id: string;
  device_name: string;
  addr: string;
  game_count: number;
  file_server_port: number;
  last_seen_ago_secs: number;
}

// Mirror of the Rust `PeerGame` (lan/server.rs).
export interface PeerGame {
  id: string;
  game_name: string;
  install_size_mb: number;
  shareable: boolean;
}

// Mirror of the Rust `DownloadProgress` (lan/install.rs).
export interface DownloadProgress {
  install_token: string;
  source_device_name: string;
  // The peer-library id of the game being installed — lets the game list mark
  // which row is the active download.
  source_game_id: string;
  game_name: string;
  bytes_done: number;
  bytes_total: number;
  current_file: string;
  status: "starting" | "transferring" | "done" | "error" | "canceled";
  message?: string;
  new_game_id?: string;
  bytes_per_second: number;
}
