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
  RepackInstallResult,
  SyncStatus,
  UploadSnapshot,
  SearchCandidate,
  RawConflictDetails,
  SaveRevision,
  PullResult,
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
  /** Removes the library entry AND deletes its install folder from disk. */
  deleteGameFromDisk: (id: string): Promise<void> => invoke('delete_game_from_disk', { id }),

  // Config
  getConfig: (): Promise<ConfigData> => invoke('get_config'),
  updateConfig: (data: ConfigData): Promise<ConfigData> => invoke('update_config', { data }),
  detectUmuRun: (): Promise<string> => invoke('detect_umu_run'),
  appPlatform: (): Promise<string> => invoke('app_platform'),
  checkDependencies: (): Promise<DepStatus[]> => invoke('check_dependencies'),

  // Gamepad presence — drives the "switch to Gamepad layout?" prompt.
  anyGamepadConnected: (): Promise<boolean> => invoke('any_gamepad_connected'),

  // Decky plugin installer (SteamOS / Linux)
  deckyPluginStatus: (): Promise<DeckyPluginInfo> => invoke('decky_plugin_status'),
  installDeckyPlugin: (): Promise<void> => invoke('install_decky_plugin'),

  // Proton / Linux launch
  listProtonVersions: (): Promise<ProtonVersion[]> => invoke('list_proton_versions'),
  installProtonDeps: (gameId: string, verbs: string): Promise<string> =>
    invoke('install_proton_deps', { gameId, verbs }),

  /** Run a repack `setup.exe` through Proton with the install folder mounted as
   *  a Wine drive (Linux). Resolves when the installer process exits. */
  runRepackInstaller: (
    setupExe: string,
    gameName: string,
    installDirOverride?: string,
    protonVersionOverride?: string,
  ): Promise<RepackInstallResult> =>
    invoke('run_repack_installer', { setupExe, gameName, installDirOverride, protonVersionOverride }),

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
  fetchHero: (gameId: string): Promise<string | null> => invoke('fetch_hero', { gameId }),

  // Steam Store metadata (description, developer, publisher, genres, release
  // date). Resolves true when any empty field was populated.
  fetchMetadata: (gameId: string): Promise<boolean> => invoke('fetch_metadata', { gameId }),

  // Steam shortcut
  addSpoolToSteam: (): Promise<AddToSteamResult> => invoke('add_spool_to_steam'),
  addToSteam: (gameId: string): Promise<AddToSteamResult> => invoke('add_to_steam', { gameId }),

  // Open a file/folder with the OS default handler. Goes through Rust (not
  // the opener plugin) so it can strip the AppImage environment before
  // spawning the host file manager on Linux — see system_open.rs / issue #95.
  openPath: (path: string): Promise<void> => invoke('open_path', { path }),

  // Armoury Crate launcher
  generateArmouryLauncher: (gameId: string): Promise<string> =>
    invoke('generate_armoury_launcher', { gameId }),

  // Windows registry compat-flag probe — true when the OS has the
  // exe flagged "always run as administrator" via AppCompatFlags.
  getRunAsAdminInRegistry: (exePath: string): Promise<boolean> =>
    invoke('get_run_as_admin_in_registry', { exePath }),

  // Cloud control-plane reachability (rclone remote probe)
  currentSyncStatus: (): Promise<SyncStatus> => invoke('current_sync_status'),
  refreshSyncStatus: (): Promise<SyncStatus> => invoke('refresh_sync_status'),

  // Cloud OAuth authentication
  checkCloudRemoteExists: (provider: string): Promise<boolean> =>
    invoke('check_cloud_remote_exists', { provider }),
  connectCloudOAuth: (provider: string): Promise<void> =>
    invoke('connect_cloud_oauth', { provider }),
  cancelCloudOAuth: (): Promise<void> => invoke('cancel_cloud_oauth'),

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
  /**
   * Launch a game through the run workflow. `steal` overrides a *suspended*
   * play-state lock held by another sleeping device (the "Play here instead"
   * path) — the server refuses to steal a live, actively-played lock.
   */
  launchGame: (gameId: string, steal = false): Promise<void> =>
    invoke('launch_game', { gameId, steal }),
  manualBackup: (
    gameId: string,
  ): Promise<{ game_count: number; bytes_total: number }> =>
    invoke('manual_backup', { gameId }),
  manualRestore: (gameId: string): Promise<{ game_count: number }> =>
    invoke('manual_restore', { gameId }),
  /**
   * Pull cloud saves down to this device and restore them to disk, without
   * launching the game ("Sync now"). Pull-only — never uploads. The `outcome`
   * tells the caller what happened: `pulled` (cloud was ahead, now applied),
   * `up_to_date`, `local_newer` (local is ahead, left untouched), or
   * `unconfigured` (no cloud remote). A true divergence rejects with a "cloud
   * sync conflict" error so the caller can open the conflict modal.
   */
  pullCloudSaves: (gameId: string): Promise<PullResult> =>
    invoke('pull_cloud_saves', { gameId }),
  /** List the save revisions ludusavi retains for a game, newest first. */
  listSaveRevisions: (gameId: string): Promise<SaveRevision[]> =>
    invoke('list_save_revisions', { gameId }),
  /**
   * Roll back to an earlier save revision. Restores the chosen backup, then
   * pins it as the new tip (immediate cloud-synced backup) so it isn't
   * clobbered by the next pre-launch restore. Blocked while a game is running.
   */
  restoreSaveRevision: (
    gameId: string,
    backupName: string,
  ): Promise<{ game_count: number }> =>
    invoke('restore_save_revision', { gameId, backupName }),
  /**
   * Refresh a game's save-backup stats (revision count + latest-backup time)
   * from ludusavi's real backup store. Fire-and-forget from the detail view;
   * the backend emits `library:changed` only when something actually changed.
   */
  refreshSaveMetadata: (gameId: string): Promise<void> =>
    invoke('refresh_save_metadata', { gameId }),
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
