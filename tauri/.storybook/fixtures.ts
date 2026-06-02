/**
 * Shared fixtures for screen-level stories (splash, settings, add, edit).
 *
 * These mirror the Rust serde types in `src/lib/types.ts`. Each `make*`
 * helper returns a fully-populated object so stories override only the
 * fields they care about.
 */
import type {
  ConfigData,
  GameEntry,
  SearchCandidate,
  DepStatus,
  DownloadProgress,
  UploadSnapshot,
  LanPeer,
  PeerGame,
} from '../src/lib/types';

/** A fully-populated GameEntry. Matches the fixture in GameDetail.test.ts. */
export function makeGame(over: Partial<GameEntry> = {}): GameEntry {
  return {
    id: 'g1',
    catalog_number: 1,
    game_name: 'Hollow Knight',
    exe_path: 'C:/Games/HollowKnight/hollow_knight.exe',
    safe_name: 'hollow-knight',
    cover_image_path: null,
    hero_image_path: null,
    added_at: '2026-01-12T09:00:00Z',
    last_played_at: '2026-05-28T21:14:00Z',
    launcher_exe_path: null,
    game_folder_path: 'C:/Games/HollowKnight',
    run_as_admin: false,
    use_proton: false,
    proton_version_path: null,
    wine_prefix_path: null,
    launch_args: null,
    description:
      'A 2D action-adventure through a vast, ruined kingdom of insects and heroes. Explore twisting caverns, battle tainted creatures and escape ancient labyrinths.',
    developer: 'Team Cherry',
    publisher: 'Team Cherry',
    genres: ['Metroidvania', 'Action', 'Adventure'],
    release_date: '2017-02-24',
    install_size_mb: 9216,
    playtime_minutes: 1873,
    lan_shared: false,
    lan_share_folder: null,
    save_backup_count: 12,
    save_last_backed_up_at: '2026-05-28T23:02:00Z',
    save_backup_size_mb: 34,
    install_source: 'Steam',
    lan_install_source_device_name: null,
    lan_install_source_device_id: null,
    steam_id: 367520,
    gog_id: null,
    lutris_slug: null,
    manifest_install_dir: null,
    save_paths: ['C:/Users/you/AppData/LocalLow/Team Cherry/Hollow Knight'],
    accent_color: '#6fb7c9',
    sync_badge: 'synced',
    cloud_sync_baseline: null,
    ...over,
  };
}

/** A fully-populated ConfigData with cloud + LAN unconfigured by default. */
export function makeConfig(over: Partial<ConfigData> = {}): ConfigData {
  return {
    steamgriddb_enabled: true,
    steamgriddb_api_key: '',
    spool_exe: 'C:/Users/you/AppData/Local/Spool/spool.exe',
    device_id: 'dev-abc123',
    device_name: 'Desktop-PC',
    lan_share_enabled: true,
    lan_share_port: 47632,
    lan_install_dir: 'C:/Users/you/AppData/Local/Spool/lan-games',
    lan_download_max_mbps: 0,
    umu_run_path: '',
    default_proton_path: '',
    ui_mode: 'auto',
    tray_intro_seen: true,
    onboarding_completed: true,
    cloud_provider: '',
    cloud_remote: '',
    cloud_base_path: 'spool',
    cloud_path: '',
    rclone_args: '',
    cloud_webdav_url: '',
    cloud_webdav_username: '',
    save_retention_full: 3,
    ...over,
  };
}

/** A ludusavi search candidate. */
export function makeCandidate(over: Partial<SearchCandidate> = {}): SearchCandidate {
  return {
    name: 'Hollow Knight',
    score: 1,
    save_path: null,
    save_paths: ['C:/Users/you/AppData/LocalLow/Team Cherry/Hollow Knight'],
    steam_id: 367520,
    gog_id: null,
    lutris_slug: null,
    manifest_install_dir: 'Hollow Knight',
    manifest_install_dirs: ['Hollow Knight'],
    install_root: null,
    ...over,
  };
}

/** A ranked candidate list for the Add Game "matches" / "search" states. */
export const SAMPLE_CANDIDATES: SearchCandidate[] = [
  makeCandidate({ name: 'Hollow Knight', score: 0.98, steam_id: 367520 }),
  makeCandidate({ name: 'Hollow Knight: Silksong', score: 0.71, steam_id: 1030300 }),
  makeCandidate({ name: 'Hollow', score: 0.42, steam_id: 522260 }),
];

