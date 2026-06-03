import type { LibraryGame } from "../../types";
import { formatRelativeTime } from "../../lib/format";
import { BadgeShell } from "./badge-shell";

export function SpoolLastPlayedBadge({ game }: { game: LibraryGame }) {
    const lastPlayed = game.last_played_at ? formatRelativeTime(game.last_played_at) : null;

    if (!lastPlayed) return null;

    return (
        <BadgeShell>
            <div style={{ display: "flex", flexDirection: "row", gap: "0.25rem", alignItems: "center" }}>
                <div>
                    Last played 
                </div>
                <div style={{ color: "#79c0ff" }}>
                    {lastPlayed}
                </div>
            </div>
        </BadgeShell>
    );
}
