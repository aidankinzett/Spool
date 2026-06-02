/**
 * Shared fixtures for screen-level stories (splash, settings, add, edit).
 *
 * These mirror the Rust serde types in `src/lib/types.ts`. Each `make*`
 * helper returns a fully-populated object so stories override only the
 * fields they care about.
 */
import type { ConfigData, GameEntry, SearchCandidate, DepStatus } from '../src/lib/types';

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
    manifest_install_dir: null,
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