/** A reachable, fully-found dependency-doctor result (Linux settings). */
export const SAMPLE_DEPS: DepStatus[] = [
  { name: 'umu-run', found: true, path: '/usr/bin/umu-run', source: 'system', install_hint: '' },
  { name: 'ludusavi', found: true, path: '/opt/spool/ludusavi', source: 'bundled', install_hint: '' },
  { name: 'rclone', found: true, path: '/opt/spool/rclone', source: 'bundled', install_hint: '' },
];

/** An in-flight LAN download. */
export function makeDownload(over: Partial<DownloadProgress> = {}): DownloadProgress {
  return {
    install_token: 'tok-1',
    source_device_id: 'dev-deck',
    source_device_name: 'Steam Deck',
    source_game_id: 'pg1',
    game_name: 'Celeste',
    bytes_done: 620 * 1024 * 1024,
    bytes_total: 1280 * 1024 * 1024,
    current_file: 'Celeste/Content/textures.bank',
    status: 'transferring',
    message: null,
    new_game_id: null,
    bytes_per_second: 48 * 1024 * 1024,
    cover_image_path: null,
    ...over,
  };
}

/** A couple of in-flight uploads (this device serving peers). */
export const SAMPLE_UPLOADS: UploadSnapshot[] = [
  {
    session_id: 'up-1',
    game_id: 'g1',
    game_name: 'Hollow Knight',
    peer_addr: '192.168.1.42',
    last_seen_ago_secs: 1,
    cancelled: false,
    bytes_total: 9216 * 1024 * 1024,
    bytes_sent: 3100 * 1024 * 1024,
  },
  {
    session_id: 'up-2',
    game_id: 'g2',
    game_name: 'Stardew Valley',
    peer_addr: '192.168.1.51',
    last_seen_ago_secs: 4,
    cancelled: false,
    bytes_total: 520 * 1024 * 1024,
    bytes_sent: 480 * 1024 * 1024,
  },
];

/** LAN peers on the network. */
export const SAMPLE_PEERS: LanPeer[] = [
  {
    device_id: 'dev-deck',
    device_name: 'Steam Deck',
    addr: '192.168.1.42',
    game_count: 14,
    version: 1,
    file_server_port: 47632,
    last_seen_ago_secs: 2,
  },
  {
    device_id: 'dev-ally',
    device_name: 'ROG Ally',
    addr: '192.168.1.51',
    game_count: 7,
    version: 1,
    file_server_port: 47632,
    last_seen_ago_secs: 5,
  },
];

/** Games served by a peer. */
export const SAMPLE_PEER_GAMES: PeerGame[] = [
  {
    id: 'pg1',
    catalog_number: 3,
    game_name: 'Celeste',
    developer: 'Maddy Makes Games',
    publisher: 'Maddy Makes Games',
    genres: ['Platformer'],
    install_size_mb: 1280,
    release_date: '2018-01-25',
    steam_id: 504230,
    gog_id: null,
    lutris_slug: null,
    shareable: true,
  },
  {
    id: 'pg2',
    catalog_number: 8,
    game_name: 'Stardew Valley',
    developer: 'ConcernedApe',
    publisher: 'ConcernedApe',
    genres: ['Simulation', 'RPG'],
    install_size_mb: 520,
    release_date: '2016-02-26',
    steam_id: 413150,
    gog_id: null,
    lutris_slug: null,
    shareable: true,
  },
];

/** A multi-entry library for the main-window stories. */
export const SAMPLE_LIBRARY: GameEntry[] = [
  makeGame(),
  makeGame({
    id: 'g2',
    catalog_number: 2,
    game_name: 'Stardew Valley',
    safe_name: 'stardew-valley',
    developer: 'ConcernedApe',
    publisher: 'ConcernedApe',
    genres: ['Simulation', 'RPG'],
    accent_color: '#8bbf5a',
    steam_id: 413150,
    playtime_minutes: 9042,
    install_size_mb: 520,
    last_played_at: '2026-06-01T19:30:00Z',
    sync_badge: 'cloud-newer',
  }),
  makeGame({
    id: 'g3',
    catalog_number: 3,
    game_name: 'Celeste',
    safe_name: 'celeste',
    developer: 'Maddy Makes Games',
    genres: ['Platformer'],
    accent_color: '#c95ec0',
    steam_id: 504230,
    playtime_minutes: 0,
    last_played_at: null,
    save_backup_count: 0,
    save_last_backed_up_at: null,
    sync_badge: null,
  }),
  makeGame({
    id: 'g4',
    catalog_number: 4,
    game_name: 'Hades',
    safe_name: 'hades',
    developer: 'Supergiant Games',
    genres: ['Roguelike', 'Action'],
    accent_color: '#e0703a',
    steam_id: 1145360,
    playtime_minutes: 3120,
    last_played_at: '2026-05-20T12:00:00Z',
    sync_badge: 'local-newer',
  }),
];
