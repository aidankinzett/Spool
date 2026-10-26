import { Bookmark, CircleX, Gamepad2, Trophy } from '@lucide/svelte';
import type { DisplayGame } from '$lib/types';

/** Where the user is with a game. Mirrors `PLAY_STATUSES` in library.rs. */
export type PlayStatus = 'playing' | 'to-play' | 'finished' | 'dropped';

export type PlayStatusDef = {
  id: PlayStatus;
  label: string;
  icon: typeof Gamepad2;
  color: string;
};

/** Display order for the sidebar, shelf tabs and pickers. */
export const PLAY_STATUSES: PlayStatusDef[] = [
  { id: 'playing', label: 'Playing', icon: Gamepad2, color: 'var(--color-ok)' },
  { id: 'to-play', label: 'To Play', icon: Bookmark, color: 'var(--color-info)' },
  { id: 'finished', label: 'Finished', icon: Trophy, color: 'var(--color-spool)' },
  { id: 'dropped', label: 'Dropped', icon: CircleX, color: 'var(--color-ink-2)' },
];

/** The definition for a game's status, or null when it has none or has one this
 *  build doesn't know (synced from a newer Spool). */
export function playStatusOf(g: { play_status: string | null }): PlayStatusDef | null {
  return PLAY_STATUSES.find((s) => s.id === g.play_status) ?? null;
}

/** Number of games per status. Statuses this build doesn't know aren't counted. */
export function countByStatus(games: { play_status: string | null }[]): Record<PlayStatus, number> {
  const m = Object.fromEntries(PLAY_STATUSES.map((s) => [s.id, 0])) as Record<PlayStatus, number>;
  for (const g of games) {
    if (g.play_status && g.play_status in m) m[g.play_status as PlayStatus] += 1;
  }
  return m;
}

/** Narrow a display list to one status. A null status passes the list through. */
export function scopeToStatus(games: DisplayGame[], status: PlayStatus | null): DisplayGame[] {
  if (!status) return games;
  return games.filter((g) => g.play_status === status);
}
