import { ButtonItem, PanelSectionRow } from "@decky/ui";
import { useEffect, useState } from "react";
import type { LibraryGame } from "../types";
import { useServerBase } from "../hooks/use-server-base";
import { addLibraryGameShortcut, existingShortcutAppId } from "../lib/launch";

// QAM list of every Spool game. Games not yet added to Steam are selectable and
// adding one creates a non-Steam shortcut (no launch) so its details show on the
// Steam game page. Games that already have a shortcut are shown as added/inert.
export function AddToSteamList() {
  const { base, error: baseError } = useServerBase();
  const [games, setGames] = useState<LibraryGame[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [added, setAdded] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState<string | null>(null);

  useEffect(() => {
    if (!base) return;
    let cancelled = false;
    void (async () => {
      try {
        const res = await fetch(`${base}/library`);
        const data: unknown = await res.json();
        if (cancelled) return;
        if (res.ok && Array.isArray(data)) {
          const list = data as LibraryGame[];
          setGames(list);
          // Seed the added set from shortcuts Steam already knows about.
          setAdded(new Set(list.filter((g) => existingShortcutAppId(g) != null).map((g) => g.id)));
        } else {
          setError("Couldn't load your library.");
        }
      } catch {
        if (!cancelled) setError("Couldn't load your library.");
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [base]);

  const add = async (game: LibraryGame) => {
    if (!base) return;
    setBusy(game.id);
    const appid = await addLibraryGameShortcut(base, game.id, game.shortcut_app_id ?? null);
    if (appid != null) setAdded((prev) => new Set(prev).add(game.id));
    setBusy(null);
  };

  const err = baseError ?? error;
  if (err) return <PanelSectionRow><div style={{ opacity: 0.8 }}>{err}</div></PanelSectionRow>;
  if (!games) return <PanelSectionRow><div style={{ opacity: 0.7 }}>Loading…</div></PanelSectionRow>;
  if (games.length === 0)
    return <PanelSectionRow><div style={{ opacity: 0.7 }}>No games in your library yet.</div></PanelSectionRow>;

  return (
    <>
      {games.map((game) => {
        const isAdded = added.has(game.id);
        const isBusy = busy === game.id;
        return (
          <PanelSectionRow key={game.id}>
            <ButtonItem
              layout="inline"
              label={game.game_name}
              disabled={isAdded || isBusy}
              onClick={() => void add(game)}
            >
              {isAdded ? "✓ Added" : isBusy ? "Adding…" : "Add to Steam"}
            </ButtonItem>
          </PanelSectionRow>
        );
      })}
    </>
  );
}
