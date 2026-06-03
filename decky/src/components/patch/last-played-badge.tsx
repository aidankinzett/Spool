import type { LibraryGame } from "../../types";
import { formatRelativeTime } from "../../lib/format";
import { BadgeShell } from "./badge-shell";

export function SpoolLastPlayedBadge({ game }: { game: LibraryGame }) {
    const lastPlayed = game.last_played_at ? formatRelativeTime(game.last_played_at) : null;

    if (!lastPlayed) return null;

    return (
        <BadgeShell>
            Last played {lastPlayed}
        </BadgeShell>
    );
}
