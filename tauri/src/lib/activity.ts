/**
 * Pure helpers for the post-game summary and the activity timeline. They join
 * a game's play sessions (`list_play_sessions`) with its achievement list
 * (`get_game_achievements`): an unlock belongs to one of this device's
 * sessions when the session's achievement watcher caught it (`live_at`).
 *
 * No Tauri imports: the Decky plugin imports this file too.
 */

import type { AchievementView, PlaySession } from './types';

/** The achievement watcher takes its baseline just before the session's start
 *  time is stamped, so a live detection can precede `started_at` slightly. */
const WATCH_LEAD_MS = 5_000;
/** The watcher reads the files once more after the game exits, after
 *  `ended_at` is stamped. */
const FINAL_READ_MS = 120_000;

function byLiveAt(a: AchievementView, b: AchievementView): number {
  return (a.live_at ?? 0) - (b.live_at ?? 0);
}

/**
 * The achievements a Spool session on this device caught live at or after
 * `sinceMs`, oldest first. Matches on `live_at` (when Spool saw the unlock)
 * rather than `unlocked_at`, which falls back to the import time for stats
 * files that record no unlock times — so the baseline import at launch would
 * otherwise look like a session's worth of new unlocks.
 */
export function unlockedSince(items: AchievementView[], sinceMs: number): AchievementView[] {
  return items.filter((a) => a.live_at !== null && a.live_at >= sinceMs).sort(byLiveAt);
}

/**
 * The achievements this device's `session` caught live, oldest first.
 * `nextStartMs` is the start of this device's following session, if any, so a
 * quick relaunch can't claim the same unlock twice.
 */
export function sessionUnlocks(
  session: PlaySession,
  items: AchievementView[],
  nextStartMs: number | null = null,
): AchievementView[] {
  const start = Date.parse(session.started_at) - WATCH_LEAD_MS;
  let end = Date.parse(session.ended_at) + FINAL_READ_MS;
  if (Number.isNaN(start) || Number.isNaN(end)) return [];
  if (nextStartMs !== null) end = Math.min(end, nextStartMs - WATCH_LEAD_MS);
  return items
    .filter((a) => a.live_at !== null && a.live_at >= start && a.live_at < end)
    .sort(byLiveAt);
}

export type TimelineEntry =
  | {
      kind: 'session';
      session: PlaySession;
      /** Whole minutes played. */
      minutes: number;
      isThisDevice: boolean;
      /** Unix ms the entry sorts by (session end). */
      at: number;
    }
  | {
      kind: 'unlocks';
      /** Newest first. */
      items: AchievementView[];
      /** Other device's name; null for this device. */
      deviceName: string | null;
      /** Unix ms of the newest unlock in the group. */
      at: number;
    };

export type TimelineDay = {
  /** Local midnight of the day, unix ms. */
  day: number;
  /** Newest first. */
  entries: TimelineEntry[];
};

/** Local midnight of `ms`, built from (y, m, d) so DST days stay whole. */
function localMidnight(ms: number): number {
  const d = new Date(ms);
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
}

/**
 * Sessions and unlocks grouped by local calendar day, newest day first. Each
 * day has one entry per session (keyed to the day it ended) and one unlocks
 * entry per device that unlocked something that day.
 */
export function buildTimeline(
  sessions: PlaySession[],
  items: AchievementView[],
  thisDeviceId: string | null,
): TimelineDay[] {
  const days = new Map<number, TimelineEntry[]>();
  const push = (at: number, e: TimelineEntry) => {
    const key = localMidnight(at);
    const list = days.get(key);
    if (list) list.push(e);
    else days.set(key, [e]);
  };

  for (const s of sessions) {
    const at = new Date(s.ended_at).getTime();
    if (Number.isNaN(at)) continue;
    push(at, {
      kind: 'session',
      session: s,
      minutes: Math.round(s.duration_secs / 60),
      // Until the device id is known, treat every session as this device's so
      // no device chips flash up and then vanish.
      isThisDevice: thisDeviceId === null || s.device_id === thisDeviceId,
      at,
    });
  }

  // Unlocks per (day, device).
  const groups = new Map<string, AchievementView[]>();
  for (const a of items) {
    if (a.unlocked_at === null) continue;
    const key = `${localMidnight(a.unlocked_at * 1000)}|${a.unlocked_on ?? ''}`;
    const list = groups.get(key);
    if (list) list.push(a);
    else groups.set(key, [a]);
  }
  for (const group of groups.values()) {
    group.sort((a, b) => (b.unlocked_at ?? 0) - (a.unlocked_at ?? 0));
    const at = (group[0].unlocked_at ?? 0) * 1000;
    push(at, { kind: 'unlocks', items: group, deviceName: group[0].unlocked_on, at });
  }

  return [...days.entries()]
    .sort(([a], [b]) => b - a)
    .map(([day, entries]) => ({ day, entries: entries.sort((a, b) => b.at - a.at) }));
}
