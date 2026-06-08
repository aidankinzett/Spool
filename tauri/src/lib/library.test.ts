import { describe, it, expect } from 'vitest';
import {
  filterGames,
  matchLocal,
  dedupKey,
  mergeDisplayGames,
  isSyntheticPeerId,
  downloadMatchesGame,
  sourcesOf,
  shareableSources,
} from '$lib/library.svelte';
import type { GameEntry, LanPeer, PeerGame, PeerSource } from '$lib/types';

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
    installed: true,
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
    manifest_install_dir: null,
    save_paths: [],
    custom_save: null,
    manifest_override: null,
    accent_color: null,
    sync_badge: null,
    cloud_sync_baseline: null,
    save_last_backer_device: null,
    save_cloud_revision_at: null,
    steam_app_id: null,
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

// ── Peer-game merge ────────────────────────────────────────────────────────

function pg(over: Partial<PeerGame> & { id: string; game_name: string }): PeerGame {
  return {
    catalog_number: 0,
    developer: '',
    publisher: '',
    genres: [],
    install_size_mb: 100,
    release_date: null,
    steam_id: null,
    gog_id: null,
    lutris_slug: null,
    shareable: true,
    ...over,
  };
}

const PEER_A: LanPeer = {
  device_id: 'dev-a',
  device_name: 'Deck',
  addr: '192.168.1.10',
  game_count: 1,
  version: 1,
  file_server_port: 47632,
  last_seen_ago_secs: 1,
};
const PEER_B: LanPeer = { ...PEER_A, device_id: 'dev-b', device_name: 'Desktop', addr: '192.168.1.11' };

/** Build the per-device catalogue record mergeDisplayGames takes (peer metadata
 *  captured alongside its games). LanPeer satisfies the PeerMeta subset. */
function catalogs(...entries: [LanPeer, PeerGame[]][]): Record<string, { peer: LanPeer; games: PeerGame[] }> {
  return Object.fromEntries(entries.map(([peer, games]) => [peer.device_id, { peer, games }]));
}

describe('matchLocal', () => {
  it('matches by steam_id first', () => {
    const local = [g({ id: 'x', game_name: 'Other', steam_id: 42 })];
    expect(matchLocal(local, { steam_id: 42, game_name: 'Renamed' })?.id).toBe('x');
  });

  it('falls back to exact game_name when no steam_id', () => {
    const local = [g({ id: 'x', game_name: 'Hollow Knight' })];
    expect(matchLocal(local, { steam_id: null, game_name: 'Hollow Knight' })?.id).toBe('x');
  });

  it('does not match two known, differing steam_ids by name', () => {
    const local = [g({ id: 'x', game_name: 'Hollow Knight', steam_id: 1 })];
    expect(matchLocal(local, { steam_id: 2, game_name: 'Hollow Knight' })).toBeNull();
  });

  it('returns null with no match', () => {
    expect(matchLocal([g({ id: 'x', game_name: 'A' })], { steam_id: null, game_name: 'B' })).toBeNull();
  });
});

describe('dedupKey', () => {
  it('keys by steam_id when present', () => {
    expect(dedupKey({ steam_id: 7, game_name: 'A' })).toBe('sid:7');
  });
  it('keys by normalized name otherwise', () => {
    expect(dedupKey({ steam_id: null, game_name: 'Hollow Knight' })).toBe('name:hollow knight');
  });
});

describe('isSyntheticPeerId', () => {
  it('is true only for synthetic peer-row ids', () => {
    expect(isSyntheticPeerId('peer:sid:42')).toBe(true);
    expect(isSyntheticPeerId('peer:name:celeste')).toBe(true);
    // Real DB ids (incl. an uninstalled-local row a peer offers) are not synthetic.
    expect(isSyntheticPeerId('g6')).toBe(false);
    expect(isSyntheticPeerId('a1b2-uuid')).toBe(false);
  });
});

