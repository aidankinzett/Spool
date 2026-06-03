import { useEffect, useRef } from "react";
import { useServerBase } from "../../hooks/use-server-base";
import { useSpoolPlaytime } from "../../hooks/use-spool-playtime";
import { useBackingUp } from "../../hooks/use-backing-up";
import { useParams } from "../../lib/steam";
import { SpoolPlaytimeBadge } from "./playtime-badge";
import { SpoolBackupBadge } from "./backup-badge";
import { SpoolMark } from "../spool-mark";
import { BadgeShell } from "./badge-shell";
import { SpoolLastPlayedBadge } from "./last-played-badge";

// Badge wrapper injected into the game detail page's InnerContainer via
// afterPatch. Uses useParams to read appid from Steam's internal router —
// window.location.pathname is always '/index.html' in Steam's CEF context.
//
// The library is fetched once here and the resolved entry is handed to each
// badge, so adding more badges doesn't multiply the HTTP traffic.
export function PatchWrapper() {
  const { base } = useServerBase();
  const { appid: appidStr } = useParams<{ appid: string }>();
  const appid = parseInt(appidStr ?? "0", 10);
  const { game, loading, refresh } = useSpoolPlaytime(appid, base);
  const backingUp = useBackingUp(appid);

  // When a backup finishes (backingUp falls back to false), re-fetch so the
  // badge swaps the spinner for the fresh "backed up · synced" line.
  const wasBackingUp = useRef(backingUp);
  useEffect(() => {
    if (wasBackingUp.current && !backingUp) void refresh();
    wasBackingUp.current = backingUp;
  }, [backingUp, refresh]);

  // Keep showing the badges while a backup runs even before the first fetch
  // resolves, so the spinner isn't gated behind `loading`/`game`.
  if (!appid || (!backingUp && (loading || !game))) return null;

  return (
    <div style={{ display: "flex", flexDirection: "row", gap: "0.25rem", padding: "0.5rem 2.8vw" }}>
      <BadgeShell>
        <div style={{ display: "flex", flexDirection: "row", gap: "0.4rem", alignItems: "center" }}>
          <SpoolMark size={16} />
          <div>
            Spool
          </div>
        </div>
      </BadgeShell>
      {game && <SpoolLastPlayedBadge game={game} />}
      {game && <SpoolPlaytimeBadge game={game} />}
      <SpoolBackupBadge game={game} backingUp={backingUp} />
    </div>
  );
}
