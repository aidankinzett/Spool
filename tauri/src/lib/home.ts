/**
 * Pure helpers for the desktop library's Home view: the recently played row
 * and the sort order of the all-games section.
 */

import type { DisplayGame } from './types';

export type HomeSort = 'name' | 'recent' | 'playtime' | 'size';

export const HOME_SORTS: { value: HomeSort; label: string }[] = [
  { value: 'name', label: 'A–Z' },
  { value: 'recent', label: 'Last played' },
  { value: 'playtime', label: 'Playtime' },
  { value: 'size', label: 'Size' },
];

type Sortable = Pick<
  DisplayGame,
  'game_name' | 'last_played_at' | 'playtime_minutes' | 'install_size_mb'
>;

/** Played games, most recent first, capped at `limit`. */
export function recentGames<T extends Sortable>(games: T[], limit: number): T[] {
  return games
    .filter((g) => g.last_played_at != null)
    .sort((a, b) => (b.last_played_at ?? '').localeCompare(a.last_played_at ?? ''))
    .slice(0, limit);
}

const byName = (a: Sortable, b: Sortable) =>
  a.game_name.localeCompare(b.game_name, undefined, { sensitivity: 'base' });

/** A sorted copy of `games`. Ties (and unplayed games under 'recent') fall
 *  back to name order. */
export function sortGames<T extends Sortable>(games: T[], sort: HomeSort): T[] {
  const list = [...games];
  switch (sort) {
    case 'recent':
      return list.sort(
        (a, b) =>
          (b.last_played_at ?? '').localeCompare(a.last_played_at ?? '') || byName(a, b),
      );
    case 'playtime':
      return list.sort((a, b) => b.playtime_minutes - a.playtime_minutes || byName(a, b));
    case 'size':
      return list.sort((a, b) => b.install_size_mb - a.install_size_mb || byName(a, b));
    default:
      return list.sort(byName);
  }
}
