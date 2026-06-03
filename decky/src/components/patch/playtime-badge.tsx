import type { LibraryGame } from "../../types";
import { formatPlaytime } from "../../lib/format";
import { BadgeShell } from "./badge-shell";

export function SpoolPlaytimeBadge({ game }: { game: LibraryGame }) {
  if (game.playtime_minutes <= 0) return null;

  return (
    <BadgeShell>
      {formatPlaytime(game.playtime_minutes)} played
    </BadgeShell>
  );
}
