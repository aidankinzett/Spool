import { useCallback, useEffect, useState } from "react";
import type { LibraryGame } from "../types";
import { findSpoolGame } from "../lib/launch";
import { getLibrary, triggerFold } from "../lib/server";

// One page's lookup of the Spool game for an appid: the library as it is, then
// the library again after a cross-device fold.
interface Lookup {
  at: number;
  first: Promise<LibraryGame | null>;
  /** `changed`: whether the fold changed anything. */
  folded: Promise<{ game: LibraryGame | null; changed: boolean }>;
}

// Shared by every component on a game page that needs the game (the Spool bar,
// the post-game summary and the Activity tab), so opening a page fetches the
// library and folds once rather than once per component. Reused only briefly.
const SHARE_MS = 10_000;
let shared: { key: string; lookup: Lookup } | null = null;

function lookup(base: string, appid: number): Lookup {
  const key = `${base}|${appid}`;
  if (shared?.key === key && Date.now() - shared.lookup.at < SHARE_MS) return shared.lookup;
  const fetchGame = async () => findSpoolGame(await getLibrary(base), appid);
  const first = fetchGame();
  const folded = first
    .catch(() => null)
    .then(async () => {
      const changed = await triggerFold(base).catch(() => false);
      return { game: await fetchGame(), changed };
    });
  const l = { at: Date.now(), first, folded };
  shared = { key, lookup: l };
  return l;
}

// Hook: fetch the library once and return the Spool game for the given Steam
// appid, then again after a cross-device fold. `folded` turns true once that
// fold has changed something, for data loaded alongside the game to reload.
// `refresh` re-fetches /library on demand (e.g. after a backup finishes) so
// fields like save_last_backed_up_at / sync_badge update in place.
export function useSpoolPlaytime(
  appid: number,
  base: string | null,
): { game: LibraryGame | null; loading: boolean; folded: boolean; refresh: () => Promise<void> } {
  const [game, setGame] = useState<LibraryGame | null>(null);
  const [loading, setLoading] = useState(true);
  const [folded, setFolded] = useState(false);

  const refresh = useCallback(async () => {
    if (!base || !appid) return;
    try {
      setGame(findSpoolGame(await getLibrary(base), appid));
    } catch {
      /* leave the last-known game in place on a transient failure */
    }
  }, [appid, base]);

  useEffect(() => {
    if (!base || !appid) {
      setLoading(false);
      return;
    }
    let cancelled = false;
    const l = lookup(base, appid);
    // The library as it is, so the bar appears fast.
    l.first
      .then((g) => {
        if (!cancelled) setGame(g);
      })
      .catch(() => {});
    // Then after the fold, so playtime and last-played reflect every device.
    l.folded
      .then(({ game: g, changed }) => {
        if (cancelled) return;
        setGame(g);
        setFolded(changed);
        setLoading(false);
      })
      .catch(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [appid, base]);

  return { game, loading, folded, refresh };
}
