import { useEffect, useState } from "react";
import type { GameAchievements, LibraryGame, PlaySession } from "../types";
import { getGameAchievements, getGameSessions } from "../lib/server";

export interface GameActivity {
  /** Null until loaded, or when the request failed. */
  achievements: GameAchievements | null;
  /** Every device's sessions for the game. */
  sessions: PlaySession[];
  /** This device's id; null until loaded. */
  deviceId: string | null;
}

const EMPTY: GameActivity = { achievements: null, sessions: [], deviceId: null };

// The last load, shared by every component on the page that shows the game's
// activity (the post-game summary above the tabs and the timeline in the
// Activity tab), so the achievements request, which rescans the game's stats
// files, runs once per page. Keyed by everything the load depends on, and
// reused only briefly so a later visit rescans.
const SHARE_MS = 30_000;
let shared: { key: string; at: number; promise: Promise<GameActivity> } | null = null;

function load(base: string, id: string, key: string): Promise<GameActivity> {
  if (shared?.key === key && Date.now() - shared.at < SHARE_MS) return shared.promise;
  const promise = Promise.allSettled([getGameAchievements(base, id), getGameSessions(base, id)]).then(
    ([achievements, sessions]): GameActivity => {
      // Both failed (e.g. the server restarting): don't hand this to the
      // page's other components, so the next one to load retries.
      if (achievements.status === "rejected" && sessions.status === "rejected" && shared?.promise === promise) {
        shared = null;
      }
      return {
        achievements: achievements.status === "fulfilled" ? achievements.value : null,
        sessions: sessions.status === "fulfilled" ? sessions.value.sessions : [],
        deviceId: sessions.status === "fulfilled" ? sessions.value.device_id || null : null,
      };
    },
  );
  shared = { key, at: Date.now(), promise };
  return promise;
}

// Loads a game's achievement list and play sessions. Reloads when the game's
// last-played time moves (a session ended here, or another device's was
// folded in), and once the page's cross-device fold has changed something
// (`folded`, from useSpoolPlaytime), which also covers unlocks folded in from
// other devices.
export function useGameActivity(
  base: string | null,
  game: LibraryGame | null,
  folded: boolean,
): GameActivity {
  const [activity, setActivity] = useState<{ key: string; data: GameActivity } | null>(null);
  const id = game?.id ?? null;
  const key = `${base}|${id}|${game?.last_played_at ?? ""}|${folded}`;

  useEffect(() => {
    if (!base || !id) return;
    let stale = false;
    void load(base, id, key).then((data) => {
      if (!stale) setActivity({ key, data });
    });
    return () => {
      stale = true;
    };
  }, [base, id, key]);

  // Never show the previous game's data while the next one loads; while the
  // same game reloads, keep showing what's there.
  if (!activity || !activity.key.startsWith(`${base}|${id}|`)) return EMPTY;
  return activity.data;
}
