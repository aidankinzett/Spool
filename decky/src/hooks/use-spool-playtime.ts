import { useCallback, useEffect, useState } from "react";
import type { LibraryGame } from "../types";
import { findSpoolGame } from "../lib/launch";

// Hook: fetch the library once and return the Spool game for the given Steam
// appid. `refresh` re-fetches /library on demand (e.g. after a backup finishes)
// so fields like save_last_backed_up_at / sync_badge update in place.
export function useSpoolPlaytime(
  appid: number,
  base: string | null,
): { game: LibraryGame | null; loading: boolean; refresh: () => Promise<void> } {
  const [game, setGame] = useState<LibraryGame | null>(null);
  const [loading, setLoading] = useState(true);

  const fetchGame = useCallback(async (): Promise<LibraryGame | null> => {
    const res = await fetch(`${base}/library`);
    if (!res.ok) throw new Error(`bad status ${res.status}`);
    const games = (await res.json()) as LibraryGame[];
    return findSpoolGame(games, appid);
  }, [appid, base]);

  const refresh = useCallback(async () => {
    if (!base || !appid) return;
    try {
      setGame(await fetchGame());
    } catch {
      /* leave the last-known game in place on a transient failure */
    }
  }, [appid, base, fetchGame]);

  useEffect(() => {
    if (!base || !appid) {
      setLoading(false);
      return;
    }
    let cancelled = false;
    void (async () => {
      try {
        // Fetch current library immediately so the badge appears fast.
        if (!cancelled) setGame(await fetchGame());

        // Trigger a cross-device fold in the background, then refresh.
        await fetch(`${base}/fold`, { method: "POST" }).catch(() => undefined);
        if (cancelled) return;

        if (!cancelled) {
          setGame(await fetchGame());
          setLoading(false);
        }
      } catch {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [appid, base, fetchGame]);

  return { game, loading, refresh };
}
