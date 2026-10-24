import { describe, it, expect } from 'vitest';
import { recentGames, sortGames } from '$lib/home';

const game = (
  game_name: string,
  over: { last_played_at?: string | null; playtime_minutes?: number; install_size_mb?: number } = {},
) => ({
  game_name,
  last_played_at: over.last_played_at ?? null,
  playtime_minutes: over.playtime_minutes ?? 0,
  install_size_mb: over.install_size_mb ?? 0,
});

describe('recentGames', () => {
  it('keeps played games, newest first, up to the limit', () => {
    const list = [
      game('A', { last_played_at: '2026-01-01T00:00:00Z' }),
      game('B'),
      game('C', { last_played_at: '2026-03-01T00:00:00Z' }),
      game('D', { last_played_at: '2026-02-01T00:00:00Z' }),
    ];
    expect(recentGames(list, 2).map((g) => g.game_name)).toEqual(['C', 'D']);
  });
});

describe('sortGames', () => {
  const list = [
    game('beta', { playtime_minutes: 10, install_size_mb: 5 }),
    game('Alpha', { last_played_at: '2026-01-01T00:00:00Z', playtime_minutes: 10 }),
    game('gamma', { last_played_at: '2026-02-01T00:00:00Z', install_size_mb: 50 }),
  ];

  it('sorts by name case-insensitively', () => {
    expect(sortGames(list, 'name').map((g) => g.game_name)).toEqual(['Alpha', 'beta', 'gamma']);
  });

  it('sorts unplayed games last under recent', () => {
    expect(sortGames(list, 'recent').map((g) => g.game_name)).toEqual(['gamma', 'Alpha', 'beta']);
  });

  it('breaks playtime ties by name', () => {
    expect(sortGames(list, 'playtime').map((g) => g.game_name)).toEqual(['Alpha', 'beta', 'gamma']);
  });

  it('sorts by size, largest first', () => {
    expect(sortGames(list, 'size').map((g) => g.game_name)).toEqual(['gamma', 'beta', 'Alpha']);
  });

  it('does not mutate the input', () => {
    const before = list.map((g) => g.game_name);
    sortGames(list, 'size');
    expect(list.map((g) => g.game_name)).toEqual(before);
  });
});
