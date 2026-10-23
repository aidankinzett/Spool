import { describe, it, expect, vi } from 'vitest';
import { buildTimeline, sessionUnlocks } from '$lib/activity';
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

function ach(api_name: string, iso: string | null, unlocked_on: string | null = null): AchievementView {
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
  };
}

describe('sessionUnlocks', () => {
  it('keeps unlocks inside the session window, with slack at both ends', () => {
    const items = [
      ach('before', '2026-06-03T09:58:00Z'),
      ach('skew', '2026-06-03T09:59:30Z'),
      ach('mid', '2026-06-03T10:30:00Z'),
      ach('exit', '2026-06-03T11:01:30Z'),
      ach('after', '2026-06-03T11:03:00Z'),
      ach('locked', null),
    ];
    expect(sessionUnlocks(session(), items, true).map((a) => a.api_name)).toEqual([
      'skew',
      'mid',
      'exit',
    ]);
  });

  it("matches this device's session to this device's unlocks only", () => {
    const items = [ach('here', '2026-06-03T10:30:00Z'), ach('there', '2026-06-03T10:31:00Z', 'Deck')];
    expect(sessionUnlocks(session(), items, true).map((a) => a.api_name)).toEqual(['here']);
    const peer = session({ device_id: 'dev-b', device_name: 'Deck' });
    expect(sessionUnlocks(peer, items, false).map((a) => a.api_name)).toEqual(['there']);
  });

  it('sorts oldest first', () => {
    const items = [ach('b', '2026-06-03T10:40:00Z'), ach('a', '2026-06-03T10:20:00Z')];
    expect(sessionUnlocks(session(), items, true).map((a) => a.api_name)).toEqual(['a', 'b']);
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

  it("marks other devices' sessions", () => {
    const days = buildTimeline([session({ device_id: 'dev-b' })], [], 'dev-a');
    const e = days[0].entries[0];
    expect(e.kind === 'session' && e.isThisDevice).toBe(false);
  });
});
