import type { LibraryGame } from "../../types";
import { formatRelativeTime } from "../../lib/format";

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
export function SpoolBackupBadge({ game }: { game: LibraryGame }) {
  if (!game.save_last_backed_up_at) return null;

  const when = formatRelativeTime(game.save_last_backed_up_at);
  const sync = syncState(game.sync_badge);
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
      <span style={{ opacity: 0.6, marginRight: "0.4rem" }}>☁️</span>
      Save backed up {when}
      {sync && (
        <>
          {sep}
          <span style={{ color: sync.color }}>{sync.label}</span>
        </>
      )}
    </div>
  );
}
