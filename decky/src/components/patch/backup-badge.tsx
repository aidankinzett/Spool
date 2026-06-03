import type { LibraryGame } from "../../types";
import { formatRelativeTime } from "../../lib/format";
import { BadgeShell } from "./badge-shell";
import { Spinner } from "@decky/ui";

// Maps the Rust `sync_badge` value (see library.rs) to a short label + colour.
// `null` means there's nothing to say about cloud sync (cloud not configured or
// no backup history), so fall back to the relative backup time.
function syncState(badge: string | null): { label: string; color: string } | null {
  switch (badge) {
    case "synced":
      return { label: "Synced", color: "#7ee787" };
    case "local-newer":
      return { label: "Not yet uploaded", color: "#e3b341" };
    case "cloud-newer":
      return { label: "Cloud has newer save", color: "#79c0ff" };
    default:
      return null;
  }
}

// Badge injected on the Steam /library/app/:appid page showing Spool's save
// backup status: when the save was last backed up, coloured by its cloud-sync
// state. While `backingUp` is set (a forced-close fallback backup is running) it
// shows a spinner instead, regardless of any prior backup history.
export function SpoolBackupBadge({
  game,
  backingUp,
}: {
  game: LibraryGame | null;
  backingUp: boolean;
}) {
  if (backingUp) {
    return (
      <BadgeShell>
        <div style={{ display: "flex", flexDirection: "column", gap: "0.125rem", alignItems: "center" }}>
          <div>
            SAVE BACKUP
          </div>
          <Spinner style={{ width: "14px", height: "14px" }} />
        </div>
      </BadgeShell>
    );
  }

  if (!game?.save_last_backed_up_at) return null;

  const sync = syncState(game.sync_badge);

  return (
    <BadgeShell>
      <div style={{ display: "flex", flexDirection: "column", gap: "0.125rem", alignItems: "center" }}>
        <div>
          SAVE BACKUP
        </div>
        <div style={{ color: sync ? sync.color : "#79c0ff" }}>
          {sync ? sync.label : formatRelativeTime(game.save_last_backed_up_at)}
        </div>
      </div>
    </BadgeShell>
  );
}
