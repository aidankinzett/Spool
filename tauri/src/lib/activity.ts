/**
 * Pure helpers for the post-game summary and the activity timeline. They join
 * a game's play sessions (`list_play_sessions`) with its achievement list
 * (`get_game_achievements`): an unlock belongs to a session when its time falls
 * inside the session and it was recorded on the same device.
 */

import { assetUrl } from './api';
import type { AchievementView, PlaySession } from './types';

/** Seconds before a session's start an unlock still counts, for clock skew
 *  between the emulator's timestamp and the session record. */
const BEFORE_SECS = 60;
/** Seconds after a session's end an unlock still counts. The achievement files
 *  are read once more after the game exits, and an emulator may write its stats
 *  file during shutdown. */
const AFTER_SECS = 120;

/** Unix seconds for an RFC 3339 timestamp; NaN when it doesn't parse. */
function secs(iso: string): number {
  return new Date(iso).getTime() / 1000;
}

/** Whether an unlock was recorded on the session's device. `unlocked_on` is
 *  null for this device's unlocks and names the other device otherwise. */
function sameDevice(a: AchievementView, session: PlaySession, isThisDevice: boolean): boolean {
  return isThisDevice ? a.unlocked_on === null : a.unlocked_on === session.device_name;
}

/** The achievements unlocked during `session`, oldest first. */
export function sessionUnlocks(
  session: PlaySession,
  items: AchievementView[],
  isThisDevice: boolean,
): AchievementView[] {
  const start = secs(session.started_at) - BEFORE_SECS;
  const end = secs(session.ended_at) + AFTER_SECS;
  if (Number.isNaN(start) || Number.isNaN(end)) return [];
  return items
    .filter(
      (a) =>
        a.unlocked_at !== null &&
        a.unlocked_at >= start &&
        a.unlocked_at <= end &&
        sameDevice(a, session, isThisDevice),
    )
    .sort((a, b) => (a.unlocked_at ?? 0) - (b.unlocked_at ?? 0));
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
      isThisDevice: s.device_id === thisDeviceId,
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

/** Icon URL for an achievement: the colour icon once unlocked, the grey one
 *  while locked. Local schema icons are app-data paths served via `asset:`. */
export function achievementIconSrc(a: AchievementView): string | null {
  const src = a.unlocked_at !== null ? a.icon : (a.icon_gray ?? a.icon);
  if (!src) return null;
  return src.startsWith('http') ? src : assetUrl(src);
}
