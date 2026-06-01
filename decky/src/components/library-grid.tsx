import { useEffect, useState } from "react";
import type { LibraryGame } from "../types";
import { launchLibraryGame } from "../lib/launch";
import { CoverGrid } from "./cover-grid";

// ── Local library grid ─────────────────────────────────────────────────────
export function LibraryGrid({ base }: { base: string }) {
  const [games, setGames] = useState<LibraryGame[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const res = await fetch(`${base}/library`);
        const data = (await res.json()) as LibraryGame[];
        if (!cancelled) setGames(data);
      } catch {
        if (!cancelled) setError("Couldn’t load your library.");
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [base]);

  const coverUrl = (g: LibraryGame): string | null => {
    if (!g.cover_image_path) return null;
    const file = g.cover_image_path.split(/[/\\]/).pop();
    return file ? `${base}/covers/${encodeURIComponent(file)}` : null;
  };

  if (error) return <div style={{ opacity: 0.8 }}>{error}</div>;
  if (!games) return <div style={{ opacity: 0.7 }}>Loading…</div>;
  if (games.length === 0)
    return <div style={{ opacity: 0.7 }}>No games in your library yet.</div>;

  return (
    <CoverGrid
      onActivate={(id) => {
        const g = games.find((g) => g.id === id);
        void launchLibraryGame(base, id, g?.shortcut_app_id ?? null);
      }}
      tiles={games.map((g) => ({
        key: g.id,
        name: g.game_name,
        coverUrl: coverUrl(g),
        accentColor: g.accent_color,
      }))}
    />
  );
}
