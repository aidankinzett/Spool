import type { LibraryGame } from "../../types";
import { formatRelativeTime } from "../../lib/format";
import { BadgeShell, BadgeSep } from "./badge-shell";
import { Spinner } from "@decky/ui";

// Maps the Rust `sync_badge` value (see library.rs) to a short label + colour.
// `null` means there's nothing to say about cloud sync (cloud not configured
// or no backup history), so we just show the local backup line.
function syncState(badge: string | null): { label: string; color: string } | null {
  switch (badge) {
    case "synced":
      return { label: "Synced to cloud", color: "#7ee787" };
    case "local-newer":
      return { label: "Not yet uploaded", color: "#e3b341" };
    case "cloud-newer":
      return { label: "Cloud has newer save", color: "#79c0ff" };
    default:
      return null;
  }
}

// Badge injected on the Steam /library/app/:appid page showing Spool's save
// backup status: when the save was last backed up and its cloud-sync state.
// While `backingUp` is set (a forced-close fallback backup is running) it shows
// a spinner instead, regardless of any prior backup history.
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
        <Spinner style={{ width: "14px", height: "14px", marginRight: "0.4rem" }} />
        Backing up save…
      </BadgeShell>
    );
  }

  if (!game?.save_last_backed_up_at) return null;

  const when = formatRelativeTime(game.save_last_backed_up_at);
  const sync = syncState(game.sync_badge);

  return (
    <BadgeShell>
      Save backed up {when}
      {sync && (
        <>
          <BadgeSep />
          <span style={{ color: sync.color }}>{sync.label}</span>
        </>
      )}
    </BadgeShell>
  );
}
