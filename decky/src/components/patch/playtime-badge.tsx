import type { LibraryGame } from "../../types";
import { formatPlaytime, formatRelativeTime } from "../../lib/format";

// Badge injected on the Steam /library/app/:appid page when Spool has a match.
export function SpoolPlaytimeBadge({ game }: { game: LibraryGame }) {
  if (game.playtime_minutes <= 0) return null;
  const lastPlayed = game.last_played_at ? formatRelativeTime(game.last_played_at) : null;

  const sep = <span style={{ opacity: 0.3, margin: "0 0.3rem" }}>·</span>;

  return (
    <div
      style={{
        display: "inline-flex",
        alignItems: "center",
        padding: "0.75rem 0.75rem",
        borderRadius: "4px",
        background: "rgba(255,255,255,0.08)",
        fontSize: "0.8rem",
        fontWeight: 600,
      }}
    >
      <span style={{ opacity: 0.6, marginRight: "0.4rem" }}>💾</span>
      {formatPlaytime(game.playtime_minutes)} played
      {lastPlayed && <>{sep}Last played {lastPlayed}</>}
    </div>
  );
}
