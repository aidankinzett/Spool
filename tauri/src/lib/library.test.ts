import { describe, it, expect } from 'vitest';
import { filterGames } from '$lib/library.svelte';
import type { GameEntry } from '$lib/types';

function g(over: Partial<GameEntry> & { id: string; game_name: string }): GameEntry {
  return {
    catalog_number: 1,
    exe_path: '',
    safe_name: '',
    cover_image_path: null,
    hero_image_path: null,
    added_at: null,
    last_played_at: null,
    launcher_exe_path: null,
    game_folder_path: null,
    run_as_admin: false,
    description: '',
    developer: '',
    publisher: '',
    genres: [],
    release_date: null,
    install_size_mb: 0,
    playtime_minutes: 0,
    lan_shared: false,
    lan_share_folder: null,
    save_backup_count: 0,
    save_last_backed_up_at: null,
    save_backup_size_mb: 0,
    install_source: '',
    lan_install_source_device_name: null,
    lan_install_source_device_id: null,
    steam_id: null,
    gog_id: null,
    lutris_slug: null,
    has_cloud_save: false,
    manifest_install_dir: null,
    save_paths: [],
    accent_color: null,
    sync_badge: null,
    use_proton: false,
    proton_version_path: null,
    wine_prefix_path: null,
    launch_args: null,
    ...over,
  };
}

const HOLLOW  = g({ id: 'hk', game_name: 'Hollow Knight', playtime_minutes: 120, last_played_at: '2026-05-20T10:00:00Z' });
const ELDEN   = g({ id: 'er', game_name: 'Elden Ring',    playtime_minutes: 0,   last_played_at: null, added_at: '2026-05-15T10:00:00Z' });
const CELESTE = g({ id: 'ce', game_name: 'Celeste',       playtime_minutes: 0,   last_played_at: null, added_at: null });
const GAMES = [HOLLOW, ELDEN, CELESTE];

describe('filterGames — filter: all', () => {
  it('returns all games with no search query', () => {
    expect(filterGames(GAMES, 'all', '')).toEqual(GAMES);
  });

  it('filters case-insensitively by game_name', () => {
    expect(filterGames(GAMES, 'all', 'hollow')).toEqual([HOLLOW]);
    expect(filterGames(GAMES, 'all', 'HOLLOW')).toEqual([HOLLOW]);
    expect(filterGames(GAMES, 'all', 'eld')).toEqual([ELDEN]);
  });

  it('returns empty array when search matches nothing', () => {
    expect(filterGames(GAMES, 'all', 'xyzzy')).toEqual([]);
  });

  it('ignores whitespace-only search query', () => {
    expect(filterGames(GAMES, 'all', '   ')).toEqual(GAMES);
  });
});

describe('filterGames — filter: played', () => {
  it('returns only games with playtime_minutes > 0', () => {
    expect(filterGames(GAMES, 'played', '')).toEqual([HOLLOW]);
  });

  it('combines played filter with search', () => {
    expect(filterGames(GAMES, 'played', 'elden')).toEqual([]);
    expect(filterGames(GAMES, 'played', 'hollow')).toEqual([HOLLOW]);
  });
});

describe('filterGames — filter: recent', () => {
  it('excludes games with no last_played_at and no added_at', () => {
    const result = filterGames(GAMES, 'recent', '');
    expect(result).not.toContainEqual(CELESTE);
  });

  it('includes games with either last_played_at or added_at', () => {
    const result = filterGames(GAMES, 'recent', '');
    expect(result).toContainEqual(HOLLOW);
    expect(result).toContainEqual(ELDEN);
  });

  it('sorts most-recent first (last_played_at preferred over added_at)', () => {
    const result = filterGames(GAMES, 'recent', '');
    expect(result[0]).toEqual(HOLLOW); // 2026-05-20 > 2026-05-15
    expect(result[1]).toEqual(ELDEN);
  });

  it('combines recent filter with search', () => {
    expect(filterGames(GAMES, 'recent', 'elden')).toEqual([ELDEN]);
  });
});
