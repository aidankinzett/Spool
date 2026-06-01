import { useEffect, useState } from "react";
import type { LibraryGame } from "../types";
import { findSpoolGame } from "../lib/launch";

// Hook: fetch the library once and return the Spool playtime for the given Steam appid.
export function useSpoolPlaytime(
  appid: number,
  base: string | null,
): { game: LibraryGame | null; loading: boolean } {
  const [game, setGame] = useState<LibraryGame | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    if (!base || !appid) {
      setLoading(false);
      return;
    }
    let cancelled = false;
    void (async () => {
      try {
        // Fetch current library immediately so the badge appears fast.
        const first = await fetch(`${base}/library`);
        if (!first.ok) throw new Error(`bad status ${first.status}`);
        const initial = (await first.json()) as LibraryGame[];
        if (!cancelled) setGame(findSpoolGame(initial, appid));

        // Trigger a cross-device fold in the background, then refresh.
        await fetch(`${base}/fold`, { method: "POST" }).catch(() => undefined);
        if (cancelled) return;

        const second = await fetch(`${base}/library`);
        if (!second.ok) throw new Error(`bad status ${second.status}`);
        const fresh = (await second.json()) as LibraryGame[];
        if (!cancelled) {
          setGame(findSpoolGame(fresh, appid));
          setLoading(false);
        }
      } catch {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [appid, base]);

  return { game, loading };
}