describe('mergeDisplayGames', () => {
  it('drops a peer copy of a game already installed here', () => {
    const games = [g({ id: 'local', game_name: 'Hollow Knight', installed: true })];
    const out = mergeDisplayGames(games, catalogs([PEER_A, [pg({ id: 'p1', game_name: 'Hollow Knight' })]]));
    expect(out).toHaveLength(1);
    expect(out[0].id).toBe('local');
    expect(out[0].peer_source).toBeUndefined();
  });

  it('annotates an uninstalled local row as downloadable (no duplicate row)', () => {
    const games = [g({ id: 'local', game_name: 'Hollow Knight', installed: false })];
    const out = mergeDisplayGames(games, catalogs([PEER_A, [pg({ id: 'p1', game_name: 'Hollow Knight' })]]));
    expect(out).toHaveLength(1);
    expect(out[0].id).toBe('local');
    expect(out[0].peer_source?.device_id).toBe('dev-a');
    expect(out[0].peer_source?.source_game_id).toBe('p1');
    expect(out[0].peer_sources?.map((s) => s.device_id)).toEqual(['dev-a']);
    // The uninstalled local row adopts the peer's size (the download size), not
    // its own stale/zero recorded value.
    expect(out[0].install_size_mb).toBe(100);
  });

  it('lists every device that offers an uninstalled local game', () => {
    const games = [g({ id: 'local', game_name: 'Hollow Knight', installed: false })];
    // Discovered Desktop-then-Deck; the merge must still order + pick by name.
    const out = mergeDisplayGames(
      games,
      catalogs(
        [PEER_B, [pg({ id: 'p2', game_name: 'Hollow Knight' })]],
        [PEER_A, [pg({ id: 'p1', game_name: 'Hollow Knight' })]],
      ),
    );
    expect(out).toHaveLength(1);
    expect(out[0].id).toBe('local');
    expect(out[0].peer_sources?.map((s) => s.device_id)).toEqual(['dev-a', 'dev-b']);
    expect(out[0].peer_source?.device_id).toBe('dev-a');
  });

  it('adds a synthetic row for a peer game with no local entry', () => {
    const out = mergeDisplayGames([], catalogs([PEER_A, [pg({ id: 'p1', game_name: 'Celeste', steam_id: 504230 })]]));
    expect(out).toHaveLength(1);
    expect(out[0].id).toBe('peer:sid:504230');
    expect(out[0].installed).toBe(false);
    expect(out[0].peer_source?.device_id).toBe('dev-a');
    expect(out[0].peer_sources).toHaveLength(1);
  });

  it('collapses the same game shared by two peers to one synthetic row, listing both sources', () => {
    const out = mergeDisplayGames(
      [],
      catalogs([PEER_A, [pg({ id: 'p1', game_name: 'Celeste' })]], [PEER_B, [pg({ id: 'p2', game_name: 'Celeste' })]]),
    );
    expect(out).toHaveLength(1);
    // Both devices are offered, each keeping its own source_game_id.
    expect(out[0].peer_sources?.map((s) => s.device_id)).toEqual(['dev-a', 'dev-b']);
    expect(out[0].peer_sources?.find((s) => s.device_id === 'dev-b')?.source_game_id).toBe('p2');
  });

  it('picks the primary source by device name regardless of discovery order', () => {
    // Desktop ('dev-b') announced before Deck ('dev-a'); the name-sort still
    // makes 'Deck' the primary, so the default source is deterministic.
    const out = mergeDisplayGames(
      [],
      catalogs([PEER_B, [pg({ id: 'p2', game_name: 'Celeste' })]], [PEER_A, [pg({ id: 'p1', game_name: 'Celeste' })]]),
    );
    expect(out[0].peer_source?.device_id).toBe('dev-a');
    expect(out[0].peer_sources?.map((s) => s.device_name)).toEqual(['Deck', 'Desktop']);
  });

  it('prefers a shareable source as primary over a non-shareable one that sorts first by name', () => {
    // 'Deck' (sorts first alphabetically) lists the game but can't serve it
    // (shareable:false — no folder); 'Desktop' can. The primary must be the
    // device that can actually be downloaded from, so the Download button isn't
    // disabled while a working copy exists.
    const out = mergeDisplayGames(
      [],
      catalogs(
        [PEER_A, [pg({ id: 'p1', game_name: 'Celeste', shareable: false })]],
        [PEER_B, [pg({ id: 'p2', game_name: 'Celeste', shareable: true })]],
      ),
    );
    expect(out).toHaveLength(1);
    expect(out[0].peer_source?.device_id).toBe('dev-b');
    // Both are still listed (the non-shareable one for context), shareable first.
    expect(out[0].peer_sources?.map((s) => s.device_id)).toEqual(['dev-b', 'dev-a']);
  });

  it('ignores an empty catalogue set', () => {
    const out = mergeDisplayGames([g({ id: 'local', game_name: 'A' })], {});
    expect(out).toHaveLength(1);
    expect(out[0].id).toBe('local');
  });
});

describe('sourcesOf / shareableSources', () => {
  function ps(device_id: string, shareable: boolean): PeerSource {
    return { device_id, device_name: device_id, addr: '', file_server_port: 1, source_game_id: 'g', shareable };
  }

  it('sourcesOf reads peer_sources, falling back to the lone peer_source', () => {
    const all = [ps('a', true), ps('b', false)];
    expect(sourcesOf({ peer_sources: all }).map((s) => s.device_id)).toEqual(['a', 'b']);
    expect(sourcesOf({ peer_source: ps('a', true) }).map((s) => s.device_id)).toEqual(['a']);
    expect(sourcesOf({})).toEqual([]);
  });

  it('shareableSources drops devices that can not serve the game', () => {
    const all = [ps('a', true), ps('b', false), ps('c', true)];
    expect(shareableSources({ peer_sources: all }).map((s) => s.device_id)).toEqual(['a', 'c']);
    expect(shareableSources({ peer_source: ps('b', false) })).toEqual([]);
  });
});

describe('downloadMatchesGame', () => {
  function ps(device_id: string, source_game_id: string): PeerSource {
    return { device_id, device_name: device_id, addr: '', file_server_port: 1, source_game_id, shareable: true };
  }
  const sources = [ps('dev-a', 'p1'), ps('dev-b', 'p2')];

  it('matches the primary source', () => {
    expect(
      downloadMatchesGame({ source_game_id: 'p1', source_device_id: 'dev-a' }, { peer_source: sources[0], peer_sources: sources }),
    ).toBe(true);
  });

  it('matches a non-primary source (a chooser pick)', () => {
    expect(
      downloadMatchesGame({ source_game_id: 'p2', source_device_id: 'dev-b' }, { peer_source: sources[0], peer_sources: sources }),
    ).toBe(true);
  });

  it('requires both the game id and device id to match', () => {
    // Right game, wrong device (and the converse) must not match — two peers
    // share a game name but each carries its own source_game_id + device_id.
    expect(downloadMatchesGame({ source_game_id: 'p1', source_device_id: 'dev-b' }, { peer_sources: sources })).toBe(false);
    expect(downloadMatchesGame({ source_game_id: 'pX', source_device_id: 'dev-a' }, { peer_sources: sources })).toBe(false);
  });

  it('is false for a null download', () => {
    expect(downloadMatchesGame(null, { peer_sources: sources })).toBe(false);
  });

  it('falls back to the lone peer_source when peer_sources is absent', () => {
    expect(downloadMatchesGame({ source_game_id: 'p1', source_device_id: 'dev-a' }, { peer_source: ps('dev-a', 'p1') })).toBe(true);
  });
});
