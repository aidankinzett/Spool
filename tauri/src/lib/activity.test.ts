import { describe, it, expect, vi } from 'vitest';
import { buildTimeline, sessionUnlocks, unlockedSince } from '$lib/activity';
import type { AchievementView, PlaySession } from '$lib/types';

vi.mock('$lib/api', () => ({ assetUrl: (p: string) => `asset://${p}` }));

function session(over: Partial<PlaySession> = {}): PlaySession {
  return {
    session_id: 'dev-a:1',
    device_id: 'dev-a',
    device_name: 'Desk',
    game_name: 'Game',
    started_at: '2026-06-03T10:00:00Z',
    ended_at: '2026-06-03T11:00:00Z',
    duration_secs: 3600,
    ...over,
  };
}

function ach(
  api_name: string,
  iso: string | null,
  unlocked_on: string | null = null,
  liveIso: string | null = null,
): AchievementView {
  return {
    api_name,
    display_name: api_name,
    description: '',
    hidden: false,
    icon: null,
    icon_gray: null,
    global_percent: null,
    unlocked_at: iso === null ? null : new Date(iso).getTime() / 1000,
    unlocked_on,
    live_at: liveIso === null ? null : new Date(liveIso).getTime(),
  };
}

/** An unlock this device caught live at `iso`. */
function live(api_name: string, iso: string): AchievementView {
  return ach(api_name, iso, null, iso);
}

describe('sessionUnlocks', () => {
  it('keeps live detections inside the session, with slack for the baseline and final read', () => {
    const items = [
      live('before', '2026-06-03T09:59:00Z'),
      live('lead', '2026-06-03T09:59:57Z'),
      live('mid', '2026-06-03T10:30:00Z'),
      live('exit', '2026-06-03T11:01:30Z'),
      live('after', '2026-06-03T11:03:00Z'),
      ach('locked', null),
    ];
    expect(sessionUnlocks(session(), items).map((a) => a.api_name)).toEqual([
      'lead',
      'mid',
      'exit',
    ]);
  });

  it('ignores imports, even when their unlock time falls inside the session', () => {
    // A stats file with no unlock times: the baseline import at launch stamps
    // `unlocked_at` with the import time, but it was never caught live.
    const items = [ach('imported', '2026-06-03T10:00:01Z'), ach('peer', '2026-06-03T10:30:00Z', 'Deck')];
    expect(sessionUnlocks(session(), items)).toEqual([]);
  });

  it("stops at the next session's start", () => {
    const items = [live('first', '2026-06-03T10:30:00Z'), live('second', '2026-06-03T11:01:00Z')];
    const nextStart = new Date('2026-06-03T11:00:30Z').getTime();
    expect(sessionUnlocks(session(), items, nextStart).map((a) => a.api_name)).toEqual(['first']);
  });

  it('sorts oldest first', () => {
    const items = [live('b', '2026-06-03T10:40:00Z'), live('a', '2026-06-03T10:20:00Z')];
    expect(sessionUnlocks(session(), items).map((a) => a.api_name)).toEqual(['a', 'b']);
  });
});

describe('unlockedSince', () => {
  it('keeps only live detections at or after the cutoff', () => {
    const since = new Date('2026-06-03T10:00:00Z').getTime();
    const items = [
      live('old', '2026-06-03T09:00:00Z'),
      live('new', '2026-06-03T10:10:00Z'),
      ach('imported', '2026-06-03T10:05:00Z'),
    ];
    expect(unlockedSince(items, since).map((a) => a.api_name)).toEqual(['new']);
  });
});

describe('buildTimeline', () => {
  it('groups by local day, newest day and newest entry first', () => {
    const day1 = new Date(2026, 5, 3, 12, 0).toISOString();
    const day1Late = new Date(2026, 5, 3, 23, 30).toISOString();
    const day2 = new Date(2026, 5, 4, 0, 30).toISOString();
    const sessions = [
      session({ session_id: 's1', started_at: day1, ended_at: day1, duration_secs: 1830 }),
      session({ session_id: 's2', started_at: day1Late, ended_at: day2, duration_secs: 3600 }),
    ];
    const items = [ach('A', day1), ach('B', day1Late), ach('C', day2), ach('L', null)];
    const days = buildTimeline(sessions, items, 'dev-a');

    expect(days.map((d) => d.day)).toEqual([
      new Date(2026, 5, 4).getTime(),
      new Date(2026, 5, 3).getTime(),
    ]);
    // The session that crossed midnight lands on the day it ended.
    expect(days[0].entries.map((e) => e.kind)).toEqual(['session', 'unlocks']);
    const [s2, c] = days[0].entries;
    expect(s2.kind === 'session' && s2.minutes).toBe(60);
    expect(s2.kind === 'session' && s2.isThisDevice).toBe(true);
    expect(c.kind === 'unlocks' && c.items.map((a) => a.api_name)).toEqual(['C']);

    const unlocks = days[1].entries.find((e) => e.kind === 'unlocks');
    expect(unlocks?.kind === 'unlocks' && unlocks.items.map((a) => a.api_name)).toEqual(['B', 'A']);
  });

  it('splits one day into an unlocks entry per device', () => {
    const t = new Date(2026, 5, 3, 12, 0).toISOString();
    const days = buildTimeline([], [ach('A', t), ach('B', t, 'Deck')], 'dev-a');
    expect(days).toHaveLength(1);
    const names = days[0].entries.map((e) => (e.kind === 'unlocks' ? e.deviceName : 'x'));
    expect(names.sort()).toEqual(['Deck', null].sort());
  });

  it("marks other devices' sessions once the device id is known", () => {
    const peer = [session({ device_id: 'dev-b' })];
    const known = buildTimeline(peer, [], 'dev-a')[0].entries[0];
    expect(known.kind === 'session' && known.isThisDevice).toBe(false);
    const unknown = buildTimeline(peer, [], null)[0].entries[0];
    expect(unknown.kind === 'session' && unknown.isThisDevice).toBe(true);
  });
});
