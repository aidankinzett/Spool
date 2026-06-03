import type { LibraryGame } from "../../types";
import { formatPlaytime } from "../../lib/format";
import { BadgeShell } from "./badge-shell";

export function SpoolPlaytimeBadge({ game }: { game: LibraryGame }) {
  if (game.playtime_minutes <= 0) return null;

  return (
    <BadgeShell>
      <div style={{ display: "flex", flexDirection: "column", gap: "0", alignItems: "center" }}>
        <div>
          PLAY TIME
        </div>
        <div style={{ color: "#79c0ff" }}>
          {formatPlaytime(game.playtime_minutes)}
        </div>
      </div>
    </BadgeShell>
  );
}
