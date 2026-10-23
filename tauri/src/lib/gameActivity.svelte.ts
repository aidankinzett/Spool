/**
 * Loads a game's achievement list and play sessions once for the detail pane,
 * so the cards that read them (achievements, activity, post-game summary,
 * timeline) share one fetch. `get_game_achievements` rescans the game's
 * emulator stats files, so each card loading its own copy would repeat that
 * scan on every selection.
 *
 * Call during component initialisation; the effects are torn down with the
 * component.
 */

import { listen } from '@tauri-apps/api/event';
import { api } from './api';
import { isSyntheticPeerId } from './library.svelte';
import type { AchievementUnlockedEvent, GameAchievements, GameEntry, PlaySession } from './types';

export function createGameActivity(getGame: () => GameEntry) {
  /** The last responses, tagged with the game they're for, so switching games
   *  never shows the previous game's data while the new one loads. */
  let achievements = $state<{ id: string; data: GameAchievements | null } | null>(null);
  let sessions = $state<{ name: string; list: PlaySession[] } | null>(null);
  let deviceId = $state<string | null>(null);
  /** Bumped by an unlock event for this game to trigger a reload. */
  let unlockTick = $state(0);

  // The library store replaces every GameEntry object on each
  // `library:changed`, so the effects below key on these primitives, not on
  // the entry, or they'd rerun (rescan, re-subscribe) on every unrelated
  // refresh.
  const gameId = $derived(getGame().id);
  const gameName = $derived(getGame().game_name);
  const lastPlayed = $derived(getGame().last_played_at);
  const playtime = $derived(getGame().playtime_minutes);

  // This device's id. Fetched again on each game change until it resolves, so
  // one failed read doesn't hide the post-game summary for good.
  $effect(() => {
    void gameId;
    if (deviceId !== null) return;
    api
      .getConfig()
      .then((c) => {
        if (c?.device_id) deviceId = c.device_id;
      })
      .catch(() => {});
  });

  // Reload achievements when the selected game changes, after a session ends
  // (last_played_at moves), and on a live unlock for this game.
  $effect(() => {
    const id = gameId;
    void lastPlayed;
    void unlockTick;
    // Peer-only rows (id `peer:…`) aren't library entries; there's nothing to scan.
    if (isSyntheticPeerId(id)) return;
    let stale = false;
    api
      .getGameAchievements(id)
      .then((d) => {
        if (!stale) achievements = { id, data: d ?? null };
      })
      .catch(() => {
        if (!stale) achievements = null;
      });
    return () => {
      stale = true;
    };
  });

  // Reload the per-session history when the selected game changes, and when
  // this game's aggregate moves — a session just ended (or a peer's session was
  // folded in) bumps playtime_minutes / last_played_at and re-emits
  // library:changed, but not game_name. Best-effort: a failure leaves the
  // history empty. (#270)
  $effect(() => {
    const name = gameName;
    void lastPlayed;
    void playtime;
    let stale = false;
    api
      .listPlaySessions(name)
      .then((s) => {
        if (!stale) sessions = { name, list: s ?? [] };
      })
      .catch(() => {
        if (!stale) sessions = { name, list: [] };
      });
    return () => {
      stale = true;
    };
  });

  $effect(() => {
    const id = gameId;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    let unlistenFold: (() => void) | undefined;
    listen<AchievementUnlockedEvent>('achievement:unlocked', (e) => {
      if (e.payload.game_id === id) unlockTick++;
    })
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch(() => {});
    // Unlocks from other devices, folded in from the rclone control plane.
    listen('achievements:changed', () => unlockTick++)
      .then((fn) => {
        if (disposed) fn();
        else unlistenFold = fn;
      })
      .catch(() => {});
    return () => {
      disposed = true;
      unlisten?.();
      unlistenFold?.();
    };
  });

  return {
    /** Null until loaded (or when the fetch failed). */
    get achievements(): GameAchievements | null {
      return achievements?.id === gameId ? achievements.data : null;
    },
    get sessions(): PlaySession[] {
      return sessions?.name === gameName ? sessions.list : [];
    },
    /** This device's id, for telling its sessions from other devices'. */
    get deviceId(): string | null {
      return deviceId;
    },
  };
}
