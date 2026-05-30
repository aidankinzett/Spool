// Typed wrappers around Tauri's `invoke` IPC bridge. All backend calls go
// through this module — gives us a single place to add caching, mocking for
// tests, or telemetry later, and keeps `invoke<T>(...)` ceremony out of
// every component.

import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import type {
  AddToSteamResult,
  ConfigData,
  DownloadProgress,
  GameEntry,
  DepStatus,
  LanPeer,
  NewGame,
  PeerGame,
  ProtonVersion,
  SyncStatus,
  UploadSnapshot,
  SearchCandidate,
  RawConflictDetails,
} from './types';

/** Status of the companion Spool Backup Decky plugin (mirrors the Rust
 *  `DeckyPluginInfo` in `decky_install.rs`). Defined here rather than in
 *  types.ts because it's only consumed through the two api methods below. */
export interface DeckyPluginInfo {
  /** This platform can install the plugin at all (Linux/SteamOS only). */
  supported: boolean;
  /** A copy already exists in ~/homebrew/plugins/spool-backup. */
  installed: boolean;
  /** Version from the installed plugin's package.json, if any. */
  installedVersion: string | null;
  /** Version of the plugin embedded in this Spool build. */
  bundledVersion: string;
  /** Decky Loader itself appears installed (~/homebrew exists). */
  deckyPresent: boolean;
}

export const api = {
  // Library
  listGames: (): Promise<GameEntry[]> => invoke('list_games'),
  addGame: (newGame: NewGame): Promise<GameEntry> => invoke('add_game', { newGame }),
  updateGame: (entry: GameEntry): Promise<GameEntry> => invoke('update_game', { entry }),
  removeGame: (id: string): Promise<boolean> => invoke('remove_game', { id }),

  // Config
  getConfig: (): Promise<ConfigData> => invoke('get_config'),
  updateConfig: (data: ConfigData): Promise<ConfigData> => invoke('update_config', { data }),
  detectLudusavi: (): Promise<string> => invoke('detect_ludusavi'),
  detectUmuRun: (): Promise<string> => invoke('detect_umu_run'),
  appPlatform: (): Promise<string> => invoke('app_platform'),
  checkDependencies: (): Promise<DepStatus[]> => invoke('check_dependencies'),

  // Decky plugin installer (SteamOS / Linux)
  deckyPluginStatus: (): Promise<DeckyPluginInfo> => invoke('decky_plugin_status'),
  installDeckyPlugin: (): Promise<void> => invoke('install_decky_plugin'),

  // Proton / Linux launch
  listProtonVersions: (): Promise<ProtonVersion[]> => invoke('list_proton_versions'),
  installProtonDeps: (gameId: string, verbs: string): Promise<string> =>
    invoke('install_proton_deps', { gameId, verbs }),

  // Ludusavi — Add Game flow
  searchGames: (query: string): Promise<SearchCandidate[]> => invoke('search_games', { query }),
  searchByExe: (exePath: string): Promise<SearchCandidate[]> =>
    invoke('search_by_exe', { exePath }),
  openLudusaviGui: (): Promise<void> => invoke('open_ludusavi_gui'),
  setCloudWebdav: (
    url: string,
    username: string,
    password: string,
    provider: string
  ): Promise<void> => invoke('set_cloud_webdav', { url, username, password, provider }),

  // SteamGridDB
  fetchCover: (gameId: string): Promise<string | null> => invoke('fetch_cover', { gameId }),

  // Steam shortcut
  addToSteam: (gameId: string): Promise<AddToSteamResult> => invoke('add_to_steam', { gameId }),

  // Armoury Crate launcher
  generateArmouryLauncher: (gameId: string): Promise<string> =>
    invoke('generate_armoury_launcher', { gameId }),

  // Windows registry compat-flag probe — true when the OS has the
  // exe flagged "always run as administrator" via AppCompatFlags.
  getRunAsAdminInRegistry: (exePath: string): Promise<boolean> =>
    invoke('get_run_as_admin_in_registry', { exePath }),

  // Sync server
  currentSyncStatus: (): Promise<SyncStatus> => invoke('current_sync_status'),
  refreshSyncStatus: (): Promise<SyncStatus> => invoke('refresh_sync_status'),
  syncRegisterAccount: (
    serverUrl: string,
    adminSecret: string,
    username: string,
  ): Promise<string> =>
    invoke('sync_register_account', { serverUrl, adminSecret, username }),
  useServerSaveStorage: (): Promise<void> => invoke('use_server_save_storage'),

  // LAN
  listLanPeers: (): Promise<LanPeer[]> => invoke('list_lan_peers'),
  fetchPeerGames: (addr: string, port: number): Promise<PeerGame[]> =>
    invoke('fetch_peer_games', { addr, port }),
  startPeerInstall: (
    peerAddr: string,
    peerPort: number,
    gameId: string,
  ): Promise<string> =>
    invoke('start_peer_install', { peerAddr, peerPort, gameId }),
  currentPeerDownload: (): Promise<DownloadProgress | null> =>
    invoke('current_peer_download'),
  cancelPeerInstall: (installToken: string): Promise<boolean> =>
    invoke('cancel_peer_install', { installToken }),
  listActiveUploads: (): Promise<UploadSnapshot[]> => invoke('list_active_uploads'),
  cancelUpload: (sessionId: string): Promise<boolean> =>
    invoke('cancel_upload', { sessionId }),

  // Run workflow
  launchGame: (gameId: string): Promise<void> => invoke('launch_game', { gameId }),
  manualBackup: (
    gameId: string,
  ): Promise<{ game_count: number; bytes_total: number }> =>
    invoke('manual_backup', { gameId }),
  manualRestore: (gameId: string): Promise<{ game_count: number }> =>
    invoke('manual_restore', { gameId }),
  /**
   * Resolve a cloud-sync conflict in-app by picking which copy wins, then
   * land the reconciled saves. `side` is `'local'` (keep this device, upload)
   * or `'cloud'` (keep the cloud, download). Throws if the resolve fails —
   * the caller surfaces that and keeps the "Open Ludusavi" fallback.
   */
  resolveCloudConflict: (
    gameId: string,
    side: 'local' | 'cloud',
  ): Promise<{ game_count: number }> =>
    invoke('resolve_cloud_conflict', { gameId, side }),
  getCloudConflictDetails: (gameId: string): Promise<RawConflictDetails> =>
    invoke('get_cloud_conflict_details', { gameId }),

  // Lifecycle — pulls + clears the game id queued by a startup `--run` invocation.
  takePendingRun: (): Promise<string | null> => invoke('take_pending_run'),

  // Game-Mode splash — signals that the splash's `run:phase` listener is wired
  // so the attached `--run` workflow can start without racing the first phases.
  notifySplashReady: (): Promise<void> => invoke('notify_splash_ready'),
} as const;

/**
 * Turn an absolute filesystem path (from a `GameEntry`) into a URL that the
 * webview can load via the `asset:` protocol. Returns `null` for null/missing
 * input so callers can use it directly in template expressions.
 */
export function assetUrl(path: string | null | undefined): string | null {
  if (!path) return null;
  return convertFileSrc(path);
}
