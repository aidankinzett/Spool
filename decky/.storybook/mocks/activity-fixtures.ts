import type { AchievementView, GameAchievements, PlaySession } from "../../src/types";

// Play sessions and achievements for the Activity-tab stories. Times are
// relative to now so "TODAY" / "2h ago" labels stay meaningful.

export const THIS_DEVICE = "deck-1";
const HOUR = 60 * 60 * 1000;
const DAY = 24 * HOUR;

function session(device_id: string, device_name: string, endedAgoMs: number, mins: number): PlaySession {
  const ended = Date.now() - endedAgoMs;
  const started = ended - mins * 60_000;
  return {
    session_id: `${device_id}:${started}`,
    device_id,
    device_name,
    game_name: "Onimusha: Way of the Sword",
    started_at: new Date(started).toISOString(),
    ended_at: new Date(ended).toISOString(),
    duration_secs: mins * 60,
  };
}

export const SESSIONS: PlaySession[] = [
  session("pc-1", "GamingPC", 9 * DAY, 45),
  session(THIS_DEVICE, "Steam Deck", 3 * DAY, 62),
  session("pc-1", "GamingPC", DAY + 2 * HOUR, 145),
  session(THIS_DEVICE, "Steam Deck", 2 * HOUR, 80),
];

const icon = (n: number) => `https://placehold.co/64x64/2b2216/d7c9a0?text=${n}`;

function ach(n: number, name: string, description: string, extra: Partial<AchievementView> = {}): AchievementView {
  return {
    api_name: `ACH_${n}`,
    display_name: name,
    description,
    hidden: false,
    icon: icon(n),
    icon_gray: icon(n),
    global_percent: 40 - n * 3,
    unlocked_at: null,
    unlocked_on: null,
    live_at: null,
    ...extra,
  };
}

const sessionEnd = Date.now() - 2 * HOUR;

export const ACHIEVEMENTS: GameAchievements = {
  appid: 2638890,
  schema_source: "steam",
  total: 8,
  unlocked: 5,
  items: [
    // Caught live during this device's latest session.
    ach(1, "Wide Awake", "Clear the prologue.", {
      unlocked_at: Math.floor((sessionEnd - 30 * 60_000) / 1000),
      live_at: sessionEnd - 30 * 60_000,
    }),
    ach(2, "Untouchable", "Perform 50 Grab Reversals.", {
      global_percent: 4.2,
      unlocked_at: Math.floor((sessionEnd - 5 * 60_000) / 1000),
      live_at: sessionEnd - 5 * 60_000,
    }),
    // Unlocked on another device.
    ach(3, "Helm Splitter", "Break 100 shields or pieces of enemy armor.", {
      unlocked_at: Math.floor((Date.now() - DAY - 3 * HOUR) / 1000),
      unlocked_on: "GamingPC",
    }),
    ach(4, "Winning Is What Matters", "Carry out 50 attacks with environmental objects.", {
      unlocked_at: Math.floor((Date.now() - DAY - 3 * HOUR) / 1000),
      unlocked_on: "GamingPC",
    }),
    ach(5, "Onimusha Showdown", "Defeat the first boss.", {
      unlocked_at: Math.floor((Date.now() - 9 * DAY) / 1000),
      unlocked_on: "GamingPC",
    }),
    ach(6, "Peerless", "Clear the game on the hardest difficulty."),
    ach(7, "Secret Ending", "", { hidden: true, global_percent: 1.1 }),
    ach(8, "Collector", "Find every collectible."),
  ],
};
